use core::mem::size_of;
use alloc::vec::Vec;

pub const EXT2_MAGIC: u16 = 0xef53;
pub const EXT2_ROOT_INO: u32 = 2;
pub const EXT2_BLOCK_SIZE: usize = 1024;

pub const EXT2_S_IFREG: u16 = 0x8000;
pub const EXT2_S_IFDIR: u16 = 0x4000;
pub const EXT2_S_IFLNK: u16 = 0xa000;
pub const EXT2_S_IRWXU: u16 = 0x01c0;
pub const EXT2_S_IRUSR: u16 = 0x0100;
pub const EXT2_S_IWUSR: u16 = 0x0080;
pub const EXT2_S_IXUSR: u16 = 0x0040;
pub const EXT2_S_IRWXG: u16 = 0x0038;
pub const EXT2_S_IRWXO: u16 = 0x0007;

pub const EXT2_FT_UNKNOWN: u8 = 0;
pub const EXT2_FT_REG_FILE: u8 = 1;
pub const EXT2_FT_DIR: u8 = 2;
pub const EXT2_FT_CHRDEV: u8 = 3;
pub const EXT2_FT_BLKDEV: u8 = 4;
pub const EXT2_FT_FIFO: u8 = 5;
pub const EXT2_FT_SOCK: u8 = 6;
pub const EXT2_FT_SYMLINK: u8 = 7;

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Ext2Superblock {
    pub inodes_count: u32,
    pub blocks_count: u32,
    pub r_blocks_count: u32,
    pub free_blocks_count: u32,
    pub free_inodes_count: u32,
    pub first_data_block: u32,
    pub log_block_size: u32,
    pub log_frag_size: u32,
    pub blocks_per_group: u32,
    pub frags_per_group: u32,
    pub inodes_per_group: u32,
    pub mtime: u32,
    pub wtime: u32,
    pub mnt_count: u16,
    pub max_mnt_count: u16,
    pub magic: u16,
    pub state: u16,
    pub errors: u16,
    pub minor_rev_level: u16,
    pub lastcheck: u32,
    pub checkinterval: u32,
    pub creator_os: u32,
    pub rev_level: u32,
    pub def_resuid: u16,
    pub def_resgid: u16,
    pub first_ino: u32,
    pub inode_size: u16,
    pub block_group_nr: u16,
    pub feature_compat: u32,
    pub feature_incompat: u32,
    pub feature_ro_compat: u32,
    pub uuid: [u8; 16],
    pub volume_name: [u8; 16],
    pub last_mounted: [u8; 64],
    pub algo_bitmap: u32,
    pub prealloc_blocks: u8,
    pub prealloc_dir_blocks: u8,
    pub padding: [u8; 6],
    pub journal_uuid: [u8; 16],
    pub journal_inum: u32,
    pub journal_dev: u32,
    pub last_orphan: u32,
    pub hash_seed: [u32; 4],
    pub def_hash_version: u8,
    pub padding2: [u8; 3],
    pub default_mount_options: u32,
    pub first_meta_bg: u32,
    pub reserved: [u8; 760],
}

impl Ext2Superblock {
    pub fn validate(&self) -> bool {
        self.magic == EXT2_MAGIC
    }

    pub fn block_size(&self) -> usize {
        1024 << self.log_block_size
    }

    pub fn inode_size(&self) -> usize {
        if self.rev_level >= 1 {
            self.inode_size as usize
        } else {
            128
        }
    }

    pub fn group_count(&self) -> u32 {
        (self.blocks_count + self.blocks_per_group - 1) / self.blocks_per_group
    }

    pub fn fragments_per_group(&self) -> u32 {
        self.blocks_per_group
    }

    pub fn num_groups(&self) -> u32 {
        (self.blocks_count + self.blocks_per_group - 1) / self.blocks_per_group as u32
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Ext2BlockGroupDesc {
    pub block_bitmap: u32,
    pub inode_bitmap: u32,
    pub inode_table: u32,
    pub free_blocks_count: u16,
    pub free_inodes_count: u16,
    pub used_dirs_count: u16,
    pub pad: u16,
    pub reserved: [u8; 12],
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Ext2Inode {
    pub mode: u16,
    pub uid: u16,
    pub size: u32,
    pub atime: u32,
    pub ctime: u32,
    pub mtime: u32,
    pub dtime: u32,
    pub gid: u16,
    pub links_count: u16,
    pub blocks: u32,
    pub flags: u32,
    pub osd1: u32,
    pub block: [u32; 15],
    pub generation: u32,
    pub file_acl: u32,
    pub dir_acl: u32,
    pub faddr: u32,
    pub osd2: [u8; 12],
}

impl Ext2Inode {
    pub fn is_dir(&self) -> bool {
        self.mode & EXT2_S_IFDIR != 0
    }

    pub fn is_file(&self) -> bool {
        self.mode & EXT2_S_IFREG != 0
    }

    pub fn is_symlink(&self) -> bool {
        self.mode & EXT2_S_IFLNK != 0
    }

    pub fn size(&self) -> u64 {
        self.size as u64
    }

    pub fn block_ptr(&self, index: u32) -> u32 {
        self.block[index as usize]
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct Ext2DirEntry {
    pub inode: u32,
    pub rec_len: u16,
    pub name_len: u8,
    pub file_type: u8,
}

impl Ext2DirEntry {
    pub fn name<'a>(&self, data: &'a [u8]) -> &'a str {
        let offset = size_of::<Ext2DirEntry>();
        let name_len = self.name_len as usize;
        core::str::from_utf8(&data[offset..offset + name_len]).unwrap_or("?")
    }
}

pub struct Ext2Filesystem {
    pub superblock: Option<Ext2Superblock>,
    pub block_group_descs: [Option<Ext2BlockGroupDesc>; 128],
    pub device: &'static str,
    pub block_size: usize,
    pub mounted: bool,
    pub read_only: bool,
}

impl Ext2Filesystem {
    pub const fn new() -> Self {
        Ext2Filesystem {
            superblock: None,
            block_group_descs: [None; 128],
            device: "",
            block_size: EXT2_BLOCK_SIZE,
            mounted: false,
            read_only: true,
        }
    }

    pub fn read_superblock(&mut self, data: &[u8]) -> bool {
        if data.len() < size_of::<Ext2Superblock>() {
            return false;
        }

        let sb = unsafe { &*(data.as_ptr() as *const Ext2Superblock) };
        if !sb.validate() {
            return false;
        }

        self.block_size = sb.block_size();

        let group_count = sb.num_groups();
        let bgd_offset = if self.block_size == 1024 { 2048 } else { self.block_size as u32 };

        for i in 0..group_count.min(128) {
            if (bgd_offset as usize + (i as usize * size_of::<Ext2BlockGroupDesc>())) < data.len() {
                let bgd = unsafe {
                    &*(data.as_ptr().add(bgd_offset as usize + i as usize * size_of::<Ext2BlockGroupDesc>()) as *const Ext2BlockGroupDesc)
                };
                self.block_group_descs[i as usize] = Some(*bgd);
            }
        }

        self.superblock = Some(*sb);
        self.mounted = true;
        true
    }

    pub fn get_block(&self, block_num: u32) -> u64 {
        block_num as u64 * self.block_size as u64
    }

    pub fn read_block<'a>(&self, data: &'a [u8], block_num: u32) -> Option<&'a [u8]> {
        let offset = self.get_block(block_num) as usize;
        if offset + self.block_size <= data.len() {
            Some(&data[offset..offset + self.block_size])
        } else {
            None
        }
    }

    pub fn read_inode(&self, data: &[u8], inode_num: u32) -> Option<Ext2Inode> {
        let sb = self.superblock.as_ref()?;
        let group = (inode_num - 1) / sb.inodes_per_group;
        let index = (inode_num - 1) % sb.inodes_per_group;

        let bgd = self.block_group_descs[group as usize].as_ref()?;
        let inode_table_block = bgd.inode_table;
        let inode_offset = inode_table_block as u64 * self.block_size as u64
            + index as u64 * sb.inode_size() as u64;

        if inode_offset as usize + size_of::<Ext2Inode>() <= data.len() {
            let inode = unsafe { &*(data.as_ptr().add(inode_offset as usize) as *const Ext2Inode) };
            Some(*inode)
        } else {
            None
        }
    }

    pub fn read_dir(&self, data: &[u8], inode_num: u32) -> Vec<(u32, alloc::string::String, u8)> {
        let mut entries = Vec::new();

        if let Some(inode) = self.read_inode(data, inode_num) {
            if !inode.is_dir() {
                return entries;
            }

            for i in 0..12 {
                let block_num = inode.block[i];
                if block_num == 0 {
                    break;
                }

                let block_data = match self.read_block(data, block_num) {
                    Some(d) => d,
                    None => break,
                };

                let mut offset = 0;
                while offset < block_data.len() {
                    if offset + size_of::<Ext2DirEntry>() > block_data.len() {
                        break;
                    }

                    let entry = unsafe { &*(block_data.as_ptr().add(offset) as *const Ext2DirEntry) };

                    if entry.inode == 0 {
                        break;
                    }

                    let name = entry.name(block_data);
                    let name_copy = alloc::string::String::from(name);
                    entries.push((entry.inode, name_copy, entry.file_type));

                    if entry.rec_len == 0 {
                        break;
                    }
                    offset += entry.rec_len as usize;
                }
            }
        }

        entries
    }
}

pub static mut EXT2: Ext2Filesystem = Ext2Filesystem::new();
pub static EXT2_DATA: spin::Mutex<Option<alloc::vec::Vec<u8>>> = spin::Mutex::new(None);

pub fn init_ext2() -> bool {
    println!("ext2: support compile");
    true
}

pub fn mount_root() -> bool {
    use core::sync::atomic::Ordering;
    if !crate::ramdisk::RAMDISK_PRESENT.load(Ordering::SeqCst) {
        println!("mount: ramdisk absent");
        return false;
    }

    let mut data = alloc::vec![0u8; crate::ramdisk::RAMDISK_SIZE];

    for block in 0..crate::ramdisk::BLOCK_COUNT {
        let mut buf = [0u8; crate::ramdisk::BLOCK_SIZE];
        if !crate::ramdisk::read_block(block, &mut buf) {
            println!("mount: erreur lecture bloc {}", block);
            return false;
        }
        let offset = block * crate::ramdisk::BLOCK_SIZE;
        data[offset..offset + crate::ramdisk::BLOCK_SIZE].copy_from_slice(&buf);
    }

    unsafe {
        if EXT2.read_superblock(&data) {
            *EXT2_DATA.lock() = Some(data);
            println!("  ext2 monte depuis ramdisk (racine inode 2)");
            true
        } else {
            println!("  ramdisk: pas de superblock ext2 valide");
            false
        }
    }
}

fn with_data<F, R>(f: F) -> R
where
    F: FnOnce(&[u8], &Ext2Filesystem) -> R,
{
    let guard = EXT2_DATA.lock();
    unsafe {
        if let Some(ref data) = *guard {
            f(data.as_slice(), &EXT2)
        } else {
            f(&[], &EXT2)
        }
    }
}

pub fn read_file(path: &str) -> Option<alloc::vec::Vec<u8>> {
    unsafe {
        if EXT2.superblock.is_none() {
            return None;
        }
    }
    let guard = EXT2_DATA.lock();
    let data = guard.as_ref()?;
    unsafe {
        let entries = EXT2.read_dir(data, EXT2_ROOT_INO);
        for (ino, name, ftype) in &entries {
            if ftype == &EXT2_FT_REG_FILE && name == path {
                if let Some(inode) = EXT2.read_inode(data, *ino) {
                    let mut file_data = alloc::vec::Vec::new();
                    for i in 0..12 {
                        let block_num = inode.block[i];
                        if block_num == 0 {
                            break;
                        }
                        if let Some(block_data) = EXT2.read_block(data, block_num) {
                            let sz = (inode.size() as usize).min(file_data.len() + block_data.len());
                            let remain = sz.saturating_sub(file_data.len());
                            file_data.extend_from_slice(&block_data[..remain]);
                            if file_data.len() >= inode.size() as usize {
                                break;
                            }
                        }
                    }
                    return Some(file_data);
                }
            }
        }
    }
    None
}

pub mod shell_commands {
    use super::*;

    pub fn cmd_mount() {
        if unsafe { EXT2.mounted } {
            println!("ext2: deja monte");
        } else {
            println!("ext2: pas de peripherique");
        }
    }

    pub fn cmd_umount() {
        println!("ext2: demonte");
    }

    pub fn cmd_fsck() {
        println!("ext2: fsck pas implemente");
    }

    pub fn cmd_tune2fs() {
        println!("ext2: tune2fs pas implemente");
    }

    pub fn cmd_resize2fs() {
        println!("ext2: resize2fs pas implemente");
    }

    pub fn cmd_debugfs() {
        println!("ext2: debugfs pas implemente");
    }

    pub fn cmd_ls() {
        with_data(|data, ext2| {
            if ext2.superblock.is_none() {
                println!("ext2: pas monte");
                return;
            }
            let entries = ext2.read_dir(data, EXT2_ROOT_INO);
            for (ino, name, ftype) in &entries {
                let t = match *ftype {
                    EXT2_FT_DIR => 'd',
                    EXT2_FT_REG_FILE => '-',
                    EXT2_FT_SYMLINK => 'l',
                    _ => '?',
                };
                print!("{} {:6} {}\n", t, ino, name);
            }
        });
    }

    pub fn cmd_cat(filename: &str) {
        with_data(|data, ext2| {
            if ext2.superblock.is_none() {
                println!("ext2: pas monte");
                return;
            }
            let entries = ext2.read_dir(data, EXT2_ROOT_INO);
            for (ino, name, ftype) in &entries {
                if ftype == &EXT2_FT_REG_FILE && name == filename {
                    if let Some(inode) = ext2.read_inode(data, *ino) {
                        let mut pos: usize = 0;
                        for i in 0..12 {
                            let block_num = inode.block[i];
                            if block_num == 0 {
                                break;
                            }
                            if let Some(block_data) = ext2.read_block(data, block_num) {
                                let sz = (inode.size() as usize).min(pos + block_data.len());
                                let end = sz.saturating_sub(pos);
                                print!("{}", core::str::from_utf8(&block_data[..end]).unwrap_or("???\n"));
                                pos += end;
                                if pos >= inode.size() as usize {
                                    break;
                                }
                            }
                        }
                        if pos == 0 {
                            println!("(fichier vide)");
                        }
                        return;
                    }
                }
            }
            println!("ext2: fichier introuvable: {}", filename);
        });
    }

    pub fn cmd_stat(path: &str) {
        println!("ext2: stat de {}", path);
    }
}