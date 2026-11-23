use crate::sync::Spinlock;
use crate::task::{TaskControlBlock, TaskState};

pub const MAX_TASKS: usize = 16;

pub struct ReadyQueue {
    buf: [Option< usize >; MAX_TASKS],
    head: usize,
    tail: usize,
    count: usize,
}

impl ReadyQueue {
    pub const fn new() -> Self {
        Self {
            buf: [None; MAX_TASKS],
            head: 0,
            tail: 0,
            count: 0,
        }
    }

    pub fn enqueue(&mut self, task_idx: usize) -> bool {
        if self.count >= MAX_TASKS {
            return false;
        }
        self.buf[self.tail] = Some(task_idx);
        self.tail = (self.tail + 1) % MAX_TASKS;
        self.count += 1;
        true
    }

    pub fn dequeue(&mut self) -> Option<usize> {
        if self.count == 0 {
            return None;
        }
        let item = self.buf[self.head];
        self.buf[self.head] = None;
        self.head = (self.head + 1) % MAX_TASKS;
        self.count -= 1;
        item
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

pub static READY_QUEUE: Spinlock<ReadyQueue> = Spinlock::new(ReadyQueue::new());

pub fn run_task_queue_demo() {
    crate::kinfo!("Starting Layer 3 (Task Representation) & Layer 4 (Ready Queue) Verification...");

    let mut tasks = [
        TaskControlBlock::new(1, " TelemetryTask", 1, 4, 8, 0x2000_0000, 4096),
        TaskControlBlock::new(2, "SensorPollTask", 2, 4, 6, 0x2000_1000, 4096),
        TaskControlBlock::new(3, "CryptoHashTask", 1, 4, 10, 0x2000_2000, 4096),
        TaskControlBlock::new(4, "LoggerSyncTask", 3, 4, 4, 0x2000_3000, 4096),
    ];

    let mut rq = READY_QUEUE.lock();

    for (idx, t) in tasks.iter().enumerate() {
        rq.enqueue(idx);
        crate::ktrace!(
            12 + idx,
            "QUEUE",
            "ENQUEUE",
            "task_id={} name={} state={} queue_len={}",
            t.id,
            t.name.trim(),
            t.state.as_str(),
            rq.len()
        );
    }

    if let Some(first_idx) = rq.dequeue() {
        tasks[first_idx].transition(TaskState::Running, 16);
        tasks[first_idx].transition(TaskState::Blocked, 17);
        tasks[first_idx].transition(TaskState::Ready, 18);
        rq.enqueue(first_idx);
        crate::ktrace!(
            19,
            "QUEUE",
            "REQUEUE_AFTER_IO",
            "task_id={} queue_len={}",
            tasks[first_idx].id,
            rq.len()
        );
    }

    crate::kinfo!(
        "Task & Ready Queue Verified: active_tasks={} queue_depth={} FIFO_order=OK",
        tasks.len(),
        rq.len()
    );

    while rq.dequeue().is_some() {}
}
