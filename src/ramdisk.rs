use core::sync::atomic::{AtomicBool, Ordering};

pub const RAMDISK_SIZE: usize = 4 * 1024 * 1024;
pub const BLOCK_SIZE: usize = 512;
pub const BLOCK_COUNT: usize = RAMDISK_SIZE / BLOCK_SIZE;

pub static RAMDISK_PRESENT: AtomicBool = AtomicBool::new(false);

#[repr(align(4096))]
struct RamdiskBuffer {
    data: [u8; RAMDISK_SIZE],
}

static RAMDISK: RamdiskBuffer = RamdiskBuffer { data: [0; RAMDISK_SIZE] };

pub fn init_ramdisk() {
    RAMDISK_PRESENT.store(true, Ordering::SeqCst);
    println!("Ramdisk: {} Mo initialise", RAMDISK_SIZE / (1024 * 1024));
}

pub fn read_block(block: usize, buf: &mut [u8; BLOCK_SIZE]) -> bool {
    if block >= BLOCK_COUNT {
        return false;
    }
    let offset = block * BLOCK_SIZE;
    buf.copy_from_slice(&RAMDISK.data[offset..offset + BLOCK_SIZE]);
    true
}

pub fn write_block(block: usize, buf: &[u8; BLOCK_SIZE]) -> bool {
    if block >= BLOCK_COUNT {
        return false;
    }
    let offset = block * BLOCK_SIZE;
    let dst = &RAMDISK.data[offset..offset + BLOCK_SIZE];
    unsafe {
        core::ptr::copy_nonoverlapping(buf.as_ptr(), dst.as_ptr() as *mut u8, BLOCK_SIZE);
    }
    true
}

pub fn fill_pattern(byte: u8) {
    let slice = unsafe {
        core::slice::from_raw_parts_mut(&RAMDISK.data as *const u8 as *mut u8, RAMDISK_SIZE)
    };
    slice.fill(byte);
}

pub fn write_at(offset: usize, data: &[u8]) -> bool {
    if offset + data.len() > RAMDISK_SIZE {
        return false;
    }
    let dst = &RAMDISK.data[offset..offset + data.len()];
    unsafe {
        core::ptr::copy_nonoverlapping(data.as_ptr(), dst.as_ptr() as *mut u8, data.len());
    }
    true
}
