use spin::Mutex;
use alloc::vec::Vec;

pub const MAX_SYSFS_ENTRIES: usize = 256;

#[derive(Copy, Clone)]
pub struct SysfsEntry {
    pub name: [u8; 64],
    pub data: [u8; 256],
    pub data_len: usize,
    pub entry_type: SysfsType,
    pub attr: SysfsAttr,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SysfsType {
    File,
    Directory,
    Symlink,
    Device,
    Bus,
    Class,
}

#[derive(Copy, Clone)]
pub struct SysfsAttr {
    pub readable: bool,
    pub writable: bool,
    pub mode: u16,
}

impl SysfsEntry {
    pub fn new_file(name: &str, data: &str, mode: u16) -> Self {
        let mut n = [0u8; 64];
        let mut d = [0u8; 256];
        
        let name_len = name.len().min(63);
        let data_len = data.len().min(255);
        
        n[..name_len].copy_from_slice(name.as_bytes());
        d[..data_len].copy_from_slice(data.as_bytes());
        
        SysfsEntry {
            name: n,
            data: d,
            data_len,
            entry_type: SysfsType::File,
            attr: SysfsAttr {
                readable: true,
                writable: (mode & 0o200) != 0,
                mode,
            },
        }
    }

    pub fn new_dir(name: &str) -> Self {
        let mut n = [0u8; 64];
        let name_len = name.len().min(63);
        n[..name_len].copy_from_slice(name.as_bytes());
        
        SysfsEntry {
            name: n,
            data: [0; 256],
            data_len: 0,
            entry_type: SysfsType::Directory,
            attr: SysfsAttr {
                readable: true,
                writable: false,
                mode: 0o40555,
            },
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name).unwrap_or("")
    }

    pub fn data_str(&self) -> &str {
        core::str::from_utf8(&self.data[..self.data_len]).unwrap_or("")
    }
}

pub struct Sysfs {
    pub entries: [Option<SysfsEntry>; MAX_SYSFS_ENTRIES],
    pub entry_count: usize,
}

impl Sysfs {
    pub const fn new() -> Self {
        Sysfs {
            entries: [None; MAX_SYSFS_ENTRIES],
            entry_count: 0,
        }
    }

    pub fn init(&mut self) {
        self.entry_count = 0;
        
        self.add_entry(SysfsEntry::new_dir("bus"));
        self.add_entry(SysfsEntry::new_dir("class"));
        self.add_entry(SysfsEntry::new_dir("devices"));
        self.add_entry(SysfsEntry::new_dir("kernel"));
        self.add_entry(SysfsEntry::new_dir("module"));
        self.add_entry(SysfsEntry::new_dir("firmware"));
        
        self.add_entry(SysfsEntry::new_file("kernel/uevent", "FOO=bar\n", 0o644));
        self.add_entry(SysfsEntry::new_file("kernel/version", "Vibe-OS\n", 0o444));
    }

    fn add_entry(&mut self, entry: SysfsEntry) {
        if self.entry_count < MAX_SYSFS_ENTRIES {
            self.entries[self.entry_count] = Some(entry);
            self.entry_count += 1;
        }
    }

    pub fn lookup(&self, path: &str) -> Option<&SysfsEntry> {
        let parts: Vec<&str> = path.split('/').collect();
        
        for i in 0..self.entry_count {
            if let Some(ref entry) = self.entries[i] {
                let name = entry.name_str();
                if parts.len() == 1 && name == parts[0] {
                    return Some(entry);
                }
            }
        }
        None
    }

    pub fn ls(&self, path: &str) {
        println!("{}", path);
        
        for i in 0..self.entry_count {
            if let Some(ref entry) = self.entries[i] {
                let name = entry.name_str();
                if name.starts_with(path) && name.len() > path.len() {
                    let remaining = &name[path.len()..];
                    if !remaining.is_empty() && !remaining.contains('/') {
                        let t = match entry.entry_type {
                            SysfsType::Directory => "d",
                            _ => "-",
                        };
                        println!("{} {}", t, remaining);
                    }
                }
            }
        }
    }

    pub fn read(&self, path: &str) -> Option<alloc::string::String> {
        if let Some(entry) = self.lookup(path) {
            Some(alloc::string::String::from(entry.data_str()))
        } else {
            None
        }
    }

    pub fn write(&mut self, path: &str, data: &str) -> bool {
        for i in 0..self.entry_count {
            if let Some(ref mut entry) = self.entries[i] {
                let name = entry.name_str();
                if name == path {
                    let len = data.len().min(255);
                    entry.data[..len].copy_from_slice(&data.as_bytes()[..len]);
                    entry.data_len = len;
                    return true;
                }
            }
        }
        false
    }
}

pub static SYSFS: Mutex<Sysfs> = Mutex::new(Sysfs::new());

pub fn init_sysfs() {
    SYSFS.lock().init();
    println!("/sys: initialise");
}

pub mod class {
    

    pub fn net() -> alloc::string::String {
        alloc::string::String::from("net: empty\n")
    }

    pub fn block() -> alloc::string::String {
        alloc::string::String::from("block: empty\n")
    }

    pub fn fs() -> alloc::string::String {
        alloc::string::String::from("fs: ext2, proc\n")
    }

    pub fn input() -> alloc::string::String {
        alloc::string::String::from("input: event0\n")
    }

    pub fn sound() -> alloc::string::String {
        alloc::string::String::from("sound: card0\n")
    }

    pub fn graphics() -> alloc::string::String {
        alloc::string::String::from("graphics: fb0\n")
    }
}

pub mod devices {
    

    pub fn cpu() -> alloc::string::String {
        alloc::string::String::from("cpu0: online\n")
    }

    pub fn memory() -> alloc::string::String {
        let (mb, _) = crate::memory::get_physical_memory_info();
        alloc::string::String::from(format!("MemTotal: {} kB\nMemFree: {} kB\n", mb * 1024, mb * 512))
    }

    pub fn devices() -> alloc::string::String {
        alloc::string::String::from("1:0:0:0 ram\n")
    }
}

pub mod module {
    

    pub fn ls() -> alloc::string::String {
        alloc::string::String::from("Module Size  Used by:\n")
    }

    pub fn load(name: &str) -> bool {
        println!("Chargement module: {}", name);
        true
    }

    pub fn unload(name: &str) -> bool {
        println!("Depchargement module: {}", name);
        true
    }
}