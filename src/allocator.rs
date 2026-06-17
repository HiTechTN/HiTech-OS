use core::alloc::{GlobalAlloc, Layout};
use core::mem::size_of;
use core::ptr;

#[repr(align(4096))]
struct Heap {
    data: [u8; 0x100000],
}

static HEAP: Heap = Heap { data: [0; 0x100000] };

static ALLOCATOR: spin::Mutex<HeapAllocator> = spin::Mutex::new(HeapAllocator::new());

const MAGIC_FREE: u32 = 0xDEADBEEF;
const MAGIC_USED: u32 = 0xC0FFEE;
const MIN_BLOCK_SIZE: usize = 32;
const HEADER_SIZE: usize = size_of::<BlockHeader>();

#[repr(C)]
struct BlockHeader {
    size: usize,
    magic: u32,
    next: *mut BlockHeader,
    prev: *mut BlockHeader,
}

unsafe impl Send for HeapAllocator {}

pub struct HeapAllocator {
    start: usize,
    end: usize,
    free_head: *mut BlockHeader,
    initialized: bool,
}

pub struct GlobalAllocator;

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATOR.lock().alloc(layout.size(), layout.align())
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        ALLOCATOR.lock().dealloc(ptr, layout.size())
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

impl HeapAllocator {
    pub const fn new() -> Self {
        HeapAllocator {
            start: 0,
            end: 0,
            free_head: ptr::null_mut(),
            initialized: false,
        }
    }

    pub fn init(&mut self, start: usize, end: usize) {
        self.start = start;
        self.end = end;
        let total_size = end - start;
        if total_size >= MIN_BLOCK_SIZE + HEADER_SIZE {
            let block = start as *mut BlockHeader;
            unsafe {
                (*block).size = total_size;
                (*block).magic = MAGIC_FREE;
                (*block).next = ptr::null_mut();
                (*block).prev = ptr::null_mut();
            }
            self.free_head = block;
        }
        self.initialized = true;
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> *mut u8 {
        if !self.initialized {
            return ptr::null_mut();
        }
        let adjusted = if size < MIN_BLOCK_SIZE { MIN_BLOCK_SIZE } else { size };

        let mut curr = self.free_head;
        while !curr.is_null() {
            unsafe {
                let block_size = (*curr).size;
                let block_addr = curr as usize;
                let data_addr = block_addr + HEADER_SIZE;
                let aligned_data = (data_addr + align - 1) & !(align - 1);
                let total_used = (aligned_data - block_addr) + adjusted;

                if total_used <= block_size {
                    let remaining = block_size - total_used;
                    if remaining >= MIN_BLOCK_SIZE + HEADER_SIZE {
                        let next_block = (block_addr + total_used) as *mut BlockHeader;
                        (*next_block).size = remaining;
                        (*next_block).magic = MAGIC_FREE;
                        (*next_block).next = (*curr).next;
                        (*next_block).prev = (*curr).prev;
                        if !(*curr).next.is_null() {
                            (*(*curr).next).prev = next_block;
                        }
                        self.free_head = next_block;
                        (*curr).size = total_used;
                    } else {
                        self.remove_free(curr);
                    }
                    (*curr).magic = MAGIC_USED;
                    return aligned_data as *mut u8;
                }
            }
            unsafe { curr = (*curr).next; }
        }
        ptr::null_mut()
    }

    pub fn dealloc(&mut self, ptr: *mut u8, _size: usize) {
        if ptr.is_null() { return; }
        let block = unsafe { (ptr as *mut u8).sub(HEADER_SIZE) as *mut BlockHeader };
        unsafe {
            if (*block).magic != MAGIC_USED { return; }
            (*block).magic = MAGIC_FREE;
        }
        self.insert_free(block);
    }

    fn remove_free(&mut self, block: *mut BlockHeader) {
        unsafe {
            if (*block).prev.is_null() {
                self.free_head = (*block).next;
            } else {
                (*(*block).prev).next = (*block).next;
            }
            if !(*block).next.is_null() {
                (*(*block).next).prev = (*block).prev;
            }
        }
    }

    fn insert_free(&mut self, block: *mut BlockHeader) {
        let addr = block as usize;
        let mut curr = self.free_head;
        let mut prev: *mut BlockHeader = ptr::null_mut();

        while !curr.is_null() && (curr as usize) < addr {
            unsafe { prev = curr; curr = (*curr).next; }
        }

        unsafe {
            (*block).next = curr;
            (*block).prev = prev;
            if prev.is_null() {
                self.free_head = block;
            } else {
                (*prev).next = block;
            }
            if !curr.is_null() {
                (*curr).prev = block;
            }
        }

        self.coalesce(block);
    }

    fn coalesce(&mut self, block: *mut BlockHeader) {
        let addr = block as usize;
        unsafe {
            if !(*block).next.is_null() {
                let next_addr = (*block).next as usize;
                if addr + (*block).size == next_addr {
                    let next = (*block).next;
                    (*block).size += (*next).size;
                    (*block).next = (*next).next;
                    if !(*next).next.is_null() {
                        (*(*next).next).prev = block;
                    }
                }
            }
            if !(*block).prev.is_null() {
                let prev = (*block).prev;
                let prev_addr = prev as usize;
                if prev_addr + (*prev).size == addr {
                    (*prev).size += (*block).size;
                    (*prev).next = (*block).next;
                    if !(*block).next.is_null() {
                        (*(*block).next).prev = prev;
                    }
                }
            }
        }
    }
}

pub fn init_heap() {
    let start = HEAP.data.as_ptr() as usize;
    let end = start + HEAP.data.len();
    ALLOCATOR.lock().init(start, end);
}

pub fn allocator() -> &'static spin::Mutex<HeapAllocator> {
    &ALLOCATOR
}
