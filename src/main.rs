#![no_std]
#![no_main]

use core::panic::PanicInfo;
use vibe_os::{println, VERSION, run_shell};

#[no_mangle]
pub extern "C" fn _start(boot_info: &'static bootloader::BootInfo) -> ! {
    println!("Vibe-OS {} - PRET", VERSION);

    vibe_os::memory::init_paging(boot_info);
    vibe_os::init();

    run_shell();

    loop {
        x86_64::instructions::hlt();
    }
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    loop {}
}