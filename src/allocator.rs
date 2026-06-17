use core::alloc::{GlobalAlloc, Layout};

#[repr(align(4096))]
struct Heap {
    data: [u8; 0x100000],
}

static HEAP: Heap = Heap { data: [0; 0x100000] };

static ALLOCATOR: spin::Mutex<BumpAllocator> = spin::Mutex::new(BumpAllocator::new(0, 0));

pub struct GlobalAllocator;

unsafe impl GlobalAlloc for GlobalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATOR.lock().alloc(layout.size(), layout.align())
            .unwrap_or(core::ptr::null_mut())
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
    }
}

#[global_allocator]
static GLOBAL_ALLOCATOR: GlobalAllocator = GlobalAllocator;

pub struct BumpAllocator {
    start: usize,
    end: usize,
    current: usize,
}

impl BumpAllocator {
    pub const fn new(start: usize, end: usize) -> Self {
        BumpAllocator { start, end, current: start }
    }

    pub fn init(&mut self, start: usize, end: usize) {
        self.start = start;
        self.end = end;
        self.current = start;
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> Option<*mut u8> {
        let aligned = (self.current + align - 1) & !(align - 1);
        let new_end = aligned + size;

        if new_end > self.end {
            None
        } else {
            self.current = new_end;
            Some(aligned as *mut u8)
        }
    }
}

pub fn init_heap() {
    let start = HEAP.data.as_ptr() as usize;
    let end = start + HEAP.data.len();
    ALLOCATOR.lock().init(start, end);
}

pub fn allocator() -> &'static spin::Mutex<BumpAllocator> {
    &ALLOCATOR
}
