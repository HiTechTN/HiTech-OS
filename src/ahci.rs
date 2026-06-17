
#[allow(dead_code)]
pub const AHCI_BAR: u32 = 0x3000;
pub const AHCI_SIZE: u32 = 0x800;

pub const AHCI_CAP: u32 = 0x00;
pub const AHCI_GHC: u32 = 0x04;
pub const AHCI_PI: u32 = 0x0c;
pub const AHCI_VER: u32 = 0x10;
pub const AHCI_EM_CTL: u32 = 0x1c;
pub const AHCI_EM_STS: u32 = 0x20;

pub const GHC_HR: u32 = 0x00000001;
pub const GHC_AE: u32 = 0x00000001;
pub const GHC_MP: u32 = 0x00010000;
pub const GHC_IE: u32 = 0x00000002;

pub const HBA_P0: u32 = 0x100;
pub const HBA_P_CLB: u32 = 0x00;
pub const HBA_P_CLBU: u32 = 0x04;
pub const HBA_P_FB: u32 = 0x08;
pub const HBA_P_FBU: u32 = 0x0c;
pub const HBA_P_IS: u32 = 0x10;
pub const HBA_P_IE: u32 = 0x14;
pub const HBA_P_CMD: u32 = 0x18;
pub const HBA_P_TFD: u32 = 0x20;
pub const HBA_P_SIG: u32 = 0x24;
pub const HBA_P_SSTS: u32 = 0x28;
pub const HBA_P_SCTL: u32 = 0x2c;
pub const HBA_P_SERR: u32 = 0x30;
pub const HBA_P_SACT: u32 = 0x34;
pub const HBA_P_CI: u32 = 0x38;
pub const HBA_P_NTFY: u32 = 0x40;

pub const PORT_CMD_START: u32 = 0x00000001;
pub const PORT_CMD_FIS_RX: u32 = 0x00000001;
pub const PORT_CMD_FIS_TX: u32 = 0x00000002;
pub const PORT_CMD_UPDATE: u32 = 0x00000004;
pub const PORT_CMD_HPCP: u32 = 0x00000010;
pub const PORT_CMD_PSS: u32 = 0x00000020;
pub const PORT_CMD_FLBDS: u32 = 0x00000040;
pub const PORT_CMD_FLBDI: u32 = 0x00000080;
pub const PORT_CMD_CLKS: u32 = 0x00000100;
pub const PORT_CMD_WARM: u32 = 0x00000200;
pub const PORT_CMD_CLO: u32 = 0x00000400;

pub const PORT_TFD_BSY: u32 = 0x00000001;
pub const PORT_TFD_DRQ: u32 = 0x00000001;
pub const PORT_TFD_STS: u32 = 0x00000040;
pub const PORT_TFD_DMAR: u32 = 0x00000080;

pub const SATA_SIG_ATA: u32 = 0x00000101;
pub const SATA_SIG_ATAPI: u32 = 0x14eb1401;
pub const SATA_SIG_SEMB: u32 = 0x23c30100;
pub const SATA_SIG_PM: u32 = 0x33ceb967;

pub const FIS_TYPE_H2D: u8 = 0x27;
pub const FIS_TYPE_D2H: u8 = 0x34;
pub const FIS_TYPE_SET: u8 = 0x39;
pub const FIS_TYPE_PIO: u8 = 0x5f;
pub const FIS_TYPE_DEV: u8 = 0xa1;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AhciError {
    NotFound,
    NoPorts,
    PortBusy,
    CommandFailed,
    IoError,
    Timeout,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PortSpeed {
    Gen1,
    Gen2, 
    Gen3,
    Unknown,
}

#[derive(Copy, Clone, Debug)]
pub struct HbaPort {
    pub port_addr: u32,
    pub clb: u32,
    pub clbu: u32,
    pub fb: u32,
    pub fbu: u32,
    pub is: u32,
    pub ie: u32,
    pub cmd: u32,
    pub rsvd: u32,
    pub tfd: u32,
    pub sig: u32,
    pub ssts: u32,
    pub sctl: u32,
    pub serr: u32,
    pub sact: u32,
    pub ci: u32,
    pub ntfy: u32,
}

impl HbaPort {
    pub fn new(addr: u32) -> Self {
        HbaPort {
            port_addr: addr,
            clb: readl(addr + HBA_P_CLB),
            clbu: readl(addr + HBA_P_CLBU),
            fb: readl(addr + HBA_P_FB),
            fbu: readl(addr + HBA_P_FBU),
            is: readl(addr + HBA_P_IS),
            ie: readl(addr + HBA_P_IE),
            cmd: readl(addr + HBA_P_CMD),
            rsvd: 0,
            tfd: readl(addr + HBA_P_TFD),
            sig: readl(addr + HBA_P_SIG),
            ssts: readl(addr + HBA_P_SSTS),
            sctl: readl(addr + HBA_P_SCTL),
            serr: readl(addr + HBA_P_SERR),
            sact: readl(addr + HBA_P_SACT),
            ci: readl(addr + HBA_P_CI),
            ntfy: readl(addr + HBA_P_NTFY),
        }
    }

    pub fn start(&mut self) {
        self.cmd |= (PORT_CMD_START | PORT_CMD_FIS_RX | PORT_CMD_FIS_TX) as u32;
        writel(self.port_addr + HBA_P_CMD, self.cmd);
    }

    pub fn stop(&mut self) {
        self.cmd &= !(PORT_CMD_START | PORT_CMD_FIS_RX | PORT_CMD_FIS_TX) as u32;
        writel(self.port_addr + HBA_P_CMD, self.cmd);
    }

    pub fn is_connected(&self) -> bool {
        (self.ssts & 0xf) == 0x03
    }

    pub fn read(&mut self, _sector: u64, count: u32, buffer: &mut [u8]) -> Result<(), AhciError> {
        if buffer.len() < (count as usize * 512) {
            return Err(AhciError::IoError);
        }

        if (self.tfd & PORT_TFD_BSY) != 0 {
            return Err(AhciError::PortBusy);
        }

        Ok(())
    }

    pub fn write(&mut self, _sector: u64, _count: u32, _buffer: &[u8]) -> Result<(), AhciError> {
        if (self.tfd & PORT_TFD_BSY) != 0 {
            return Err(AhciError::PortBusy);
        }

        Ok(())
    }
}

pub struct AhciController {
    pub base: u32,
    pub present: bool,
    pub ports: [Option<HbaPort>; 32],
    pub port_count: u8,
    pub pi_mask: u32,
    pub version: u32,
}

impl AhciController {
    pub const fn new() -> Self {
        AhciController {
            base: 0,
            present: false,
            ports: [None; 32],
            port_count: 0,
            pi_mask: 0,
            version: 0,
        }
    }

    pub fn init(&mut self, base: u32) -> Result<(), AhciError> {
        self.base = base;

        let _cap = readl(base + AHCI_CAP);
        self.pi_mask = readl(base + AHCI_PI);

        if self.pi_mask == 0 {
            return Err(AhciError::NoPorts);
        }

        self.enable();

        for i in 0..32 {
            if (self.pi_mask & (1 << i)) != 0 {
                if self.port_count < 31 {
                    let port_addr = base + HBA_P0 as u32 + (i as u32 * 0x100);
                    self.ports[i] = Some(HbaPort::new(port_addr));
                    self.port_count += 1;
                }
            }
        }

        self.present = true;
        Ok(())
    }

    fn enable(&mut self) {
        let ghc = readl(self.base + AHCI_GHC);
        writel(self.base + AHCI_GHC, ghc | GHC_AE);
    }

    pub fn get_port(&self, index: u8) -> Option<&HbaPort> {
        if index as usize >= 32 {
            return None;
        }
        self.ports[index as usize].as_ref()
    }

    pub fn probe_ports(&self) {
        println!("Ports SATA disponibles:");
        
        for i in 0..self.port_count {
            if let Some(ref port) = self.ports[i as usize] {
                let connected = port.is_connected();
                let status = if connected { "connecte" } else { "vide" };
                println!("  port{}: {}", i, status);
            }
        }
    }
}

fn readl(addr: u32) -> u32 {
    unsafe { (addr as *const u32).read_volatile() }
}

fn writel(addr: u32, value: u32) {
    unsafe { (addr as *mut u32).write_volatile(value) }
}

pub static mut AHCI: AhciController = AhciController::new();

pub fn init_ahci() -> Result<(), AhciError> {
    println!("Initialisation SATA/AHCI...");

    let bar = crate::pcie::find_device_by_class(0x01, 0x06)
        .and_then(|(b, d, f)| crate::acpi::pci::read_bar(b, d, f, 5))
        .or_else(|| crate::pcie::find_device_by_class(0x01, 0x06)
            .and_then(|(b, d, f)| crate::acpi::pci::read_bar(b, d, f, 0)))
        .or_else(|| {
            println!("  AHCI non trouve sur PCI, utilise bar par defaut");
            Some((AHCI_BAR as u64, false))
        });
    
    let base = match bar {
        Some((addr, _is_io)) => addr as u32,
        None => {
            println!("  AHCI indisponible");
            return Err(AhciError::NotFound);
        }
    };
    
    unsafe {
        AHCI.init(base)?;
    }
    
    println!("  {} ports trouves", unsafe { AHCI.port_count });
    unsafe { AHCI.probe_ports(); }
    
    Ok(())
}