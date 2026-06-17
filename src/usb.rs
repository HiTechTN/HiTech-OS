
pub const USB1_BASE: u32 = 0xffa00000;
pub const USB1_LENGTH: u32 = 0x100000;

pub const UHCI_BASE: u16 = 0xc000;

pub const USB_PORT_SC_RESET: u32 = 0x2000;
pub const USB_PORT_SC_SUSPEND: u32 = 0x1000;
pub const USB_PORT_SC_OVRN: u32 = 0x1000;
pub const USB_PORT_SC_OVRC: u32 = 0x0080;
pub const USB_PORT_SC_RESUME: u32 = 0x0040;
pub const USB_PORT_SC_DP: u32 = 0x0020;
pub const USB_PORT_SC_DM: u32 = 0x0010;
pub const USB_PORT_SC_CONNECT: u32 = 0x0008;
pub const USB_PORT_SC_ENABLED: u32 = 0x0004;
pub const USB_PORT_SC_SPEED: u32 = 0x0002;
pub const USB_PORT_SC_LINE: u32 = 0x0001;

pub const OHCI_USB_CMD: usize = 0;
pub const OHCI_USB_STS: usize = 0x02;
pub const OHCI_USB_INTR: usize = 0x04;
pub const OHCI_USB_FRM_NUM: usize = 0x08;
pub const OHCI_USB_FM_NUM: usize = 0x0c;
pub const OHCI_USB_FM_LOAD: usize = 0x10;
pub const OHCI_USB_CTL: usize = 0x18;

pub const CMD_RS: u32 = 0x00000001;
pub const CMD_HCRESET: u32 = 0x00000002;
pub const CMD_GRIS端M: u32 = 0x00000004;
pub const CMD_SUSPEND: u32 = 0x00000008;

pub const STS_HCHALTED: u32 = 0x00000001;
pub const STS_PROCESSING: u32 = 0x00000002;
pub const STS_SCHERR: u32 = 0x00000004;
pub const STS_STSERR: u32 = 0x00000008;
pub const STS_RWC: u32 = 0x00000010;
pub const STS_ERROR: u32 = 0x40;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UsbError {
    NotFound,
    NotReady,
    Timeout,
    TransferFailed,
    DeviceError,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum UsbSpeed {
    Low,
    Full,
    High,
}

#[derive(Copy, Clone)]
pub struct UsbDevice {
    pub address: u8,
    pub port: u8,
    pub speed: UsbSpeed,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub vendor_id: u16,
    pub product_id: u16,
    pub max_packet_size: u8,
}

impl UsbDevice {
    pub const fn new() -> Self {
        UsbDevice {
            address: 0,
            port: 0,
            speed: UsbSpeed::Full,
            device_class: 0,
            device_subclass: 0,
            device_protocol: 0,
            vendor_id: 0,
            product_id: 0,
            max_packet_size: 64,
        }
    }
}

pub struct UsbHostController {
    pub base: u32,
    pub present: bool,
    pub devices: [Option<UsbDevice>; 128],
    pub device_count: u8,
}

impl UsbHostController {
    pub const fn new() -> Self {
        UsbHostController {
            base: 0,
            present: false,
            devices: [None; 128],
            device_count: 0,
        }
    }

    pub fn init(&mut self, base: u32) -> Result<(), UsbError> {
        self.base = base;
        
        let cmd = self.readl(OHCI_USB_CMD);
        if cmd == 0xffffffff {
            return Err(UsbError::NotFound);
        }
        
        self.writel(OHCI_USB_CMD, CMD_HCRESET);
        
        for _ in 0..100 {
            let cmd = self.readl(OHCI_USB_CMD);
            if cmd & CMD_HCRESET == 0 {
                break;
            }
        }
        
        self.present = true;
        Ok(())
    }

    fn readl(&self, offset: usize) -> u32 {
        unsafe {
            let ptr = (self.base + offset as u32) as *const u32;
            ptr.read_volatile()
        }
    }

    fn writel(&self, offset: usize, value: u32) {
        unsafe {
            let ptr = (self.base + offset as u32) as *mut u32;
            ptr.write_volatile(value)
        }
    }

    pub fn start(&mut self) {
        let cmd = self.readl(OHCI_USB_CMD);
        self.writel(OHCI_USB_CMD, cmd | CMD_RS);
    }

    pub fn stop(&mut self) {
        let cmd = self.readl(OHCI_USB_CMD);
        self.writel(OHCI_USB_CMD, cmd & !CMD_RS);
    }

    pub fn scan_ports(&mut self) {
        for port in 0..8 {
            if self.port_connected(port) {
                let _ = self.reset_port(port);
            }
        }
    }

    fn port_connected(&self, port: u8) -> bool {
        let offset = 0x10 + (port as usize) * 4;
        let status = self.readl(offset);
        (status & USB_PORT_SC_CONNECT) != 0
    }

    fn reset_port(&mut self, port: u8) -> Result<(), UsbError> {
        let offset = 0x10 + (port as usize) * 4;
        
        self.writel(offset, USB_PORT_SC_RESET);
        
        for _ in 0..100 {
            let status = self.readl(offset);
            if (status & USB_PORT_SC_RESET) == 0 {
                break;
            }
        }
        
        self.writel(offset, 0);
        
        for _ in 0..10 {
            let status = self.readl(offset);
            if (status & USB_PORT_SC_ENABLED) != 0 {
                let speed = if (status & USB_PORT_SC_SPEED) != 0 {
                    UsbSpeed::High
                } else {
                    UsbSpeed::Full
                };
                
                if self.device_count < 127 {
                    let dev = UsbDevice {
                        address: self.device_count + 1,
                        port,
                        speed,
                        device_class: 0,
                        device_subclass: 0,
                        device_protocol: 0,
                        vendor_id: 0,
                        product_id: 0,
                        max_packet_size: 64,
                    };
                    self.devices[self.device_count as usize] = Some(dev);
                    self.device_count += 1;
                }
                return Ok(());
            }
        }
        
        Err(UsbError::Timeout)
    }

    pub fn get_device(&self, address: u8) -> Option<&UsbDevice> {
        self.devices.get(address as usize).and_then(|d| d.as_ref())
    }
}

pub static mut USB_CONTROLLER: UsbHostController = UsbHostController::new();

pub fn init_usb() -> Result<(), UsbError> {
    println!("Initialisation USB...");

    let mmio_base = crate::pcie::find_device_by_class(0x0c, 0x03)
        .and_then(|(b, d, f)| crate::acpi::pci::read_bar(b, d, f, 0))
        .map(|(addr, _is_io)| {
            if addr > 0xffff_ffff { USB1_BASE } else { addr as u32 }
        })
        .unwrap_or(USB1_BASE);
    
    unsafe {
        if let Err(e) = USB_CONTROLLER.init(mmio_base) {
            println!("  OHCI non trouve, essaie fallback...");
            return Err(e);
        }
    }
    
    unsafe {
        USB_CONTROLLER.start();
        USB_CONTROLLER.scan_ports();
    }
    
    println!("  {} peripheriques USB detectes", unsafe { USB_CONTROLLER.device_count });
    Ok(())
}

pub mod hub {
    use super::*;

    pub const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
    pub const USB_REQUEST_SET_ADDRESS: u8 = 0x05;
    pub const USB_REQUEST_SET_CONFIGURATION: u8 = 0x09;
    pub const USB_REQUEST_GET_STATUS: u8 = 0x00;

    #[derive(Copy, Clone)]
    pub struct UsbRequest {
        pub request_type: u8,
        pub request: u8,
        pub value: u16,
        pub index: u16,
        pub length: u16,
    }

    impl UsbRequest {
        pub fn new(request_type: u8, request: u8, value: u16, index: u16, length: u16) -> Self {
            UsbRequest {
                request_type,
                request,
                value,
                index,
                length,
            }
        }
    }

    pub struct UsbDescriptor {
        pub length: u8,
        pub descriptor_type: u8,
        pub data: [u8; 256],
    }

    impl UsbDescriptor {
        pub fn device_descriptor() -> Self {
            UsbDescriptor {
                length: 18,
                descriptor_type: 1,
                data: [0; 256],
            }
        }

        pub fn parse_device(data: &[u8]) -> Option<UsbDevice> {
            if data.len() < 18 {
                return None;
            }
            
            Some(UsbDevice {
                address: 0,
                port: 0,
                speed: UsbSpeed::Full,
                device_class: data[4],
                device_subclass: data[5],
                device_protocol: data[7],
                vendor_id: (data[8] as u16) | ((data[9] as u16) << 8),
                product_id: (data[10] as u16) | ((data[11] as u16) << 8),
                max_packet_size: data[7],
            })
        }
    }
}

pub mod hid {
    

    pub const HID_CLASS_CODE: u8 = 0x03;
    pub const BOOT_SUBCLASS_CODE: u8 = 0x01;
    pub const BOOT_PROTOCOL_KEYBOARD: u8 = 0x01;
    pub const BOOT_PROTOCOL_MOUSE: u8 = 0x02;

    pub const DESCRIPTOR_TYPE_HID: u8 = 0x21;
    pub const DESCRIPTOR_TYPE_REPORT: u8 = 0x22;
    pub const DESCRIPTOR_TYPE_PHYSICAL: u8 = 0x23;

    #[derive(Copy, Clone)]
    pub struct KeyboardReport {
        pub modifiers: u8,
        pub reserved: u8,
        pub keycodes: [u8; 6],
    }

    impl KeyboardReport {
        pub const fn empty() -> Self {
            KeyboardReport {
                modifiers: 0,
                reserved: 0,
                keycodes: [0; 6],
            }
        }

        pub fn parse(data: &[u8]) -> Option<KeyboardReport> {
            if data.len() < 8 {
                return None;
            }
            
            Some(KeyboardReport {
                modifiers: data[0],
                reserved: data[1],
                keycodes: [data[2], data[3], data[4], data[5], data[6], data[7]],
            })
        }
    }

    #[derive(Copy, Clone)]
    pub struct MouseReport {
        pub buttons: u8,
        pub x: i8,
        pub y: i8,
        pub wheel: i8,
    }

    impl MouseReport {
        pub const fn empty() -> Self {
            MouseReport {
                buttons: 0,
                x: 0,
                y: 0,
                wheel: 0,
            }
        }

        pub fn parse(data: &[u8]) -> Option<MouseReport> {
            if data.len() < 4 {
                return None;
            }
            
            Some(MouseReport {
                buttons: data[0],
                x: data[1] as i8,
                y: data[2] as i8,
                wheel: data[3] as i8,
            })
        }
    }
}