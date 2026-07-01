// store_deadbeef.cu — minimal SM75 kernel for the ADR-007 CUDA hygiene
// check (libdevice non-contamination) and the eventual N5.1 SASS capture.
//
// Deliberately uses NO math builtins / no __device__ intrinsics, so ptxas
// links ZERO libdevice (Attachment-A) device code — the emitted SASS is
// purely this kernel's logic (MOV immediate -> STG -> EXIT), which is the
// whole point of the contamination check.
//
// extern "C" keeps the symbol name unmangled so cuobjdump output is legible.
extern "C" __global__ void store_deadbeef(unsigned int *out) {
    out[0] = 0xDEADBEEFu;
}
