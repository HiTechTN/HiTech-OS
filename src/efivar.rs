use spin::Mutex;

pub const EFI_SYSTEM_TABLE_PTR: *const u32 = 0x0001ffff as *const u32;
pub const EFI_RUNTIME_SERVICES: u64 = 0xfffffffffffe0000;
pub const EFI_MEMORY_MAP_SIZE: usize = 0x8000;

pub const EFI_VARIABLE_NON_VOLATILE: u32 = 0x00000001;
pub const EFI_VARIABLE_BOOTSERVICE_ACCESS: u32 = 0x00000002;
pub const EFI_VARIABLE_RUNTIME_ACCESS: u32 = 0x00000004;
pub const EFI_VARIABLE_HARDWARE_ERROR_RECORD: u32 = 0x00000008;
pub const EFI_VARIABLE_AUTHENTICATED_WRITE_ACCESS: u32 = 0x00000010;
pub const EFI_VARIABLE_TIME_BASED_AUTHENTICATED_WRITE_ACCESS: u32 = 0x00000020;
pub const EFI_VARIABLE_APPEND_WRITE: u32 = 0x00000040;
pub const EFI_VARIABLE_ENHANCED_AUTHENTICATED_ACCESS: u32 = 0x00000080;

#[derive(Copy, Clone)]
#[repr(C)]
pub struct EfiVariable {
    pub vendor_guid: [u8; 16],
    pub attributes: u32,
    pub data_size: usize,
    pub name_size: usize,
}

pub struct EfiVariableStore {
    pub present: bool,
    pub count: u32,
    pub variables: [Option<EfiVariableEntry>; 64],
}

#[derive(Copy, Clone)]
pub struct EfiVariableEntry {
    pub name: [u8; 256],
    pub name_len: usize,
    pub data: [u8; 4096],
    pub data_len: usize,
    pub guid: [u8; 16],
    pub attributes: u32,
}

impl EfiVariableEntry {
    pub fn new(name: &str, data: &[u8], guid: [u8; 16], attributes: u32) -> Self {
        let mut entry = EfiVariableEntry {
            name: [0; 256],
            name_len: name.len().min(255),
            data: [0; 4096],
            data_len: data.len().min(4096),
            guid,
            attributes,
        };
        entry.name[..entry.name_len].copy_from_slice(name.as_bytes());
        entry.data[..entry.data_len].copy_from_slice(&data[..entry.data_len]);
        entry
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

impl EfiVariableStore {
    pub const fn new() -> Self {
        EfiVariableStore {
            present: false,
            count: 0,
            variables: [None; 64],
        }
    }

    pub fn init(&mut self) {
        self.add_variable("BootOrder", &[0x00, 0x00, 0x00, 0x00],
            [0x8b, 0xe4, 0xdf, 0x61, 0x93, 0xca, 0x11, 0xd2, 0xaa, 0x0d, 0x00, 0xe0, 0x98, 0x03, 0x2b, 0x8c],
            EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS);

        self.add_variable("Boot0000", &[0x01, 0x00, 0x00, 0x00],
            [0x8b, 0xe4, 0xdf, 0x61, 0x93, 0xca, 0x11, 0xd2, 0xaa, 0x0d, 0x00, 0xe0, 0x98, 0x03, 0x2b, 0x8c],
            EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS);

        self.add_variable("Timeout", &[0x03],
            [0x8b, 0xe4, 0xdf, 0x61, 0x93, 0xca, 0x11, 0xd2, 0xaa, 0x0d, 0x00, 0xe0, 0x98, 0x03, 0x2b, 0x8c],
            EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS);

        self.present = true;
    }

    pub fn add_variable(&mut self, name: &str, data: &[u8], guid: [u8; 16], attributes: u32) {
        if self.count as usize >= 64 {
            return;
        }
        let entry = EfiVariableEntry::new(name, data, guid, attributes);
        self.variables[self.count as usize] = Some(entry);
        self.count += 1;
    }

    pub fn get_variable(&self, name: &str) -> Option<&EfiVariableEntry> {
        for i in 0..self.count as usize {
            if let Some(ref entry) = self.variables[i] {
                if entry.name_str() == name {
                    return Some(entry);
                }
            }
        }
        None
    }

    pub fn set_variable(&mut self, name: &str, data: &[u8], guid: [u8; 16], attributes: u32) {
        for i in 0..self.count as usize {
            if let Some(ref mut entry) = self.variables[i] {
                if entry.name_str() == name {
                    entry.data_len = data.len().min(4096);
                    entry.data[..entry.data_len].copy_from_slice(&data[..entry.data_len]);
                    entry.guid = guid;
                    entry.attributes = attributes;
                    return;
                }
            }
        }
        self.add_variable(name, data, guid, attributes);
    }

    pub fn delete_variable(&mut self, name: &str) -> bool {
        for i in 0..self.count as usize {
            if let Some(ref entry) = self.variables[i] {
                if entry.name_str() == name {
                    self.variables[i] = None;
                    for j in i..self.count as usize - 1 {
                        self.variables[j] = self.variables[j + 1];
                    }
                    self.count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn list_variables(&self) {
        println!("EFI Variables:");
        for i in 0..self.count as usize {
            if let Some(ref entry) = self.variables[i] {
                println!("  {} (size: {}) attr: {:x}",
                    entry.name_str(), entry.data_len, entry.attributes);
            }
        }
    }
}

pub static EFIVAR: Mutex<EfiVariableStore> = Mutex::new(EfiVariableStore::new());

pub fn init_efivar() {
    EFIVAR.lock().init();
    println!("efivar: EFI variable support");
}

pub mod shell_commands {
    use super::*;

    pub fn cmd_list() {
        EFIVAR.lock().list_variables();
    }

    pub fn cmd_get(name: &str) {
        if let Some(entry) = EFIVAR.lock().get_variable(name) {
                println!("{} = {} (size: {})",
                    entry.name_str(),
                    core::str::from_utf8(&entry.data[..entry.data_len.min(64)]).unwrap_or("?"),
                    entry.data_len);
            } else {
                println!("Variable non trouvee: {}", name);
            }
    }

    pub fn cmd_set(name: &str, value: &str) {
        EFIVAR.lock().set_variable(name, value.as_bytes(),
                [0; 16],
                EFI_VARIABLE_BOOTSERVICE_ACCESS | EFI_VARIABLE_RUNTIME_ACCESS);
        println!("Variable {} = {}", name, value);
    }

    pub fn cmd_delete(name: &str) {
        if EFIVAR.lock().delete_variable(name) {
                println!("Variable {} supprimee", name);
            } else {
                println!("Variable non trouvee: {}", name);
            }
    }
}

pub mod efi_time {
    

    pub fn get_year() -> u16 {
        2026
    }

    pub fn get_month() -> u8 {
        4
    }

    pub fn get_day() -> u8 {
        26
    }

    pub fn get_hour() -> u8 {
        12
    }

    pub fn get_minute() -> u8 {
        0
    }

    pub fn get_second() -> u8 {
        0
    }

    pub fn set_time(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) {
        let _ = (year, month, day, hour, minute, second);
    }
}