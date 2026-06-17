use spin::Mutex;

pub const NSIG: usize = 32;
pub const SIG_BLOCK: i32 = 0;
pub const SIG_UNBLOCK: i32 = 1;
pub const SIG_SETMASK: i32 = 2;

pub const SIGHUP: i32 = 1;
pub const SIGINT: i32 = 2;
pub const SIGQUIT: i32 = 3;
pub const SIGILL: i32 = 4;
pub const SIGTRAP: i32 = 5;
pub const SIGABRT: i32 = 6;
pub const SIGBUS: i32 = 7;
pub const SIGFPE: i32 = 8;
pub const SIGKILL: i32 = 9;
pub const SIGUSR1: i32 = 10;
pub const SIGSEGV: i32 = 11;
pub const SIGUSR2: i32 = 12;
pub const SIGPIPE: i32 = 13;
pub const SIGALRM: i32 = 14;
pub const SIGTERM: i32 = 15;
pub const SIGSTKFLT: i32 = 16;
pub const SIGCHLD: i32 = 17;
pub const SIGCONT: i32 = 18;
pub const SIGSTOP: i32 = 19;
pub const SIGTSTP: i32 = 20;
pub const SIGTTIN: i32 = 21;
pub const SIGTTOU: i32 = 22;
pub const SIGURG: i32 = 23;
pub const SIGXCPU: i32 = 24;
pub const SIGXFSZ: i32 = 25;
pub const SIGVTALRM: i32 = 26;
pub const SIGPROF: i32 = 27;
pub const SIGWINCH: i32 = 28;
pub const SIGIO: i32 = 29;
pub const SIGPWR: i32 = 30;
pub const SIGSYS: i32 = 31;

pub const SA_NOCLDSTOP: u64 = 0x00000001;
pub const SA_NOCLDWAIT: u64 = 0x00000002;
pub const SA_SIGINFO: u64 = 0x00000004;
pub const SA_ONSTACK: u64 = 0x08000000;
pub const SA_RESTART: u64 = 0x10000000;
pub const SA_NODEFER: u64 = 0x40000000;
pub const SA_RESETHAND: u64 = 0x80000000;

pub const SIG_DFL: usize = 0;
pub const SIG_IGN: usize = 1;
pub const SIG_ERR: usize = !0usize;

#[derive(Copy, Clone)]
pub struct SignalAction {
    pub handler: usize,
    pub flags: u64,
    pub mask: u64,
    pub restorer: usize,
}

impl SignalAction {
    pub const fn default() -> Self {
        SignalAction { handler: SIG_DFL, flags: 0, mask: 0, restorer: 0 }
    }
}

pub struct SignalManager {
    pub pending: u64,
    pub blocked: u64,
    pub actions: [SignalAction; NSIG],
}

impl SignalManager {
    pub const fn new() -> Self {
        SignalManager {
            pending: 0,
            blocked: 0,
            actions: [SignalAction::default(); NSIG],
        }
    }

    pub fn send_signal(&mut self, sig: i32) -> bool {
        if sig <= 0 || sig as usize >= NSIG {
            return false;
        }
        if sig == SIGKILL || sig == SIGSTOP {
            self.pending |= 1 << sig;
            return true;
        }
        self.pending |= 1 << sig;
        true
    }

    pub fn handle_pending(&mut self) -> Option<i32> {
        let pending = self.pending & !self.blocked;
        if pending == 0 {
            return None;
        }
        let sig = pending.trailing_zeros() as i32;
        self.pending &= !(1 << sig);

        if sig as usize >= NSIG {
            return None;
        }

        let action = self.actions[sig as usize];
        match action.handler {
            SIG_DFL => {
                match sig {
                    SIGKILL | SIGTERM | SIGINT | SIGQUIT | SIGABRT | SIGBUS
                    | SIGFPE | SIGILL | SIGSEGV | SIGPIPE | SIGALRM | SIGPROF
                    | SIGVTALRM | SIGSYS | SIGTRAP => Some(sig),
                    _ => None,
                }
            }
            SIG_IGN => None,
            _ => {
                Some(sig)
            }
        }
    }

    pub fn set_action(&mut self, sig: i32, action: SignalAction) -> bool {
        if sig <= 0 || sig as usize >= NSIG || sig == SIGKILL || sig == SIGSTOP {
            return false;
        }
        self.actions[sig as usize] = action;
        true
    }

    pub fn get_action(&self, sig: i32) -> Option<SignalAction> {
        if sig <= 0 || sig as usize >= NSIG {
            return None;
        }
        Some(self.actions[sig as usize])
    }

    pub fn block_signal(&mut self, sig: i32) -> bool {
        if sig <= 0 || sig as usize >= NSIG {
            return false;
        }
        self.blocked |= 1 << sig;
        true
    }

    pub fn unblock_signal(&mut self, sig: i32) -> bool {
        if sig <= 0 || sig as usize >= NSIG {
            return false;
        }
        self.blocked &= !(1 << sig);
        true
    }

    pub fn is_signal_pending(&self, sig: i32) -> bool {
        if sig <= 0 || sig as usize >= NSIG {
            return false;
        }
        (self.pending & (1 << sig)) != 0
    }

    pub fn signal_name(sig: i32) -> &'static str {
        match sig {
            SIGHUP => "SIGHUP",
            SIGINT => "SIGINT",
            SIGQUIT => "SIGQUIT",
            SIGILL => "SIGILL",
            SIGTRAP => "SIGTRAP",
            SIGABRT => "SIGABRT",
            SIGBUS => "SIGBUS",
            SIGFPE => "SIGFPE",
            SIGKILL => "SIGKILL",
            SIGUSR1 => "SIGUSR1",
            SIGSEGV => "SIGSEGV",
            SIGUSR2 => "SIGUSR2",
            SIGPIPE => "SIGPIPE",
            SIGALRM => "SIGALRM",
            SIGTERM => "SIGTERM",
            SIGCHLD => "SIGCHLD",
            SIGCONT => "SIGCONT",
            SIGSTOP => "SIGSTOP",
            SIGTSTP => "SIGTSTP",
            SIGTTIN => "SIGTTIN",
            SIGTTOU => "SIGTTOU",
            SIGURG => "SIGURG",
            SIGXCPU => "SIGXCPU",
            SIGXFSZ => "SIGXFSZ",
            SIGVTALRM => "SIGVTALRM",
            SIGPROF => "SIGPROF",
            SIGWINCH => "SIGWINCH",
            SIGIO => "SIGIO",
            SIGPWR => "SIGPWR",
            SIGSYS => "SIGSYS",
            _ => "SIGUNKNOWN",
        }
    }
}

pub static SIGNAL_MANAGER: Mutex<SignalManager> = Mutex::new(SignalManager::new());

pub fn init_signals() {
    println!("Signaux: {} signaux definis", NSIG);
}

pub mod sigaction {
    use super::*;

    pub fn sigaction_set(sig: i32, action: &SignalAction) -> bool {
        SIGNAL_MANAGER.lock().set_action(sig, *action)
    }

    pub fn sigaction_get(sig: i32) -> Option<SignalAction> {
        SIGNAL_MANAGER.lock().get_action(sig)
    }

    pub fn sigprocmask(how: i32, set: u64) -> bool {
        let mut mgr = SIGNAL_MANAGER.lock();
        let _block = mgr.blocked;
        match how {
            SIG_BLOCK => { mgr.blocked |= set; true }
            SIG_UNBLOCK => { mgr.blocked &= !set; true }
            SIG_SETMASK => { mgr.blocked = set; true }
            _ => false,
        }
    }

    pub fn signal_default_handler(sig: i32) {
        match sig {
            SIGKILL | SIGTERM | SIGINT | SIGQUIT => {
                println!("Signal {} recu, arret", SignalManager::signal_name(sig));
            }
            SIGSEGV | SIGBUS | SIGFPE | SIGILL | SIGABRT => {
                println!("ERREUR: {} - segmentation/arith/fault", SignalManager::signal_name(sig));
            }
            SIGCHLD => {}
            _ => {}
        }
    }
}