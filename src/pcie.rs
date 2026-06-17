
pub const PCIE_EXPRESS_CONFIG: u64 = 0xe0000000;

pub const PCIE_CAP_ID: u8 = 0x10;
pub const PCIE_CAP_VER: u8 = 0x1;
pub const PCIE_LINK_CAP: u8 = 0x0c;
pub const PCIE_LINK_STS: u8 = 0x0e;
pub const PCIE_LINK_CTL: u8 = 0x10;
pub const PCIE_SLOT_CAP: u8 = 0x14;
pub const PCIE_SLOT_CTL: u8 = 0x18;
pub const PCIE_ROOT_CAP: u8 = 0x1c;
pub const PCIE_ROOT_CTL: u8 = 0x20;
pub const PCIE_ROOT_STS: u8 = 0x22;
pub const PCIE_DEVICE_CAP: u8 = 0x24;
pub const PCIE_DEVICE_STS: u8 = 0x28;
pub const PCIE_DEVICE_CTL: u8 = 0x2c;
pub const PCIE_LINK_CAP_MAX: u16 = 0x8000;
pub const PCIE_LINK_CAP_GEN1: u8 = 1;
pub const PCIE_LINK_CAP_GEN2: u8 = 2;
pub const PCIE_LINK_CAP_GEN3: u8 = 3;
pub const PCIE_LINK_STS_LINK_UP: u16 = 0x0001;
pub const PCIE_LINK_STS_TRAINING: u16 = 0x0010;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PcieError {
    NotFound,
    NotReady,
    LinkDown,
    NoGart,
}

#[derive(Copy, Clone, Debug)]
pub struct PcieLinkStatus {
    pub link_speed: u8,
    pub link_width: u16,
    pub training: bool,
    pub up: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct PcieDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u32,
    pub revision: u8,
    pub interrupt_line: u8,
    pub msi_supported: bool,
    pub msix_supported: bool,
}

impl PcieDevice {
    pub fn is_pcie(&self) -> bool {
        true
    }
}

pub struct PcieBus {
    pub devices: [Option<PcieDevice>; 256],
    pub device_count: u8,
    pub express_capable: bool,
}

impl PcieBus {
    pub const fn new() -> Self {
        PcieBus {
            devices: [None; 256],
            device_count: 0,
            express_capable: false,
        }
    }

    pub fn scan(&mut self) {
        println!("Scan PCIe...");
        
        for bus in 0..=255 {
            for device in 0..32 {
                for function in 0..8 {
                    let vendor = crate::acpi::pci::get_vendor_id(bus, device, function);
                    if vendor != 0xffff {
                        if self.device_count < 255 {
                            let dev = PcieDevice {
                                bus,
                                device,
                                function,
                                vendor_id: vendor,
                                device_id: (crate::acpi::pci::read_address(bus, device, function, 0x02) >> 16) as u16,
                                class_code: crate::acpi::pci::get_class_code(bus, device, function),
                                revision: crate::acpi::pci::read_address(bus, device, function, 0x08) as u8,
                                interrupt_line: 0,
                                msi_supported: false,
                                msix_supported: false,
                            };
                            self.devices[self.device_count as usize] = Some(dev);
                            self.device_count += 1;
                        }
                    }
                }
            }
        }
        
        self.express_capable = true;
        println!("  {} peripheriques trouves", self.device_count);
    }

    pub fn get_device(&self, bus: u8, device: u8, function: u8) -> Option<&PcieDevice> {
        for i in 0..self.device_count {
            if let Some(ref dev) = self.devices[i as usize] {
                if dev.bus == bus && dev.device == device && dev.function == function {
                    return Some(dev);
                }
            }
        }
        None
    }

    pub fn read_extended_config(&self, bus: u8, device: u8, function: u8, offset: u16) -> u32 {
        let address = (1u32 << 31)
            | (PCIE_EXPRESS_CONFIG as u32)
            | ((bus as u32) << 20)
            | ((device as u32) << 15)
            | ((function as u32) << 12)
            | ((offset & 0xfff) as u32);
        
        unsafe {
            let ptr = address as *const u32;
            ptr.read_volatile()
        }
    }

    pub fn write_extended_config(&self, bus: u8, device: u8, function: u8, offset: u16, value: u32) {
        let address = (1u32 << 31)
            | (PCIE_EXPRESS_CONFIG as u32)
            | ((bus as u32) << 20)
            | ((device as u32) << 15)
            | ((function as u32) << 12)
            | ((offset & 0xfff) as u32);
        
        unsafe {
            let ptr = address as *mut u32;
            ptr.write_volatile(value);
        }
    }
}

pub static mut PCIE: PcieBus = PcieBus::new();

pub fn scan_bus() {
    unsafe {
        PCIE.scan();
    }
}

pub fn init_pcie() {
    unsafe {
        PCIE.scan();
    }
}