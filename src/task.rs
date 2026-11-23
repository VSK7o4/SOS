#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    Ready,
    Running,
    Blocked,
    Terminated,
}

impl TaskState {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskState::Ready => "READY",
            TaskState::Running => "RUNNING",
            TaskState::Blocked => "BLOCKED",
            TaskState::Terminated => "TERMINATED",
        }
    }
}

#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct CpuContext {
    pub rsp: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
}

impl CpuContext {
    pub const fn empty() -> Self {
        Self {
            rsp: 0,
            rbp: 0,
            rbx: 0,
            r12: 0,
            r13: 0,
            r14: 0,
            r15: 0,
            rip: 0,
        }
    }
}

#[derive(Copy, Clone)]
pub struct TaskControlBlock {
    pub id: usize,
    pub name: &'static str,
    pub state: TaskState,
    pub priority: u8,
    pub time_slice: usize,
    pub remaining_ticks: usize,
    pub total_ticks_run: usize,
    pub wait_ticks: usize,
    pub work_Target: usize,
    pub work_done: usize,
    pub stack_base: usize,
    pub stack_size: usize,
    pub context: CpuContext,
}

impl TaskControlBlock {
    pub const fn new(
        id: usize,
        name: &'static str,
        priority: u8,
        time_slice: usize,
        work_Target: usize,
        stack_base: usize,
        stack_size: usize,
    ) -> Self {
        Self {
            id,
            name,
            state: TaskState::Ready,
            priority,
            time_slice,
            remaining_ticks: time_slice,
            total_ticks_run: 0,
            wait_ticks: 0,
            work_Target,
            work_done: 0,
            stack_base,
            stack_size,
            context: CpuContext::empty(),
        }
    }

    pub fn transition(&mut self, new_state: TaskState, tick: usize) {
        let old = self.state.as_str();
        self.state = new_state;
        crate::ktrace!(
            tick,
            "TASK",
            "STATE_CHANGE",
            "task_id={} name={} from={} to={}",
            self.id,
            self.name,
            old,
            self.state.as_str()
        );
    }
}
