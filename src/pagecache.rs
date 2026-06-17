use spin::Mutex;

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_CACHE_SIZE: usize = 1024;
pub const CACHELINE_SIZE: usize = 64;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PageState {
    Clean,
    Dirty,
    Locked,
    Referenced,
    Active,
    Writeback,
}

#[derive(Copy, Clone)]
pub struct CachePage {
    pub data: [u8; PAGE_SIZE],
    pub block: u64,
    pub state: PageState,
    pub inode: u32,
    pub device: u8,
    pub referenced: bool,
    pub dirty: bool,
}

impl CachePage {
    pub const fn empty() -> Self {
        CachePage {
            data: [0; PAGE_SIZE],
            block: 0,
            state: PageState::Clean,
            inode: 0,
            device: 0,
            referenced: false,
            dirty: false,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.state != PageState::Clean || self.block != 0
    }

    pub fn mark_dirty(&mut self) {
        self.state = PageState::Dirty;
        self.dirty = true;
    }

    pub fn mark_clean(&mut self) {
        self.state = PageState::Clean;
        self.dirty = false;
    }

    pub fn clear(&mut self) {
        self.data = [0; PAGE_SIZE];
        self.block = 0;
        self.state = PageState::Clean;
        self.inode = 0;
        self.device = 0;
        self.referenced = false;
        self.dirty = false;
    }
}

pub struct PageCache {
    pub pages: [CachePage; PAGE_CACHE_SIZE],
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub writes: u64,
}

impl PageCache {
    pub const fn new() -> Self {
        PageCache {
            pages: [CachePage::empty(); PAGE_CACHE_SIZE],
            hits: 0,
            misses: 0,
            evictions: 0,
            writes: 0,
        }
    }

    fn hash(&self, device: u8, inode: u32, block: u64) -> usize {
        let mut h = device as u64 ^ (inode as u64) << 32 ^ block;
        h ^= h >> 16;
        h ^= h >> 8;
        (h as usize) % PAGE_CACHE_SIZE
    }

    pub fn lookup(&mut self, device: u8, inode: u32, block: u64) -> Option<&mut [u8; PAGE_SIZE]> {
        let index = self.hash(device, inode, block);
        let page = &mut self.pages[index];

        if page.device == device && page.inode == inode && page.block == block {
            page.referenced = true;
            self.hits += 1;
            Some(&mut page.data)
        } else {
            self.misses += 1;
            None
        }
    }

    pub fn insert(&mut self, device: u8, inode: u32, block: u64) -> &mut [u8; PAGE_SIZE] {
        let evict_index = self.find_victim();
        let page = &mut self.pages[evict_index];
        page.clear();
        page.device = device;
        page.inode = inode;
        page.block = block;
        page.state = PageState::Active;
        page.referenced = true;
        &mut page.data
    }

    pub fn insert_data(&mut self, device: u8, inode: u32, block: u64, data: &[u8]) {
        let page_data = self.insert(device, inode, block);
        let len = data.len().min(PAGE_SIZE);
        page_data[..len].copy_from_slice(&data[..len]);
    }

    pub fn mark_dirty(&mut self, device: u8, inode: u32, block: u64) -> bool {
        let index = self.hash(device, inode, block);
        let page = &mut self.pages[index];

        if page.device == device && page.inode == inode && page.block == block {
            page.mark_dirty();
            true
        } else {
            false
        }
    }

    pub fn flush_page(&mut self, index: usize) -> bool {
        let page = &mut self.pages[index];
        if page.dirty {
            page.mark_clean();
            self.writes += 1;
            true
        } else {
            false
        }
    }

    pub fn flush_all(&mut self) {
        for i in 0..PAGE_CACHE_SIZE {
            self.flush_page(i);
        }
    }

    pub fn flush_device(&mut self, device: u8) {
        for i in 0..PAGE_CACHE_SIZE {
            if self.pages[i].device == device {
                self.flush_page(i);
            }
        }
    }

    fn find_victim(&mut self) -> usize {
        let mut oldest = 0;

        for i in 0..PAGE_CACHE_SIZE {
            if self.pages[i].block == 0 {
                return i;
            }

            if !self.pages[i].referenced {
                if self.pages[i].state == PageState::Clean {
                    oldest = i;
                }
            } else {
                self.pages[i].referenced = false;
            }
        }

        self.evictions += 1;
        oldest
    }

    pub fn stats(&self) -> alloc::string::String {
        alloc::string::String::from(format!(
            "hits: {} misses: {} rate: {:.1}% evictions: {} writes: {}",
            self.hits, self.misses,
            if self.hits + self.misses > 0 {
                (self.hits as f64 / (self.hits + self.misses) as f64) * 100.0
            } else { 0.0 },
            self.evictions, self.writes
        ))
    }

    pub fn clear(&mut self) {
        for page in &mut self.pages {
            page.clear();
        }
        self.hits = 0;
        self.misses = 0;
        self.evictions = 0;
        self.writes = 0;
    }
}

pub static PAGE_CACHE: Mutex<PageCache> = Mutex::new(PageCache::new());

pub fn init_page_cache() {
    println!("Page cache: {} pages", PAGE_CACHE_SIZE);
}

pub mod readahead {
    pub const READAHEAD_WINDOW: u32 = 8;
    pub const READAHEAD_MAX: u32 = 256;

    pub struct ReadAheadState {
        pub window: u32,
        pub current: u64,
        pub prev: u64,
    }

    impl ReadAheadState {
        pub const fn new() -> Self {
            ReadAheadState { window: READAHEAD_WINDOW, current: 0, prev: 0 }
        }

        pub fn should_readahead(&self, block: u64) -> bool {
            block >= self.current && block < self.current + self.window as u64
        }

        pub fn update(&mut self, block: u64) {
            self.prev = self.current;
            self.current = block;
        }
    }
}

pub mod writeback {
    use super::*;
    use spin::Mutex;

    pub static WRITEBACK_INTERVAL: Mutex<u32> = Mutex::new(5000);
    pub static WRITEBACK_ACTIVE: Mutex<bool> = Mutex::new(false);

    pub fn periodic_flush() {
        PAGE_CACHE.lock().flush_all();
    }

    pub fn start_writeback() {
        *WRITEBACK_ACTIVE.lock() = true;
    }

    pub fn stop_writeback() {
        *WRITEBACK_ACTIVE.lock() = false;
    }

    pub fn is_writeback_active() -> bool {
        *WRITEBACK_ACTIVE.lock()
    }
}

pub mod vmscan {
    

    pub const SWAP_CLUSTER_MAX: u32 = 32;
    pub const SWAP_PREFETCH: u32 = 8;

    pub struct VmscanState {
        pub pages_scanned: u64,
        pub pages_freed: u64,
    }

    impl VmscanState {
        pub const fn new() -> Self {
            VmscanState { pages_scanned: 0, pages_freed: 0 }
        }

        pub fn scan(&mut self) -> u64 {
            self.pages_scanned += 1;
            0
        }

        pub fn reclaim(&mut self) -> bool {
            self.pages_freed += 1;
            true
        }
    }
}