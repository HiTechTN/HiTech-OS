
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

pub const ATA_CMD_READ_DMA_EXT: u8 = 0x25;
pub const ATA_CMD_WRITE_DMA_EXT: u8 = 0x35;
pub const ATA_CMD_IDENTIFY: u8 = 0xec;

#[repr(C, packed)]
pub struct FisRegH2D {
    pub fis_type: u8,
    pub pm_port: u8,
    pub command: u8,
    pub features_low: u8,
    pub lba0: u8,
    pub lba1: u8,
    pub lba2: u8,
    pub device: u8,
    pub lba3: u8,
    pub lba4: u8,
    pub lba5: u8,
    pub features_high: u8,
    pub count_low: u8,
    pub count_high: u8,
    pub icc: u8,
    pub control: u8,
    pub rsvd: [u8; 4],
}

#[repr(C, align(128))]
pub struct AhciCmdTable {
    pub fis: [u8; 64],
    pub acmd: [u8; 16],
    pub rsvd: [u8; 48],
    pub prdt: [AhciPrdtEntry; 8],
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct AhciPrdtEntry {
    pub dba: u32,
    pub dbau: u32,
    pub rsvd: u32,
    pub dbc: u32,
}

#[repr(C, align(1024))]
pub struct AhciCmdHeader {
    pub resv: [u8; 8],
    pub prdtl: u16,
    pub prdbc: u32,
    pub ctbau: u32,
    pub ctba: u32,
    pub rsvd2: [u32; 4],
}

pub static mut AHCI_CMD_SLOT: AhciCmdTable = AhciCmdTable {
    fis: [0; 64],
    acmd: [0; 16],
    rsvd: [0; 48],
    prdt: [AhciPrdtEntry { dba: 0, dbau: 0, rsvd: 0, dbc: 0 }; 8],
};

pub static mut AHCI_CMD_LIST: AhciCmdHeader = AhciCmdHeader {
    resv: [0; 8],
    prdtl: 0,
    prdbc: 0,
    ctbau: 0,
    ctba: 0,
    rsvd2: [0; 4],
};

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

    pub fn read(&mut self, sector: u64, count: u32, buffer: &mut [u8]) -> Result<(), AhciError> {
        if buffer.len() < (count as usize * 512) {
            return Err(AhciError::IoError);
        }

        if (self.tfd & (PORT_TFD_BSY | PORT_TFD_DRQ)) != 0 {
            return Err(AhciError::PortBusy);
        }

        let fis = FisRegH2D {
            fis_type: FIS_TYPE_H2D,
            pm_port: 0x80,
            command: ATA_CMD_READ_DMA_EXT,
            features_low: 0,
            lba0: (sector >> 0) as u8,
            lba1: (sector >> 8) as u8,
            lba2: (sector >> 16) as u8,
            device: 0x40,
            lba3: (sector >> 24) as u8,
            lba4: (sector >> 32) as u8,
            lba5: (sector >> 40) as u8,
            features_high: 0,
            count_low: count as u8,
            count_high: (count >> 8) as u8,
            icc: 0,
            control: 0,
            rsvd: [0; 4],
        };

        let cmd_table_addr: u32;
        let cmd_addr: u32;
        unsafe {
            cmd_table_addr = &raw mut AHCI_CMD_SLOT as u32;
            AHCI_CMD_SLOT.fis = [0u8; 64];
            core::ptr::copy_nonoverlapping(
                &fis as *const FisRegH2D as *const u8,
                AHCI_CMD_SLOT.fis.as_mut_ptr(),
                core::mem::size_of::<FisRegH2D>(),
            );
            AHCI_CMD_SLOT.prdt[0] = AhciPrdtEntry {
                dba: buffer.as_ptr() as u32,
                dbau: 0,
                rsvd: 0,
                dbc: ((count * 512 - 1) | 0x8000_0000),
            };
            AHCI_CMD_SLOT.acmd = [0u8; 16];

            cmd_addr = &raw mut AHCI_CMD_LIST as u32;
            AHCI_CMD_LIST.prdtl = 1;
            AHCI_CMD_LIST.prdbc = 0;
            AHCI_CMD_LIST.ctba = cmd_table_addr;
            AHCI_CMD_LIST.ctbau = 0;
        }

        writel(self.port_addr + HBA_P_CLB, cmd_addr);
        writel(self.port_addr + HBA_P_CLBU, 0);
        writel(self.port_addr + HBA_P_FB, cmd_addr + 0x100);
        writel(self.port_addr + HBA_P_FBU, 0);

        writel(self.port_addr + HBA_P_CMD, self.cmd | PORT_CMD_START | PORT_CMD_FIS_RX);

        writel(self.port_addr + HBA_P_CI, 1);

        for _ in 0..100_000 {
            if readl(self.port_addr + HBA_P_CI) & 1 == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }

        Err(AhciError::Timeout)
    }

    pub fn write(&mut self, sector: u64, count: u32, buffer: &[u8]) -> Result<(), AhciError> {
        if buffer.len() < (count as usize * 512) {
            return Err(AhciError::IoError);
        }

        if (self.tfd & (PORT_TFD_BSY | PORT_TFD_DRQ)) != 0 {
            return Err(AhciError::PortBusy);
        }

        let fis = FisRegH2D {
            fis_type: FIS_TYPE_H2D,
            pm_port: 0x80,
            command: ATA_CMD_WRITE_DMA_EXT,
            features_low: 0,
            lba0: (sector >> 0) as u8,
            lba1: (sector >> 8) as u8,
            lba2: (sector >> 16) as u8,
            device: 0x40,
            lba3: (sector >> 24) as u8,
            lba4: (sector >> 32) as u8,
            lba5: (sector >> 40) as u8,
            features_high: 0,
            count_low: count as u8,
            count_high: (count >> 8) as u8,
            icc: 0,
            control: 0,
            rsvd: [0; 4],
        };

        let cmd_table_addr: u32;
        let cmd_addr: u32;
        unsafe {
            cmd_table_addr = &raw mut AHCI_CMD_SLOT as u32;
            AHCI_CMD_SLOT.fis = [0u8; 64];
            core::ptr::copy_nonoverlapping(
                &fis as *const FisRegH2D as *const u8,
                AHCI_CMD_SLOT.fis.as_mut_ptr(),
                core::mem::size_of::<FisRegH2D>(),
            );
            AHCI_CMD_SLOT.prdt[0] = AhciPrdtEntry {
                dba: buffer.as_ptr() as u32,
                dbau: 0,
                rsvd: 0,
                dbc: ((count * 512 - 1) | 0x8000_0000),
            };
            AHCI_CMD_SLOT.acmd = [0u8; 16];
            cmd_addr = &raw mut AHCI_CMD_LIST as u32;
            AHCI_CMD_LIST.prdtl = 1;
            AHCI_CMD_LIST.prdbc = 0;
            AHCI_CMD_LIST.ctba = cmd_table_addr;
            AHCI_CMD_LIST.ctbau = 0;
        }

        writel(self.port_addr + HBA_P_CLB, cmd_addr);
        writel(self.port_addr + HBA_P_CLBU, 0);
        writel(self.port_addr + HBA_P_FB, cmd_addr + 0x100);
        writel(self.port_addr + HBA_P_FBU, 0);

        writel(self.port_addr + HBA_P_CMD, self.cmd | PORT_CMD_START | PORT_CMD_FIS_RX);

        writel(self.port_addr + HBA_P_CI, 1);

        for _ in 0..100_000 {
            if readl(self.port_addr + HBA_P_CI) & 1 == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }

        Err(AhciError::Timeout)
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