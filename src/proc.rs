use spin::Mutex;

pub const MAX_PROC_ENTRIES: usize = 256;

#[derive(Copy, Clone)]
pub struct ProcEntry {
    pub pid: u32,
    pub name: [u8; 256],
    pub state: [u8; 8],
    pub ppid: u32,
    pub uid: u32,
    pub gid: u32,
    pub vm_size: u64,
    pub vm_rss: u64,
    pub vm_shared: u64,
    pub vm_data: u64,
    pub vm_stk: u64,
    pub vm_swap: u64,
    pub cpu_time: u64,
    pub start_time: u64,
}

impl ProcEntry {
    pub fn new(pid: u32, name: &str) -> Self {
        let mut n = [0u8; 256];
        let len = name.len().min(255);
        n[..len].copy_from_slice(name.as_bytes());
        
        ProcEntry {
            pid,
            name: n,
            state: *b"Running ",
            ppid: 0,
            uid: 0,
            gid: 0,
            vm_size: 0,
            vm_rss: 0,
            vm_shared: 0,
            vm_data: 0,
            vm_stk: 0,
            vm_swap: 0,
            cpu_time: 0,
            start_time: 0,
        }
    }

    pub fn as_status(&self) -> alloc::string::String {
        alloc::string::String::from(format!(
            "Name:\t{}\nState:\t{}\nPid:\t{}\nPPid:\t{}\nUid:\t{}\nGid:\t{}\nVmSize:\t{} kB\nVmRSS:\t{} kB\n",
            core::str::from_utf8(&self.name).unwrap_or(""),
            core::str::from_utf8(&self.state).unwrap_or(""),
            self.pid,
            self.ppid,
            self.uid,
            self.gid,
            self.vm_size,
            self.vm_rss
        ))
    }

    pub fn as_stat(&self) -> alloc::string::String {
        alloc::string::String::from(format!(
            "{} ({}) S {} {} {} {} 0 0 0 0 {} 0 {} 0 0 0 0 0 0 0",
            self.pid,
            core::str::from_utf8(&self.name).unwrap_or(""),
            self.ppid,
            self.pid,
            self.state[0] as char,
            self.state[1] as char,
            self.cpu_time,
            self.start_time
        ))
    }

    pub fn as_cmdline(&self) -> alloc::string::String {
        alloc::string::String::from("")
    }

    pub fn as_maps(&self) -> alloc::string::String {
        alloc::string::String::from("00400000-00401000 r-xp 00000000 00:0 0 [vdso]\n")
    }
}

pub struct ProcFileSystem {
    pub entries: [Option<ProcEntry>; MAX_PROC_ENTRIES],
    pub next_pid: u32,
    pub mount_count: u32,
}

impl ProcFileSystem {
    pub const fn new() -> Self {
        ProcFileSystem {
            entries: [None; MAX_PROC_ENTRIES],
            next_pid: 1,
            mount_count: 0,
        }
    }

    pub fn init(&mut self) {
        self.next_pid = 1;
    }

    pub fn add_process(&mut self, name: &str) -> u32 {
        let pid = self.next_pid;
        self.next_pid += 1;
        
        if (pid as usize) < MAX_PROC_ENTRIES {
            self.entries[pid as usize] = Some(ProcEntry::new(pid, name));
        }
        
        pid
    }

    pub fn remove_process(&mut self, pid: u32) {
        if (pid as usize) < MAX_PROC_ENTRIES {
            self.entries[pid as usize] = None;
        }
    }

    pub fn get_process(&self, pid: u32) -> Option<&ProcEntry> {
        if (pid as usize) < MAX_PROC_ENTRIES {
            self.entries[pid as usize].as_ref()
        } else {
            None
        }
    }

    pub fn ls(&self) {
        println!("total {}", self.next_pid - 1);
        
        for i in 1..self.next_pid {
            if let Some(ref entry) = self.entries[i as usize] {
                let name = core::str::from_utf8(&entry.name).unwrap_or("?");
                println!("{}", name);
            }
        }
    }

    pub fn read_file(&self, pid: u32, name: &str) -> Option<alloc::string::String> {
        if let Some(ref entry) = self.get_process(pid) {
            match name {
                "status" => Some(entry.as_status()),
                "stat" => Some(entry.as_stat()),
                "cmdline" => Some(entry.as_cmdline()),
                "maps" => Some(entry.as_maps()),
                _ => None,
            }
        } else {
            None
        }
    }
}

pub static PROC: Mutex<ProcFileSystem> = Mutex::new(ProcFileSystem::new());

pub fn init_proc() {
    PROC.lock().init();
    println!("/proc: initialise");
}

pub mod kernel {
    

    pub fn uptime() -> alloc::string::String {
        let ticks = *crate::timer::TICKS.lock();
        alloc::string::String::from(format!("{}.{:02}\n", ticks / 100, ticks % 100))
    }

    pub fn version() -> alloc::string::String {
        alloc::string::String::from(format!(
            "Vibe-OS {} {}\n",
            crate::VERSION,
            crate::BUILD_DATE
        ))
    }

    pub fn cmdline() -> alloc::string::String {
        alloc::string::String::from("")
    }

    pub fn modules() -> alloc::string::String {
        alloc::string::String::from("Module Size  Used by:\n")
    }

    pub fn stat() -> alloc::string::String {
        alloc::string::String::from("cpu  0 0 0 0 0 0 0 0 0 0\n")
    }

    pub fn interrupts() -> alloc::string::String {
        alloc::string::String::from("            CPU0\n")
    }

    pub fn devices() -> alloc::string::String {
        alloc::string::String::from("chrdev:    1 ram0\n")
    }

    pub fn filesystems() -> alloc::string::String {
        alloc::string::String::from("nodev   proc\n")
    }
}