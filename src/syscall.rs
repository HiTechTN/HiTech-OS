use spin::Mutex;

pub const SYSCALL_WRITE: u64 = 1;
pub const SYSCALL_READ: u64 = 2;
pub const SYSCALL_OPEN: u64 = 3;
pub const SYSCALL_CLOSE: u64 = 4;
pub const SYSCALL_MMAP: u64 = 9;
pub const SYSCALL_MUNMAP: u64 = 11;
pub const SYSCALL_BRK: u64 = 12;
pub const SYSCALL_IOCTL: u64 = 16;
pub const SYSCALL_ACCESS: u64 = 21;
pub const SYSCALL_MKDIR: u64 = 39;
pub const SYSCALL_RMDIR: u64 = 40;
pub const SYSCALL_UNLINK: u64 = 41;
pub const SYSCALL_READLINK: u64 = 42;
pub const SYSCALL_CLONE: u64 = 56;
pub const SYSCALL_EXECVE: u64 = 59;
pub const SYSCALL_EXIT: u64 = 60;
pub const SYSCALL_WAIT4: u64 = 61;
pub const SYSCALL_KILL: u64 = 62;
pub const SYSCALL_GETPID: u64 = 64;
pub const SYSCALL_UNAME: u64 = 65;
pub const SYSCALL_SEMGET: u64 = 64;
pub const SYSCALL_SEMOP: u64 = 65;
pub const SYSCALL_SEMCTL: u64 = 66;
pub const SYSCALL_SHMGET: u64 = 67;
pub const SYSCALL_SHMAT: u64 = 68;
pub const SYSCALL_SHMDT: u64 = 70;
pub const SYSCALL_MSGGET: u64 = 71;
pub const SYSCALL_MSGSND: u64 = 72;
pub const SYSCALL_MSGRCV: u64 = 73;
pub const SYSCALL_MSGCTL: u64 = 74;
pub const SYSCALL_FCNTL: u64 = 55;
pub const SYSCALL_FLOCK: u64 = 73;
pub const SYSCALL_FSYNC: u64 = 74;
pub const SYSCALL_FDATASYNC: u64 = 75;
pub const SYSCALL_TRUNCATE: u64 = 76;
pub const SYSCALL_FTRUNCATE: u64 = 77;
pub const SYSCALL_GETDENTS: u64 = 78;
pub const SYSCALL_GETCWD: u64 = 79;
pub const SYSCALL_CHDIR: u64 = 80;
pub const SYSCALL_FCHDIR: u64 = 81;
pub const SYSCALL_RENAME: u64 = 82;
pub const SYSCALL_MKDEV: u64 = 83;
pub const SYSCALL_UTIMES: u64 = 88;
pub const SYSCALL_STATFS: u64 = 89;
pub const SYSCALL_FSTATFS: u64 = 90;
pub const SYSCALL_SYSINFO: u64 = 99;
pub const SYSCALL_GETUID: u64 = 102;
pub const SYSCALL_SYSLOG: u64 = 103;
pub const SYSCALL_GETGID: u64 = 104;
pub const SYSCALL_SETUID: u64 = 105;
pub const SYSCALL_SETGID: u64 = 106;
pub const SYSCALL_GETEUID: u64 = 107;
pub const SYSCALL_GETEGID: u64 = 108;
pub const SYSCALL_SETPGID: u64 = 109;
pub const SYSCALL_GETPPID: u64 = 110;
pub const SYSCALL_GETPGRP: u64 = 111;
pub const SYSCALL_SETSID: u64 = 113;
pub const SYSCALL_SETREUID: u64 = 114;
pub const SYSCALL_SETREGID: u64 = 115;
pub const SYSCALL_GETGROUPS: u64 = 115;
pub const SYSCALL_SETGROUPS: u64 = 116;
pub const SYSCALL_SETRESUID: u64 = 117;
pub const SYSCALL_GETRESUID: u64 = 118;
pub const SYSCALL_GETPGID: u64 = 121;
pub const SYSCALL_SETFSUID: u64 = 122;
pub const SYSCALL_SETFSGID: u64 = 123;
pub const SYSCALL_GETSID: u64 = 124;
pub const SYSCALL_CAPGET: u64 = 125;
pub const SYSCALL_CAPSET: u64 = 126;
pub const SYSCALL_RT_SIGACTION: u64 = 134;
pub const SYSCALL_RT_SIGPROCMASK: u64 = 135;
pub const SYSCALL_RT_SIGPENDING: u64 = 136;
pub const SYSCALL_RT_SIGTIMEDWAIT: u64 = 137;
pub const SYSCALL_RT_SIGWAITINFO: u64 = 138;
pub const SYSCALL_SIGALTSTACK: u64 = 139;
pub const SYSCALL_UTIMENSAT: u64 = 140;
pub const SYSCALL_EOF: u64 = 0;
pub const SYSCALL_NR: u64 = 512;

#[derive(Copy, Clone)]
pub enum SyscallResult {
    Success(i64),
    Error(i64),
}

impl SyscallResult {
    pub fn is_error(&self) -> bool {
        match self {
            SyscallResult::Error(_) => true,
            _ => false,
        }
    }

    pub fn code(&self) -> i64 {
        match self {
            SyscallResult::Success(v) => *v,
            SyscallResult::Error(c) => *c,
        }
    }
}

pub static LAST_ERRNO: Mutex<i64> = Mutex::new(0);

pub fn set_errno(errno: i64) {
    *LAST_ERRNO.lock() = errno;
}

pub fn get_errno() -> i64 {
    *LAST_ERRNO.lock()
}

pub struct SyscallHandler;

impl SyscallHandler {
    pub const fn new() -> Self {
        SyscallHandler
    }

    pub fn handle(&self, nr: u64, args: [u64; 6]) -> SyscallResult {
        match nr {
            SYSCALL_WRITE => self.sys_write(args[0] as i32, args[1] as *const u8, args[2]),
            SYSCALL_READ => self.sys_read(args[0] as i32, args[1] as *mut u8, args[2]),
            SYSCALL_OPEN => self.sys_open(args[0] as *const i8, args[1] as u32, args[2] as i32),
            SYSCALL_CLOSE => self.sys_close(args[0] as i32),
            SYSCALL_EXIT => self.sys_exit(args[0] as i32),
            SYSCALL_GETPID => SyscallResult::Success(0),
            SYSCALL_GETUID => SyscallResult::Success(0),
            SYSCALL_GETGID => SyscallResult::Success(0),
            SYSCALL_GETEUID => SyscallResult::Success(0),
            SYSCALL_GETEGID => SyscallResult::Success(0),
            SYSCALL_GETPPID => SyscallResult::Success(0),
            _ => SyscallResult::Error(core::i64::MAX),
        }
    }

    fn sys_write(&self, _fd: i32, buf: *const u8, count: u64) -> SyscallResult {
        if buf == 0 as *const u8 {
            return SyscallResult::Error(9);
        }
        
        let _ = count;
        SyscallResult::Success(0)
    }

    fn sys_read(&self, _fd: i32, buf: *mut u8, count: u64) -> SyscallResult {
        if buf == 0 as *mut u8 {
            return SyscallResult::Error(9);
        }
        
        let _ = count;
        SyscallResult::Success(0)
    }

    fn sys_open(&self, path: *const i8, flags: u32, mode: i32) -> SyscallResult {
        if path == 0 as *const i8 {
            return SyscallResult::Error(9);
        }
        
        let _ = flags;
        let _ = mode;
        SyscallResult::Success(0)
    }

    fn sys_close(&self, fd: i32) -> SyscallResult {
        let _ = fd;
        SyscallResult::Success(0)
    }

    fn sys_exit(&self, status: i32) -> SyscallResult {
        println!("Processus termine avec status: {}", status);
        loop {}
    }
}

pub static mut SYSCALL_HANDLER: SyscallHandler = SyscallHandler::new();

pub fn init_syscalls() {
    println!("Interface syscalls: initialisee");
}

pub fn syscall(nr: u64, args: [u64; 6]) -> i64 {
    unsafe {
        SYSCALL_HANDLER.handle(nr, args).code()
    }
}