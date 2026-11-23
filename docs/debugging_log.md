# SOS Engineering Debugging Log (Observe -> Localize -> Hypothesize -> Fix)

| Issue ID | Subsystem | Observed Symptom | Root Cause Localization | Fix Applied & Verification |
|---|---|---|---|---|
| BUG-01 | Build / Target | Port I/O assembly syntax mismatch during initial UART bringup | Inline assembly needed explicit in("dx") port, in("al") val with options(nomem, nostack, preserves_flags) | Standardized outb/inb wrappers in src/console.rs; verified 38400 baud serial output in QEMU. |
| BUG-02 | Memory (Layer 8) | Compiler errors when attempting to use Vec and unstable pointer_is_aligned_to in #![no_std] | Bare-metal x86_64-unknown-uefi without alloc standard library cannot use Vec or nightly alignment helpers | Engineered deterministic BumpAllocator (64 KB) with manual (next + align - 1) & !(align - 1) alignment + FreeListAllocator (64x256B) in pure core protected by Spinlock. |
| BUG-03 | Trace Capture | OVMF firmware emits ANSI escape sequences and binary control bytes at start of serial log | QEMU redirects both UEFI firmware console and COM1 0x3F8 to the same serial log file | Updated scripts/analyze_traces.py to strip non-ASCII prefix bytes and filter structured [INFO]/[TRACE] lines into traces/sos_clean_trace.txt. |
