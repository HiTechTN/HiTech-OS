use spin::Mutex;

pub const MAX_NAME_LEN: usize = 256;
pub const MAX_INODES: usize = 64;
pub const MAX_OPEN_FILES: usize = 16;
pub const BLOCK_SIZE: usize = 512;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FileType {
    Regular,
    Directory,
    CharDevice,
    BlockDevice,
    Pipe,
    Symlink,
}

#[derive(Copy, Clone)]
pub struct DirEntry {
    pub name: [u8; MAX_NAME_LEN],
    pub name_len: usize,
    pub inode: u32,
    pub file_type: FileType,
    pub size: u64,
    pub permissions: u16,
}

impl DirEntry {
    pub fn new(name: &str, inode: u32, file_type: FileType) -> Self {
        let mut n = [0u8; MAX_NAME_LEN];
        let len = name.len().min(MAX_NAME_LEN);
        n[..len].copy_from_slice(name.as_bytes()[..len].as_ref());
        DirEntry {
            name: n,
            name_len: len,
            inode,
            file_type,
            size: 0,
            permissions: 0o644,
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }
}

#[derive(Copy, Clone)]
pub struct Inode {
    pub inode: u32,
    pub file_type: FileType,
    pub size: u64,
    pub blocks: u32,
    pub atime: u32,
    pub mtime: u32,
    pub data: [u8; 128],
}

impl Inode {
    pub fn new(inode: u32, file_type: FileType) -> Self {
        Inode {
            inode,
            file_type,
            size: 0,
            blocks: 0,
            atime: 0,
            mtime: 0,
            data: [0; 128],
        }
    }
}

#[derive(Copy, Clone)]
pub struct FileDescriptor {
    pub inode: u32,
    pub offset: u64,
    pub flags: u32,
    pub status: FileStatus,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum FileStatus {
    Closed,
    ReadOnly,
    WriteOnly,
    ReadWrite,
}

impl FileDescriptor {
    pub fn new(inode: u32) -> Self {
        FileDescriptor {
            inode,
            offset: 0,
            flags: 0,
            status: FileStatus::ReadOnly,
        }
    }
}

pub struct VirtualFileSystem {
    pub inodes: [Option<Inode>; MAX_INODES],
    pub next_inode: u32,
    pub root_inode: u32,
    pub fd_table: [Option<FileDescriptor>; MAX_OPEN_FILES],
    pub next_fd: u32,
}

impl VirtualFileSystem {
    pub const fn new() -> Self {
        VirtualFileSystem {
            inodes: [None; MAX_INODES],
            next_inode: 1,
            root_inode: 1,
            fd_table: [None; MAX_OPEN_FILES],
            next_fd: 0,
        }
    }

    pub fn init(&mut self) {
        self.inodes[1] = Some(Inode::new(1, FileType::Directory));
        self.root_inode = 1;
    }

    pub fn create(&mut self, _name: &str, file_type: FileType) -> Option<u32> {
        let inode = self.next_inode;
        self.next_inode += 1;
        
        if inode as usize >= MAX_INODES {
            return None;
        }
        
        self.inodes[inode as usize] = Some(Inode::new(inode, file_type));
        Some(inode)
    }

    pub fn open(&mut self, inode: u32) -> Option<u32> {
        if inode as usize >= MAX_INODES || self.inodes[inode as usize].is_none() {
            return None;
        }
        
        let fd = self.next_fd;
        self.next_fd += 1;
        
        if fd as usize >= MAX_OPEN_FILES {
            return None;
        }
        
        self.fd_table[fd as usize] = Some(FileDescriptor::new(inode));
        Some(fd)
    }

    pub fn close(&mut self, fd: u32) -> bool {
        if fd as usize >= MAX_OPEN_FILES {
            return false;
        }
        
        self.fd_table[fd as usize] = None;
        true
    }

    pub fn get_inode(&self, inode: u32) -> Option<&Inode> {
        self.inodes.get(inode as usize).and_then(|i| i.as_ref())
    }
}

pub static VFS: Mutex<VirtualFileSystem> = Mutex::new(VirtualFileSystem::new());

pub fn init_vfs() {
    VFS.lock().init();
    println!("VFS initialise");
}

pub mod shell_commands {
    use super::*;

    pub fn cmd_ls() {
        let vfs = VFS.lock();
        
        if let Some(inode) = vfs.get_inode(vfs.root_inode) {
            println!("Total: {} octets", inode.size);
        }
        
        println!("drwxr-xr-x  2 root root  4096 .");
        println!("drwxr-xr-x  2 root  4096 ..");
        println!("-rw-r--r--  1 root root     0 dev/");
        println!("-rw-r--r--  1 root root     0 etc/");
        println!("-rw-r--r--  1 root root     0 proc/");
    }

    pub fn cmd_cat(args: &[&str]) {
        if args.is_empty() {
            println!("Usage: cat <fichier>");
            return;
        }
        
        let filename = args[0];
        
        if filename.starts_with('/') {
            println!("chemin absolu non supporte");
        } else {
            println!("Fichier non trouve: {}", filename);
        }
    }

    pub fn cmd_mkdir(args: &[&str]) {
        if args.is_empty() {
            println!("Usage: mkdir <repertoire>");
            return;
        }
        
        let mut vfs = VFS.lock();
        
        match vfs.create(args[0], FileType::Directory) {
            Some(_) => println!("Repertoire cree: {}", args[0]),
            None => println!("Erreur: creation impossible"),
        }
    }

    pub fn cmd_touch(args: &[&str]) {
        if args.is_empty() {
            println!("Usage: touch <fichier>");
            return;
        }
        
        let mut vfs = VFS.lock();
        
        match vfs.create(args[0], FileType::Regular) {
            Some(_) => println!("Fichier cree: {}", args[0]),
            None => println!("Erreur: creation impossible"),
        }
    }

    pub fn cmd_rm(args: &[&str]) {
        if args.is_empty() {
            println!("Usage: rm <fichier>");
            return;
        }
        println!("Suppression non implementee");
    }

    pub fn cmd_df() {
        println!("Syst. de fichiers    1K-blocks   Used Available Use% Montedans");
        println!("/dev/sda1              10240      512      9728   5% /");
    }
}