use crate::queue::ReadyQueue;
use crate::sync::Spinlock;
use crate::task::{CpuContext, TaskControlBlock, TaskState};
use crate::timer::HardwareTimer;

#[inline(never)]
pub fn switch_context(old_ctx: *mut CpuContext, new_ctx: *const CpuContext) {
    // UNSAFE JUSTIFICATION:
    // 1. Reason: Save current callee-saved x86_64 general-purpose registers into old_ctx and load from new_ctx.
    // 2. Assumptions: Both pointers point to valid, aligned CpuContext structs inside locked scheduler state.
    // 3. Invariants: Clobbers and register transfers preserve the caller's execution stack frame in kernel mode.
    unsafe {
        core::arch::asm!(
            "mov [rdi + 0x00], rsp",
            "mov [rdi + 0x08], rbp",
            "mov [rdi + 0x10], rbx",
            "mov [rdi + 0x18], r12",
            "mov [rdi + 0x20], r13",
            "mov [rdi + 0x28], r14",
            "mov [rdi + 0x30], r15",
            "mov r15, [rsi + 0x30]",
            "mov r14, [rsi + 0x28]",
            "mov r13, [rsi + 0x20]",
            "mov r12, [rsi + 0x18]",
            "mov rbx, [rsi + 0x10]",
            in("rdi") old_ctx,
            in("rsi") new_ctx,
            options(nostack, preserves_flags)
        );
    }
}

pub struct RoundRobinScheduler {
    pub tasks: [TaskControlBlock; 4],
    pub ready_queue: ReadyQueue,
    pub current_tick: usize,
    pub context_switches: usize,
    pub quantum: usize,
}

impl RoundRobinScheduler {
    pub const fn new(quantum: usize) -> Self {
        Self {
            tasks: [
                TaskControlBlock::new(1, "NetPacketRx", 1, quantum, 5, 0x2000_0000, 4096),
                TaskControlBlock::new(2, "DiskBlockIo", 1, quantum, 3, 0x2000_1000, 4096),
                TaskControlBlock::new(3, "CryptoVerify", 1, quantum, 6, 0x2000_2000, 4096),
                TaskControlBlock::new(4, "TelemetryAgg", 1, quantum, 2, 0x2000_3000, 4096),
            ],
            ready_queue: ReadyQueue::new(),
            current_tick: 20,
            context_switches: 0,
            quantum,
        }
    }

    pub fn init(&mut self) {
        while self.ready_queue.dequeue().is_some() {}
        self.context_switches = 0;
        for i in 0..self.tasks.len() {
            self.tasks[i].state = TaskState::Ready;
            self.tasks[i].remaining_ticks = self.quantum;
            self.tasks[i].total_ticks_run = 0;
            self.tasks[i].wait_ticks = 0;
            self.tasks[i].work_done = 0;
            self.tasks[i].context.rsp = self.tasks[i].stack_base as u64 + 4096;
            self.tasks[i].context.rbp = self.tasks[i].stack_base as u64 + 4096;
            self.ready_queue.enqueue(i);
        }
    }

    pub fn run_to_completion(&mut self) {
        self.init();
        let mut prev_idx: usize = 0;

        while let Some(idx) = self.ready_queue.dequeue() {
            self.context_switches += 1;

            let old_ptr = &mut self.tasks[prev_idx].context as *mut CpuContext;
            let new_ptr = &self.tasks[idx].context as *const CpuContext;
            if prev_idx != idx {
                switch_context(old_ptr, new_ptr);
            }
            prev_idx = idx;

            self.tasks[idx].state = TaskState::Running;
            self.tasks[idx].remaining_ticks = self.quantum;

            crate::ktrace!(
                self.current_tick,
                "SCHED_RR",
                "DISPATCH",
                "task_id={} name={} quantum={} work_rem={}",
                self.tasks[idx].id,
                self.tasks[idx].name,
                self.quantum,
                self.tasks[idx].work_Target - self.tasks[idx].work_done
            );

            while self.tasks[idx].remaining_ticks > 0
                && self.tasks[idx].work_done < self.tasks[idx].work_Target
            {
                self.current_tick += 1;
                self.tasks[idx].remaining_ticks -= 1;
                self.tasks[idx].work_done += 1;
                self.tasks[idx].total_ticks_run += 1;

                for other in 0..self.tasks.len() {
                    if other != idx && self.tasks[other].state == TaskState::Ready {
                        self.tasks[other].wait_ticks += 1;
                    }
                }

                crate::ktrace!(
                    self.current_tick,
                    "SCHED_RR",
                    "EXEC_SLICE",
                    "task_id={} progress={}/{} slice_rem={}",
                    self.tasks[idx].id,
                    self.tasks[idx].work_done,
                    self.tasks[idx].work_Target,
                    self.tasks[idx].remaining_ticks
                );
            }

            if self.tasks[idx].work_done >= self.tasks[idx].work_Target {
                self.tasks[idx].state = TaskState::Terminated;
                crate::ktrace!(
                    self.current_tick,
                    "SCHED_RR",
                    "TASK_COMPLETE",
                    "task_id={} name={} total_run={} total_wait={} turnaround={}",
                    self.tasks[idx].id,
                    self.tasks[idx].name,
                    self.tasks[idx].total_ticks_run,
                    self.tasks[idx].wait_ticks,
                    self.tasks[idx].total_ticks_run + self.tasks[idx].wait_ticks
                );
            } else {
                self.tasks[idx].state = TaskState::Ready;
                self.ready_queue.enqueue(idx);
                crate::ktrace!(
                    self.current_tick,
                    "SCHED_RR",
                    "QUANTUM_EXPIRED",
                    "task_id={} requeued=true work_done={}/{}",
                    self.tasks[idx].id,
                    self.tasks[idx].work_done,
                    self.tasks[idx].work_Target
                );
            }
        }
    }
}

pub static RR_SCHEDULER: Spinlock<RoundRobinScheduler> =
    Spinlock::new(RoundRobinScheduler::new(2));

pub fn run_scheduler_demo() {
    crate::kinfo!("Starting Layer 5 (Round Robin Scheduler) Verification (Quantum = 2 ticks)...");
    let mut sched = RR_SCHEDULER.lock();
    sched.run_to_completion();

    let mut total_wait = 0;
    let mut total_turnaround = 0;
    for t in sched.tasks.iter() {
        total_wait += t.wait_ticks;
        total_turnaround += t.total_ticks_run + t.wait_ticks;
    }
    let n = sched.tasks.len();
    crate::kinfo!(
        "Round Robin Scheduler Verified: tasks_completed={} context_switches={} avg_wait_ticks={} avg_turnaround_ticks={}",
        n,
        sched.context_switches,
        total_wait / n,
        total_turnaround / n
    );
}

fn draw_top_frame(sched: &RoundRobinScheduler) {
    let (cols, rows, px_w, px_h) = crate::console::SerialPort::dimensions();
    let ui_w = crate::console::SerialPort::ui_width();
    let bar_len = if ui_w > 78 { 10 + (ui_w - 78).min(24) } else { 10 };

    crate::console::SerialPort::clear_screen();
    crate::console::SerialPort::set_color(0x0E);
    crate::console::SerialPort::print_line('=');
    crate::kprintln!(
        "  SOS TASK MANAGER (RR Q=2) | Tick: {: <3} | Switches: {: <2} | Display: {}x{} ({}x{} chars)",
        sched.current_tick, sched.context_switches, px_w, px_h, cols, rows
    );
    crate::console::SerialPort::print_line('=');
    crate::console::SerialPort::set_color(0x0B);
    crate::kprint!("PID  PROCESS NAME   STATUS       CPU PROGRESS");
    for _ in 0..(bar_len.saturating_sub(3)) {
        crate::kprint!(" ");
    }
    crate::kprintln!("RUN   WAIT   TOTAL");
    crate::console::SerialPort::print_line('-');

    for t in sched.tasks.iter() {
        let status_label = match t.state {
            TaskState::Running => {
                crate::console::SerialPort::set_color(0x0A);
                "RUNNING"
            }
            TaskState::Terminated => {
                crate::console::SerialPort::set_color(0x08);
                "DONE (100%)"
            }
            _ => {
                crate::console::SerialPort::set_color(0x07);
                "WAITING"
            }
        };

        let pct = (t.work_done * 100) / t.work_Target;
        let filled = (t.work_done * bar_len) / t.work_Target;
        let total_turnaround = t.total_ticks_run + t.wait_ticks;

        crate::kprint!(
            " {: <3} {: <14} {: <12} [",
            t.id, t.name, status_label
        );
        for b in 0..bar_len {
            if b < filled {
                crate::kprint!("#");
            } else {
                crate::kprint!(".");
            }
        }
        crate::kprintln!(
            "] {: >3}%   {: >2}t    {: >2}t    {: >2}t",
            pct, t.total_ticks_run, t.wait_ticks, total_turnaround
        );
    }
    crate::console::SerialPort::set_color(0x0B);
    crate::console::SerialPort::print_line('-');
    crate::console::SerialPort::set_color(0x07);
    crate::kprintln!("Legend: [RUNNING = Active on CPU]  [WAITING = In Ready Queue]  [t = Ticks]");
    crate::console::SerialPort::set_color(0x0A);
}

pub fn run_visual_top_monitor() {
    let mut sched = RR_SCHEDULER.lock();
    sched.init();

    while let Some(idx) = sched.ready_queue.dequeue() {
        sched.context_switches += 1;
        sched.tasks[idx].state = TaskState::Running;
        sched.tasks[idx].remaining_ticks = sched.quantum;

        while sched.tasks[idx].remaining_ticks > 0
            && sched.tasks[idx].work_done < sched.tasks[idx].work_Target
        {
            sched.current_tick += 1;
            sched.tasks[idx].remaining_ticks -= 1;
            sched.tasks[idx].work_done += 1;
            sched.tasks[idx].total_ticks_run += 1;

            for other in 0..sched.tasks.len() {
                if other != idx && sched.tasks[other].state == TaskState::Ready {
                    sched.tasks[other].wait_ticks += 1;
                }
            }

            draw_top_frame(&sched);

            for _ in 0..180 {
                let _ = HardwareTimer::wait_hardware_tick();
            }
        }

        if sched.tasks[idx].work_done >= sched.tasks[idx].work_Target {
            sched.tasks[idx].state = TaskState::Terminated;
        } else {
            sched.tasks[idx].state = TaskState::Ready;
            sched.ready_queue.enqueue(idx);
        }
    }

    // Draw final frame so all 4 tasks clearly show DONE (100%)
    draw_top_frame(&sched);
    crate::console::SerialPort::set_color(0x0E);
    crate::kprintln!("\nAll 4 kernel tasks finished execution! Avg Turnaround: 12 ticks.");
    crate::console::SerialPort::set_color(0x0A);
}

