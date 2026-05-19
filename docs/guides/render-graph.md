# Render Graph Guide

> Written against mabda 3.0.0-rc.2 / Cyrius 5.11.28. See
> [`usage.md`](usage.md) and
> [`../stdlib-integration.md`](../stdlib-integration.md) for the
> consumer-project setup. This guide covers
> [`src/render_graph.cyr`](../../src/render_graph.cyr) specifically.

## Why a render graph

Building a frame by hand — create encoder, open compute pass, set
pipeline, set bind group, dispatch, end pass, open render pass, bind
pipeline, draw, end pass, copy texture, finish encoder, submit — is a
lot of boilerplate that repeats across every consumer and every
variation. The render graph hides the repetition:

- **Declare nodes, not commands.** You say "this is a compute node
  with these inputs; that is a render node clearing this target; this
  is a copy from X to Y." The graph encodes everything into **one**
  command encoder and issues **one** queue submit.
- **Own transient resources.** The graph allocates ephemeral buffers
  and textures you only need for the duration of a submission and
  frees them when you release the graph. No explicit lifetime bookkeeping.
- **Survives the v3.0 backend swap.** The graph is pure Cyrius — it
  dispatches through the public mabda API (`wgpu_device_create_*`,
  `rpb_pass_begin`, `compute_dispatch`). When the native Cyrius GPU
  backend replaces wgpu-native in v3.0, the graph's public surface
  does not change.

## A three-node example

The pattern below mirrors [`programs/render_graph_e2e.cyr`](../../programs/render_graph_e2e.cyr):
compute writes into a storage buffer, a render node clears an
offscreen target to red, and a copy node reads the target back to host
memory. All three happen in a single `rg_execute` call.

```cyrius
# --- preparation: consumer owns these ---
var cp        = compute_pipeline_new(device, wgsl, "main", 1);
var bg        = wgpu_device_create_bind_group(device, bg_desc);
var rt        = texture_create_render_target_rgba8(device, 256, 256, "rt");
var rt_view   = texture_view_create_rgba8(rt, "rt-view");
var rpb       = rpb_pass_new();
rpb_pass_color(rpb, rt_view, red_color);
var read_buf  = wgpu_device_create_buffer(device, read_desc);

# --- build the graph ---
var g = rg_new();
rg_label(g, "frame-0");

var dims[12];
store32(&dims, 32); store32(&dims + 4, 1); store32(&dims + 8, 1);
rg_add_compute(g, cp, bg, &dims, "compute.double");

rg_add_render(g, rpb, 0, 0, "render.clear");

var cargs[72];
# ... pack WgpuCopyTexToBufArgs (src_texture=rt, dst_buffer=read_buf, ...)
rg_add_copy_tex_buf(g, &cargs);

# --- execute in one submission ---
rg_build(g, device);        # validates, allocates transients
rg_execute(g, device, queue);  # encode + submit
rg_release(g);              # free transients
```

That's the minimum. Everything below is elaboration.

## Node kinds

| Builder                   | Kind           | What it encodes                                                 |
|---------------------------|----------------|-----------------------------------------------------------------|
| `rg_add_compute`          | compute        | `begin_compute_pass → set_pipeline → set_bind_group → dispatch → end` |
| `rg_add_render`           | render         | `rpb_pass_begin → [set_pipeline + draw] → end`                  |
| `rg_add_copy_buf_buf`     | buffer copy    | `command_encoder_copy_buffer_to_buffer`                         |
| `rg_add_copy_tex_buf`     | texture copy   | `wgpu_shim_command_encoder_copy_texture_to_buffer`              |

Every `rg_add_*` returns a `node_id` (zero-indexed, unique per graph).
Keep it — you pass it to `rg_node_reads` / `rg_node_writes` below.

## Transient resources

Buffers or textures the graph owns for one submission:

```cyrius
var r_buf = rg_add_transient_buffer(g, 4096, usage_flags, "scratch");
var r_tex = rg_add_transient_texture(g, 1024, 1024, WGPU_TEXTURE_FORMAT_RGBA8_UNORM, usage_flags, "frame");
```

`rg_add_transient_*` returns a `res_id`. The actual GPU handle is not
allocated until `rg_build(g, device)`; after that,
`rg_transient_handle(g, res_id)` and `rg_transient_view(g, res_id)`
(texture only) return the live wgpu handles. `rg_release(g)` frees
them.

Transients whose usage fits both the graph's lifetime AND the
`aliasing_flag` set via `rg_aliasing(g, 1)` will eventually share
backing storage. The alias pass is scaffolded but **OFF by default**
in v2.5.0; set aliasing only if a consumer explicitly asks for
memory-tight frames. Until then every transient gets its own
allocation.

## Reads and writes

After adding a node, declare which resources it reads and writes:

```cyrius
var n_compute = rg_add_compute(g, cp, bg, &dims, "compute");
rg_node_reads(g, n_compute, r_buf);   # input buffer
rg_node_writes(g, n_compute, r_tex);  # writes into frame texture
```

These declarations drive two things:

1. **Topological sort** at `rg_build` time. If you insert nodes in a
   valid execution order, sort idx = insertion idx. If node A reads
   what later-inserted node B writes, `rg_build` returns `1` (cycle
   or invalid ordering). In v2.5.0 toposort respects insertion-order
   edges only — a programmatic consumer that builds out-of-order
   graphs should either pre-sort themselves or wait for the v2.5.1+
   full toposort.
2. **Alias pass input** (future). When `rg_aliasing(g, 1)` is enabled,
   a transient's `first_use` and `last_use` node positions are derived
   from these declarations; disjoint lifetimes share storage.

If you don't care about either, you can skip the declarations and the
graph executes nodes in insertion order with per-transient allocation.
That's what the three-node example above does.

## Execution semantics

`rg_execute(g, device, queue)` does exactly this:

1. Create one `WGPUCommandEncoder` labelled after `rg_label(g, ...)`.
2. Walk nodes in sorted order. Each node encodes directly into the
   encoder — no intermediate per-node encoders.
3. Finish the encoder, submit the single command buffer.
4. Return 0 on success, 1 on error (null device/queue, encoder
   create failure, unbuilt graph).

Consequences:

- **Cost scales with node count**, not with encoder count. Three
  nodes = one encoder + one submit, same as a hand-rolled
  equivalent.
- **No automatic barriers.** wgpu-native handles texture layout
  transitions and memory barriers automatically. If you need
  explicit sync primitives (semaphore, fence), reach for the raw
  encoder API — the graph doesn't expose them in v2.5.0.
- **Render passes inside the graph open with `rpb_pass_begin`**, so
  every rule that applies to that call applies inside the graph: no
  null encoder, at least one color attachment, etc.

## When NOT to use the render graph

- Per-pass command buffers you need to submit independently (cross-
  queue, frame fencing, deferred submission). Use the raw encoder
  API — the graph bundles everything into one submit.
- Conditional or branching control flow. The graph is a linear DAG;
  loops and conditionals belong in the consumer's own frame driver.
- Single-pass workloads. The boilerplate of `rg_new` + `rg_add_*` +
  `rg_build` + `rg_execute` isn't worth it for one dispatch.

## Out of scope for v2.5.0

Everything below is roadmap material, not current behavior:

- Full topological sort for out-of-order node insertion (today:
  insertion-order + cycle detection).
- Resource aliasing pass (transient lifetimes drive shared storage).
  The hooks exist — `first_use` field on `TransientResource`,
  `aliasing_flag` on the graph — but no pass reads them yet.
- Render-pass-across-nodes. Every render node opens and ends its
  own pass; there is no graph-level "shared render pass with N draws
  across N nodes" construct. That belongs in a higher-level
  frame authoring layer.
- Cross-queue coordination (compute on one queue, graphics on
  another). Single-queue only. Revisited in v3.1 with multi-queue.

## Reference

- Module source: [`src/render_graph.cyr`](../../src/render_graph.cyr)
- Regression tests: [`tests/tcyr/mabda.tcyr`](../../tests/tcyr/mabda.tcyr)
  (search for `test_rg_*`)
- GPU integration program:
  [`programs/render_graph_e2e.cyr`](../../programs/render_graph_e2e.cyr)
- `make test-render-graph-e2e` runs the integration against a live
  wgpu-native + Vulkan driver.
