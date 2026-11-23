# SOS: Educational Rust Micro-OS Architecture & Design Document

## 1. System Control & Data-Flow Architecture Diagram

    +-------------------------------------------------------------------------+
    |                    PYTHON TRACE & PERFORMANCE LAYER                     |
    |  traces/sos_clean_trace.txt --> scripts/analyze_traces.py --> CSV/Plots |
    +-------------------------------------------------------------------------+
                                         ^
                                         | (COM1 0x3F8 Serial Stream)
    +-------------------------------------------------------------------------+
    |                     INTEGRATED VALIDATION SUITE                         |
    |     src/tests.rs (TC1 Min, TC2 Multi, TC3 Boundary, TC4 Stress,         |
    |                   TC5 Failure Handling, TC6 Repeatability Hash)         |
    +-------------------------------------------------------------------------+
             |                           |                           |
             v                           v                           v
    +-------------------+   +-------------------------+   +-------------------+
    |  SCHEDULER TRACK  |   |   SYNCHRONIZATION       |   | MEMORY SUBSYSTEM  |
    |  - Round Robin    |   |   - IrqSave (RFLAGS/CLI)|   | - BumpAllocator   |
    |    (src/scheduler)|   |   - Spinlock<T>         |   |   (64 KB aligned) |
    |  - Track A: MLFQ  |   |     (AtomicBool + stats)|   | - FreeListPool    |
    |    3-Level + Aging|   |     (src/sync.rs)       |   |   (64 x 256B)     |
    |    (src/mlfq.rs)  |   +-------------------------+   |   (src/memory.rs) |
    +-------------------+                |                +-------------------+
             |                           |                           |
             +---------------------------+---------------------------+
                                         |
                                         v
    +-------------------------------------------------------------------------+
    |                 TASK & HARDWARE ABSTRACTION LAYER                       |
    |  - TaskControlBlock & CpuContext Register Switch (src/task.rs)          |
    |  - Ring-Buffer FIFO ReadyQueue (src/queue.rs)                           |
    |  - x86_64 8254 PIT Timer (0x40/0x43) & RDTSC Cycle Counter (src/timer)  |
    |  - 16550 UART Serial Driver (0x3F8) & ktrace! Macros (src/console.rs)   |
    +-------------------------------------------------------------------------+
                                         ^
                                         |
    +-------------------------------------------------------------------------+
    |              UEFI RING-0 KERNEL ENTRY (efi_main in src/main.rs)         |
    |         Target: x86_64-unknown-uefi | QEMU + OVMF (edk2-x86_64)         |
    +-------------------------------------------------------------------------+

## 2. Linux vs. SOS Micro-OS Comparison

| OS Mechanism | Linux Production Kernel | SOS Educational Micro-OS |
|---|---|---|
| Boot & Entry | UEFI Stub -> Decompress vmlinuz -> start_kernel() | Direct UEFI PE32+ efi_main() in Ring 0 (#![no_std]) |
| Task Abstraction | struct task_struct (~4 KB+, namespaces, signals, credentials) | TaskControlBlock (ID, TaskState, CpuContext, priority, timing stats) |
| CPU Scheduling | EEVDF / CFS (vruntime red-black tree) + RT classes | Core Round Robin (ReadyQueue FIFO) + Track A 3-Level MLFQ with Aging |
| Timer / Preemption | Local APIC timer + high-resolution timers (hrtimer) | x86_64 8254 PIT (0x40/0x43 at 1000 Hz) + RDTSC cycle counter |
| Synchronization | raw_spinlock_t, MCS queued spinlocks, futex, RCU | IrqSave (pushfq/cli/sti) + Spinlock<T> with contention counters |
| Memory Allocation | Buddy page-frame allocator + SLUB cache allocator | 64 KB aligned BumpAllocator + 64-block FreeListAllocator (256B blocks) |

## 3. Unsafe Rust Policy Audit

Every unsafe block in SOS follows the mandatory 3-part justification invariant:
1. src/console.rs: Port I/O instructions (out dx, al and in al, dx) on COM1 (0x3F8). Isolated inside safe SerialPort::write_byte() and kprintln! macros.
2. src/sync.rs: pushfq; pop; cli and sti inside IrqSave, plus UnsafeCell dereferencing inside SpinlockGuard. Soundness is guaranteed by AtomicBool::compare_exchange with Acquire/Release memory ordering.
3. src/memory.rs: Pointer arithmetic self.heap.as_mut_ptr().add(aligned_offset) after strict checked_add and <= HEAP_SIZE bounds verification.
4. src/scheduler.rs: Callee-saved x86_64 register context switch (rsp, rbp, rbx, r12-r15) between valid CpuContext structs while holding the scheduler spinlock.
5. src/timer.rs: Read-only rdtsc instruction reading edx:eax.
