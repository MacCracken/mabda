#!/usr/bin/env bash
# extract_dispatch.sh — normalize a PM4 dword stream for byte-diff.
#
# Reads a PM4 dump on stdin (mabda native_pm4_dump format OR RADV
# `--dump=ibs` format — both surface as text with `0xXXXXXXXX` hex
# dwords on each line) and emits a one-packet-per-line decoded
# stream that highlights what each packet does. The output is
# stable across the two input formats, so a `diff` between two
# extractor outputs surfaces register-value drift directly.
#
# Format produced (one line per packet):
#   ACQUIRE_MEM
#   SET_SH_REG  reg=0xB834  COMPUTE_PGM_HI    val=0x00000080
#   SET_SH_REG  reg=0xB838  COMPUTE_STATIC_THREAD_MGMT_SE0  vals=0xFFFFFFFF,0x00000000
#   ...
#   DISPATCH_DIRECT  dim=(1,1,1)  initiator=0x00000045
#
# Filter: by default emits only compute-relevant packets — SH_REG
# writes to the COMPUTE_* range (0xB800-0xB900), DISPATCH_DIRECT,
# and ACQUIRE_MEM. Pass --all to emit every packet (useful for
# inspecting RADV's full preamble; noisy for diff).
#
# Usage:
#   ./extract_dispatch.sh < radv.ib.txt > radv.tail.txt
#   ./extract_dispatch.sh < mabda.dump.txt > mabda.tail.txt
#   diff -u radv.tail.txt mabda.tail.txt
#
# Pass criteria (rc.2 punchlist): byte-clean diff, OR each diff
# line annotated with a "RADV emits X here, mabda emits Y, same-
# class effect" comment in the README's known-equivalents table.

set -euo pipefail

mode="compute"
if [ $# -gt 0 ] && [ "$1" = "--all" ]; then
    mode="all"
fi

# awk script: tokenize hex dwords, walk PM4 stream, decode packets.
awk -v mode="$mode" '
BEGIN {
    # Build the SH register name table — only the COMPUTE_* regs
    # we care about for byte-diff. Everything else is unnamed.
    sh_name[0xB800] = "COMPUTE_DISPATCH_INITIATOR"
    sh_name[0xB810] = "COMPUTE_START_X"
    sh_name[0xB814] = "COMPUTE_START_Y"
    sh_name[0xB818] = "COMPUTE_START_Z"
    sh_name[0xB81C] = "COMPUTE_NUM_THREAD_X"
    sh_name[0xB820] = "COMPUTE_NUM_THREAD_Y"
    sh_name[0xB824] = "COMPUTE_NUM_THREAD_Z"
    sh_name[0xB830] = "COMPUTE_PGM_LO"
    sh_name[0xB834] = "COMPUTE_PGM_HI"
    sh_name[0xB848] = "COMPUTE_PGM_RSRC1"
    sh_name[0xB84C] = "COMPUTE_PGM_RSRC2"
    sh_name[0xB854] = "COMPUTE_RESOURCE_LIMITS"
    sh_name[0xB858] = "COMPUTE_STATIC_THREAD_MGMT_SE0"
    sh_name[0xB85C] = "COMPUTE_STATIC_THREAD_MGMT_SE1"
    sh_name[0xB860] = "COMPUTE_TMPRING_SIZE"
    sh_name[0xB864] = "COMPUTE_STATIC_THREAD_MGMT_SE2"
    sh_name[0xB868] = "COMPUTE_STATIC_THREAD_MGMT_SE3"
    sh_name[0xB900] = "COMPUTE_USER_DATA_0"
    sh_name[0xB904] = "COMPUTE_USER_DATA_1"
    sh_name[0xB908] = "COMPUTE_USER_DATA_2"
    sh_name[0xB90C] = "COMPUTE_USER_DATA_3"

    # Opcode names (PM4 IT_* values).
    op_name[0x10] = "NOP"
    op_name[0x15] = "DISPATCH_DIRECT"
    op_name[0x2D] = "DRAW_INDEX_AUTO"
    op_name[0x37] = "WRITE_DATA"
    op_name[0x46] = "EVENT_WRITE"
    op_name[0x58] = "ACQUIRE_MEM"
    op_name[0x69] = "SET_CONTEXT_REG"
    op_name[0x76] = "SET_SH_REG"
    op_name[0x79] = "SET_UCONFIG_REG"

    n = 0
}

# Match every hex dword on every line. RADV format embeds them in
# a `0xOFFS  0xDWORD  MNEMONIC` shape; mabda matches the first two
# fields. Pull every 0x[8 hex] token; the first is offset, the
# second is data — but only one of them is the data dword we want.
# Distinguish: RADV offsets are sequential `0x00000000`,
# `0x00000004`, etc. Mabda offsets are `0x0000`, `0x0004`, etc. (4
# hex). The data dword is always the SECOND `0x...` token on a line.
{
    line_dwords = 0
    found_offset = 0
    for (i = 1; i <= NF; i++) {
        tok = $i
        if (tok ~ /^0x[0-9a-fA-F]+$/) {
            line_dwords++
            if (line_dwords == 1) {
                # Offset (skip — we recompute by position in the
                # stream so we are agnostic to format).
                found_offset = 1
                continue
            }
            if (line_dwords == 2 && found_offset) {
                # Data dword — strip "0x" and uppercase.
                v = toupper(substr(tok, 3))
                # Pad to 8 hex digits.
                while (length(v) < 8) v = "0" v
                stream[n++] = v
                next
            }
        }
    }
}

END {
    # Walk the stream, decoding type-3 packet headers.
    i = 0
    while (i < n) {
        hdw = stream[i]
        # Convert to integer for bit ops.
        # awk hex parsing: use strtonum() (gawk) or sprintf trick.
        # We use printf "%d" / sprintf with the awk implicit
        # convert-from-hex via "0x"-prefixed string.
        h = strtonum("0x" hdw)
        type = and(rshift(h, 30), 0x3)
        if (type != 3) {
            # Not a type-3 PM4 header. Skip 1 dword and continue.
            # (Could be a type-2 NOP or padding.)
            i++
            continue
        }
        cnt_minus_1 = and(rshift(h, 16), 0x3FFF)
        op = and(rshift(h, 8), 0xFF)
        n_data = cnt_minus_1 + 1
        if (i + n_data >= n) break    # truncated stream

        op_label = op_name[op]
        if (op_label == "") op_label = sprintf("UNKNOWN_OP_%02X", op)

        emit = 0
        line = ""

        if (op == 0x76 || op == 0x69 || op == 0x79) {
            # SET_SH_REG / SET_CONTEXT_REG / SET_UCONFIG_REG: data[0] is
            # the wire-encoded reg offset; subsequent dwords are values.
            wire = strtonum("0x" stream[i+1])
            if (op == 0x76) {
                base = 0xB000
            } else if (op == 0x69) {
                base = 0xA000
            } else {
                base = 0x30000
            }
            reg = base + wire * 4
            name = ""
            if (op == 0x76 && (reg in sh_name)) name = sh_name[reg]

            # Compute SH range: 0xB800-0xB8FF for COMPUTE_* state regs,
            # 0xB900-0xB93F for COMPUTE_USER_DATA_0..15.
            in_compute = (op == 0x76 && reg >= 0xB800 && reg < 0xB940)
            if (mode == "all" || in_compute) emit = 1

            if (emit) {
                vals = ""
                for (j = 2; j <= n_data; j++) {
                    if (vals != "") vals = vals ","
                    vals = vals "0x" stream[i+j]
                }
                if (name == "") name = "?"
                line = sprintf("%-15s reg=0x%04X  %-32s vals=%s", op_label, reg, name, vals)
            }
        } else if (op == 0x15) {
            # DISPATCH_DIRECT: 4 data dwords (dim_x, dim_y, dim_z, initiator).
            x = strtonum("0x" stream[i+1])
            y = strtonum("0x" stream[i+2])
            z = strtonum("0x" stream[i+3])
            init = strtonum("0x" stream[i+4])
            line = sprintf("%-15s dim=(%d,%d,%d)  initiator=0x%08X",
                           op_label, x, y, z, init)
            emit = 1
        } else if (op == 0x58) {
            # ACQUIRE_MEM — emit just the mnemonic and dword count.
            if (mode == "all") emit = 1
            else emit = 1   # ACQUIRE_MEM is compute-relevant context
            line = sprintf("%-15s (%d data dwords)", op_label, n_data)
        } else if (op == 0x46) {
            # EVENT_WRITE — 1 data dword carrying event_index in high bits.
            ev = strtonum("0x" stream[i+1])
            evtype = and(ev, 0x3F)
            evidx = and(rshift(ev, 8), 0xF)
            line = sprintf("%-15s event_type=0x%02X event_index=0x%X",
                           op_label, evtype, evidx)
            if (mode == "all") emit = 1
        } else if (op == 0x37) {
            # WRITE_DATA — used by mabda for the post-dispatch CP marker.
            if (mode == "all") emit = 1
            line = sprintf("%-15s (CP marker write)", op_label)
        } else if (op == 0x10) {
            # NOP padding — only emit in --all mode (otherwise noisy).
            if (mode == "all") {
                line = sprintf("%-15s (%d data dwords)", op_label, n_data)
                emit = 1
            }
        } else {
            if (mode == "all") {
                line = sprintf("%-15s (op=0x%02X, %d data dwords)",
                               op_label, op, n_data)
                emit = 1
            }
        }

        if (emit) print line
        i = i + 1 + n_data
    }
}
'
