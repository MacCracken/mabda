#!/usr/bin/env python3
"""decode_ib.py — decode a GFX PM4 stream (.bin, e.g. from `make snoop`)
against Mesa's src/amd/registers/gfx9.json (the authoritative GFX9 register DB).

Usage: GFX9_JSON=/path/gfx9.json decode_ib.py <stream.bin> [--state]
  default  one line per packet, each register write named + field-decoded
  --state  last value written per register (the state the stream leaves)
Fetch gfx9.json via the GitLab API (project 176):
  curl -o gfx9.json "https://gitlab.freedesktop.org/api/v4/projects/176/\
repository/files/src%2Famd%2Fregisters%2Fgfx9.json/raw?ref=main"
"""
import json, os, struct, sys

GFX9 = os.environ.get('GFX9_JSON', 'gfx9.json')
_j = json.load(open(GFX9))
BY_AT = {}
for r in _j['register_mappings']:
    if r['map'].get('to') == 'mm':
        BY_AT.setdefault(r['map']['at'], r)
TYPES = _j['register_types']

OPS = {0x10: 'NOP', 0x12: 'CLEAR_STATE', 0x28: 'CONTEXT_CONTROL', 0x2D: 'DRAW_INDEX_AUTO',
       0x2F: 'NUM_INSTANCES', 0x37: 'WRITE_DATA', 0x3C: 'WAIT_REG_MEM', 0x3F: 'INDIRECT_BUFFER',
       0x42: 'PFP_SYNC_ME', 0x46: 'EVENT_WRITE', 0x49: 'RELEASE_MEM', 0x50: 'DMA_DATA',
       0x58: 'ACQUIRE_MEM', 0x69: 'SET_CONTEXT_REG', 0x6A: 'SET_CONTEXT_REG_INDEX',
       0x76: 'SET_SH_REG', 0x79: 'SET_UCONFIG_REG', 0x7A: 'SET_UCONFIG_REG_INDEX',
       0x9B: 'SET_SH_REG_INDEX', 0x15: 'DISPATCH_DIRECT', 0x4A: 'PREAMBLE_CNTL'}
BASE = {0x69: 0x28000, 0x6A: 0x28000, 0x76: 0xB000, 0x9B: 0xB000, 0x79: 0x30000, 0x7A: 0x30000}


def regname(mmio):
    r = BY_AT.get(mmio)
    return r['name'] if r else 'UNKNOWN_%05X' % mmio


def fields(mmio, val):
    r = BY_AT.get(mmio)
    if not r:
        return ''
    t = TYPES.get(r.get('type_ref', ''))
    if not t:
        return ''
    out = []
    for f in t['fields']:
        lo, hi = f['bits']
        v = (val >> lo) & ((1 << (hi - lo + 1)) - 1)
        if v:
            out.append('%s=%#x' % (f['name'], v))
    return ' '.join(out)


def decode(data):
    w = struct.unpack('<%dI' % (len(data) // 4), data[:len(data) // 4 * 4])
    pkts = []
    i = 0
    while i < len(w):
        h = w[i]
        t = h >> 30
        if t == 2:  # type-2 filler
            i += 1
            continue
        if t != 3:
            pkts.append({'i': i, 'op': None, 'name': 'RAW', 'body': [h]})
            i += 1
            continue
        cnt = ((h >> 16) & 0x3FFF) + 1
        op = (h >> 8) & 0xFF
        body = list(w[i + 1:i + 1 + cnt])
        pkts.append({'i': i, 'op': op, 'hdr': h, 'name': OPS.get(op, 'OP_%02X' % op), 'body': body})
        i += 1 + cnt
    return pkts


def writes(pkts):
    out = []
    for n, p in enumerate(pkts):
        if p['op'] in BASE and p['body']:
            off = p['body'][0] & 0xFFFF
            for k, v in enumerate(p['body'][1:]):
                out.append((BASE[p['op']] + (off + k) * 4, v, n))
    return out


def main():
    data = open(sys.argv[1], 'rb').read()
    pkts = decode(data)
    if '--state' in sys.argv:
        st = {}
        for mmio, v, n in writes(pkts):
            st[mmio] = v
        for mmio in sorted(st):
            print('%05X %-40s %08x  %s' % (mmio, regname(mmio), st[mmio], fields(mmio, st[mmio])))
        return
    for p in pkts:
        if p['op'] == 0x10:
            print('%4d NOP x%d' % (p['i'], len(p['body'])))
            continue
        if p['op'] in BASE and p['body']:
            off = p['body'][0] & 0xFFFF
            print('%4d %s idx=%#x' % (p['i'], p['name'], p['body'][0] >> 16 if p['op'] in (0x6A, 0x7A, 0x9B) else 0))
            for k, v in enumerate(p['body'][1:]):
                mm = BASE[p['op']] + (off + k) * 4
                print('       %05X %-38s <- %08x  %s' % (mm, regname(mm), v, fields(mm, v)))
            continue
        print('%4d %s : %s' % (p['i'], p['name'], ' '.join('%08x' % v for v in p['body'])))


if __name__ == '__main__':
    main()
