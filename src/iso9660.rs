
pub const ISO_MAGIC: u32 = 0x43444950;
pub const ISO_VOLUME_PRIMARY: u8 = 1;
pub const ISO_VD_BOOT: u8 = 0;
pub const ISO_VD_PRIMARY: u8 = 1;
pub const ISO_VD_SUPPLEMENTARY: u8 = 2;
pub const ISO_VD_TERMINATOR: u8 = 255;
pub const ISO_SECTOR_SIZE: usize = 2048;
pub const ISO_DIR_RECORD_SIZE: usize = 33;
pub const ISO_MAX_VOLUMES: usize = 4;

pub const ISO_STANDARD_ID: [u8; 5] = *b"CD001";

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct IsoVolumeDescriptor {
    pub type_code: u8,
    pub standard_id: [u8; 5],
    pub version: u8,
    pub data: [u8; 2041],
}

impl IsoVolumeDescriptor {
    pub fn is_valid(&self) -> bool {
        self.standard_id == ISO_STANDARD_ID
    }

    pub fn is_primary(&self) -> bool {
        self.type_code == ISO_VD_PRIMARY
    }

    pub fn is_terminator(&self) -> bool {
        self.type_code == ISO_VD_TERMINATOR
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct IsoPrimaryDescriptor {
    pub type_code: u8,
    pub standard_id: [u8; 5],
    pub version: u8,
    pub _unused1: u8,
    pub system_id: [u8; 32],
    pub volume_id: [u8; 32],
    pub _unused2: [u8; 8],
    pub volume_space_size: u32,
    pub _unused3: [u8; 32],
    pub volume_set_size: u16,
    pub volume_sequence_number: u16,
    pub logical_block_size: u16,
    pub path_table_size: u32,
    pub path_table_lsb: u32,
    pub path_table_msb: u32,
    pub root_directory_record: [u8; 34],
    pub volume_set_id: [u8; 128],
    pub publisher_id: [u8; 128],
    pub preparer_id: [u8; 128],
    pub application_id: [u8; 128],
    pub copyright_file: [u8; 37],
    pub abstract_file: [u8; 37],
    pub bibliographic_file: [u8; 37],
    pub creation_date: [u8; 17],
    pub modification_date: [u8; 17],
    pub expiration_date: [u8; 17],
    pub effective_date: [u8; 17],
    pub file_structure_version: u8,
    pub _unused4: u8,
    pub application_data: [u8; 512],
    pub _reserved: [u8; 653],
}

impl IsoPrimaryDescriptor {
    pub fn volume_id_str(&self) -> &str {
        let len = self.volume_id.iter().position(|&c| c == 0).unwrap_or(32);
        core::str::from_utf8(&self.volume_id[..len]).unwrap_or("")
    }

    pub fn logical_block_size(&self) -> u16 {
        u16::from_le_bytes(self.logical_block_size.to_le_bytes())
    }

    pub fn volume_space_size(&self) -> u32 {
        u32::from_le_bytes(self.volume_space_size.to_le_bytes())
    }

    pub fn root_dir_extent(&self) -> u32 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.root_directory_record[2..6]);
        u32::from_le_bytes(bytes)
    }

    pub fn root_dir_length(&self) -> u32 {
        let mut bytes = [0u8; 4];
        bytes.copy_from_slice(&self.root_directory_record[10..14]);
        u32::from_le_bytes(bytes)
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct IsoDirectoryRecord {
    pub length: u8,
    pub ext_attr_length: u8,
    pub extent: [u8; 8],
    pub size: [u8; 8],
    pub date: [u8; 7],
    pub flags: u8,
    pub file_unit_size: u8,
    pub interleave_gap: u8,
    pub volume_sequence_number: [u8; 4],
    pub identifier_length: u8,
}

impl IsoDirectoryRecord {
    pub fn extent_lba(&self) -> u32 {
        u32::from_le_bytes([self.extent[0], self.extent[1], self.extent[2], self.extent[3]])
    }

    pub fn extent_size(&self) -> u32 {
        u32::from_le_bytes([self.size[0], self.size[1], self.size[2], self.size[3]])
    }

    pub fn is_directory(&self) -> bool {
        (self.flags & 2) != 0
    }

    pub fn identifier<'a>(&self, data: &'a [u8]) -> &'a str {
        let offset = size_of::<IsoDirectoryRecord>();
        let name_len = self.identifier_length as usize;
        let end = (offset + name_len).min(self.length as usize).min(data.len());
        if name_len > 0 && name_len <= 255 {
            let name_data = &data[offset..end];
            core::str::from_utf8(name_data).unwrap_or("?")
        } else {
            ""
        }
    }
}

use core::mem::size_of;

pub struct Iso9660Filesystem {
    pub mounted: bool,
    pub primary_desc: Option<IsoPrimaryDescriptor>,
    pub block_size: u32,
    pub volume_size: u32,
    pub volume_id: alloc::string::String,
}

impl Iso9660Filesystem {
    pub const fn new() -> Self {
        Iso9660Filesystem {
            mounted: false,
            primary_desc: None,
            block_size: ISO_SECTOR_SIZE as u32,
            volume_size: 0,
            volume_id: alloc::string::String::new(),
        }
    }

    pub fn read_volume(data: &[u8]) -> bool {
        if data.len() < 32 * 2048 {
            return false;
        }

        for sector in 16..32 {
            let offset = sector * ISO_SECTOR_SIZE;
            if offset + 2048 > data.len() {
                break;
            }

            let _vd: [u8; 7] = data[offset..offset + 7].try_into().unwrap();
            let standard_id = &data[offset + 1..offset + 6];
            
            if standard_id != ISO_STANDARD_ID.as_slice() {
                continue;
            }

            let type_code = data[offset];
            if type_code == ISO_VD_TERMINATOR {
                break;
            }
        }
        false
    }

    pub fn ls(&mut self) {
        if !self.mounted {
            println!("iso9660: pas monte");
            return;
        }
        if let Some(desc) = &self.primary_desc {
            println!("Volume: {}", desc.volume_id_str());
            println!("Block size: {}", desc.logical_block_size());
            println!("Volume size: {} blocks", desc.volume_space_size());
        }
    }
}

pub static mut ISO9660: Iso9660Filesystem = Iso9660Filesystem::new();

pub fn init_iso9660() {
    println!("iso9660: support compile");
}