use crate::queue::ReadyQueue;
use crate::sync::Spinlock;
use crate::task::{TaskControlBlock, TaskState};

pub const NUM_LEVELS: usize = 3;
pub const LEVEL_QUANTA: [usize; NUM_LEVELS] = [1, 2, 4];
pub const BOOST_INTERVAL: usize = 8;

pub struct MlfqScheduler {
    pub tasks: [TaskControlBlock; 4],
    pub queues: [ReadyQueue; NUM_LEVELS],
    pub current_tick: usize,
    pub ticks_since_boost: usize,
    pub context_switches: usize,
    pub priority_demotions: usize,
    pub priority_boosts: usize,
}

impl MlfqScheduler {
    pub const fn new() -> Self {
        Self {
            // Task 1 & 3: CPU-bound workloads (will demote Q0 -> Q1 -> Q2)
            // Task 2 & 4: Interactive/short bursts (finish quickly at high priority Q0/Q1)
            tasks: [
                TaskControlBlock::new(1, "CpuMatrixMul", 0, 1, 7, 0x2001_0000, 4096),
                TaskControlBlock::new(2, "UiKeyHandler", 0, 1, 2, 0x2001_1000, 4096),
                TaskControlBlock::new(3, "VideoEncoder", 0, 1, 6, 0x2001_2000, 4096),
                TaskControlBlock::new(4, "AudioMixerIo", 0, 1, 2, 0x2001_3000, 4096),
            ],
            queues: [ReadyQueue::new(), ReadyQueue::new(), ReadyQueue::new()],
            current_tick: 60,
            ticks_since_boost: 0,
            context_switches: 0,
            priority_demotions: 0,
            priority_boosts: 0,
        }
    }

    pub fn init(&mut self) {
        for i in 0..self.tasks.len() {
            self.tasks[i].priority = 0;
            self.queues[0].enqueue(i);
            crate::ktrace!(
                self.current_tick,
                "MLFQ",
                "ENQUEUE_INIT",
                "task_id={} name={} level=Q0 work_target={}",
                self.tasks[i].id,
                self.tasks[i].name,
                self.tasks[i].work_Target
            );
        }
    }

    fn select_next_task(&mut self) -> Option<(usize, usize)> {
        for level in 0..NUM_LEVELS {
            if let Some(task_idx) = self.queues[level].dequeue() {
                return Some((level, task_idx));
            }
        }
        None
    }

    fn boost_all_priorities(&mut self) {
        self.priority_boosts += 1;
        for level in 1..NUM_LEVELS {
            while let Some(task_idx) = self.queues[level].dequeue() {
                self.tasks[task_idx].priority = 0;
                self.queues[0].enqueue(task_idx);
            }
        }
        crate::ktrace!(
            self.current_tick,
            "MLFQ",
            "AGING_BOOST",
            "boost_count={} action=ALL_TASKS_PROMOTED_TO_Q0",
            self.priority_boosts
        );
    }

    pub fn run_to_completion(&mut self) {
        self.init();

        while let Some((level, idx)) = self.select_next_task() {
            self.context_switches += 1;
            let quantum = LEVEL_QUANTA[level];
            self.tasks[idx].state = TaskState::Running;
            self.tasks[idx].remaining_ticks = quantum;

            crate::ktrace!(
                self.current_tick,
                "MLFQ",
                "DISPATCH",
                "task_id={} name={} level=Q{} quantum={} rem_work={}",
                self.tasks[idx].id,
                self.tasks[idx].name,
                level,
                quantum,
                self.tasks[idx].work_Target - self.tasks[idx].work_done
            );

            while self.tasks[idx].remaining_ticks > 0
                && self.tasks[idx].work_done < self.tasks[idx].work_Target
            {
                self.current_tick += 1;
                self.ticks_since_boost += 1;
                self.tasks[idx].remaining_ticks -= 1;
                self.tasks[idx].work_done += 1;
                self.tasks[idx].total_ticks_run += 1;

                for other in 0..self.tasks.len() {
                    if other != idx && self.tasks[other].state == TaskState::Ready {
                        self.tasks[other].wait_ticks += 1;
                    }
                }
            }

            if self.tasks[idx].work_done >= self.tasks[idx].work_Target {
                self.tasks[idx].state = TaskState::Terminated;
                crate::ktrace!(
                    self.current_tick,
                    "MLFQ",
                    "TASK_COMPLETE",
                    "task_id={} name={} final_level=Q{} run_ticks={} wait_ticks={} turnaround={}",
                    self.tasks[idx].id,
                    self.tasks[idx].name,
                    level,
                    self.tasks[idx].total_ticks_run,
                    self.tasks[idx].wait_ticks,
                    self.tasks[idx].total_ticks_run + self.tasks[idx].wait_ticks
                );
            } else {
                self.tasks[idx].state = TaskState::Ready;
                let next_level = if level + 1 < NUM_LEVELS {
                    self.priority_demotions += 1;
                    level + 1
                } else {
                    level
                };
                self.tasks[idx].priority = next_level as u8;
                self.queues[next_level].enqueue(idx);
                crate::ktrace!(
                    self.current_tick,
                    "MLFQ",
                    "DEMOTE",
                    "task_id={} from=Q{} to=Q{} progress={}/{}",
                    self.tasks[idx].id,
                    level,
                    next_level,
                    self.tasks[idx].work_done,
                    self.tasks[idx].work_Target
                );
            }

            if self.ticks_since_boost >= BOOST_INTERVAL {
                self.ticks_since_boost = 0;
                self.boost_all_priorities();
            }
        }
    }
}

pub static MLFQ_SCHEDULER: Spinlock<MlfqScheduler> = Spinlock::new(MlfqScheduler::new());

pub fn run_mlfq_demo() {
    crate::kinfo!("Starting Track A Specialization (3-Level MLFQ Scheduler with Aging)...");
    let mut mlfq = MLFQ_SCHEDULER.lock();
    mlfq.run_to_completion();

    let io_wait = (mlfq.tasks[1].wait_ticks + mlfq.tasks[3].wait_ticks) / 2;
    let cpu_wait = (mlfq.tasks[0].wait_ticks + mlfq.tasks[2].wait_ticks) / 2;

    crate::kinfo!(
        "Track A MLFQ Verified: switches={} demotions={} aging_boosts={} interactive_avg_wait={} cpu_bound_avg_wait={}",
        mlfq.context_switches,
        mlfq.priority_demotions,
        mlfq.priority_boosts,
        io_wait,
        cpu_wait
    );
}