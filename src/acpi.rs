use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::registers::model_specific::Msr;

pub const ACPI_RSDP_SIGNATURE: u64 = 0x2052545020445352;

pub const SMBASE: u16 = 0xe000;
pub const SBASE: u16 = 0x3F8;

pub const SLP_EN: u8 = 0x20;
pub const SLP_TYP_MASK: u8 = 0x1f;

#[derive(Copy, Clone)]
pub struct RsdpDescriptor {
    pub signature: u64,
    pub checksum: u8,
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,
    pub length: u32,
    pub xsdt_address: u64,
    pub extended_checksum: u8,
}

impl RsdpDescriptor {
    pub fn validate(&self) -> bool {
        self.signature == ACPI_RSDP_SIGNATURE
    }
}

pub struct AcpiTables {
    pub rsdp: Option<RsdpDescriptor>,
    pub xsdt: Option<u64>,
    pub rsdt: Option<u32>,
}

impl AcpiTables {
    pub const fn new() -> Self {
        AcpiTables {
            rsdp: None,
            xsdt: None,
            rsdt: None,
        }
    }
}

pub static ACPI: Mutex<AcpiTables> = Mutex::new(AcpiTables::new());

pub fn init_acpi() {
    println!("Recherche des tables ACPI...");
    
    let rsdp = find_rsdp();
    
    ACPI.lock().rsdp = rsdp;
    
    println!("ACPI: initialise");
}

fn find_rsdp() -> Option<RsdpDescriptor> {
    for addr in 0x000e0000..0x00100000 {
        unsafe {
            let ptr = addr as *const RsdpDescriptor;
            let desc = &*ptr;
            if desc.signature == ACPI_RSDP_SIGNATURE {
                return Some(*desc);
            }
        }
    }
    None
}

pub mod pm {
    use super::*;

    fn pm1a_control() -> Port<u16> {
        Port::new(SBASE + 0x1000 + 0x0c)
    }

    #[allow(dead_code)]
    fn pm1b_control() -> Port<u16> {
        Port::new(SBASE + 0x1000 + 0x0d)
    }

    fn pm1a_status() -> Port<u16> {
        Port::new(SBASE + 0x1000 + 0x00)
    }

    pub fn shutdown() {
        println!("Arret ACPI...");
        
        unsafe {
            pm1a_control().write(0x2000);
        }
    }

    pub fn sleep(sleep_type: u8) {
        let slp_typ = sleep_type & SLP_TYP_MASK;
        let value = (SLP_EN as u16) | ((slp_typ as u16) << 10);
        
        unsafe {
            pm1a_control().write(value as u16);
            pm1a_status().write(0x1000);
        }
    }

    pub fn reboot() {
        unsafe {
            let mut port = Port::<u8>::new(0x64);
            port.write(0xfe);
        }
    }
}

pub mod cpu {
    use super::*;

    pub static CPU_COUNT: Mutex<u32> = Mutex::new(1);
    pub static LAPIC_ENABLED: Mutex<bool> = Mutex::new(false);

    pub fn init() {
        println!("Initialisation CPU...");
        
        let count = 1u32;
        
        if detect_lapic() {
            *LAPIC_ENABLED.lock() = true;
            println!("  LAPIC detecte");
        }
        
        *CPU_COUNT.lock() = count;
        println!("  {} coeur(s) detected(s)", count);
    }

    fn detect_lapic() -> bool {
        let id = unsafe { Msr::new(0x8020).read() };
        (id & 0xff000000) != 0
    }

    pub fn get_cpu_count() -> u32 {
        *CPU_COUNT.lock()
    }

    pub fn get_cpu_id() -> u32 {
        0
    }
}

pub mod hpet {
    use super::*;

    pub static HPET_PRESENT: Mutex<bool> = Mutex::new(false);

    pub fn init() {
        println!("HPET: detection...");
        
        *HPET_PRESENT.lock() = false;
    }

    pub fn is_present() -> bool {
        *HPET_PRESENT.lock()
    }
}

pub mod pci {
    use super::*;

    pub const CONFIG_ADDRESS: u16 = 0x0cf8;
    pub const CONFIG_DATA: u16 = 0x0cfc;

    pub fn read_address(bus: u8, device: u8, function: u8, register: u8) -> u32 {
        let address = (1u32 << 31)
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | ((register & 0xfc) as u32 & 0xfc);
        
        unsafe {
            Port::<u32>::new(CONFIG_ADDRESS).write(address);
            Port::<u32>::new(CONFIG_DATA).read()
        }
    }

    pub fn write_address(bus: u8, device: u8, function: u8, register: u8, value: u32) {
        let address = (1u32 << 31)
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | ((register & 0xfc) as u32 & 0xfc);
        
        unsafe {
            Port::<u32>::new(CONFIG_ADDRESS).write(address);
            Port::<u32>::new(CONFIG_DATA).write(value);
        }
    }

    pub fn get_vendor_id(bus: u8, device: u8, function: u8) -> u16 {
        (read_address(bus, device, function, 0) & 0xffff) as u16
    }

    pub fn get_class_code(bus: u8, device: u8, function: u8) -> u32 {
        read_address(bus, device, function, 0x08)
    }

    pub fn scan_bus() {
        println!("Scan PCI...");
        
        for bus in 0..=255 {
            for device in 0..32 {
                let vendor = get_vendor_id(bus, device, 0);
                if vendor != 0xffff {
                    let class = get_class_code(bus, device, 0);
                    println!("  {:02x}:{:02x}.0 -> classe: {:x}", 
                        bus, device, class >> 24);
                }
            }
        }
    }
}

pub mod apic {
    use super::*;

    pub const LAPIC_BASE: u64 = 0xfee00000;

    pub fn enable() {
        unsafe {
            let msr = Msr::new(0x1b).read();
            Msr::new(0x1b).write(msr | 0x800);
        }
        println!("APIC active");
    }

    pub fn eoi() {
    }

    pub fn send_ipi(_cpu: u8, _vector: u8) {
    }

    pub fn get_id() -> u32 {
        0
    }

    pub fn get_version() -> u32 {
        0
    }
}

pub mod io_apic {
    

    pub const IO_APIC_BASE: u32 = 0xfec00000;
    pub const IO_APIC_ID: u8 = 0x00;
    pub const IO_APIC_VERSION: u8 = 0x01;
    pub const IO_APIC_ARB: u8 = 0x02;
    pub const IO_APIC_REDIRECT_TABLE: u32 = 0x10;

    pub fn init() {
        println!("IO-APIC: detection...");
    }

    pub fn set_redirection(_entry: u8, _vector: u8, _delivery_mode: u8, _dest_mode: u8, _polarity: u8, _trigger: u8, _mask: u8, _dest: u8) {
    }
}

pub mod peci {
    use super::*;

    pub const CONFIG_ADDRESS: u16 = 0x0cf8;
    pub const CONFIG_DATA: u16 = 0x0cfc;

    pub fn read_address(bus: u8, device: u8, function: u8, register: u8) -> u32 {
        let address = (1u32 << 31)
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | ((register & 0xfc) as u32 & 0xfc);
        
        unsafe {
            Port::<u32>::new(CONFIG_ADDRESS).write(address);
            Port::<u32>::new(CONFIG_DATA).read()
        }
    }

    pub fn write_address(bus: u8, device: u8, function: u8, register: u8, value: u32) {
        let address = (1u32 << 31)
            | ((bus as u32) << 16)
            | ((device as u32) << 11)
            | ((function as u32) << 8)
            | ((register & 0xfc) as u32 & 0xfc);
        
        unsafe {
            Port::<u32>::new(CONFIG_ADDRESS).write(address);
            Port::<u32>::new(CONFIG_DATA).write(value);
        }
    }

    pub fn get_vendor_id(bus: u8, device: u8, function: u8) -> u16 {
        (read_address(bus, device, function, 0) & 0xffff) as u16
    }

    pub fn get_class_code(bus: u8, device: u8, function: u8) -> u32 {
        read_address(bus, device, function, 0x08)
    }

    pub fn scan_bus() {
        println!("Scan PCI...");
        
        for bus in 0..=255 {
            for device in 0..32 {
                let vendor = get_vendor_id(bus, device, 0);
                if vendor != 0xffff {
                    let class = get_class_code(bus, device, 0);
                    println!("  {:02x}:{:02x}.0 -> classe: {:x}", 
                        bus, device, class >> 24);
                }
            }
        }
    }
}