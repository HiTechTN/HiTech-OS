use core::mem::size_of;

pub const ELF_MAGIC: u32 = 0x464c457f;

pub const ELF_CLASS_32: u8 = 1;
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_LSB: u8 = 1;
pub const ELF_DATA_MSB: u8 = 2;
pub const ELF_VERSION: u8 = 1;
pub const ELF_TYPE_RELOCATABLE: u16 = 1;
pub const ELF_TYPE_EXECUTABLE: u16 = 2;
pub const ELF_TYPE_SHARED: u16 = 3;
pub const ELF_TYPE_CORE: u16 = 4;
pub const ELF_MACHINE_X86: u16 = 3;
pub const ELF_MACHINE_X86_64: u16 = 62;

#[derive(Copy, Clone)]
pub struct ElfHeader32 {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u32,
    pub e_phoff: u32,
    pub e_shoff: u32,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

impl ElfHeader32 {
    pub fn validate(&self) -> bool {
        self.e_ident[0] == 0x7f &&
        self.e_ident[1] == b'E' &&
        self.e_ident[2] == b'L' &&
        self.e_ident[3] == b'F' &&
        self.e_ident[4] == ELF_CLASS_32 &&
        self.e_ident[5] == ELF_DATA_LSB
    }

    pub fn program_header_count(&self) -> u16 {
        self.e_phnum
    }

    pub fn program_header_offset(&self) -> u32 {
        self.e_phoff
    }

    pub fn entry_point(&self) -> u32 {
        self.e_entry
    }
}

#[derive(Copy, Clone)]
pub struct ElfHeader64 {
    pub e_ident: [u8; 16],
    pub e_type: u16,
    pub e_machine: u16,
    pub e_version: u32,
    pub e_entry: u64,
    pub e_phoff: u64,
    pub e_shoff: u64,
    pub e_flags: u32,
    pub e_ehsize: u16,
    pub e_phentsize: u16,
    pub e_phnum: u16,
    pub e_shentsize: u16,
    pub e_shnum: u16,
    pub e_shstrndx: u16,
}

impl ElfHeader64 {
    pub fn validate(&self) -> bool {
        self.e_ident[0] == 0x7f &&
        self.e_ident[1] == b'E' &&
        self.e_ident[2] == b'L' &&
        self.e_ident[3] == b'F' &&
        self.e_ident[4] == ELF_CLASS_64 &&
        self.e_ident[5] == ELF_DATA_LSB
    }

    pub fn program_header_count(&self) -> u16 {
        self.e_phnum
    }

    pub fn program_header_offset(&self) -> u64 {
        self.e_phoff
    }

    pub fn entry_point(&self) -> u64 {
        self.e_entry
    }
}

#[derive(Copy, Clone)]
pub struct ProgramHeader32 {
    pub p_type: u32,
    pub p_offset: u32,
    pub p_vaddr: u32,
    pub p_paddr: u32,
    pub p_filesz: u32,
    pub p_memsz: u32,
    pub p_flags: u32,
    pub p_align: u32,
}

impl ProgramHeader32 {
    pub fn is_loadable(&self) -> bool {
        self.p_type == 1
    }

    pub fn is_executable(&self) -> bool {
        (self.p_flags & 1) != 0
    }

    pub fn is_writable(&self) -> bool {
        (self.p_flags & 2) != 0
    }

    pub fn is_readable(&self) -> bool {
        true
    }
}

#[derive(Copy, Clone)]
pub struct ProgramHeader64 {
    pub p_type: u32,
    pub p_flags: u32,
    pub p_offset: u64,
    pub p_vaddr: u64,
    pub p_paddr: u64,
    pub p_filesz: u64,
    pub p_memsz: u64,
    pub p_align: u64,
}

impl ProgramHeader64 {
    pub fn is_loadable(&self) -> bool {
        self.p_type == 1
    }

    pub fn is_executable(&self) -> bool {
        (self.p_flags & 1) != 0
    }

    pub fn is_writable(&self) -> bool {
        (self.p_flags & 2) != 0
    }

    pub fn is_readable(&self) -> bool {
        true
    }
}

pub const PT_LOAD: u32 = 1;
pub const PT_DYNAMIC: u32 = 2;
pub const PT_INTERP: u32 = 3;
pub const PT_NOTE: u32 = 4;
pub const PT_SHLIB: u32 = 5;
pub const PT_PHDR: u32 = 6;
pub const PT_TLS: u32 = 7;

pub const PF_X: u32 = 1;
pub const PF_W: u32 = 2;
pub const PF_R: u32 = 4;

pub struct ElfBinary {
    pub base: *mut u8,
    pub size: usize,
    pub entry: u64,
    pub is_64bit: bool,
    pub program_headers: [ProgramHeader64; 16],
    pub header_count: u8,
}

impl ElfBinary {
    pub const fn new() -> Self {
        ElfBinary {
            base: core::ptr::null_mut(),
            size: 0,
            entry: 0,
            is_64bit: false,
            program_headers: [ProgramHeader64 {
                p_type: 0,
                p_flags: 0,
                p_offset: 0,
                p_vaddr: 0,
                p_paddr: 0,
                p_filesz: 0,
                p_memsz: 0,
                p_align: 0,
            }; 16],
            header_count: 0,
        }
    }

    pub fn load(&mut self, data: &[u8]) -> Result<(), ElfError> {
        if data.len() < size_of::<ElfHeader64>() {
            return Err(ElfError::InvalidFormat);
        }

        let header = unsafe { &*(data.as_ptr() as *const ElfHeader64) };
        
        if !header.validate() {
            return Err(ElfError::InvalidFormat);
        }

        if header.e_ident[4] == ELF_CLASS_64 {
            self.is_64bit = true;
            self.entry = header.entry_point();
            self.header_count = header.program_header_count() as u8;
            
            let ph_offset = header.program_header_offset() as usize;
            for i in 0..self.header_count as usize {
                if ph_offset + i * size_of::<ProgramHeader64>() < data.len() {
                    let ph = unsafe {
                        *((data.as_ptr().wrapping_add(ph_offset).wrapping_add(i * size_of::<ProgramHeader64>())) 
                            as *const ProgramHeader64)
                    };
                    self.program_headers[i] = ph;
                }
            }
        } else {
            let header32 = unsafe { &*(data.as_ptr() as *const ElfHeader32) };
            self.is_64bit = false;
            self.entry = header32.entry_point() as u64;
            self.header_count = header32.program_header_count() as u8;
        }

        self.size = data.len();
        self.base = data.as_ptr() as *mut u8;
        
        Ok(())
    }

    pub fn get_loadable_segments(&self) -> &[ProgramHeader64] {
        &self.program_headers[..self.header_count as usize]
    }

    pub fn validate_permissions(&self) -> bool {
        for i in 0..self.header_count as usize {
            let ph = &self.program_headers[i];
            if ph.is_loadable() && ph.is_executable() && !ph.is_readable() {
                return false;
            }
        }
        true
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ElfError {
    InvalidFormat,
    InvalidSection,
    InvalidProgramHeader,
    UnsupportedType,
    RelocationFailed,
    SymbolNotFound,
}

pub static mut LOADED_ELF: ElfBinary = ElfBinary::new();

pub fn load_elf(data: &[u8]) -> Result<u64, ElfError> {
    unsafe {
        LOADED_ELF.load(data)?;
        Ok(LOADED_ELF.entry)
    }
}

pub fn get_loaded_elf() -> &'static ElfBinary {
    unsafe { &LOADED_ELF }
}

pub fn supports_pie() -> bool {
    true
}

pub fn resolve_symbol(_name: &str) -> Option<u64> {
    None
}