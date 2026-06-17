#![no_std]
#![feature(abi_x86_interrupt)]
#![allow(static_mut_refs)]

#[macro_use]
extern crate alloc;

#[macro_use]
pub mod vga_buffer;
pub mod interrupts;
pub mod allocator;
pub mod keyboard;
pub mod shell;
pub mod timer;
pub mod vfs;
pub mod memory;
pub mod scheduler;
pub mod disk;
pub mod acpi;
pub mod network;
pub mod usb;
pub mod audio;
pub mod graphics;
pub mod mouse;
pub mod rtc;
pub mod pcie;
pub mod ahci;
pub mod elf;
pub mod syscall;
pub mod proc;
pub mod sysfs;
pub mod ext2;
pub mod uuid;
pub mod checksum;
pub mod compress;
pub mod tmpfs;
pub mod devpts;
pub mod iso9660;
pub mod efivar;
pub mod fat32;
pub mod ntfs;
pub mod crypto;
pub mod e1000;
pub mod ac97;
pub mod block;
pub mod pagecache;
pub mod init;
pub mod pipe;
pub mod signal;
pub mod console;
pub mod rng;
pub mod resource;
pub mod ramdisk;
pub mod dhcp;

pub const VERSION: &str = "0.8.0";
pub const BUILD_DATE: &str = "2026-06-17";

pub fn init() {
    println!("=== Vibe-OS {} ===", VERSION);
    println!("Initialisation du systeme...");

    allocator::init_heap();
    println!("[OK] Heap memoire");

    interrupts::init_idt();
    println!("[OK] IDT");

    timer::init();
    println!("[OK] Timer");

    unsafe { interrupts::PICS.lock().initialize() };
    println!("[OK] PIC");

    vfs::init_vfs();
    scheduler::init_scheduler();
    disk::init_ata();
    acpi::init_acpi();
    rtc::init_rtc();
    proc::init_proc();
    sysfs::init_sysfs();
    syscall::init_syscalls();
    uuid::init_uuid();
    ext2::init_ext2();
    ext2::mount_root();
    tmpfs::init_tmpfs();
    devpts::init_devpts();
    efivar::init_efivar();
    fat32::init_fat32();
    ntfs::init_ntfs();
    crypto::init_crypto();
    ramdisk::init_ramdisk();
    block::init_block_io();
    pagecache::init_page_cache();
    init::init_init();
    pipe::init_pipes();
    signal::init_signals();
    console::init_console();
    rng::init_rng();
    resource::init_resource();

    let _ = pcie::init_pcie();
    let _ = ahci::init_ahci();
    let _ = usb::init_usb();
    let _ = audio::init_audio();
    let _ = graphics::init_graphics();
    let _ = mouse::init_mouse();
    iso9660::init_iso9660();
    let _ = network::init_network();
    let _ = e1000::init_e1000();
    let _ = ac97::init_ac97();

    x86_64::instructions::interrupts::enable();
    println!("Interruptions activees");

    println!("=== Systeme pret! (v{}) ===", VERSION);
    println!("Fonctionnalites: heap, timer, PIC, IDT, VFS, scheduler, disk, ACPI");
    println!("  RTC, proc, sysfs, syscall, UUID, ext2, tmpfs, devpts, fat32, ntfs");
    println!("  efivar, pcie, ahci, usb, audio, graphics, mouse, iso9660, crypto");
    println!("  e1000, ac97, block, pagecache, init, bootlog");
    println!("  pipe, signal, console, rng, resource");
}

pub fn run_shell() {
    shell::Shell::new().run();
}