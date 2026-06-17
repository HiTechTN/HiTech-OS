
pub const MAX_DEVPTS: usize = 32;

#[derive(Copy, Clone)]
pub struct DevptsContext {
    pub devpts: u8,
    pub pts_name: [u8; 16],
    pub pts_name_len: usize,
    pub uid: u32,
    pub gid: u32,
    pub mode: u16,
    pub xterm_emulator: bool,
    pub input_buffer: [u8; 4096],
    pub input_len: usize,
    pub output_buffer: [u8; 4096],
    pub output_len: usize,
}

impl DevptsContext {
    pub fn new(devpts: u8) -> Self {
        let mut name = [0u8; 16];
        let name_str = alloc::string::String::from(format!("pts{}", devpts));
        let bytes = name_str.as_bytes();
        name[..bytes.len()].copy_from_slice(bytes);
        
        DevptsContext {
            devpts,
            pts_name: name,
            pts_name_len: bytes.len(),
            uid: 0,
            gid: 0,
            mode: 0o620,
            xterm_emulator: false,
            input_buffer: [0; 4096],
            input_len: 0,
            output_buffer: [0; 4096],
            output_len: 0,
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.pts_name[..self.pts_name_len]).unwrap_or("")
    }

    pub fn write(&mut self, data: &[u8]) -> usize {
        let write_len = data.len().min(4096 - self.input_len);
        self.input_buffer[self.input_len..self.input_len + write_len].copy_from_slice(&data[..write_len]);
        self.input_len += write_len;
        write_len
    }

    pub fn read(&mut self, buf: &mut [u8], offset: usize) -> usize {
        let remaining = self.input_len - offset;
        let read_len = remaining.min(buf.len());
        buf[..read_len].copy_from_slice(&self.input_buffer[offset..offset + read_len]);
        read_len
    }

    pub fn flush_output(&mut self) {
        let output = &self.output_buffer[..self.output_len];
        println!("[PTY {}] {}", self.devpts,
            core::str::from_utf8(output).unwrap_or(""));
        self.output_len = 0;
    }
}

pub struct DevptsFilesystem {
    pub pts: [Option<DevptsContext>; MAX_DEVPTS],
    pub pts_count: u8,
    pub mount_count: usize,
    pub mounted: bool,
}

impl DevptsFilesystem {
    pub const fn new() -> Self {
        DevptsFilesystem {
            pts: [None; MAX_DEVPTS],
            pts_count: 0,
            mount_count: 0,
            mounted: false,
        }
    }

    pub fn init(&mut self) {
        self.mounted = true;
        self.pts_count = 0;
    }

    pub fn allocate_pty(&mut self) -> Option<u8> {
        if self.pts_count >= MAX_DEVPTS as u8 {
            return None;
        }
        let devpts = self.pts_count;
        let context = DevptsContext::new(devpts);
        self.pts[devpts as usize] = Some(context);
        self.pts_count += 1;
        Some(devpts)
    }

    pub fn free_pty(&mut self, devpts: u8) -> bool {
        if devpts as usize >= MAX_DEVPTS {
            return false;
        }
        if self.pts[devpts as usize].is_some() {
            self.pts[devpts as usize] = None;
            for i in (devpts as usize)..self.pts_count as usize - 1 {
                self.pts[i] = self.pts[i + 1];
            }
            self.pts_count -= 1;
            true
        } else {
            false
        }
    }

    pub fn get_pty(&self, devpts: u8) -> Option<&DevptsContext> {
        if devpts as usize >= MAX_DEVPTS {
            return None;
        }
        self.pts[devpts as usize].as_ref()
    }

    pub fn get_pty_mut(&mut self, devpts: u8) -> Option<&mut DevptsContext> {
        if devpts as usize >= MAX_DEVPTS {
            return None;
        }
        self.pts[devpts as usize].as_mut()
    }

    pub fn list_ptys(&self) {
        println!("pts devices disponibles:");
        for i in 0..self.pts_count as usize {
            if let Some(ref devpts) = self.pts[i] {
                println!("  {} -> mode: {:o} uid: {} gid: {}",
                    devpts.name_str(), devpts.mode, devpts.uid, devpts.gid);
            }
        }
    }
}

pub static mut DEVPTS: DevptsFilesystem = DevptsFilesystem::new();

pub fn init_devpts() {
    unsafe { DEVPTS.init() };
    println!("devpts: monte sur /dev/pts");
}

pub mod shell_commands {
    use super::*;

    pub fn cmd_alloc() {
        unsafe {
            if let Some(devpts) = DEVPTS.allocate_pty() {
                println!("Alloue: {}", DevptsContext::new(devpts).name_str());
            } else {
                println!("Pas de PTY disponible");
            }
        }
    }

    pub fn cmd_free(name: &str) {
        unsafe {
            for i in 0..DEVPTS.pts_count as usize {
                if let Some(ref devpts) = DEVPTS.pts[i] {
                    if devpts.name_str() == name {
                        DEVPTS.free_pty(i as u8);
                        println!("Libere: {}", name);
                        return;
                    }
                }
            }
            println!("PTY non trouve: {}", name);
        }
    }

    pub fn cmd_ls() {
        unsafe { DEVPTS.list_ptys(); }
    }
}