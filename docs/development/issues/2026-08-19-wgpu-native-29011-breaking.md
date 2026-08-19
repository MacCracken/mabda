# wgpu-native `v29.0.1.1` is a BREAKING change despite the patch-level version — do not bump blind

**Status:** 🟡 **OPEN** — evaluated and deliberately not taken in 4.0.10; **scheduled v4.1.0**
(see [`../roadmap.md`](../roadmap.md) § v4.1.0).
**Placement:** `deps/fetch-wgpu.sh:6`, `deps/wgpu_main.c`, `src/backend_wgpu.cyr`.
**Discovered:** 2026-08-19, mabda 4.0.10 currency sweep.
**Severity:** **Medium** — currently dormant (we are pinned to `v29.0.0.0`, which works).
It becomes **High** the moment someone bumps the pin on the reasonable assumption that
`29.0.0.0 → 29.0.1.1` is a patch.
**Affects:** the wgpu backend on every mabda version. Not the native AMD or NVIDIA paths.

## Why this file exists

mabda 4.0.9 recorded this deferral with the reasoning *"bumping an untested FFI dependency
inside a patch whose thesis is 'no output changes' is the wrong trade."* That was the right
call for the wrong reason — it treated the risk as **unquantified**. It is now quantified,
and the answer is worse than "untested": **the bump does not build, and one path fails
silently rather than loudly.** Recording the measurement so the next person does not have
to re-derive it, and does not talk themselves into it because "it's only a patch bump".

## Measured: `webgpu.h` is safe, `wgpu.h` is not

Both headers were diffed between the shipped `v29.0.0.0` and upstream `v29.0.1.1`.

**`webgpu.h` — purely additive, 19 lines, ZERO removals.** One new enum value
(`WGPUWGSLLanguageFeatureName_LinearIndexing = 0x0A`), three new `SetImmediates` entry
points and their proc typedefs. **No struct field added, removed, reordered or resized.**
So the descriptor offsets pinned in `src/wgpu_descriptors.cyr` are unaffected — which is
the thing this repo's audit checklist item 7 exists to protect, and it is fine.

**`wgpu.h` — the native-extensions header — carries at least five breaking changes:**

1. ⛔ **`WGPUSType_*` values are RENUMBERED.** `WGPUSType_PipelineLayoutExtras`
   (`0x00030003`) was **deleted** and every value after it shifted **down by one**:

   | Symbol | v29.0.0.0 | v29.0.1.1 |
   | --- | --- | --- |
   | `ShaderSourceGLSL` | `0x00030004` | `0x00030003` |
   | `InstanceExtras` | `0x00030006` | `0x00030004` |
   | `BindGroupEntryExtras` | `0x00030007` | `0x00030005` |
   | `BindGroupLayoutEntryExtras` | `0x00030008` | `0x00030006` |
   | `QuerySetDescriptorExtras` | `0x00030009` | `0x00030007` |
   | `SurfaceConfigurationExtras` | `0x0003000A` | `0x00030008` |
   | `SurfaceSourceSwapChainPanel` | `0x0003000B` | `0x00030009` |
   | `PrimitiveStateExtras` | `0x0003000C` | `0x0003000A` |

2. ⛔ **`WGPUNativeFeature_SpirvShaderPassthrough` (`0x00030017`) is GONE.** That numeric
   slot is now `WGPUNativeFeature_ClearTexture`.
3. **`WGPUPipelineLayoutExtras` — the whole struct — is removed** (`immediateDataSize`
   moved onto `WGPUPipelineLayout` as `immediateSize`).
4. **`WGPUNativeLimits` changes shape**: `maxImmediateSize` removed, and
   `maxBindingArraySamplerElementsPerShaderStage` + `maxMultiviewViewCount` added.
5. **`WGPUNativeTextureFormat_{R,Rg,Rgba}16{U,S}norm` removed**, folded into the core
   `WGPUTextureFormat_*` namespace with different spellings (`RG16Unorm`, `RGBA16Unorm`).

Also, the three `wgpuXxxSetImmediates` entry points change argument order — `(encoder,
offset, sizeBytes, data)` becomes `(encoder, offset, data, size)`. mabda does not call
them, but a consumer's launcher might.

## What breaks in mabda specifically

| Site | What happens on v29.0.1.1 |
| --- | --- |
| `deps/wgpu_main.c:75-76` | References `WGPUNativeFeature_SpirvShaderPassthrough` **by name**. The symbol no longer exists → **the C launcher does not compile.** A loud failure, which is the good case. |
| `src/backend_wgpu.cyr:1166` | `var WGPU_NATIVE_FEATURE_SPIRV_SHADER_PASSTHROUGH = 0x00030017;` is a **hardcoded numeric literal** on the Cyrius side. `0x00030017` is now `ClearTexture`. Nothing errors — mabda would **request a different feature entirely** and the passthrough path would misbehave with no diagnostic. ⛔ **This is the dangerous one.** |
| `deps/wgpu_main.c:383` | Uses `WGPUSType_InstanceExtras` **by symbol**, so it recompiles to the new `0x00030004` correctly. Safe *because* it is a symbol and not a literal. |

⭐ **The asymmetry in that table is the transferable lesson.** Every site that spells the
constant as a **name** survives a renumber; the one site that hardcodes the **number**
silently does the wrong thing. `src/backend_wgpu.cyr:1166` is Cyrius, which cannot include
`wgpu.h`, so a literal is unavoidable there — which means it needs a comment pinning it to
a specific wgpu-native version, and a check at bump time. Grep for other bare `0x0003xxxx`
literals in `src/` before any future bump.

## Verified NOT affected

- `src/wgpu_descriptors.cyr` — no `webgpu.h` struct changed, so no offset moved.
- No `src/*.cyr` hardcodes any of the renumbered `WGPUSType_*` values (grepped
  `0x00030006`, `0x00030009`, `0x0003000A`, `InstanceExtras`, `QuerySetDescriptorExtras`,
  `SurfaceConfigurationExtras`, `PipelineLayoutExtras`, `NativeLimits`) — the only hit is
  the `SpirvShaderPassthrough` literal above.
- The native AMD and native NVIDIA backends do not touch wgpu at all.

## Reproduction

```sh
curl -sL -o /tmp/w.zip \
  https://github.com/gfx-rs/wgpu-native/releases/download/v29.0.1.1/wgpu-linux-x86_64-release.zip
unzip -qo /tmp/w.zip -d /tmp/wgpu-new
diff deps/wgpu-native/include/webgpu/wgpu.h   /tmp/wgpu-new/include/webgpu/wgpu.h
diff deps/wgpu-native/include/webgpu/webgpu.h /tmp/wgpu-new/include/webgpu/webgpu.h   # additive only
```

⚠ Unpack to a scratch directory, **not** over `deps/wgpu-native/`. That tree is gitignored,
so an in-place overwrite leaves no diff to notice and no way to `git checkout` back.

## Proposed fix — scheduled v4.1.0 (a minor, not a patch)

1. Replace `src/backend_wgpu.cyr:1166`'s bare literal with a version-pinned constant plus a
   comment naming the wgpu-native release it was read from, so the next bump has an anchor.
2. Decide what replaces `SpirvShaderPassthrough`. It is deleted upstream, not renamed —
   establish whether raw-SPIR-V passthrough still exists in v29.0.1.1 at all before
   assuming a rename. (Note the prior HW finding that passthrough was already unsupported
   on RADV/Cezanne and mabda's f64 path goes through `ShaderF64` + naga instead, so the
   real-world blast radius may be smaller than the code surface suggests.)
3. Audit `deps/wgpu_main.c` against the new `wgpu.h` — it is consumer-copied, so any change
   forces every consumer to rebuild their launcher. That coordination cost is the actual
   reason this is not a patch-level change.
4. Re-run `make test-phase0` and the wgpu e2e set. There is no CI coverage for any of this;
   the wgpu path is a developer gate only.
5. Only then bump `deps/fetch-wgpu.sh:6`.

## Context

- The wgpu path is scheduled to retire: AMD-on-wgpu deprecated at v4.0.1, NVIDIA at v5.0,
  full retirement at v5.1. Weigh the work above against that horizon — it may be correct to
  stay on `v29.0.0.0` until the path leaves the tree, and simply say so out loud in each
  release rather than re-deferring silently.
- `deps/wgpu-native/` and `deps/*.o` are gitignored build artifacts; only the `VERSION` line
  in `deps/fetch-wgpu.sh` is tracked, so the eventual bump is a one-line diff plus whatever
  source changes items 1-3 require.
