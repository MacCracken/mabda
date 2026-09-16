#!/usr/bin/env python3
"""owned_state_audit.py — regenerate the register tables of
programs/diagnostics/radv_capture_triangle/owned-state-audit.md.

Every register in Mesa's GFX9 register DB (gfx9.json register_mappings, `mm`)
gets exactly one verdict:
  * written by mabda's render streams -> the owned-state table (clear-state,
    radv and mabda values, gfx9.json field decode, verdict);
  * not written -> a family row saying why the draw never consults it.
The script FAILS (exit 1) if any register is unclassified or an owned register
differs from both radv and the clear state without a written justification, so
a new register or value change cannot slip past the audit.

Inputs (all local files; see README.md in this directory for how to get them):
  --gfx9 gfx9.json            Mesa src/amd/registers/gfx9.json
  --clearstate clearstate_gfx9.h   linux drivers/gpu/drm/amd/amdgpu/, at the
                              running kernel's tag
  --radv-init BIN --radv-main BIN   radv_capture_triangle `make snoop` captures:
                              the nested gfx_init IB (tag c0-ib0) and the main
                              IB (chunk c1)
  --mabda DIR                 dump_streams output (current library)
  --mabda-before DIR          optional: dump of an earlier library for the
                              "before" column
  --check FILE                compare the generated block with the one between
                              the BEGIN/END GENERATED markers in FILE (exit 1 on
                              drift) instead of printing it
"""
import argparse
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
DECODER_DIR = os.path.join(HERE, '..', 'radv_capture_triangle')
BEGIN = '<!-- BEGIN GENERATED: owned_state_audit.py -->'
END = '<!-- END GENERATED: owned_state_audit.py -->'

# ---------------------------------------------------------------------------
# Owned registers whose value differs from BOTH radv's effective value on this
# GPU and the kernel clear state: each needs a reason (the script fails
# without one). RT- / VA-dependent registers are listed in DYNAMIC.
DYNAMIC = {
    'SPI_SHADER_PGM_LO_PS', 'SPI_SHADER_PGM_LO_VS', 'SPI_SHADER_PGM_HI_PS', 'SPI_SHADER_PGM_HI_VS',
    'SPI_SHADER_USER_DATA_PS_0', 'SPI_SHADER_USER_DATA_PS_1', 'PA_SC_SCREEN_SCISSOR_BR',
    'PA_SC_GENERIC_SCISSOR_BR', 'PA_CL_VPORT_XSCALE', 'PA_CL_VPORT_XOFFSET', 'PA_CL_VPORT_YSCALE',
    'PA_CL_VPORT_YOFFSET', 'CB_COLOR0_BASE', 'CB_COLOR0_BASE_EXT', 'CB_COLOR0_ATTRIB2',
    'CB_MRT0_EPITCH',
}
OWNED_NOTES = {
    'PA_SC_RASTER_CONFIG': 'not in the clear state; Mesa writes it only for GFX8 and older '
        '(ac_emit_raster_config) and linux v7.2.3 gfx_v9_0.c does not program it. Pre-4.1.3 write, '
        'kept: every gate incl. the 1920x1080 full-coverage scan passes with it (this GPU reports 2 RBs).',
    'SPI_PS_INPUT_ENA': 'PERSP_CENTER_ENA. Round 2 HW re-test: radv\'s 0x80 and LINEAR_CENTER 0x20 draw '
        'identically (old "black pixels" note retracted). Textured draws override to 0x302.',
    'SPI_PS_INPUT_ADDR': 'as SPI_PS_INPUT_ENA.',
    'SPI_SHADER_COL_FORMAT': 'SPI_SHADER_32_ABGR (mabda\'s FS exports f32); radv exports FP16_ABGR. Both '
        'valid export formats; gates read back exact values.',
    'PA_STEREO_CNTL': 'round 2: not in the GFX9 clear state and never written by radv on GFX9, so no '
        'CLEAR_STATE ever resets it; Mesa writes 0 on GFX11 (no CLEAR_STATE).',
    'PA_CL_GB_VERT_CLIP_ADJ': 'guard band ~254.99 vs radv 256.0 (GFX9_PA_CL_GB_CLIP_ADJ_F32; must not be 1.0).',
    'PA_CL_GB_HORZ_CLIP_ADJ': 'as PA_CL_GB_VERT_CLIP_ADJ.',
    'PA_SC_BINNER_CNTL_0': 'binning disabled (DISABLE_BINNING_USE_LEGACY_SC + Mesa\'s GFX9 disable pair); '
        'radv runs BINNING_ALLOWED here. DB_DFSM_CONTROL FORCE_OFF pairs with it.',
    'PA_SC_BINNER_CNTL_1': 'binner batch limits (only consulted with binning allowed).',
    'CB_COLOR0_ATTRIB': 'RESOURCE_TYPE 1; radv additionally sets RB_ALIGNED / PIPE_ALIGNED for its '
        'aligned surface, mabda\'s linear RT is not.',
    'PA_SU_SC_MODE_CNTL': '= clear state. Round 2 HW re-test under the clear poison: radv\'s 0x240 draws '
        'identically (old "black pixels" note retracted); culling off makes FACE / poly-mode types neutral.',
    'PA_CL_CLIP_CNTL': '= clear state (clipping disabled). Round 2 HW re-test: radv\'s 0x01080000 '
        '(clipping enabled) draws identically (old "black pixels" note retracted).',
    'CB_COLOR0_INFO': '= radv. Round 2 HW re-test: mabda\'s earlier 0x04000028 also draws identically (old '
        '"CB wrote zeros" note retracted).',
    'SPI_SHADER_PGM_RSRC3_VS': '= radv. Round 2: derived per device from the DRM_AMDGPU_INFO CU topology '
        '(native_gfx9_vs_late_alloc_derive); this Cezanne yields the capture value.',
    'SPI_SHADER_LATE_ALLOC_VS': 'as SPI_SHADER_PGM_RSRC3_VS.',
    'SPI_SHADER_PGM_RSRC3_PS': '= radv; constant on every GFX9 part (Mesa masks CU_EN only by AMD_CU_MASK).',
    'VGT_OUT_DEALLOC_CNTL': '= radv = clear state; round 2 corrected 16 -> 32 (the 30 / 32 pair radv '
        'and radeonsi run with on this GPU).',
    'PA_SC_AA_MASK_X0Y0_X1Y0': '= radv = clear state. Round 2 HW: a mask of 0 drew no pixel in any gate.',
    'PA_SC_AA_MASK_X0Y1_X1Y1': 'as PA_SC_AA_MASK_X0Y0_X1Y0.',
    'PA_SC_CLIPRECT_RULE': '= radv = clear state. Round 2 HW: a rule of 0 drew no pixel in any gate.',
}

# ---------------------------------------------------------------------------
# Families for registers mabda does NOT write: (regex on name, family, verdict).
# First match wins. Verdicts name the owned register(s) that make the family
# unreachable for mabda's draws (VS + PS, auto-index TRIANGLELIST, 1 instance,
# MRT0 RGBA8 1x, no depth/stencil, no blend, no varyings).
CTX_FAMILIES = [
    (r'^DB_', 'depth / stencil buffer',
     'No depth-stencil attachment: DB_Z_INFO / DB_STENCIL_INFO FORMAT = INVALID, DB_DEPTH_CONTROL = 0 '
     '(no test, no write), DB_RENDER_CONTROL / _OVERRIDE / _OVERRIDE2 = 0, DB_SHADER_CONTROL written. '
     'The DB never reads these; DB_COUNT_CONTROL only feeds occlusion counters.'),
    (r'^TA_BC_BASE_ADDR', 'border colour table',
     'Only for CLAMP_TO_BORDER addressing; mabda samplers use point/bilinear with clamp/repeat.'),
    (r'^COHER_DEST_BASE', 'coherent-copy destinations',
     'CP state-copy destinations; Mesa only lists them in its register-shadowing tables '
     '(ac_shadowed_regs.c), never writes them for draws.'),
    (r'^PA_SC_WINDOW_OFFSET$', 'window offset',
     'Applied only with PA_SU_SC_MODE_CNTL.VTX_WINDOW_OFFSET_ENABLE (written 0) and to scissors without '
     'WINDOW_OFFSET_DISABLE (window / generic / vport scissor TL written with it set).'),
    (r'^PA_SC_CLIPRECT_\d_(TL|BR)$', 'clip rectangles',
     'PA_SC_CLIPRECT_RULE written 0xFFFF: every inside/outside combination passes (round 2 HW: a rule of 0 '
     'drew nothing), so the rectangles never reject a pixel.'),
    (r'^PA_SC_VPORT_SCISSOR_([1-9]|1[0-5])_(TL|BR)$|^PA_SC_VPORT_Z(MIN|MAX)_([1-9]|1[0-5])$'
     r'|^PA_CL_VPORT_[XYZ](SCALE|OFFSET)_([1-9]|1[0-5])$', 'viewports 1..15',
     'Viewport index is always 0: PA_CL_VS_OUT_CNTL = 0 (no USE_VTX_VIEWPORT_INDX), PA_STEREO_CNTL = 0 '
     '(no VP_ID_MODE). Viewport 0 is written in full.'),
    (r'^PA_SC_RASTER_CONFIG_1$', 'multi-SE raster config',
     'Shader-engine pairing for multi-SE parts; this GPU has 1 SE and Mesa writes it only for GFX8 and older.'),
    (r'^CP_(PERFMON_CNTX_CNTL|PIPEID|VMID)$', 'CP bookkeeping',
     'Per-context CP perfmon / pipe / VMID bookkeeping maintained by the CP and kernel, not draw state.'),
    (r'^PA_SC_(RIGHT_VERT|LEFT_VERT|HORIZ)_GRID$', 'MSAA sample grid',
     'MSAA grid; PA_SC_AA_CONFIG = 0 (1x). Round 1 HW: moving 1x sample locations changed no pixel.'),
    (r'^VGT_MULTI_PRIM_IB_RESET_INDX$', 'primitive restart index',
     'VGT_MULTI_PRIM_IB_RESET_EN written 0.'),
    (r'^CB_BLEND_(RED|GREEN|BLUE|ALPHA)$', 'blend constant', 'Blending off: CB_BLEND0_CONTROL written 0.'),
    (r'^CB_DCC_CONTROL$', 'DCC / overwrite combiner',
     'Compressed-surface (DCC) encode controls; CB_COLOR0_INFO DCC_ENABLE = 0, no DCC surface bound.'),
    (r'^SPI_PS_INPUT_CNTL_\d+$|^SPI_INTERP_CONTROL_0$', 'PS varying inputs',
     'No interpolated varyings: SPI_VS_OUT_CONFIG VS_EXPORT_COUNT = 0 and SPI_PS_IN_CONTROL NUM_INTERP = 0 '
     '(written); screen position arrives via SPI_PS_INPUT_ENA POS_*_FLOAT (written).'),
    (r'^CB_BLEND[1-7]_CONTROL$|^CB_MRT[1-7]_EPITCH$|^CB_COLOR[1-7]_', 'MRT1..7',
     'Unbound: CB_COLOR1..7_INFO FORMAT = COLOR_INVALID (owned) and CB_TARGET_MASK / CB_SHADER_MASK '
     'enable MRT0 only (written).'),
    (r'^CB_COLOR0_CLEAR_WORD[01]$', 'fast-clear colour', 'CB_COLOR0_INFO FAST_CLEAR = 0 (written).'),
    (r'^(CS|GFX)_COPY_STATE$', 'PM4 COPY_STATE ids', 'Source ids for the COPY_STATE packet, not rasterizer state.'),
    (r'^PA_CL_POINT_|^PA_SU_POINT_', 'points', 'Point primitives only; VGT_PRIMITIVE_TYPE = TRIANGLELIST (owned).'),
    (r'^PA_SU_LINE_|^PA_SC_LINE_', 'lines / stipple', 'Line primitives only; VGT_PRIMITIVE_TYPE = TRIANGLELIST.'),
    (r'^PA_CL_UCP_|^PA_CL_PROG_NEAR_CLIP_Z$', 'user clip planes',
     'PA_CL_CLIP_CNTL written with UCP_ENA_0..5 = 0, ZCLIP_PROG_NEAR_ENA = 0 and CLIP_DISABLE = 1.'),
    (r'^VGT_DMA_|^VGT_DRAW_INITIATOR$|^VGT_IMMED_DATA$|^VGT_EVENT_(ADDRESS_REG|INITIATOR)$'
     r'|^VGT_DISPATCH_DRAW_INDEX$', 'per-draw CP-loaded',
     'Loaded by the CP from each draw / event packet (DRAW_INDEX_AUTO, EVENT_WRITE), not persistent state.'),
    (r'^PA_CL_OBJPRIM_ID_CNTL$', 'object / primitive id',
     'Primitive-id generation for PS inputs; no FS reads it and VGT_PRIMITIVEID_EN = 0 (written).'),
    (r'^PA_CL_NGG_CNTL$|^PA_SC_NGG_MODE_CNTL$', 'NGG primitive shaders',
     'NGG path off: VGT_SHADER_STAGES_EN PRIMGEN_EN = 0 (written).'),
    (r'^VGT_HOS_(MAX|MIN)_TESS_LEVEL$|^VGT_HOS_REUSE_DEPTH$|^VGT_TESS_DISTRIBUTION$|^VGT_LS_HS_CONFIG$'
     r'|^VGT_TF_PARAM$', 'tessellation',
     'LS/HS stages off (VGT_SHADER_STAGES_EN) and legacy tessellation off (VGT_HOS_CNTL TESS_MODE owned 0).'),
    (r'^VGT_GROUP_', 'vertex grouping path', 'VGT_OUTPUT_PATH_CNTL PATH_SELECT owned 0 (vertex-reuse path).'),
    (r'^VGT_GS_|^VGT_ES_PER_GS$|^VGT_GSVS_RING_|^VGT_ESGS_RING_ITEMSIZE$', 'geometry shader',
     'GS / ES stages off: VGT_GS_MODE = 0 and VGT_SHADER_STAGES_EN (written).'),
    (r'^VGT_PRIMITIVEID_RESET$', 'primitive id reset', 'VGT_PRIMITIVEID_EN written 0.'),
    (r'^VGT_INSTANCE_STEP_RATE_[01]$', 'instanced attribute step',
     'Per-instance vertex-fetch step rates; mabda has no vertex attributes (auto-index VS) and 1 instance.'),
    (r'^VGT_STRMOUT_(BUFFER_SIZE|VTX_STRIDE|BUFFER_OFFSET|DRAW_OPAQUE)', 'stream-out buffers',
     'VGT_STRMOUT_CONFIG / _BUFFER_CONFIG owned 0.'),
    (r'^PA_SU_POLY_OFFSET_', 'polygon offset',
     'PA_SU_SC_MODE_CNTL POLY_OFFSET_*_ENABLE = 0 (written) and no depth buffer.'),
    (r'^PA_SC_CENTROID_PRIORITY_', 'centroid order', 'MSAA centroid only; 1x and no centroid inputs.'),
    (r'^PA_SC_AA_SAMPLE_LOCS_', '1x sample locations',
     'PA_SC_AA_CONFIG = 0 (1x). Round 1 HW negative control N5: S0 at (-8,-8)/16 changed no gate pixel.'),
]
SH_FAMILIES = [
    (r'^SPI_SHADER_USER_DATA_(PS|VS)_\d+$', 'PS / VS user data',
     'Preloaded into SGPRs only up to RSRC2 USER_SGPR (PS 2, VS 3); the fullscreen VS and solid-red FS read '
     'no SGPR, and the textured / array FS read s0/s1 = USER_DATA_PS_0/1, which the textured override writes.'),
    (r'^SPI_SHADER_(PGM|USER)_[A-Z0-9_]*(ES|GS|HS|LS)(_\d+)?$|^SPI_SHADER_PGM_RSRC2_GS_VS$'
     r'|^SPI_SHADER_USER_DATA_(ADDR_(LO|HI)_(GS|HS)|(ES|LS)_\d+)$', 'ES / GS / HS / LS stages',
     'Stages disabled by VGT_SHADER_STAGES_EN (written).'),
    (r'^SPI_SHADER_USER_DATA_COMMON_\d+$', 'common user data',
     'Broadcast user-data slots; not loaded for the VS / PS RSRC2 USER_SGPR declared by mabda.'),
    (r'^COMPUTE_', 'compute shader', 'Compute dispatch state; mabda\'s compute composers write their own.'),
]
UCFG_FAMILIES = [
    (r'^VGT_(NUM_INDICES|NUM_INSTANCES|INDEX_TYPE)$', 'per-draw CP-loaded',
     'Loaded from each DRAW_INDEX_AUTO / NUM_INSTANCES packet (NUM_INSTANCES packet written per draw).'),
    (r'^CP_COHER_START_DELAY$|^TA_CS_BC_BASE_ADDR', 'compute preamble',
     'Mesa writes these in ac_init_compute_preamble_state; mabda\'s compute composers write them. Not '
     'consulted by the rasterizer.'),
    (r'^CP_|^SCRATCH_|^RLC_|^GRBM_GFX_INDEX$|^SQ_THREAD_TRACE|^SQC_', 'CP / RLC / debug',
     'CP ring, IB, fence, statistics and debug-trace state maintained by the CP and kernel (GRBM_GFX_INDEX: '
     'MMIO instance select, broadcast by the kernel; Mesa writes it only for GFX8 harvested raster configs).'),
    (r'^DB_(OCCLUSION|ZPASS)_COUNT', 'occlusion counters', 'Results, written by the GPU.'),
    (r'^PA_SC_(H?P3D_)?TRAP_SCREEN_|^PA_SC_TRAP_SCREEN_', 'screen trap debug',
     'Debug trap on a screen coordinate; no Mesa driver writes it.'),
    (r'^PA_SC_SCREEN_EXTENT_(MIN|MAX)_[01]$', 'screen extent',
     'Only with PA_SC_SCREEN_EXTENT_CONTROL slice enables (owned 0).'),
    (r'^PA_STATE_STEREO_X$', 'stereo offset', 'Only with PA_STEREO_CNTL EN_STEREO (owned 0).'),
    (r'^PA_SU_LINE_STIPPLE_VALUE$|^PA_SC_LINE_STIPPLE_STATE$', 'line stipple state',
     'Lines only; radv writes 0 every preamble.'),
    (r'^VGT_(GSVS_RING_SIZE|HS_OFFCHIP_PARAM|TF_RING_SIZE|TF_MEMORY_BASE)', 'GS / tessellation buffers',
     'GS / HS / LS stages off.'),
    (r'^VGT_STRMOUT_BUFFER_FILLED_SIZE_\d$', 'stream-out results', 'Stream-out off (owned).'),
    (r'^WD_(CNTL_SB|INDEX|POS)_BUF_BASE', 'work-distributor NGG buffers', 'NGG off (PRIMGEN_EN = 0).'),
]
RANGE_NOTE = {
    'cfg': 'CONFIG space 0x008000-0x00AFFF (GRBM / CP status, SQ resource words, GB_TILE_MODE / '
           'GB_ADDR_CONFIG golden values). Programmed by the kernel or read-only status; Mesa 26.2.2 '
           'writes CONFIG registers only for GFX6 (PA_CL_ENHANCE and friends).',
    'other': 'Outside the SET_CONTEXT_REG / SET_SH_REG / SET_UCONFIG_REG / SET_CONFIG_REG packet spaces '
             '(GRBM / SRBM / RLC / BIF / interrupt / device registers the kernel programs). Mesa 26.2.2 '
             'emits no GFX9 draw state there.',
}


def load_clearstate(path):
    txt = open(path).read()
    arrays = {}
    for m in re.finditer(r'gfx9_SECT_CONTEXT_def_(\d+)\[\] = \{(.*?)\};', txt, re.S):
        vals = []
        for line in m.group(2).strip().splitlines():
            lm = re.match(r'\s*(0x[0-9a-fA-F]+|0),\s*//\s*(\w+)', line)
            if lm:
                vals.append((int(lm.group(1), 16), lm.group(2)))
        arrays[int(m.group(1))] = vals
    out = {}
    for n, start, cnt in re.findall(r'\{gfx9_SECT_CONTEXT_def_(\d+), 0x([0-9a-f]+), (\d+) \}', txt):
        vals = arrays[int(n)]
        if len(vals) != int(cnt):
            sys.exit('clearstate extent %s: %d values, header says %s' % (n, len(vals), cnt))
        for k, (v, name) in enumerate(vals):
            if name != 'HOLE':
                out[0x28000 + (int(start, 16) - 0xA000 + k) * 4] = (name, v)
    return out


def stream_state(dec, path):
    pkts = dec.decode(open(path, 'rb').read())
    draws = [n for n, p in enumerate(pkts) if p['op'] == 0x2D]
    last = max(draws) if draws else len(pkts)
    st = {}
    for mmio, v, n in dec.writes(pkts):
        if n < last:
            st[mmio] = v
    return st


def h(v):
    return '—' if v is None else '%08x' % v


def classify(name, families):
    for rx, fam, verdict in families:
        if re.search(rx, name):
            return fam, verdict
    return None, None


def generate(a):
    os.environ['GFX9_JSON'] = a.gfx9
    sys.dont_write_bytecode = True   # never leave __pycache__ in the source tree
    sys.path.insert(0, DECODER_DIR)
    import decode_ib as dec
    j = json.load(open(a.gfx9))
    cs = load_clearstate(a.clearstate)
    radv = {}
    for mmio, v, n in dec.writes(dec.decode(open(a.radv_init, 'rb').read())):
        radv[mmio] = v
    radv.update(stream_state(dec, a.radv_main))
    names = ('clear_triangle', 'pass_untextured', 'pass_textured')
    cur = {n: stream_state(dec, os.path.join(a.mabda, n + '.bin')) for n in names}
    before = None
    if a.mabda_before:
        before = {n: stream_state(dec, os.path.join(a.mabda_before, n + '.bin')) for n in names}
    written = set().union(*[set(s) for s in cur.values()])
    regs = {}
    for r in j['register_mappings']:
        if r['map'].get('to') == 'mm':
            regs.setdefault(r['map']['at'], r['name'])
    out = []
    errors = []

    # ---- owned table ----
    out.append('### A. Registers the render streams write (%d)\n' % len(written))
    out.append('`clear` = kernel clear state (— = not in it); `radv` = radv\'s value on this GPU (explicit '
               'write in its gfx_init or main IB, else the clear state it relies on); `mabda` = the '
               'untextured pass_draw stream (textured-only writes marked *); `before` = the earlier '
               'library. Fields decode `mabda` with gfx9.json.\n')
    hdr = '| MMIO | register | clear | radv | before | mabda | fields | verdict |'
    if not before:
        hdr = hdr.replace(' before |', '')
    out.append(hdr)
    out.append('|' + '---|' * (hdr.count('|') - 1))
    for mmio in sorted(written):
        name = regs.get(mmio, 'UNMAPPED_%05X' % mmio)
        if name.startswith('UNMAPPED'):
            errors.append('owned register %05X is not in gfx9.json' % mmio)
        tex_only = mmio not in cur['pass_untextured']
        mv = cur['pass_textured'].get(mmio) if tex_only else cur['pass_untextured'].get(mmio)
        if mv is None:
            mv = cur['clear_triangle'].get(mmio)
        cv = cs.get(mmio, (None, None))[1]
        rv = radv.get(mmio, cv)
        bv = None
        if before:
            bv = before['pass_textured'].get(mmio) if tex_only else before['pass_untextured'].get(mmio)
            if bv is None:
                bv = before['clear_triangle'].get(mmio)
        if name in OWNED_NOTES:
            verdict = OWNED_NOTES[name]
        elif name in DYNAMIC:
            verdict = 'RT extent / shader or descriptor VA dependent.'
        elif mv == rv:
            verdict = '= radv' + (' = clear state' if mv == cv else '')
        elif mv == cv:
            verdict = '= clear state (radv %s)' % h(rv)
        else:
            verdict = None
            errors.append('owned %s (%05X): mabda %s differs from radv %s and clear %s without a note'
                          % (name, mmio, h(mv), h(rv), h(cv)))
        if before and bv != mv:
            verdict = ('**changed** — ' if bv is not None else '**added** — ') + (verdict or '')
        row = [('%05X' % mmio), name + (' *' if tex_only else ''), h(cv), h(rv)]
        if before:
            row.append(h(bv) if bv is not None else 'not written')
        row += [h(mv), dec.fields(mmio, mv) or '', verdict or 'MISSING']
        out.append('| ' + ' | '.join(row) + ' |')

    # ---- unwritten, by space ----
    spaces = [
        ('B. Context registers not written', lambda m: 0x28000 <= m < 0x29000, CTX_FAMILIES),
        ('C. SH registers not written', lambda m: 0xB000 <= m < 0xC000, SH_FAMILIES),
        ('D. UConfig registers not written', lambda m: 0x30000 <= m < 0x31000, UCFG_FAMILIES),
    ]
    for title, inrange, fams in spaces:
        members = [(m, regs[m]) for m in sorted(regs) if inrange(m) and m not in written]
        groups = {}
        order = []
        for m, n in members:
            fam, verdict = classify(n, fams)
            if fam is None:
                errors.append('%s: %s (%05X) has no family' % (title, n, m))
                continue
            if fam not in groups:
                groups[fam] = (verdict, [])
                order.append(fam)
            groups[fam][1].append((m, n))
        out.append('\n### %s (%d)\n' % (title, len(members)))
        out.append('| family | registers | clear-state values | radv writes | verdict |')
        out.append('|---|---|---|---|---|')
        for fam in order:
            verdict, ms = groups[fam]
            vals = sorted(set(h(cs[m][1]) for m, _ in ms if m in cs))
            incs = sum(1 for m, _ in ms if m in cs)
            csv = ('all %s' % vals[0] if len(vals) == 1 else ', '.join(vals)) if incs else 'not in clear state'
            if incs and incs != len(ms):
                csv += ' (%d of %d in it)' % (incs, len(ms))
            rw = ['%s=%s' % (n, h(radv[m])) for m, n in ms if m in radv]
            out.append('| %s | %s | %s | %s | %s |' % (
                fam, ' '.join('%s (%05X)' % (n, m) for m, n in ms), csv, ' '.join(rw) or '—', verdict))

    cfg = [n for m, n in sorted(regs.items()) if 0x8000 <= m < 0xB000]
    other = [n for m, n in sorted(regs.items())
             if not (0x8000 <= m < 0xC000 or 0x28000 <= m < 0x29000 or 0x30000 <= m < 0x31000)]
    out.append('\n### E. Other register spaces\n')
    out.append('| space | count | registers | verdict |')
    out.append('|---|---|---|---|')
    out.append('| CONFIG 0x8000-0xAFFF | %d | %s | %s |' % (len(cfg), ' '.join(cfg), RANGE_NOTE['cfg']))
    out.append('| everything else | %d | (not listed) | %s |' % (len(other), RANGE_NOTE['other']))
    total = len(regs)
    out.append('\nTotal: %d gfx9.json `mm` registers — %d written, %d classified by family, %d CONFIG, '
               '%d in other register spaces.' % (
                   total, len(written),
                   sum(1 for m in regs if m not in written and (0x28000 <= m < 0x29000 or 0xB000 <= m < 0xC000
                                                                or 0x30000 <= m < 0x31000)),
                   len(cfg), len(other)))
    return '\n'.join(out) + '\n', errors


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--gfx9', required=True)
    p.add_argument('--clearstate', required=True)
    p.add_argument('--radv-init', required=True)
    p.add_argument('--radv-main', required=True)
    p.add_argument('--mabda', required=True)
    p.add_argument('--mabda-before')
    p.add_argument('--check')
    a = p.parse_args()
    text, errors = generate(a)
    for e in errors:
        print('ERROR: ' + e, file=sys.stderr)
    if errors:
        return 1
    if a.check:
        doc = open(a.check).read()
        if BEGIN not in doc or END not in doc:
            print('ERROR: %s has no generated block markers' % a.check, file=sys.stderr)
            return 1
        block = doc.split(BEGIN, 1)[1].split(END, 1)[0].strip('\n') + '\n'
        if block != text:
            print('ERROR: %s generated block is stale; regenerate it' % a.check, file=sys.stderr)
            return 1
        print('owned-state audit tables up to date')
        return 0
    sys.stdout.write(text)
    return 0


if __name__ == '__main__':
    sys.exit(main())
