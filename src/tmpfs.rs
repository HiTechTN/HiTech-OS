use spin::Mutex;
use crate::vfs::FileType;
use alloc::vec::Vec;

pub const TMPFS_MAX_FILES: usize = 256;
pub const TMPFS_BLOCK_SIZE: usize = 4096;

#[derive(Copy, Clone)]
pub struct TmpfsData {
    pub data: [u8; TMPFS_BLOCK_SIZE],
    pub len: usize,
}

impl TmpfsData {
    pub const fn new() -> Self {
        TmpfsData {
            data: [0; TMPFS_BLOCK_SIZE],
            len: 0,
        }
    }
}

#[derive(Copy, Clone)]
pub struct TmpfsNode {
    pub inode: u32,
    pub file_type: FileType,
    pub size: u64,
    pub blocks: u32,
    pub uid: u32,
    pub gid: u32,
    pub mode: u16,
    pub atime: u64,
    pub mtime: u64,
    pub ctime: u64,
    pub data: [u8; TMPFS_BLOCK_SIZE],
    pub data_len: usize,
    pub name: [u8; 256],
    pub name_len: usize,
    pub entries: [Option<u32>; 64],
    pub entry_count: u8,
}

impl TmpfsNode {
    pub const fn new() -> Self {
        TmpfsNode {
            inode: 0,
            file_type: FileType::Regular,
            size: 0,
            blocks: 0,
            uid: 0,
            gid: 0,
            mode: 0o644,
            atime: 0,
            mtime: 0,
            ctime: 0,
            data: [0; TMPFS_BLOCK_SIZE],
            data_len: 0,
            name: [0; 256],
            name_len: 0,
            entries: [None; 64],
            entry_count: 0,
        }
    }

    pub fn set_name(&mut self, name: &str) {
        let len = name.len().min(255);
        self.name[..len].copy_from_slice(name.as_bytes());
        self.name_len = len;
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    pub fn write(&mut self, data: &[u8], offset: usize) -> Result<usize, ()> {
        let end = offset + data.len();
        if end > TMPFS_BLOCK_SIZE {
            return Err(());
        }
        let write_len = data.len();
        self.data[offset..end].copy_from_slice(data);
        if end > self.data_len {
            self.data_len = end;
        }
        self.size = self.data_len as u64;
        Ok(write_len)
    }

    pub fn read(&self, offset: usize, length: usize) -> &[u8] {
        let end = (offset + length).min(self.data_len);
        &self.data[offset..end]
    }
}

pub struct Tmpfs {
    pub nodes: [Option<TmpfsNode>; TMPFS_MAX_FILES],
    pub node_count: usize,
    pub mount_count: usize,
    pub mounted: bool,
}

impl Tmpfs {
    pub const fn new() -> Self {
        Tmpfs {
            nodes: [None; TMPFS_MAX_FILES],
            node_count: 0,
            mount_count: 0,
            mounted: false,
        }
    }

    pub fn init(&mut self) {
        let mut root = TmpfsNode::new();
        root.inode = 1;
        root.file_type = FileType::Directory;
        root.mode = 0o755;
        root.set_name("/");
        self.nodes[1] = Some(root);
        self.node_count = 2;
        self.mounted = true;
    }

    pub fn create(&mut self, parent_inode: u32, name: &str, file_type: FileType) -> Option<u32> {
        let inode = self.node_count;
        if inode >= TMPFS_MAX_FILES {
            return None;
        }

        let mut node = TmpfsNode::new();
        node.inode = inode as u32;
        node.file_type = file_type;
        node.set_name(name);
        self.nodes[inode] = Some(node);
        self.node_count += 1;

        if let Some(ref mut parent) = self.nodes[parent_inode as usize] {
            for entry in parent.entries.iter_mut() {
                if entry.is_none() {
                    *entry = Some(inode as u32);
                    parent.entry_count += 1;
                    break;
                }
            }
        }

        Some(inode as u32)
    }

    pub fn mkdir(&mut self, path: &str) -> Option<u32> {
        let parent = 1;
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let name = parts.last()?;
        self.create(parent, name, FileType::Directory)
    }

    pub fn touch(&mut self, path: &str) -> Option<u32> {
        let parent = 1;
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let name = parts.last()?;
        self.create(parent, name, FileType::Regular)
    }

    pub fn list_dir(&self, parent_inode: u32) -> Vec<(u32, alloc::string::String, FileType)> {
        let mut result = Vec::new();
        if let Some(ref parent) = self.nodes[parent_inode as usize] {
            for entry in parent.entries.iter() {
                if let Some(inode) = entry {
                    if let Some(ref node) = self.nodes[*inode as usize] {
                        let name = node.name_str();
                        let name_copy = alloc::string::String::from(name);
                        result.push((node.inode, name_copy, node.file_type));
                    }
                }
            }
        }
        result
    }

    pub fn write_file(&mut self, inode: u32, data: &[u8]) -> Result<usize, ()> {
        if let Some(ref mut node) = self.nodes[inode as usize] {
            node.write(data, 0)
        } else {
            Err(())
        }
    }

    pub fn read_file(&self, inode: u32) -> Option<&[u8]> {
        if let Some(ref node) = self.nodes[inode as usize] {
            if node.data_len > 0 {
                Some(&node.data[..node.data_len])
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn ls(&self) {
        println!("total {}", self.node_count);
        for i in 1..self.node_count {
            if let Some(ref node) = self.nodes[i] {
                let t = match node.file_type {
                    FileType::Directory => 'd',
                    _ => '-',
                };
                println!("{}rwxr-xr-x {:3} {:4} {}",
                    t, node.uid, node.size, node.name_str());
            }
        }
    }
}

pub static TMPFS: Mutex<Tmpfs> = Mutex::new(Tmpfs::new());

pub fn init_tmpfs() {
    TMPFS.lock().init();
    println!("tmpfs: monte");
}

pub mod shell_commands {
    use super::*;

    pub fn cmd_ls() {
        TMPFS.lock().ls();
    }

    pub fn cmd_touch(name: &str) {
        TMPFS.lock().touch(name);
    }

    pub fn cmd_mkdir(name: &str) {
        TMPFS.lock().mkdir(name);
    }

    pub fn cmd_cat(name: &str) {
        let parts: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
        let file_name = parts.last().map_or(name, |v| *v);
        for i in 1..TMPFS.lock().node_count {
            if let Some(ref node) = TMPFS.lock().nodes[i] {
                if node.name_str() == file_name {
                    if let Some(data) = TMPFS.lock().read_file(i as u32) {
                        println!("{}", core::str::from_utf8(data).unwrap_or(""));
                    }
                    return;
                }
            }
        }
        println!("Fichier non trouve: {}", file_name);
    }

    pub fn cmd_echo_to_file(name: &str, content: &str) {
        let parts: Vec<&str> = name.split('/').filter(|s| !s.is_empty()).collect();
        let file_name = parts.last().map_or(name, |v| *v);
        for i in 1..TMPFS.lock().node_count {
            if let Some(ref mut node) = TMPFS.lock().nodes[i] {
                if node.name_str() == file_name {
                    let _ = TMPFS.lock().write_file(i as u32, content.as_bytes());
                    return;
                }
            }
        }
        if let Some(inode) = TMPFS.lock().touch(file_name) {
            let _ = TMPFS.lock().write_file(inode, content.as_bytes());
        }
    }
}