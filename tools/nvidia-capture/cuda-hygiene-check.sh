#!/usr/bin/env bash
# cuda-hygiene-check.sh — ADR-007 CUDA build-hygiene checks.
# Build-time only; needs the CUDA toolkit but NO GPU/driver (ptxas/cuobjdump
# are offline). Run after `sudo pacman -S cuda`.
#
#   1. libdevice non-contamination: compile the trivial store-0xDEADBEEF
#      kernel for sm_75, dump its SASS, and prove no libdevice/Attachment-A
#      device code or extra functions got linked in.
#   2. EULA: locate the shipped EULA.txt and print the load-bearing sections
#      (§1.2 item 5 open-source, §1.2 item 8 anti-RE-for-porting, §1.3 ownership).
set -uo pipefail

HERE="$(cd "$(dirname "$0")" && pwd)"
CU="$HERE/store_deadbeef.cu"
CUBIN="$HERE/store_deadbeef.cubin"

# Locate the CUDA toolkit (Arch installs to /opt/cuda).
CUDA="${CUDA_HOME:-}"
if [ -z "$CUDA" ]; then
  if command -v nvcc >/dev/null 2>&1; then CUDA="$(dirname "$(dirname "$(command -v nvcc)")")"
  elif [ -d /opt/cuda ]; then CUDA=/opt/cuda
  elif [ -d /usr/local/cuda ]; then CUDA=/usr/local/cuda
  fi
fi
NVCC="$CUDA/bin/nvcc"; CUOBJDUMP="$CUDA/bin/cuobjdump"; NVDISASM="$CUDA/bin/nvdisasm"
export PATH="$CUDA/bin:$PATH"

echo "=== CUDA toolkit ==="
if [ ! -x "$NVCC" ]; then echo "FAIL: nvcc not found (install: sudo pacman -S cuda)"; exit 2; fi
"$NVCC" --version | tail -3
echo

echo "=== [1] compile store_deadbeef.cu -> sm_75 cubin (offline, no GPU) ==="
"$NVCC" -arch=sm_75 -cubin "$CU" -o "$CUBIN" || { echo "FAIL: nvcc compile"; exit 3; }
echo "wrote $CUBIN ($(stat -c%s "$CUBIN") bytes)"
echo

echo "=== [1a] SASS dump (expect a handful of insns: MOV imm / STG / EXIT) ==="
"$CUOBJDUMP" --dump-sass "$CUBIN"
echo

echo "=== [1b] functions in cubin (expect exactly 1: store_deadbeef) ==="
nfuncs=$("$CUOBJDUMP" --dump-elf "$CUBIN" 2>/dev/null | grep -cE '^\s*\.text\.|STT_FUNC|\.nv\.info\.' || true)
"$CUOBJDUMP" --dump-elf "$CUBIN" 2>/dev/null | grep -iE 'symbol|\.text\.|function' | head -40
echo

echo "=== [1c] libdevice / Attachment-A contamination scan ==="
# A clean store kernel links NO libdevice. Flag any __nv_* (libdevice mangled),
# external device-code references, or unexpected extra .text sections.
contam=$("$CUOBJDUMP" --dump-sass --dump-elf "$CUBIN" 2>/dev/null \
         | grep -iE '__nv_|libdevice|__internal_|nv\.constant.*lib' || true)
if [ -n "$contam" ]; then
  echo "REVIEW: possible libdevice symbols present:"; echo "$contam"
else
  echo "CLEAN: no libdevice/__nv_/Attachment-A device-code symbols found."
fi
echo

echo "=== [1d] launch-ABI metadata for the N4.2 QMD (regcount / shared mem / params) ==="
"$CUOBJDUMP" --dump-resource-usage "$CUBIN" 2>/dev/null || \
  "$CUOBJDUMP" --dump-elf "$CUBIN" 2>/dev/null | grep -iE 'REG|SHARED|PARAM|CONSTANT|\.nv\.info' | head
echo

echo "=== [2] shipped EULA.txt — locate + print load-bearing sections ==="
EULA="$(find "$CUDA" -maxdepth 3 -iname 'EULA*' 2>/dev/null | head -1)"
if [ -z "$EULA" ]; then
  echo "REVIEW: no EULA.txt under $CUDA (check $CUDA/share/doc or the install click-through)."
else
  echo "EULA: $EULA"
  echo "--- §1.2 (limitations: items 5 open-source + 8 anti-RE-for-porting) ---"
  grep -niE '1\.2|reverse engineer|open source|open-source|non-NVIDIA|port' "$EULA" | head -25
  echo "--- §1.3 (ownership of your applications / compiled output) ---"
  grep -niE '1\.3|ownership|your application|all rights' "$EULA" | head -15
  echo "(full text: less '$EULA')"
fi
echo
echo "=== hygiene check complete — review [1c] CLEAN + [2] sections, then archive"
echo "    store_deadbeef.cu + the SASS dump in-tree under GPL-3.0 at N5.1 ==="
