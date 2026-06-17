use spin::Mutex;


pub const MAX_TASKS: usize = 16;
pub const KERNEL_STACK_SIZE: usize = 16384;
pub const USER_STACK_SIZE: usize = 16384;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum TaskState {
    Ready,
    Running,
    Waiting,
    Sleeping(u64),
    Terminated,
    Zombie,
}

#[derive(Copy, Clone)]
pub struct TaskContext {
    pub rsp: u64,
    pub cr3: u64,
    pub rflags: u64,
    pub rip: u64,
    pub cs: u16,
    pub ss: u16,
}

impl TaskContext {
    pub const fn empty() -> Self {
        TaskContext {
            rsp: 0,
            cr3: 0,
            rflags: 0,
            rip: 0,
            cs: 0,
            ss: 0,
        }
    }
}

#[derive(Copy, Clone)]
pub struct Task {
    pub id: u8,
    pub pid: u32,
    pub ppid: u32,
    pub state: TaskState,
    pub priority: u8,
    pub nice: i8,
    pub context: TaskContext,
    pub kernel_stack: [u8; KERNEL_STACK_SIZE],
    pub user_stack: [u8; USER_STACK_SIZE],
    pub exit_code: i32,
    pub creation_time: u64,
}

impl Task {
    pub fn new(id: u8, _ip: fn(), ppid: u32) -> Task {
        let _stack_bottom = id as usize * KERNEL_STACK_SIZE;
        Task {
            id,
            pid: id as u32,
            ppid,
            state: TaskState::Ready,
            priority: 128,
            nice: 0,
            context: TaskContext::empty(),
            kernel_stack: [0; KERNEL_STACK_SIZE],
            user_stack: [0; USER_STACK_SIZE],
            exit_code: 0,
            creation_time: 0,
        }
    }
}

pub struct Process {
    pub pid: u32,
    pub ppid: u32,
    pub tasks: [Option<Task>; MAX_TASKS],
    pub task_count: u8,
}

impl Process {
    pub const fn new(pid: u32, ppid: u32) -> Self {
        Process {
            pid,
            ppid,
            tasks: [None; MAX_TASKS],
            task_count: 0,
        }
    }

    pub fn add_task(&mut self, ip: fn()) -> Option<u8> {
        if self.task_count as usize >= MAX_TASKS {
            return None;
        }
        
        let id = self.task_count;
        self.task_count += 1;
        self.tasks[id as usize] = Some(Task::new(id, ip, self.pid));
        Some(id)
    }
}

pub static mut KERNEL_TSS: x86_64::structures::tss::TaskStateSegment = 
    x86_64::structures::tss::TaskStateSegment::new();

pub static TASKS: Mutex<[Option<Task>; MAX_TASKS]> = Mutex::new([None; MAX_TASKS]);
pub static TASK_COUNT: Mutex<u8> = Mutex::new(0);
pub static CURRENT_TASK: Mutex<u8> = Mutex::new(0);

pub fn init_scheduler() {
    println!("Scheduler initialise (max {} taches)", MAX_TASKS);
}

pub mod scheduler {
    use super::*;

    pub fn spawn(ip: fn()) -> Option<u32> {
        let count = *TASK_COUNT.lock();
        
        if count as usize >= MAX_TASKS {
            return None;
        }

        let id = count;
        *TASK_COUNT.lock() = count + 1;
        
        let task = Task::new(id, ip, 0);
        TASKS.lock()[id as usize] = Some(task);
        
        Some(id as u32)
    }

    pub fn kill(pid: u32) -> bool {
        if pid as usize >= MAX_TASKS {
            return false;
        }
        
        if let Some(ref mut task) = TASKS.lock()[pid as usize] {
            task.state = TaskState::Zombie;
            true
        } else {
            false
        }
    }

    pub fn sleep_ticks(_ticks: u64) {
        *CURRENT_TASK.lock() = 0;
    }

    pub fn yield_cpu() {
        let mut current = *CURRENT_TASK.lock();
        let count = *TASK_COUNT.lock();
        
        if count > 1 {
            current = (current + 1) % count;
            *CURRENT_TASK.lock() = current;
        }
    }

    pub fn get_current_pid() -> u32 {
        *CURRENT_TASK.lock() as u32
    }

    pub fn list_tasks() {
        println!("PID  PPID STATE        PRIO NICE");
        println!("--  ---- -----        ---- ----");
        
        for i in 0..*TASK_COUNT.lock() {
            if let Some(ref task) = TASKS.lock()[i as usize] {
                let state = match task.state {
                    TaskState::Ready => "Ready   ",
                    TaskState::Running => "Running",
                    TaskState::Waiting => "Waiting",
                    TaskState::Sleeping(_) => "Sleep  ",
                    TaskState::Terminated => "Term   ",
                    TaskState::Zombie => "Zombie ",
                };
                println!("{:3}  {:3} {}     {:3}   {:3}", 
                    task.pid, task.ppid, state, task.priority as u32, task.nice as i32);
            }
        }
    }

    pub fn get_task_count() -> u8 {
        *TASK_COUNT.lock()
    }
}

pub mod sync {
    use super::*;

    pub struct Mutex {
        locked: spin::Mutex<bool>,
    }

    impl Mutex {
        pub const fn new() -> Self {
            Mutex { locked: spin::Mutex::new(false) }
        }

        pub fn lock(&self) {
            while *self.locked.lock() {
                scheduler::yield_cpu();
            }
            *self.locked.lock() = true;
        }

        pub fn unlock(&self) {
            *self.locked.lock() = false;
        }
    }

    pub struct Semaphore {
        count: spin::Mutex<u32>,
        max: u32,
    }

    impl Semaphore {
        pub const fn new(max: u32) -> Self {
            Semaphore {
                count: spin::Mutex::new(0),
                max,
            }
        }

        pub fn wait(&self) {
            loop {
                let mut c = self.count.lock();
                if *c < self.max {
                    *c += 1;
                    break;
                }
                drop(c);
                scheduler::yield_cpu();
            }
        }

        pub fn signal(&self) {
            let mut c = self.count.lock();
            if *c > 0 {
                *c -= 1;
            }
        }
    }

    pub struct CondVar;

    impl CondVar {
        pub const fn new() -> Self {
            CondVar
        }

        pub fn wait(&self, _mutex: &Mutex) {
            scheduler::yield_cpu();
        }

        pub fn signal(&self) {}
        pub fn broadcast(&self) {}
    }
}