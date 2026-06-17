use alloc::vec::Vec;

pub const FAT32_EOC: u32 = 0x0ffffff8;
pub const FAT32_BAD: u32 = 0x0ffffff7;
pub const FAT32_FREE: u32 = 0x00000000;
pub const FAT32_ROOT_CLUSTER: u32 = 2;
pub const FAT32_BPB_SIZE: usize = 512;
pub const FAT32_MAX_FILE: usize = 256;
pub const FAT32_DIR_ENTRY_SIZE: usize = 32;

pub const FAT_ATTR_READ_ONLY: u8 = 0x01;
pub const FAT_ATTR_HIDDEN: u8 = 0x02;
pub const FAT_ATTR_SYSTEM: u8 = 0x04;
pub const FAT_ATTR_VOLUME_ID: u8 = 0x08;
pub const FAT_ATTR_DIRECTORY: u8 = 0x10;
pub const FAT_ATTR_ARCHIVE: u8 = 0x20;
pub const FAT_ATTR_LONG_NAME: u8 = 0x0f;

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Fat32BPB {
    pub jmp_boot: [u8; 3],
    pub oem_name: [u8; 8],
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
    pub sectors_per_fat_32: u32,
    pub ext_flags: u16,
    pub fs_version: u16,
    pub root_cluster: u32,
    pub fs_info: u16,
    pub bk_boot_sec: u16,
    pub reserved: [u8; 12],
    pub drive_number: u8,
    pub reserved1: u8,
    pub boot_signature: u8,
    pub volume_id: u32,
    pub volume_label: [u8; 11],
    pub fs_type: [u8; 8],
}

impl Fat32BPB {
    pub fn validate(&self) -> bool {
        self.bytes_per_sector == 512 && self.sectors_per_cluster > 0
    }

    pub fn total_sectors(&self) -> u32 {
        if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u32
        } else {
            self.total_sectors_32
        }
    }

    pub fn fat_size(&self) -> u32 {
        self.sectors_per_fat_32
    }

    pub fn first_data_sector(&self) -> u32 {
        self.reserved_sectors as u32 + self.num_fats as u32 * self.sectors_per_fat_32
    }

    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        self.first_data_sector() + (cluster - 2) * self.sectors_per_cluster as u32
    }

    pub fn data_clusters(&self) -> u32 {
        let data_sectors = self.total_sectors().saturating_sub(self.first_data_sector());
        data_sectors / self.sectors_per_cluster as u32
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct FatDirEntry {
    pub name: [u8; 11],
    pub attr: u8,
    pub nt_res: u8,
    pub create_time_tenth: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub access_date: u16,
    pub first_cluster_hi: u16,
    pub write_time: u16,
    pub write_date: u16,
    pub first_cluster_lo: u16,
    pub file_size: u32,
}

impl FatDirEntry {
    pub fn is_free(&self) -> bool {
        self.name[0] == 0xe5
    }

    pub fn is_end(&self) -> bool {
        self.name[0] == 0x00
    }

    pub fn is_directory(&self) -> bool {
        self.attr & FAT_ATTR_DIRECTORY != 0
    }

    pub fn is_volume(&self) -> bool {
        self.attr & FAT_ATTR_VOLUME_ID != 0
    }

    pub fn is_long_name(&self) -> bool {
        self.attr == FAT_ATTR_LONG_NAME
    }

    pub fn first_cluster(&self) -> u32 {
        (self.first_cluster_hi as u32) << 16 | self.first_cluster_lo as u32
    }

    pub fn name_str(&self) -> alloc::string::String {
        if self.is_long_name() {
            return alloc::string::String::from("<long>");
        }

        let base_end = self.name.iter().position(|&c| c == b' ').unwrap_or(8);
        let base = core::str::from_utf8(&self.name[..base_end]).unwrap_or("?");
        let mut s = alloc::string::String::from(base);

        if !self.is_directory() {
            let ext_end = self.name[8..].iter().position(|&c| c == b' ').unwrap_or(3);
            let ext = core::str::from_utf8(&self.name[8..8 + ext_end]).unwrap_or("");
            if !ext.is_empty() {
                s.push('.');
                s.push_str(ext);
            }
        }
        s
    }
}

pub struct Fat32Filesystem {
    pub bpb: Option<Fat32BPB>,
    pub fat: [u32; 65536],
    pub fat_entries: u32,
    pub mounted: bool,
    pub volume_label: alloc::string::String,
}

impl Fat32Filesystem {
    pub const fn new() -> Self {
        Fat32Filesystem {
            bpb: None,
            fat: [0; 65536],
            fat_entries: 0,
            mounted: false,
            volume_label: alloc::string::String::new(),
        }
    }

    pub fn read_bpb(&mut self, data: &[u8]) -> bool {
        if data.len() < 512 {
            return false;
        }

        let bpb = unsafe { &*(data.as_ptr() as *const Fat32BPB) };
        if !bpb.validate() {
            return false;
        }

        self.bpb = Some(*bpb);

        let fat_sectors = bpb.sectors_per_fat_32 as usize;
        let fat_start = bpb.reserved_sectors as usize;
        let fat_bytes = fat_sectors * 512;
        let fat_end = fat_start * 512 + fat_bytes.min(65536 * 4);

        if fat_end <= data.len() {
            let fat_data = &data[fat_start * 512..fat_end];
            let entries = fat_data.len() / 4;
            self.fat_entries = entries as u32;

            for i in 0..entries.min(65536) {
                self.fat[i] = u32::from_le_bytes(
                    fat_data[i * 4..i * 4 + 4].try_into().unwrap()
                );
            }
        }

        let label_slice = &bpb.volume_label;
        let label_end = label_slice.iter().position(|&c| c == 0 || c == b' ').unwrap_or(11);
        self.volume_label = alloc::string::String::from(
            core::str::from_utf8(&label_slice[..label_end]).unwrap_or("NO NAME")
        );

        self.mounted = true;
        true
    }

    pub fn get_next_cluster(&self, cluster: u32) -> u32 {
        if cluster as usize >= self.fat_entries as usize {
            return FAT32_EOC;
        }
        self.fat[cluster as usize]
    }

    pub fn is_eoc(&self, cluster: u32) -> bool {
        cluster >= FAT32_EOC
    }

    pub fn is_free(&self, cluster: u32) -> bool {
        cluster == FAT32_FREE
    }

    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        if let Some(bpb) = &self.bpb {
            bpb.cluster_to_sector(cluster)
        } else {
            0
        }
    }

    pub fn read_file(&self, data: &[u8], first_cluster: u32) -> Option<alloc::vec::Vec<u8>> {
        let mut result = alloc::vec::Vec::new();
        let mut cluster = first_cluster;

        while !self.is_eoc(cluster) {
            let sector = self.cluster_to_sector(cluster) as usize;
            let bps = self.bpb.as_ref()?.bytes_per_sector as usize;
            let spc = self.bpb.as_ref()?.sectors_per_cluster as usize;

            let offset = sector * bps;
            if offset + bps * spc > data.len() {
                break;
            }

            let end = (offset + bps * spc).min(data.len());
            result.extend_from_slice(&data[offset..end]);

            cluster = self.get_next_cluster(cluster);
        }

        Some(result)
    }

    pub fn ls_root(&self, data: &[u8]) -> Vec<(u32, alloc::string::String, u8)> {
        let mut entries = Vec::new();

        let root_sector = self.cluster_to_sector(FAT32_ROOT_CLUSTER) as usize;
        let bps = self.bpb.as_ref().map_or(512, |b| b.bytes_per_sector as usize);
        let spc = self.bpb.as_ref().map_or(1, |b| b.sectors_per_cluster as usize);

        if root_sector * bps + bps * spc > data.len() {
            return entries;
        }

        let dir_data = &data[root_sector * bps..root_sector * bps + bps * spc];
        let mut offset = 0;

        while offset + FAT32_DIR_ENTRY_SIZE <= dir_data.len() {
            let entry = unsafe { &*(dir_data.as_ptr().add(offset) as *const FatDirEntry) };

            if entry.is_end() {
                break;
            }

            if !entry.is_free() && !entry.is_volume() && !entry.is_long_name() {
                entries.push((
                    entry.first_cluster(),
                    entry.name_str(),
                    if entry.is_directory() { 2u8 } else { 1u8 },
                ));
            }

            offset += FAT32_DIR_ENTRY_SIZE;
        }

        entries
    }
}

pub static mut FAT32: Fat32Filesystem = Fat32Filesystem::new();

pub fn init_fat32() {
    println!("FAT32: support compile");
}

pub mod shell_commands {
    use super::*;

    pub fn cmd_ls() {
        let data: &[u8] = &[];
        unsafe {
            let entries = FAT32.ls_root(data);
            for (cluster, name, ftype) in &entries {
                let prefix = if *ftype == 2 { "d" } else { "-" };
                println!("{} {} {}", prefix, cluster, name);
            }
        }
    }

    pub fn cmd_mount() {
        println!("FAT32: montage");
    }

    pub fn cmd_umount() {
        println!("FAT32: demontage");
    }
}