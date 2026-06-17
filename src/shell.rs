use crate::keyboard::read_key;
use alloc::vec::Vec;

const PROMPT: &str = "vibeOS> ";

pub struct Shell;

impl Shell {
    pub fn new() -> Self {
        Shell
    }

    pub fn run(&mut self) {
        println!("\nBienvenue sur Vibe-OS {}", crate::VERSION);
        println!("Tapez 'help' pour les commandes\n");
        
        loop {
            print!("{}", PROMPT);
            if self.read_and_execute() {
                break;
            }
        }
    }

    fn read_and_execute(&mut self) -> bool {
        let mut buffer = [0u8; 256];
        let mut pos = 0;
        
        loop {
            if let Some(c) = read_key() {
                match c {
                    b'\n' => break,
                    8 | 127 => {
                        if pos > 0 { pos -= 1; }
                    }
                    b if pos < 255 => {
                        buffer[pos] = b;
                        pos += 1;
                    }
                    _ => {}
                }
            }
        }
        
        if pos == 0 {
            return false;
        }
        
        let slice = &buffer[..pos];
        self.execute(slice);
        false
    }

    fn execute(&mut self, command: &[u8]) {
        let first = command.iter()
            .position(|&b| b == b' ')
            .map(|i| &command[..i])
            .unwrap_or(command);
        
        match first {
            b"help" => self.cmd_help(),
            b"clear" => self.cmd_clear(),
            b"echo" => self.cmd_echo(command),
            b"info" => self.cmd_info(),
            b"mem" => self.cmd_mem(),
            b"date" => self.cmd_date(),
            b"time" => self.cmd_time(),
            b"uptime" => self.cmd_uptime(),
            b"reboot" => self.cmd_reboot(),
            b"shutdown" => self.cmd_shutdown(),
            b"ls" => self.cmd_ls(),
            b"cat" => self.cmd_file(command, "cat"),
            b"mkdir" => self.cmd_file(command, "mkdir"),
            b"touch" => self.cmd_file(command, "touch"),
            b"rm" => self.cmd_file(command, "rm"),
            b"df" => self.cmd_df(),
            b"ps" => self.cmd_ps(),
            b"kill" => self.cmd_kill(command),
            b"ifconfig" => self.cmd_ifconfig(),
            b"ping" => self.cmd_ping(),
            b"lspci" => self.cmd_lspci(),
            b"lsusb" => self.cmd_lsusb(),
            b"test" => self.cmd_test(),
            b"beep" => self.cmd_beep(),
            b"clearcolor" => self.cmd_clearcolor(),
            b"draw" => self.cmd_draw(),
            b"exit" => self.cmd_exit(),
            _ => {
                let s = core::str::from_utf8(first).unwrap_or("?");
                println!("Commande inconnue: {}", s);
            }
        }
    }

    fn cmd_help(&self) {
        println!("Commandes disponibles:");
        println!("  Systeme: help, clear, info, mem, date, time, uptime, reboot, shutdown, exit");
        println!("  Fichiers: ls, cat, mkdir, touch, rm, df");
        println!("  Processus: ps, kill");
        println!("  Reseau: ifconfig, ping");
        println!("  Materiel: lspci, lsusb");
        println!("  Audio: beep, test");
        println!("  Graphique: clearcolor, draw");
    }

    fn cmd_clear(&self) {
        use crate::vga_buffer::{Writer, ColorCode, Color, BUFFER_HEIGHT};
        
        let color_code = ColorCode::new(Color::Yellow, Color::Black);
        let mut writer = Writer {
            column_position: 0,
            color_code,
            buffer: unsafe { &mut *(0xb8000 as *mut crate::vga_buffer::Buffer) },
        };
        
        for _ in 0..BUFFER_HEIGHT {
            writer.new_line();
        }
    }

    fn cmd_echo(&self, command: &[u8]) {
        if let Some(pos) = command.iter().position(|&b| b == b' ') {
            let text = &command[pos+1..];
            if !text.is_empty() {
                if let Ok(s) = core::str::from_utf8(text) {
                    println!("{}", s);
                }
            }
        }
    }

    fn cmd_info(&self) {
        println!("Vibe-OS {}", crate::VERSION);
        println!("Build: {}", crate::BUILD_DATE);
        println!("Kernel: rust");
        println!("Architecture: x86_64");
        println!("Modules: vfs, scheduler, disk, acpi, network, usb, audio, graphics");
    }

    fn cmd_mem(&self) {
        println!("Heap: 1MB alloue statiquement");
        let (mb, _) = crate::memory::get_physical_memory_info();
        println!("Memoire physique: {} MB", mb);
    }

    fn cmd_date(&self) {
        let dt = crate::rtc::get_time();
        println!("{}", dt.format());
    }

    fn cmd_time(&self) {
        let ticks = *crate::timer::TICKS.lock();
        println!("Ticks: {}", ticks);
    }

    fn cmd_uptime(&self) {
        println!("Uptime: {} secondes", crate::timer::uptime_seconds());
    }

    fn cmd_reboot(&self) {
        println!("Redemarrage...");
        unsafe { x86_64::instructions::port::Port::<u8>::new(0x64).write(0xfe); }
    }

    fn cmd_shutdown(&self) {
        println!("Arret du systeme...");
        crate::acpi::pm::shutdown();
    }

    fn cmd_ls(&self) {
        crate::vfs::shell_commands::cmd_ls();
    }

    fn cmd_file(&self, command: &[u8], op: &str) {
        let args: Vec<&[u8]> = command.split(|&b| b == b' ').collect();
        if args.len() > 1 {
            let name = core::str::from_utf8(args[1]).unwrap_or("");
            match op {
                "cat" => crate::vfs::shell_commands::cmd_cat(&[name]),
                "mkdir" => crate::vfs::shell_commands::cmd_mkdir(&[name]),
                "touch" => crate::vfs::shell_commands::cmd_touch(&[name]),
                "rm" => crate::vfs::shell_commands::cmd_rm(&[name]),
                _ => {}
            }
        } else {
            println!("Usage: {} <fichier>", op);
        }
    }

    fn cmd_df(&self) {
        crate::vfs::shell_commands::cmd_df();
    }

    fn cmd_ps(&self) {
        crate::scheduler::scheduler::list_tasks();
    }

    fn cmd_kill(&self, command: &[u8]) {
        let args: Vec<&[u8]> = command.split(|&b| b == b' ').collect();
        if args.len() > 1 {
            if let Ok(pid) = core::str::from_utf8(args[1]).unwrap_or("").parse::<u32>() {
                if crate::scheduler::scheduler::kill(pid) {
                    println!("Processus {} tue", pid);
                } else {
                    println!("Erreur: processus non trouve");
                }
            }
        } else {
            println!("Usage: kill <pid>");
        }
    }

    fn cmd_ifconfig(&self) {
        println!("eth0: pilote RTL8139");
        println!("  MAC: 00:00:00:00:00:00");
        println!("  IP: 192.168.1.100");
    }

    fn cmd_ping(&self) {
        println!("ping: cible non specifiee");
    }

    fn cmd_lspci(&self) {
        crate::pcie::scan_bus();
    }

    fn cmd_lsusb(&self) {
        println!("Peripheriques USB:");
        println!(" Hub racine");
    }

    fn cmd_test(&self) {
        println!("Test audio...");
    }

    fn cmd_beep(&self) {
        println!("Bip!");
    }

    fn cmd_clearcolor(&self) {
        println!("Efface l'ecran avec couleur (non implemente)");
    }

    fn cmd_draw(&self) {
        println!("Commandes de dessin (non implemente)");
    }

    fn cmd_exit(&self) {
        println!("Au revoir!");
        loop { x86_64::instructions::hlt(); }
    }
}