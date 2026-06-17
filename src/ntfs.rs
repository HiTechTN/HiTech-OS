use alloc::vec::Vec;

pub const NTFS_MFT_RECORD_SIZE: usize = 1024;
pub const NTFS_MFT_INODE: u64 = 0;
pub const NTFS_MFT_MIRR_INODE: u64 = 1;
pub const NTFS_LOG_FILE_INODE: u64 = 2;
pub const NTFS_VOLUME_INODE: u64 = 3;
pub const NTFS_ATTR_DEF_INODE: u64 = 4;
pub const NTFS_ROOT_INODE: u64 = 5;
pub const NTFS_BITMAP_INODE: u64 = 6;
pub const NTFS_BOOT_INODE: u64 = 7;
pub const NTFS_BADCLUS_INODE: u64 = 8;

pub const NTFS_MFT_ZONE: u64 = 0;
pub const NTFS_DATA_ZONE: u64 = 1;

pub const NTFS_FILE_NAME: u32 = 0x30;
pub const NTFS_DATA: u32 = 0x80;
pub const NTFS_INDEX_ROOT: u32 = 0x90;
pub const NTFS_INDEX_ALLOCATION: u32 = 0xa0;
pub const NTFS_BITMAP: u32 = 0xb0;
pub const NTFS_VOLUME_INFO: u32 = 0x70;
pub const NTFS_VOLUME_NAME: u32 = 0x60;

pub const ATTR_TYPE_STANDARD_INFORMATION: u32 = 0x10;
pub const ATTR_TYPE_ATTRIBUTE_LIST: u32 = 0x20;
pub const ATTR_TYPE_FILE_NAME: u32 = 0x30;
pub const ATTR_TYPE_OBJECT_ID: u32 = 0x40;
pub const ATTR_TYPE_SECURITY_DESCRIPTOR: u32 = 0x50;
pub const ATTR_TYPE_VOLUME_NAME: u32 = 0x60;
pub const ATTR_TYPE_VOLUME_INFORMATION: u32 = 0x70;
pub const ATTR_TYPE_DATA: u32 = 0x80;
pub const ATTR_TYPE_INDEX_ROOT: u32 = 0x90;
pub const ATTR_TYPE_INDEX_ALLOCATION: u32 = 0xa0;
pub const ATTR_TYPE_BITMAP: u32 = 0xb0;
pub const ATTR_TYPE_REPARSE_POINT: u32 = 0xc0;
pub const ATTR_TYPE_EA_INFORMATION: u32 = 0xd0;
pub const ATTR_TYPE_EA: u32 = 0xe0;
pub const ATTR_TYPE_PROPERTY_SET: u32 = 0xf0;
pub const ATTR_TYPE_LOGGED_UTILITY_STREAM: u32 = 0x100;

pub const ATTR_FLAG_COMPRESSED: u16 = 0x0001;
pub const ATTR_FLAG_ENCRYPTED: u16 = 0x4000;
pub const ATTR_FLAG_SPARSE: u16 = 0x8000;

pub const FILE_NAME_POSIX: u8 = 0;
pub const FILE_NAME_WIN32: u8 = 1;
pub const FILE_NAME_DOS: u8 = 2;
pub const FILE_NAME_WIN32_AND_DOS: u8 = 3;

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct NtfsBootRecord {
    pub jmp_boot: [u8; 3],
    pub oem_id: [u8; 8],
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub root_entries: u16,
    pub total_sectors_16: u16,
    pub media_descriptor: u8,
    pub sectors_per_fat_16: u16,
    pub sectors_per_track: u16,
    pub num_heads: u16,
    pub hidden_sectors: u32,
    pub total_sectors_32: u32,
    pub _unused: [u8; 8],
    pub total_sectors_64: u64,
    pub mft_lcn: u64,
    pub mft_mirr_lcn: u64,
    pub clusters_per_mft_record: i8,
    pub clusters_per_index_record: i8,
    pub volume_serial: u64,
    pub checksum: u32,
}

impl NtfsBootRecord {
    pub fn validate(&self) -> bool {
        &self.oem_id == b"NTFS    "
    }

    pub fn bytes_per_sector(&self) -> u32 {
        self.bytes_per_sector as u32
    }

    pub fn sectors_per_cluster(&self) -> u32 {
        self.sectors_per_cluster as u32
    }

    pub fn mft_lcn(&self) -> u64 {
        self.mft_lcn
    }

    pub fn mft_record_size(&self) -> u32 {
        let val = self.clusters_per_mft_record as i8;
        if val >= 0 {
            (1 << val) as u32
        } else {
            1 << (-val) as u32
        }
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct NtfsMftRecordHeader {
    pub magic: [u8; 4],
    pub usa_offset: u16,
    pub usa_count: u16,
    pub lsn: u64,
    pub sequence_number: u16,
    pub link_count: u16,
    pub attrs_offset: u16,
    pub flags: u16,
    pub bytes_used: u32,
    pub bytes_allocated: u32,
    pub base_mft_record: u64,
    pub next_attr_id: u16,
    pub record_number: u32,
}

impl NtfsMftRecordHeader {
    pub fn is_in_use(&self) -> bool {
        (self.flags & 0x0001) != 0
    }

    pub fn is_directory(&self) -> bool {
        (self.flags & 0x0002) != 0
    }

    pub fn validate(&self) -> bool {
        &self.magic == b"FILE"
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct NtfsAttrHeader {
    pub attr_type: u32,
    pub attr_length: u32,
    pub non_resident: u8,
    pub name_length: u8,
    pub name_offset: u16,
    pub flags: u16,
    pub attr_id: u16,
}

impl NtfsAttrHeader {
    pub fn is_resident(&self) -> bool {
        self.non_resident == 0
    }

    pub fn is_non_resident(&self) -> bool {
        self.non_resident != 0
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct NtfsResidentAttr {
    pub header: NtfsAttrHeader,
    pub value_length: u32,
    pub value_offset: u16,
    pub flags: u8,
    pub reserved: u8,
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct NtfsNonResidentAttr {
    pub header: NtfsAttrHeader,
    pub lowest_vcn: u64,
    pub highest_vcn: u64,
    pub mapping_pairs_offset: u16,
    pub compression_unit: u16,
    pub reserved: [u8; 4],
    pub allocated_size: u64,
    pub data_size: u64,
    pub initialized_size: u64,
    pub compressed_size: u64,
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct NtfsFileNameAttr {
    pub parent_directory: u64,
    pub creation_time: u64,
    pub last_change_time: u64,
    pub last_write_time: u64,
    pub last_access_time: u64,
    pub allocated_size: u64,
    pub data_size: u64,
    pub file_attributes: u32,
    pub ea_size: u32,
    pub file_name_length: u8,
    pub file_name_type: u8,
}

pub struct NtfsFilesystem {
    pub boot_record: Option<NtfsBootRecord>,
    pub mft_data: [u8; 65536],
    pub mounted: bool,
    pub volume_name: alloc::string::String,
    pub serial: u64,
}

impl NtfsFilesystem {
    pub const fn new() -> Self {
        NtfsFilesystem {
            boot_record: None,
            mft_data: [0; 65536],
            mounted: false,
            volume_name: alloc::string::String::new(),
            serial: 0,
        }
    }

    pub fn read_boot(&mut self, data: &[u8]) -> bool {
        if data.len() < 512 {
            return false;
        }

        let boot = unsafe { &*(data.as_ptr() as *const NtfsBootRecord) };
        if !boot.validate() {
            return false;
        }

        self.boot_record = Some(*boot);
        self.serial = boot.volume_serial;
        self.mounted = true;
        true
    }

    pub fn read_mft_record(&self, data: &[u8], record_num: u64) -> Option<NtfsMftRecordHeader> {
        let boot = self.boot_record.as_ref()?;
        let mft_lcn = boot.mft_lcn();
        let record_size = boot.mft_record_size() as u64;
        let bps = boot.bytes_per_sector() as u64;
        let spc = boot.sectors_per_cluster() as u64;

        let offset = mft_lcn * spc * bps + record_num * record_size;
        if offset as usize + NTFS_MFT_RECORD_SIZE > data.len() {
            return None;
        }

        let record = unsafe { &*(data.as_ptr().add(offset as usize) as *const NtfsMftRecordHeader) };
        if record.validate() {
            Some(*record)
        } else {
            None
        }
    }

    pub fn read_attr(&self, data: &[u8], mft_record: &NtfsMftRecordHeader, attr_type: u32) -> Option<Vec<u8>> {
        let mut offset = mft_record.attrs_offset as usize;
        let max_offset = mft_record.bytes_used as usize;

        while offset + 16 <= max_offset && offset <= data.len() {
            let attr = unsafe { &*(data.as_ptr().add(offset) as *const NtfsAttrHeader) };

            if attr.attr_type == 0xffffffff {
                break;
            }

            if attr.attr_type == attr_type {
                if attr.is_resident() {
                    let resident = unsafe { &*(data.as_ptr().add(offset) as *const NtfsResidentAttr) };
                    let value_offset = offset + resident.value_offset as usize;
                    let value_end = value_offset + resident.value_length as usize;
                    if value_end <= data.len() {
                        return Some(Vec::from(&data[value_offset..value_end]));
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }

            offset += attr.attr_length as usize;
        }

        None
    }

    pub fn read_file_name(&self, data: &[u8], mft_record: &NtfsMftRecordHeader) -> Option<alloc::string::String> {
        let attr_data = self.read_attr(data, mft_record, ATTR_TYPE_FILE_NAME)?;
        if attr_data.len() < 66 {
            return None;
        }
        let name_attr = unsafe { &*(attr_data.as_ptr() as *const NtfsFileNameAttr) };
        let name_len = name_attr.file_name_length as usize;
        let name_start = 66;

        if name_start + name_len > attr_data.len() {
            return None;
        }

        let name_bytes = &attr_data[name_start..name_start + name_len];
        let name = core::str::from_utf8(name_bytes).unwrap_or("?");
        Some(alloc::string::String::from(name))
    }

    pub fn ls_root(&self, data: &[u8]) -> Vec<(u64, alloc::string::String, u8)> {
        let mut entries = Vec::new();

        if let Some(_root_record) = self.read_mft_record(data, NTFS_ROOT_INODE) {
            for inode in 10..30 {
                if let Some(record) = self.read_mft_record(data, inode) {
                    if record.is_in_use() {
                        if let Some(name) = self.read_file_name(data, &record) {
                            entries.push((
                                inode,
                                name,
                                if record.is_directory() { 2u8 } else { 1u8 },
                            ));
                        }
                    }
                }
            }
        }

        entries
    }
}

pub static mut NTFS: NtfsFilesystem = NtfsFilesystem::new();

pub fn init_ntfs() {
    println!("NTFS: support compile (lecture)");
}

pub mod shell_commands {
    

    pub fn cmd_ls() {
        println!("NTFS: liste du repertoire racine");
    }

    pub fn cmd_mount() {
        println!("NTFS: montage");
    }
}