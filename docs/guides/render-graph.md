# Render Graph Guide

> Written against mabda 4.5.0 / Cyrius 6.7.18. See
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
- **Backend-agnostic.** The graph is pure Cyrius and dispatches
  through the public mabda API, so the same builder code runs
  unchanged on all three backends: wgpu-native (cross-vendor default),
  native AMD (added alongside wgpu at v3.0), and native NVIDIA (added
  at v4.0). The native backends were added *alongside* wgpu, not as a
  replacement — the graph's public builder surface is identical across
  paths. Two executors sit under that surface: `rg_execute(g, device,
  queue)` is the wgpu single-submit path, and `rg_execute_mq(g, ctx)`
  is the ctx-taking, backend-routed path (real multi-queue on native
  AMD; single-submit-equivalent on wgpu and native NVIDIA).

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
| `rg_add_copy_tex_buf`     | texture copy   | `wgpu_command_encoder_copy_texture_to_buffer`                   |

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

The aliasing planner (shipped v3.4.3) is **OFF by default**
(`aliasing_flag` defaults to 0). When you enable it with
`rg_aliasing(g, 1)`, the build runs a greedy interval-coloring pass
that assigns disjoint-lifetime transients of the same kind a shared
byte offset into one backing block. The chosen offset is queryable via
`rg_transient_offset(g, res_id)`, the transient whose slot it took over
via `rg_transient_alias_of(g, res_id)`, and the block totals via
`rg_plan_aliasing_stats(g, out)`. Textures are sized by their format
(4.5.0; they were all sized as RGBA8).

What happens with the plan depends on the backend you build on with
`rg_build_ctx(g, ctx)` (4.5.0):

- **Native AMD:** the graph allocates its transients itself, in one GTT
  allocation (`rg_backing_bytes(g)`), and each transient is a view into
  it. `rg_transient_handle` gives a NativeBuf (buffers) or a
  NativeTexture (textures), and `rg_transient_target` gives a NativeRT
  for a texture with `WGPU_TEXTURE_USAGE_RENDER_ATTACHMENT`, so a node
  can render into a transient that a later node samples. With aliasing
  on, transients share bytes at the planner's offsets, and
  `rg_execute_mq` waits, before a node that reuses a slot, for every
  queue that used it. With aliasing off, every transient has its own
  bytes. Native transient textures are RGBA8, or RGBA8 sRGB if only
  sampled. The graph owns the views: release the graph, never a view.
- **wgpu:** one resource per transient, as with `rg_build`. wgpu's safe
  API has no placed or aliased resources, so the plan is advisory there:
  a consumer applies the offsets to its own sub-allocated store.

With aliasing on, the planner keeps two kinds of transient out of
shared bytes for the whole graph:
- an **input**, whose first user reads it, is planned from the first
  node, so fill it after the build and before `rg_execute_mq`;
- an **output**, whose last user writes it, is planned to the last node,
  so read it after `rg_execute_mq` returns.

Scratch that one node writes and a later one reads still shares bytes
with other scratch. `rg_transient_alias_of` tells you which transient
reuses which. Before every node that uses a reused transient,
`rg_execute_mq` waits for every queue that used its slot. Past 16 nodes
the executor also drains its queues before reusing its command-stream
slices.

Node ids stay valid after the build: `rg_node_set_pass`,
`rg_node_queue_get` and `rg_node_batch_idx` take the id `rg_add_*`
returned. `rg_node_reads` / `rg_node_writes` / `rg_node_queue` return 1
after the build, because the dependencies are fixed then.
`rg_release` frees the backing, zeroes the views and unbuilds the graph.
A graph is bound to the context `rg_build_ctx` built it on.

On native, build the graph's nodes before its transients have views:
add render nodes with pass 0, call `rg_build_ctx`, create each pass from
`rg_transient_target`, and give it to the node with
`rg_node_set_pass(g, node, pass)`. Compute bindings are your own arrays,
so fill them from `rg_transient_handle` after the build.

## Reads and writes

After adding a node, declare which resources it reads and writes:

```cyrius
var n_compute = rg_add_compute(g, cp, bg, &dims, "compute");
rg_node_reads(g, n_compute, r_buf);   # input buffer
rg_node_writes(g, n_compute, r_tex);  # writes into frame texture
```

These declarations drive two things:

1. **Topological sort** at `rg_build` time. `rg_build` runs Kahn's
   algorithm, inferring a writer→reader edge for every pair where one
   node writes a resource another reads — **independent of insertion
   order** (order-independent toposort shipped v3.4.3). Insert nodes in
   any order; a reader placed before its writer still sorts correctly.
   In-order graphs are just the subset where sort idx = insertion idx.
   Only a genuine cycle fails: `rg_build` returns `1` when the sorted
   count comes up short of the node count.
2. **Alias planner input.** When `rg_aliasing(g, 1)` is enabled, each
   transient's `first_use` and `last_use` node positions are derived
   from these declarations, and the planner (see above) uses those
   lifetimes to compute shared-offset placement.

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
- **No consumer-facing sync primitives.** On the wgpu path,
  wgpu-native handles texture layout transitions and memory barriers
  automatically. On native AMD, `rg_execute_mq` inserts cross-ring
  fence edges internally between dependent nodes on different queues.
  Either way the graph exposes no semaphore/fence API to the consumer;
  if you need explicit sync objects, reach for the raw encoder API.
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

## Multi-queue execution

Cross-queue coordination shipped in v3.2.13–v3.2.14 (Phase R). Declare
per-node queue affinity with `rg_node_queue(g, node_id, kind)` and
execute with `rg_execute_mq(g, ctx)`:

- On **native AMD**, the scheduler runs nodes on distinct HW rings
  (GFX / COMPUTE) ordered by in-CS timeline waits and cross-ring fence
  edges.
- On **wgpu** and **native NVIDIA** (any non-AMD backend), there is one
  device queue, so `rg_execute_mq` falls through to the single-submit
  path — serialized-equivalent, the identical command stream, with
  affinity honored as ordering only.

Omitting affinity reproduces the single-queue ordering. The plain
`rg_execute(g, device, queue)` single-submit path is always available.

## Out of scope (current)

Roadmap material, not current behavior:

- Backing reuse on wgpu. Native AMD shares one allocation since 4.5.0
  (above); wgpu's safe API has no placed or aliased resources, so wgpu
  keeps one resource per transient. Native NVIDIA has no render-graph
  executor yet (scheduled in v5.0, see the roadmap).
- Render-pass-across-nodes. Every render node opens and ends its
  own pass; there is no graph-level "shared render pass with N draws
  across N nodes" construct. That belongs in a higher-level
  frame authoring layer.

## Reference

- Module source: [`src/render_graph.cyr`](../../src/render_graph.cyr)
- Regression tests: [`tests/tcyr/render.tcyr`](../../tests/tcyr/render.tcyr)
  (search for `test_rg_*`)
- GPU integration programs:
  [`programs/render_graph_e2e.cyr`](../../programs/render_graph_e2e.cyr) (wgpu)
  and [`programs/native_render_graph_mq_e2e.cyr`](../../programs/native_render_graph_mq_e2e.cyr)
  (native multi-queue, HW-verified on Cezanne).
- `make test-render-graph-e2e` runs the integration against a live
  wgpu-native + Vulkan driver.
