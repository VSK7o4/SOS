# CA-6 Final Technical Report & Viva Defense Guide: SOS Rust Micro-OS

## Part 1: Quantitative Experimental Results (From QEMU Execution)

### 1. Core Round Robin Scheduler (Quantum = 2 Ticks)
- Tasks Completed: 4 (NetPacketRx, DiskBlockIo, CryptoVerify, TelemetryAgg)
- Context Switches: 9
- Average Waiting Time: 8.25 ticks (integer metric: 8 ticks)
- Average Turnaround Time: 12.25 ticks (integer metric: 12 ticks)
- Per-Task Breakdown:
  - TelemetryAgg (work=2): Run = 2, Wait = 6, Turnaround = 8
  - DiskBlockIo (work=3): Run = 3, Wait = 8, Turnaround = 11
  - NetPacketRx (work=5): Run = 5, Wait = 9, Turnaround = 14
  - CryptoVerify (work=6): Run = 6, Wait = 10, Turnaround = 16

### 2. Specialization Track A: 3-Level MLFQ Scheduler with Priority Aging
- Queue Quanta: Q0 = 1 tick, Q1 = 2 ticks, Q2 = 4 ticks, Aging Boost Interval = 8 ticks
- Context Switches: 13 | Priority Demotions: 9 | Aging Boosts: 2
- Interactive I/O Tasks (UiKeyHandler, AudioMixerIo) Average Wait: 6.5 ticks (integer metric: 6 ticks)
- CPU-Bound Tasks (CpuMatrixMul, VideoEncoder) Average Wait: 10.0 ticks
- Engineering Conclusion: MLFQ reduced waiting time for short interactive tasks by 35% compared to CPU-bound tasks while preventing starvation via periodic AGING_BOOST promotions back to Q0.

### 3. Required Validation Test Cases (TC1 - TC6)
| Test Case | Configuration | Total Ticks | Context Switches | Result |
|---|---|---|---|---|
| TC1 - Minimum | 2 Tasks, Quantum = 2 | 7 | 4 | PASS |
| TC2 - Multiple | 5 Tasks, Quantum = 2 | 17 | 10 | PASS |
| TC3 - Boundary | 4 Tasks, Quantum = 1 (min slice) | 14 | 14 | PASS |
| TC4 - Stress | 12 Concurrent Tasks, Quantum = 2 | 37 | 21 | PASS |
| TC5 - Failure Case | Queue overflow (17th task), 128KB OOM, 0-byte alloc, invalid block free (999) | N/A | N/A | PASS (All Safely Rejected) |
| TC6 - Repeatability | Two identical 4-task runs compared via 64-bit FNV-1a schedule hash | 14 / 14 | 8 / 8 | PASS (0x124B30AC7BC44CE2) |

---

## Part 2: Complete Answers to All 20 Individual Viva Questions (Section 32)

1. What happens immediately after the kernel starts?
   UEFI firmware (OVMF) loads BOOTX64.EFI into memory in 64-bit Long Mode (Ring 0) and jumps to efi_main() in src/main.rs. The kernel immediately initializes the 16550 UART serial controller at I/O port 0x3F8 (38400 baud, 8N1) so all subsequent subsystems have deterministic diagnostic output.
2. How is a task represented?
   Each task is represented by a TaskControlBlock (src/task.rs) containing id, name, state (Ready, Running, Blocked, Terminated), priority, time_slice, remaining_ticks, total_ticks_run, wait_ticks, work_Target, work_done, stack_base, stack_size, and a #[repr(C)] CpuContext struct storing callee-saved x86_64 registers (rsp, rbp, rbx, r12-r15, rip).
3. How does the scheduler select the next task?
   In Round Robin (src/scheduler.rs), the scheduler dequeues the head index from the FIFO ring-buffer ReadyQueue (O(1) time). In MLFQ (src/mlfq.rs), select_next_task() scans priority queues from highest priority (Q0, quantum=1) down to lowest (Q2, quantum=4) and dequeues the first ready task in the highest non-empty level.
4. Why did you choose Round Robin?
   Round Robin guarantees starvation-free fairness and bounded response time (O(N * q)) with minimal O(1) queue overhead, making it the ideal baseline scheduler to compare against our Track A MLFQ specialization.
5. What triggers a scheduling decision?
   A scheduling decision is triggered when: (a) a running task's time-slice quantum expires (remaining_ticks == 0) on a timer tick, (b) a task completes its work and transitions to TERMINATED, or (c) a task blocks/yields on I/O.
6. What happens during a timer tick?
   The 8254 PIT (0x40/0x43) generates a periodic 1000 Hz hardware tick measured alongside the CPU RDTSC counter (src/timer.rs). SYSTEM_TICKS increments atomically, the active task's remaining_ticks decrements, waiting tasks increment wait_ticks, and if the slice reaches zero, the task is preempted.
7. How is shared kernel data protected?
   Shared kernel structures (SHARED_STATE, BUMP_ALLOCATOR, FREELIST_ALLOCATOR, READY_QUEUE, schedulers) are wrapped in Spinlock<T> paired with IrqSave (src/sync.rs), which saves RFLAGS and executes cli to prevent interrupt-handler deadlocks on the local CPU.
8. Why is synchronization required?
   Without synchronization, concurrent access from preemptive timer interrupts or multiple tasks causes torn reads/writes and race conditions (e.g., corrupting the allocator's next offset or breaking checksum == counter * 42).
9. How does your allocator work?
   We implemented two allocators in src/memory.rs: (a) BumpAllocator (64 KB static array) which aligns next upward via (next + align - 1) & !(align - 1) for fast O(1) sequential allocations, and (b) FreeListAllocator (64 fixed 256-byte blocks = 16 KB) which tracks block occupancy to support O(1) deallocation and immediate block reuse (reused_block=0).
10. What happens when allocation fails?
    Both allocators return Option<(usize, *mut u8)> (None on failure) rather than panicking. In run_memory_demo() and TC5, requesting 128 KB from the 64 KB heap or a 0-byte allocation increments failed_count and returns None, which the kernel logs as BUMP_OOM_HANDLED.
11. Where is unsafe used and why?
    unsafe is restricted to 5 audited locations: UART/PIT port I/O (out dx, al / in al, dx), RFLAGS/cli/sti interrupt control, UnsafeCell dereferencing inside SpinlockGuard, bounds-checked heap pointer arithmetic, and switch_context() callee-saved register saving/restoring.
12. What invariant does your unsafe code rely on?
    In SpinlockGuard, the invariant is that locked was atomically transitioned from false -> true with Ordering::Acquire, guaranteeing exclusive mutable access to UnsafeCell<T>. In BumpAllocator, the invariant is aligned_offset + size <= HEAP_SIZE verified via checked_add.
13. What happens if two tasks access the same resource?
    The second task's lock() spins (or try_lock() returns None), atomically incrementing the contentions counter until the first task drops its SpinlockGuard with Ordering::Release.
14. How did you test the scheduler?
    Through 6 systematic test cases (TC1 2-task min, TC2 5-task mix, TC3 quantum=1 boundary, TC4 12-task stress, TC5 queue overflow rejection, TC6 deterministic FNV-1a repeatability hash) plus a direct comparison against 3-Level MLFQ.
15. How did you establish that the scheduler is actually working?
    By emitting per-tick [TRACE] events (DISPATCH, EXEC_SLICE, QUANTUM_EXPIRED, DEMOTE, AGING_BOOST, TASK_COMPLETE) over UART 0x3F8 and parsing them with scripts/analyze_traces.py into quantitative CSV metrics and SVG charts.
16. How is your Micro-OS different from Linux?
    Linux uses a complex red-black tree EEVDF/CFS scheduler, virtual memory paging with Buddy/SLUB allocators, and queued MCS spinlocks, whereas SOS implements transparent, fixed-bound mechanisms (FIFO Ring-Buffer RR + 3-Level MLFQ, 64KB Bump + 16KB Free-List, atomic Spinlock<T>) to make every state transition observable.
17. What did CA-5 teach you that helped this project?
    Observing Linux scheduling latency and lock contention in CA-5 showed how time-slice length trades off response time against context-switch overhead—directly verified by TC2 (10 switches at Q=2) vs. TC3 (14 switches at Q=1).
18. What CA-4 research question could be extended into your Micro-OS?
    Investigating how adaptive MLFQ quantum sizing and aging intervals impact tail latency vs. throughput under mixed I/O and CPU-bound workloads (implemented in Track A src/mlfq.rs).
19. What would you implement next?
    Hardware IDT vector wiring for asynchronous APIC timer interrupts, 4-level x86_64 page tables with a physical frame allocator, and Ring 3 (sysret/syscall) user-space task isolation.
20. What is the most important limitation of your current system?
    Tasks execute in shared Ring 0 kernel address space without hardware page-table isolation, and the BumpAllocator only reclaims memory on full reset (mitigated by pairing it with the FreeListAllocator for reusable 256-byte blocks).
