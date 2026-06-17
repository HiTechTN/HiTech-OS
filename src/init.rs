use core::cmp::min;

pub const MAX_PROCESSES: usize = 256;
pub const MAX_SERVICES: usize = 64;
pub const MAX_ENV: usize = 64;
pub const INIT_PID: u32 = 1;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ServiceState {
    Stopped,
    Starting,
    Running,
    Stopping,
    Failed,
}

impl ServiceState {
    pub fn name(&self) -> &'static str {
        match self {
            ServiceState::Stopped => "stopped",
            ServiceState::Starting => "starting",
            ServiceState::Running => "running",
            ServiceState::Stopping => "stopping",
            ServiceState::Failed => "failed",
        }
    }
}

#[derive(Copy, Clone)]
pub struct Service {
    pub name: [u8; 32],
    pub name_len: usize,
    pub pid: u32,
    pub state: ServiceState,
    pub auto_start: bool,
    pub critical: bool,
    pub restart_count: u32,
    pub max_restarts: u32,
}

impl Service {
    pub fn new(name: &str, auto_start: bool, critical: bool) -> Self {
        let mut n = [0u8; 32];
        let len = min(name.len(), 31);
        n[..len].copy_from_slice(name.as_bytes());

        Service {
            name: n,
            name_len: len,
            pid: 0,
            state: ServiceState::Stopped,
            auto_start,
            critical,
            restart_count: 0,
            max_restarts: 5,
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("?")
    }
}

pub struct Environment {
    pub vars: [(u64, u64); MAX_ENV],
    pub count: usize,
}

impl Environment {
    pub const fn new() -> Self {
        Environment { vars: [(0, 0); MAX_ENV], count: 0 }
    }

    pub fn set(&mut self, key: &str, val: &str) -> bool {
        if self.count >= MAX_ENV {
            return false;
        }
        let k = alloc::string::String::from(key);
        let v = alloc::string::String::from(val);
        self.vars[self.count] = (k.as_ptr() as u64, v.len() as u64);
        self.count += 1;
        true
    }
}

pub struct InitSystem {
    pub services: [Option<Service>; MAX_SERVICES],
    pub service_count: usize,
    pub boot_time: u64,
    pub runlevel: u8,
    pub environment: Environment,
    pub panic_on_critical_fail: bool,
}

impl InitSystem {
    pub const fn new() -> Self {
        InitSystem {
            services: [None; MAX_SERVICES],
            service_count: 0,
            boot_time: 0,
            runlevel: 1,
            environment: Environment::new(),
            panic_on_critical_fail: true,
        }
    }

    pub fn register_service(&mut self, name: &str, auto_start: bool, critical: bool) -> usize {
        if self.service_count >= MAX_SERVICES {
            return usize::MAX;
        }

        let svc = Service::new(name, auto_start, critical);
        let idx = self.service_count;
        self.services[idx] = Some(svc);
        self.service_count += 1;
        idx
    }

    pub fn start_service(&mut self, index: usize) -> bool {
        if let Some(ref mut svc) = self.services[index] {
            if svc.state != ServiceState::Stopped && svc.state != ServiceState::Failed {
                return false;
            }
            svc.state = ServiceState::Starting;
            let pid = (index as u32) + 10;
            svc.pid = pid;
            svc.state = ServiceState::Running;
            true
        } else {
            false
        }
    }

    pub fn stop_service(&mut self, index: usize) -> bool {
        if let Some(ref mut svc) = self.services[index] {
            if svc.state == ServiceState::Running || svc.state == ServiceState::Failed {
                svc.state = ServiceState::Stopping;
                svc.pid = 0;
                svc.state = ServiceState::Stopped;
                return true;
            }
            false
        } else {
            false
        }
    }

    pub fn restart_service(&mut self, index: usize) -> bool {
        self.stop_service(index);
        self.start_service(index)
    }

    pub fn service_failed(&mut self, index: usize) {
        if let Some(ref mut svc) = self.services[index] {
            svc.state = ServiceState::Failed;
            svc.restart_count += 1;

            if svc.critical && self.panic_on_critical_fail {
                println!("SERVICE CRITIQUE: {} a echoue!", svc.name_str());
            } else if svc.restart_count <= svc.max_restarts {
                println!("Redemarrage de {} (tentative {}/{})",
                    svc.name_str(), svc.restart_count, svc.max_restarts);
                self.start_service(index);
            }
        }
    }

    pub fn start_auto_services(&mut self) {
        for i in 0..self.service_count {
            if let Some(ref svc) = self.services[i] {
                if svc.auto_start && svc.state == ServiceState::Stopped {
                    self.start_service(i);
                }
            }
        }
    }

    pub fn stop_all_services(&mut self) {
        for i in (0..self.service_count).rev() {
            self.stop_service(i);
        }
    }

    pub fn list_services(&self) {
        println!("SERVICES:");
        println!("PID   ETAT      NOM");
        println!("----- --------- --------------------------------");
        for i in 0..self.service_count {
            if let Some(ref svc) = self.services[i] {
                let pid_str = if svc.pid > 0 {
                    alloc::format!("{}", svc.pid)
                } else {
                    alloc::string::String::from("-")
                };
                println!("{:5} {:9} {}", pid_str, svc.state.name(), svc.name_str());
            }
        }
    }

    pub fn set_runlevel(&mut self, level: u8) {
        self.runlevel = level;
        println!("Runlevel {} atteint", level);
    }

    pub fn shutdown(&mut self) {
        println!("Arret en cours...");
        self.stop_all_services();
        println!("Systeme arrete.");
    }

    pub fn reboot(&mut self) {
        self.shutdown();
        println!("Redemarrage...");
    }
}

pub static mut INIT: InitSystem = InitSystem::new();

pub fn init_init() {
    println!("Init: demarrage du gestionnaire de services");
    unsafe {
        INIT.boot_time = *crate::timer::TICKS.lock();
        INIT.panic_on_critical_fail = true;

        INIT.register_service("shell", true, false);
        INIT.register_service("vfs", true, true);
        INIT.register_service("network", false, false);
        INIT.register_service("audio", false, false);
        INIT.register_service("usb", false, false);

        INIT.start_auto_services();
    }
}

pub mod bootlog {
    use spin::Mutex;

    pub const MAX_LOG_ENTRIES: usize = 512;
    pub const MAX_LOG_LINE: usize = 128;

    pub struct LogEntry {
        pub msg: [u8; MAX_LOG_LINE],
        pub len: usize,
        pub level: LogLevel,
    }

    pub enum LogLevel {
        Info,
        Warn,
        Error,
        Debug,
    }

    pub struct BootLog {
        pub entries: [LogEntry; MAX_LOG_ENTRIES],
        pub count: usize,
        pub wrap: usize,
    }

    impl BootLog {
        pub const fn new() -> Self {
            const EMPTY_ENTRY: LogEntry = LogEntry {
                msg: [0; MAX_LOG_LINE],
                len: 0,
                level: LogLevel::Info,
            };
            BootLog {
                entries: [EMPTY_ENTRY; MAX_LOG_ENTRIES],
                count: 0,
                wrap: 0,
            }
        }

        pub fn log(&mut self, msg: &str, _level: LogLevel) {
            if self.count < MAX_LOG_ENTRIES {
                let idx = (self.wrap + self.count) % MAX_LOG_ENTRIES;
                let len = core::cmp::min(msg.len(), MAX_LOG_LINE - 1);
                self.entries[idx].msg[..len].copy_from_slice(&msg.as_bytes()[..len]);
                self.entries[idx].len = len;
                self.count += 1;
                if self.count > MAX_LOG_ENTRIES {
                    self.wrap = (self.wrap + 1) % MAX_LOG_ENTRIES;
                    self.count = MAX_LOG_ENTRIES;
                }
            }
        }
    }

    pub static BOOT_LOG: Mutex<BootLog> = Mutex::new(BootLog::new());

    pub fn kernel_log(msg: &str) {
        BOOT_LOG.lock().log(msg, LogLevel::Info);
    }

    pub fn kernel_warn(msg: &str) {
        BOOT_LOG.lock().log(msg, LogLevel::Warn);
    }

    pub fn kernel_error(msg: &str) {
        BOOT_LOG.lock().log(msg, LogLevel::Error);
    }

    pub fn dmesg() {
        let log = BOOT_LOG.lock();
        for i in 0..core::cmp::min(log.count, MAX_LOG_ENTRIES) {
            let idx = (log.wrap + i) % MAX_LOG_ENTRIES;
            let s = core::str::from_utf8(&log.entries[idx].msg[..log.entries[idx].len]).unwrap_or("?");
            println!("{}", s);
        }
    }
}