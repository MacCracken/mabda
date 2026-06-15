export const meta = {
  name: 'mabda-audit-2026-06-14',
  description: 'P(-1) security audit: regression-check prior findings, fan-out dimension finders with adversarial verify, external CVE sweep',
  phases: [
    { title: 'Regression', detail: 'verify 5 prior ship-blockers + 11 deferrals against current tree' },
    { title: 'Research', detail: 'amdgpu/DRM/wgpu/WebGPU CVE sweep since 2026-04-30' },
    { title: 'Find', detail: '10 dimension-focused security finders over src/*.cyr' },
    { title: 'Verify', detail: 'adversarial verification — 2 skeptics per candidate finding' },
  ],
}

// ---------------------------------------------------------------------------
// Shared context blocks
// ---------------------------------------------------------------------------

const PROJECT = `PROJECT — mabda, a GPU foundation library written in Cyrius (a manual-memory systems language). Source root: src/*.cyr (38 modules, ~12,500 lines). Dual backend: wgpu-native (via C shim deps/wgpu_main.c) + native AMD (direct DRM ioctls, GFX9 PM4, no libdrm/libc).

TRUST MODEL: library called by TRUSTED consumer code. BUT the native AMD backend is a GPU client of the kernel — a malformed ioctl payload can crash the kernel-side amdgpu/DRM driver, so the defensive posture applies INWARD (toward the kernel) as well as outward.

CYRIUS / MABDA INVARIANTS (violations of these are real bugs):
- Manual memory: alloc(N) / store64 / store32 / store16 / load64 / load32 / load16. Every struct has a HEADER COMMENT BLOCK listing field byte-offsets; the comment's stated byte count MUST equal the actual alloc(N) / var X[N]. A mismatch (comment says 96B, alloc is 88B) is a real bug. Max written offset must be < N.
- f64 internally; f32 ONLY at the GPU buffer boundary (via f64_to_f32). Writing raw f64 into a GPU buffer is a bug.
- Cyrius >> on i64 is LOGICAL right shift (NO sign extend). GPU virtual addresses (48-bit) need explicit high-bit masking; code that assumes arithmetic/sign-extending shift on a VA or negative value is a bug.
- Cyrius / and % are INTEGER ops; / truncates toward zero. Any / or % must guard divisor != 0 first.
- fncall6 into wgpu-native reliably SEGFAULTS. Any wgpu call with 6+ i64 args MUST go through a struct-packing C shim in deps/wgpu_main.c, invoked as fncall2(handle, struct_ptr). A direct fncall6 into wgpu (or any Cyrius fn with 7+ params that internally fncall*s into wgpu) is a CRITICAL-class defect. Pure-Cyrius fns may take many args — the ceiling is specifically fns that fncall* into wgpu.
- var X; is rejected by Cyrius — every var needs an initializer.
- The bump allocator is PROCESS-LIFETIME, never freed. alloc() in a per-frame hot path (compute dispatch, render draw, present) is a real leak for long-running consumers. One-shot programs are fine.
- gpu_shader_module_create is byte-polymorphic: wgpu reads bytes as WGSL UTF-8, native reads them as pre-compiled GFX9 ISA.
- Native kernel path: syscall(SYS_IOCTL, fd, num, ptr) only (no libdrm, no libc). Every syscall return must be checked; on an error path, any output buffer the caller will read must be deterministically zeroed.
- GFX9 register addresses must trace to Mesa gfx9.json. PM4 type-3 packet count field masks to 14 bits (0x3FFF).

REPORTING: report ONLY genuine defects with concrete file:line evidence and a quoted code snippet. Precision over volume — a false positive wastes verifier time. Set confidence honestly (0.0-1.0). Severity ladder: CRITICAL (exploitable immediately) / HIGH (moderate effort, or a correctness defect that breaks the cross-backend identity contract) / MEDIUM (specific conditions) / LOW (defense-in-depth).`

const PRIOR = `PRIOR AUDIT (2026-04-30) findings — do NOT re-report these as NEW. If you encounter one, set prior_ref to its tag and note current status. The regression agents verify these separately.
SHIP-BLOCKERS (were declared required before 3.0.0 GA — they should be FIXED now):
- HIGH-1: _backend_native_surface_configure ignored consumer width/height, used EDID preferred mode instead.
- HIGH-2: gpu_surface_configure_native_logind leaked the DRM master fd on every failure path after samvada_session_take_device succeeded.
- MED-1: _backend_native_surface_present returned a negative kernel errno where the slot ABI declares a positive GPU_ERR_*.
- MED-3: native render-pass viewport divided rt_width/rt_height by 2 with no even-dimension guard (odd dims -> 0.5px viewport offset, breaks cross-backend pixel identity).
- MED-6: native_kms_present drained only the first DRM event; a multi-event read queue caused -3 on the 2nd present.
DEFERRED to v3.x (confirm still-open and reassess severity):
- MED-2: no overflow guards on caller-supplied width/height in native_kms_alloc_fb (pitch*height, pitch u32 truncation vs i64 BO size).
- MED-4: TOCTOU between count-pass and fill-pass on two-pass DRM ioctls (native_kms_init, get_connector_modes) — need min(actual, capacity) clamp.
- MED-5: native_drm_set_master collapses -EINVAL to success with no probe.
- MED-7 + LOW-5: per-dispatch alloc() of PM4 scratch (1024B render draw / 256B compute dispatch) -> bump-allocator leak in frame loops.
- LOW-1: native_drm_set_master / native_drm_drop_master don't guard fd<=0.
- LOW-2: _kms_summary_print_u32 uses a 16-byte buf (overflows for >16-digit values).
- LOW-3: native buffer_create/_write/_read/_shader_module_create were stubs (NOTE: GPU_ERR_NOT_IMPLEMENTED=18 now exists in error.cyr — verify whether the stubs now return it).
- LOW-4: native render_pass_draw ignores caller vc/ic (fixed 3-vertex / 1-instance).
- LOW-6: gpu_surface_release doesn't zero ctx native_card_fd / wgpu_surface_handle after release.`

const FINDINGS_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['dimension', 'files_read', 'findings'],
  properties: {
    dimension: { type: 'string' },
    files_read: { type: 'array', items: { type: 'string' } },
    findings: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['id', 'severity', 'file', 'lines', 'title', 'cwe_class', 'description', 'impact', 'mitigation', 'code_snippet', 'confidence', 'is_new', 'prior_ref'],
        properties: {
          id: { type: 'string', description: 'short unique slug, e.g. ovf-pitch-kms' },
          severity: { type: 'string', enum: ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW'] },
          file: { type: 'string' },
          lines: { type: 'string', description: 'line or line-range, e.g. 412-419' },
          title: { type: 'string' },
          cwe_class: { type: 'string' },
          description: { type: 'string' },
          impact: { type: 'string' },
          mitigation: { type: 'string' },
          code_snippet: { type: 'string', description: 'the exact offending lines quoted' },
          confidence: { type: 'number' },
          is_new: { type: 'boolean', description: 'true if not in the prior 2026-04-30 audit' },
          prior_ref: { type: 'string', description: 'prior audit tag if this matches one, else empty string' },
        },
      },
    },
  },
}

const VERDICT_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['finding_id', 'lens', 'is_real', 'confidence', 'reasoning', 'adjusted_severity'],
  properties: {
    finding_id: { type: 'string' },
    lens: { type: 'string' },
    is_real: { type: 'boolean' },
    confidence: { type: 'number' },
    reasoning: { type: 'string', description: 'cite the actual code you re-read; explain why real or false-positive' },
    adjusted_severity: { type: 'string', enum: ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW', 'NONE'] },
  },
}

const REGRESSION_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['items'],
  properties: {
    items: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['ref', 'title', 'status', 'evidence_file', 'evidence_lines', 'current_behavior', 'recommendation'],
        properties: {
          ref: { type: 'string', description: 'prior tag, e.g. HIGH-1' },
          title: { type: 'string' },
          status: { type: 'string', enum: ['FIXED', 'STILL_OPEN', 'PARTIAL', 'CANT_DETERMINE'] },
          evidence_file: { type: 'string' },
          evidence_lines: { type: 'string' },
          current_behavior: { type: 'string', description: 'what the code does now, with a quoted snippet' },
          recommendation: { type: 'string' },
        },
      },
    },
  },
}

const CVE_SCHEMA = {
  type: 'object',
  additionalProperties: false,
  required: ['cves', 'notes'],
  properties: {
    cves: {
      type: 'array',
      items: {
        type: 'object',
        additionalProperties: false,
        required: ['id', 'subject', 'date', 'applicability', 'mabda_exposure', 'source_url'],
        properties: {
          id: { type: 'string' },
          subject: { type: 'string' },
          date: { type: 'string' },
          applicability: { type: 'string', enum: ['APPLICABLE', 'CLASS_APPLICABLE', 'NOT_APPLICABLE'] },
          mabda_exposure: { type: 'string' },
          source_url: { type: 'string' },
        },
      },
    },
    notes: { type: 'string' },
  },
}

// ---------------------------------------------------------------------------
// Finder dimensions
// ---------------------------------------------------------------------------

const DIMENSIONS = [
  {
    key: 'buffer-alloc-safety',
    files: 'src/context.cyr, src/backend.cyr, src/backend_native.cyr, src/backend_native_kms.cyr, src/render_target.cyr, src/render_pipeline.cyr, src/texture.cyr, src/buffer.cyr, src/typed_buffer.cyr, src/shader_cache.cyr, src/pipeline_cache.cyr, src/bind_group_cache.cyr, src/resource.cyr',
    focus: `BUFFER / ALLOC SAFETY. For every alloc(N) and every var X[N] in scope, find the struct's header comment block and confirm the stated byte count == N, and that the LARGEST field offset written (via store64/store32/store16) is < N. Flag: comment-vs-alloc mismatches, writes past the end, fixed-size buffers (var buf[N]) that a fmt/print/memcpy could overrun, off-by-one in offset constants, and any struct whose SIZE constant disagrees with the sum of its slot offsets. Cross-check BACKEND_SIZE, GpuContext size, and the native struct sizes.`,
  },
  {
    key: 'integer-overflow',
    files: 'src/texture.cyr, src/buffer.cyr, src/typed_buffer.cyr, src/compute.cyr, src/render_target.cyr, src/backend_native.cyr, src/backend_native_kms.cyr, src/backend_native_pm4.cyr, src/capabilities.cyr, src/vertex.cyr',
    focus: `INTEGER OVERFLOW on size/dimension/offset/count math. Every a*b, a+b, a<<n on a size, dimension, pitch, stride, byte-count, or workgroup count needs an overflow guard before use — especially width*height*bpp, pitch*height, pitch rounding ((x+255)/256*256), BO size math, PM4 dword*4 sizing, vertex_count*stride, and VA + offset. Flag where a result is later truncated to u32 (e.g. an ioctl pitch field) while a wider value is used for the BO size — the truncation mismatch is the dangerous case. Note prior MED-2 (kms pitch*height) — confirm current state.`,
  },
  {
    key: 'divide-by-zero',
    files: 'src/compute.cyr, src/capabilities.cyr, src/backend_native.cyr, src/backend_native_pm4.cyr, src/backend_native_kms.cyr, src/render_graph.cyr, src/profiler.cyr, src/color.cyr',
    focus: `DIVIDE-BY-ZERO and MODULO-BY-ZERO. Every / and % must verify the divisor is non-zero first. Workgroup-count helpers (ceil-div by workgroup_size), viewport math (rt_width/2), EMA/averaging in the profiler, any aspect-ratio or stride division. Also flag integer-division truncation that silently produces wrong results where exact math is required (prior MED-3 odd-dimension /2 viewport).`,
  },
  {
    key: 'input-validation',
    files: 'src/buffer.cyr, src/texture.cyr, src/compute.cyr, src/render_pipeline.cyr, src/render_pass.cyr, src/render_target.cyr, src/sampler.cyr, src/depth.cyr, src/bind_group.cyr, src/surface_v3.cyr, src/surface.cyr, src/capabilities.cyr, src/typed_buffer.cyr',
    focus: `INPUT VALIDATION on consumer-supplied data. Every public gpu_* entry that takes consumer data (buffer sizes, texture width/height/depth/mip/layer counts, workgroup counts, descriptor fields, label strings, offsets, surface width/height) must validate bounds / ranges / non-negativity / upper limits BEFORE use. Flag missing null-ctx guards, missing positive-dimension checks, missing upper bounds (WebGPU MAX_TEXTURE_DIMENSION_2D = 16384), missing workgroup-limit checks, and label-string length handling. Note where the wgpu and native dispatchers validate DIFFERENTLY for the same public call (a cross-backend behavior gap).`,
  },
  {
    key: 'syscall-return-handling',
    files: 'src/backend_native_amdgpu.cyr, src/backend_native_kms.cyr, src/backend_native.cyr',
    focus: `SYSCALL / IOCTL RETURN HANDLING. Every syscall(SYS_IOCTL, ...) and syscall(read/write/close/mmap/...) return value must be checked. On an error return, any output buffer the caller will subsequently read must be deterministically zeroed (not left with stale/garbage). Flag: unchecked syscall returns, error paths that leave an output struct partially written, mmap returns not checked for MAP_FAILED / negative, GEM/BO-create returns used without a validity check, two-pass ioctls that trust a post-fill count without min(actual, capacity) clamping (prior MED-4), and -EINVAL/-errno collapsed to success (prior MED-5).`,
  },
  {
    key: 'pointer-validation-and-leaks',
    files: 'src/surface_v3.cyr, src/surface.cyr, src/backend_native.cyr, src/backend_native_kms.cyr, src/backend_native_amdgpu.cyr, src/context.cyr, src/resource.cyr',
    focus: `POINTER VALIDATION and RESOURCE LEAKS on failure paths. Flag: raw deref of a consumer-supplied pointer without a null guard; fd / GEM-handle / DRM-master / syncobj / BO leaks on early-return error paths (the function acquired a resource, then returns without releasing on a later failure); use-after-release (a ctx stash field left pointing at a released surface or closed fd — prior LOW-6); double-close. Trace every acquire (open/GEM_CREATE/SET_MASTER/samvada_session_take_device/syncobj_create) to its release on EVERY exit path. Note prior HIGH-2 (master fd leak) — confirm current state.`,
  },
  {
    key: 'ffi-offsets-and-fncall6',
    files: 'src/wgpu_descriptors.cyr, src/wgpu_ffi.cyr, src/wgpu_types.cyr, src/backend_wgpu.cyr, src/context.cyr',
    focus: `WGPU FFI CORRECTNESS. (1) fncall6 ceiling: find any fncall6 (or fncall with 6+ args) that targets a wgpu-native function pointer — that segfaults and is CRITICAL; it must route through a deps/wgpu_main.c struct-packing shim called via fncall2(handle, struct_ptr). Also flag any Cyrius fn with 7+ params that internally fncall*s into wgpu. (2) Descriptor offsets: every store at +offset into a packed wgpu descriptor in wgpu_descriptors.cyr must match the wgpu-native v29 webgpu.h struct layout; flag offsets that look inconsistent with neighbors, missing padding, or a descriptor whose total size disagrees with its alloc. (3) String views: label strings should use an explicit length (wgpu_string_view_len), not assume NUL-termination.`,
  },
  {
    key: 'native-isa-pm4-va',
    files: 'src/backend_native_shaders.cyr, src/backend_native_pm4.cyr, src/backend_native_amdgpu.cyr',
    focus: `NATIVE GFX9 ISA / PM4 / VIRTUAL-ADDRESS CORRECTNESS. (1) Logical-right-shift gotcha: any >> used to extract the high bits of a 48-bit GPU VA (e.g. va>>32, va>>8 for 256B alignment) — confirm masking is explicit and correct given Cyrius >> is logical. A VA split into lo/hi words for a PM4/ioctl field is the classic bug site. (2) PM4 packet construction: type-3 header count masks to 14 bits (0x3FFF); confirm dword counts match the actual packet body length; flag any packet whose declared size != emitted size. (3) GFX9 register addresses: spot-check R_* constants are plausible GFX9 addresses (cite gfx9.json convention); flag hardcoded magic addresses without a citation. (4) Shader ISA bytes: flag any hand-encoded instruction whose comment claims an opcode that doesn't match the bytes (prior native-render TDR was an EXP opcode 0xF8 vs 0xC4 byte bug).`,
  },
  {
    key: 'cache-correctness',
    files: 'src/shader_cache.cyr, src/pipeline_cache.cyr, src/bind_group_cache.cyr, src/render_graph.cyr, src/resource.cyr, src/gpu_timestamps.cyr',
    focus: `CACHE & RESOURCE-TRACKER CORRECTNESS. (1) Hash keys: shader/pipeline/bind_group caches are u64-keyed — flag weak hashing that collides easily, keys that omit a field that affects the cached object (two different descriptors hashing equal -> wrong cached object returned), and any hash seeded such that distinct inputs map to the same bucket. (2) Capacity / eviction: fixed-size cache tables that overflow or wrap without bound. (3) render_graph toposort: cycle handling, edge-count tracking, transient-resource lifetime/aliasing correctness. (4) resource tracker: double-track / leak / counter underflow.`,
  },
  {
    key: 'internal-correctness-toolchain',
    files: 'src/error.cyr, src/surface_v3.cyr, src/backend_native.cyr, src/lib.cyr, src/profiler.cyr, src/color.cyr, src/blend.cyr, src/instancing.cyr, src/debug.cyr',
    focus: `INTERNAL CORRECTNESS + TOOLCHAIN-JUMP (cyrius 6.0->6.2, samvada 0.2.2->0.4.1) regressions. (1) Result/error propagation: Ok/Err shapes consistent; error codes returned where the public API documents them; negative-errno vs positive-GPU_ERR mixups across the backend boundary (prior MED-1). (2) Global init order and any module-level constant that depends on another module's init (Cyrius has init-order gotchas). (3) Any logical-right-shift, var X; rejection workaround, or fncall arity that the 6.0->6.2 jump could have changed semantics on. (4) samvada API: mabda calls samvada_session_take_device / samvada_session_release_device / samvada_main — confirm the call shapes match a 0.4.1 contract (signatures unchanged per CHANGELOG, but verify mabda's call sites). (5) The duplicate-fn warning: tests/tcyr/mabda.tcyr redefines present_mode_to_wgpu (already in src/surface.cyr) — assess whether the test is silently testing a stale copy.`,
  },
]

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

log(`mabda P(-1) audit: ${DIMENSIONS.length} finder dimensions + regression + CVE sweep`)

// --- Regression check (2 agents) + CVE sweep (2 agents) run as one barrier ---
const regressionAndResearch = parallel([
  () => agent(
    `${PROJECT}\n\n${PRIOR}\n\nTASK: Verify the current status of the FIVE prior SHIP-BLOCKER findings (HIGH-1, HIGH-2, MED-1, MED-3, MED-6). These were declared mandatory before the 3.0.0 GA tag (GA shipped 2026-06-02), so each SHOULD be FIXED now. For EACH: grep the codebase by FUNCTION NAME (line numbers in the prior audit are stale — the files were reorganized), read the current implementation, and determine status (FIXED / STILL_OPEN / PARTIAL / CANT_DETERMINE) with a quoted current-code snippet as evidence. Key functions: _backend_native_surface_configure (surface configure slot), gpu_surface_configure_native_logind (in surface_v3.cyr), _backend_native_surface_present, the native render-pass viewport builder (search backend_native.cyr / backend_native_pm4.cyr for rt_width/2 or viewport), native_kms_present (in backend_native_kms.cyr). Use Grep and Read freely.`,
    { label: 'regression:ship-blockers', phase: 'Regression', schema: REGRESSION_SCHEMA }
  ),
  () => agent(
    `${PROJECT}\n\n${PRIOR}\n\nTASK: Verify the current status of the ELEVEN prior DEFERRED findings (MED-2, MED-4, MED-5, MED-7, LOW-1, LOW-2, LOW-3, LOW-4, LOW-5, LOW-6 — note MED-7 and LOW-5 are the same per-dispatch alloc leak class on render vs compute). These were deferred to v3.x at GA. For EACH: grep by function name, read current code, determine status (FIXED / STILL_OPEN / PARTIAL / CANT_DETERMINE) with a quoted snippet, and re-assess whether the severity is still right. Special checks: LOW-3 — GPU_ERR_NOT_IMPLEMENTED=18 now exists in error.cyr; do the native buffer/shader stubs now return it? MED-7/LOW-5 — do native compute_dispatch and render_pass_draw still alloc() a PM4 scratch per call, or is it cached on the ctx now? Use Grep and Read freely.`,
    { label: 'regression:deferred', phase: 'Regression', schema: REGRESSION_SCHEMA }
  ),
  () => agent(
    `${PROJECT}\n\nTASK: External CVE / advisory sweep for the period 2026-04-30 (date of the last mabda audit) through 2026-06-14 (today). Search for vulnerabilities in the domains mabda's NATIVE AMD backend touches: Linux amdgpu kernel driver, DRM/KMS modeset, drm-prime / dma-buf cross-fd handle, drm-syncobj, drm-event page-flip read path, AMDGPU command-submission (CS) chunk parsing, GEM/BO allocation. Use WebSearch and WebFetch (query NVD, oss-security, the amd-gfx mailing list, kernel CVE feeds). For each relevant CVE return id, subject, date, applicability to mabda (APPLICABLE if mabda's exact code path is reachable, CLASS_APPLICABLE if the same bug class could apply, NOT_APPLICABLE otherwise), a one-line mabda_exposure assessment, and the source URL. Focus on whether any maps onto mabda's hand-laid ioctl payloads or PM4/CS streams. If a search returns nothing concrete for a domain, say so in notes rather than inventing CVEs.`,
    { label: 'research:amdgpu-drm-cve', phase: 'Research', schema: CVE_SCHEMA }
  ),
  () => agent(
    `${PROJECT}\n\nTASK: External CVE / advisory sweep for 2026-04-30 through 2026-06-14 in the WGPU / WebGPU domain: wgpu-native v29.x patch releases and security notes, WebGPU spec security advisories, naga shader-validation issues, and any integer-overflow / descriptor-confusion / texture-dimension CVEs in WebGPU implementations. Use WebSearch and WebFetch (gfx-rs/wgpu-native releases, NVD WebGPU/wgpu query, GHSA advisories). mabda pins wgpu-native v29 and ships descriptor structs hand-laid in wgpu_descriptors.cyr against webgpu.h. For each relevant item return id, subject, date, applicability (APPLICABLE/CLASS_APPLICABLE/NOT_APPLICABLE), mabda_exposure, source_url. If nothing concrete surfaces for a sub-domain, say so in notes — do not fabricate CVE numbers.`,
    { label: 'research:wgpu-webgpu-cve', phase: 'Research', schema: CVE_SCHEMA }
  ),
])

// --- Finder pipeline: find candidates -> adversarially verify each ---
const finderPipeline = pipeline(
  DIMENSIONS,
  (d) => agent(
    `${PROJECT}\n\n${PRIOR}\n\nYOU ARE THE "${d.key}" SECURITY FINDER.\nRead these files in full and audit them: ${d.files}\n\nDIMENSION FOCUS:\n${d.focus}\n\nReturn every genuine candidate finding with concrete file:line evidence and a quoted code_snippet. If this dimension is clean, return an empty findings array (that is a valid and useful result). Do not pad with speculative findings.`,
    { label: `find:${d.key}`, phase: 'Find', schema: FINDINGS_SCHEMA }
  ),
  async (found, d) => {
    if (!found || !found.findings || found.findings.length === 0) {
      return { dimension: d.key, files_read: found ? found.files_read : [], findings: [] }
    }
    // Adversarially verify each finding with 2 distinct lenses.
    const verified = await parallel(
      found.findings.flatMap((f) => {
        const ctx = `${PROJECT}\n\nA finder reported this candidate finding in mabda. Your job is to ADVERSARIALLY VERIFY it — try to REFUTE it. Re-read the cited code yourself (Grep/Read the file) before judging; do not trust the finder's snippet alone. Default to is_real=false if you cannot confirm the defect is genuinely reachable / genuinely wrong. A finding is only real if the code actually does what the finder claims AND that behavior is genuinely a defect under mabda's trust model and invariants.\n\nCANDIDATE FINDING:\nid: ${f.id}\nseverity: ${f.severity}\nfile: ${f.file}\nlines: ${f.lines}\ntitle: ${f.title}\ndescription: ${f.description}\nimpact: ${f.impact}\ncode_snippet:\n${f.code_snippet}\n`
        return [
          () => agent(
            `${ctx}\nLENS: CORRECTNESS & REACHABILITY. Re-read the actual code. Does it really do what the finding claims? Is the defective path actually reachable from a public entry point or a real driver/consumer interaction? Is the math/logic actually wrong? Confirm or refute with reasoning that quotes the real code.`,
            { label: `verify:${f.id}:correctness`, phase: 'Verify', schema: VERDICT_SCHEMA }
          ).then((v) => v ? { ...v, finding_id: f.id } : null),
          () => agent(
            `${ctx}\nLENS: FALSE-POSITIVE HUNT. Assume the finder is WRONG and find the reason: is there a guard earlier in the function, a caller that always validates, an invariant that makes the bad input impossible, a Cyrius semantic the finder misread, or is this intended/documented behavior? Only conclude is_real=true if you genuinely cannot find a reason it's safe.`,
            { label: `verify:${f.id}:fp-hunt`, phase: 'Verify', schema: VERDICT_SCHEMA }
          ).then((v) => v ? { ...v, finding_id: f.id } : null),
        ]
      })
    )
    // Attach verdicts + compute survival per finding.
    const byId = {}
    verified.filter(Boolean).forEach((v) => { (byId[v.finding_id] ||= []).push(v) })
    const findings = found.findings.map((f) => {
      const verdicts = byId[f.id] || []
      const confirms = verdicts.filter((v) => v.is_real && v.confidence >= 0.5)
      const refutes = verdicts.filter((v) => !v.is_real && v.confidence >= 0.7)
      const survives = confirms.length >= 1 && refutes.length === 0
      // adjusted severity = highest non-NONE adjusted severity among confirming verdicts, else finder's
      const sevRank = { CRITICAL: 4, HIGH: 3, MEDIUM: 2, LOW: 1, NONE: 0 }
      let adj = f.severity
      confirms.forEach((v) => { if (sevRank[v.adjusted_severity] > sevRank[adj]) adj = v.adjusted_severity })
      return { ...f, survives, adjusted_severity: adj, verdicts }
    })
    return { dimension: d.key, files_read: found.files_read, findings }
  }
)

const [regRes, finderRes] = await Promise.all([regressionAndResearch, finderPipeline])

const regression = (regRes[0] && regRes[0].items) || []
const deferred = (regRes[1] && regRes[1].items) || []
const cveAmd = regRes[2] || { cves: [], notes: '' }
const cveWgpu = regRes[3] || { cves: [], notes: '' }

const allFindings = finderRes.filter(Boolean).flatMap((r) => (r.findings || []).map((f) => ({ ...f, dimension: r.dimension })))
const survived = allFindings.filter((f) => f.survives)
const dropped = allFindings.filter((f) => !f.survives)

log(`audit done: ${survived.length} confirmed findings, ${dropped.length} dropped by verification; regression ${regression.length} ship-blockers + ${deferred.length} deferrals re-checked`)

return {
  regression_ship_blockers: regression,
  regression_deferred: deferred,
  cve_amd_drm: cveAmd,
  cve_wgpu_webgpu: cveWgpu,
  confirmed_findings: survived,
  dropped_findings: dropped.map((f) => ({ id: f.id, file: f.file, title: f.title, severity: f.severity, reason: (f.verdicts || []).map((v) => `${v.lens}:${v.is_real ? 'real' : 'refuted'}(${v.confidence})`).join('; ') })),
  files_audited_by_dimension: finderRes.filter(Boolean).map((r) => ({ dimension: r.dimension, files: r.files_read, count: (r.findings || []).length })),
}
