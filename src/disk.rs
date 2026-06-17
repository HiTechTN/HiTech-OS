use x86_64::instructions::port::Port;

pub const ATA_DATA_PORT: u16 = 0x1f0;
pub const ATA_ERROR_PORT: u16 = 0x1f1;
pub const ATA_FEATURE_PORT: u16 = 0x1f1;
pub const ATA_SECTOR_COUNT_PORT: u16 = 0x1f2;
pub const ATA_LBA_LOW_PORT: u16 = 0x1f3;
pub const ATA_LBA_MID_PORT: u16 = 0x1f4;
pub const ATA_LBA_HIGH_PORT: u16 = 0x1f5;
pub const ATA_DRIVE_PORT: u16 = 0x1f6;
pub const ATA_STATUS_PORT: u16 = 0x1f7;
pub const ATA_COMMAND_PORT: u16 = 0x1f7;

pub const ATA_CMD_READ: u8 = 0x20;
pub const ATA_CMD_WRITE: u8 = 0x30;
pub const ATA_CMD_IDENTIFY: u8 = 0xec;
pub const ATA_CMD_SET_FEATURES: u8 = 0xef;

pub const ATA_STATUS_ERR: u8 = 0x01;
pub const ATA_STATUS_DRQ: u8 = 0x08;
pub const ATA_STATUS_SRV: u8 = 0x10;
pub const ATA_STATUS_DF: u8 = 0x20;
pub const ATA_STATUS_DRDY: u8 = 0x40;
pub const ATA_STATUS_BSY: u8 = 0x80;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum AtaError {
    NotFound,
    NotReady,
    Timeout,
    IoError,
    NoMedia,
}

#[derive(Copy, Clone)]
pub struct DiskGeometry {
    pub cylinders: u32,
    pub heads: u32,
    pub sectors: u32,
    pub total_sectors: u64,
    pub sector_size: u16,
}

impl DiskGeometry {
    pub fn total_bytes(&self) -> u64 {
        self.total_sectors as u64 * self.sector_size as u64
    }
}

pub struct IdeDevice {
    pub present: bool,
    pub is_master: bool,
    pub is_ata: bool,
    pub lba_supported: bool,
    pub dma_supported: bool,
    pub model: [u8; 41],
    pub serial: [u8; 21],
    pub geometry: DiskGeometry,
}

impl IdeDevice {
    pub const fn new() -> Self {
        IdeDevice {
            present: false,
            is_master: true,
            is_ata: true,
            lba_supported: false,
            dma_supported: false,
            model: [0; 41],
            serial: [0; 21],
            geometry: DiskGeometry {
                cylinders: 0,
                heads: 0,
                sectors: 0,
                total_sectors: 0,
                sector_size: 512,
            },
        }
    }
}

impl IdeDevice {
    pub fn identify(&mut self, master: bool) -> Result<(), AtaError> {
        let drive = if master { 0xa0 } else { 0xb0 };
        
        unsafe {
            let mut status_port = Port::<u8>::new(ATA_STATUS_PORT);
            let mut drive_port = Port::<u8>::new(ATA_DRIVE_PORT);
            let mut cmd_port = Port::<u8>::new(ATA_COMMAND_PORT);
            
            drive_port.write(drive);
            status_port.read();
            
            cmd_port.write(ATA_CMD_IDENTIFY);
            
            let status = status_port.read();
            if status == 0 {
                return Err(AtaError::NotFound);
            }
            
            if status & ATA_STATUS_BSY != 0 {
                return Err(AtaError::NotReady);
            }
        }
        
        self.present = true;
        self.is_master = master;
        Ok(())
    }
}

pub static mut PRIMARY_MASTER: IdeDevice = IdeDevice::new();
pub static mut PRIMARY_SLAVE: IdeDevice = IdeDevice::new();

pub fn init_ata() {
    println!("Initialisation ATA...");
    
    unsafe {
        if PRIMARY_MASTER.identify(true).is_ok() {
            println!("  Disque primaire: present");
        }
    }
}

pub mod ata_driver {
    use super::*;

    const SECTOR_SIZE: usize = 512;
    #[allow(dead_code)]
    const MAX_RETRIES: usize = 3;

    fn wait_ready() -> Result<(), AtaError> {
        for _ in 0..10000 {
            let status = unsafe { Port::<u8>::new(ATA_STATUS_PORT).read() };
            if status & ATA_STATUS_BSY == 0 {
                if status & ATA_STATUS_ERR != 0 {
                    return Err(AtaError::IoError);
                }
                return Ok(());
            }
        }
        Err(AtaError::Timeout)
    }

    pub fn read_sector(device: bool, lba: u64, buffer: &mut [u8; SECTOR_SIZE]) -> Result<(), AtaError> {
        if buffer.len() != SECTOR_SIZE {
            return Err(AtaError::IoError);
        }

        let drive = if device { 0xb0 } else { 0xe0 };
        let lba_mode = 0x40;

        unsafe {
            Port::<u8>::new(ATA_DRIVE_PORT).write(drive | lba_mode);
            Port::<u8>::new(ATA_SECTOR_COUNT_PORT).write(1);
            Port::<u8>::new(ATA_LBA_LOW_PORT).write((lba & 0xff) as u8);
            Port::<u8>::new(ATA_LBA_MID_PORT).write(((lba >> 8) & 0xff) as u8);
            Port::<u8>::new(ATA_LBA_HIGH_PORT).write(((lba >> 16) & 0xff) as u8);
            Port::<u8>::new(ATA_COMMAND_PORT).write(ATA_CMD_READ);
        }

        wait_ready()?;

        let mut data_port = Port::<u16>::new(ATA_DATA_PORT);
        for i in 0..(SECTOR_SIZE / 2) {
            let val = unsafe { data_port.read() };
            buffer[i * 2] = (val & 0xff) as u8;
            buffer[i * 2 + 1] = ((val >> 8) & 0xff) as u8;
        }

        Ok(())
    }

    pub fn write_sector(device: bool, lba: u64, buffer: &[u8; SECTOR_SIZE]) -> Result<(), AtaError> {
        let drive = if device { 0xb0 } else { 0xe0 };
        let lba_mode = 0x40;

        unsafe {
            Port::<u8>::new(ATA_DRIVE_PORT).write(drive | lba_mode);
            Port::<u8>::new(ATA_SECTOR_COUNT_PORT).write(1);
            Port::<u8>::new(ATA_LBA_LOW_PORT).write((lba & 0xff) as u8);
            Port::<u8>::new(ATA_LBA_MID_PORT).write(((lba >> 8) & 0xff) as u8);
            Port::<u8>::new(ATA_LBA_HIGH_PORT).write(((lba >> 16) & 0xff) as u8);
            Port::<u8>::new(ATA_COMMAND_PORT).write(ATA_CMD_WRITE);
        }

        wait_ready()?;

        let mut data_port = Port::<u16>::new(ATA_DATA_PORT);
        for i in 0..(SECTOR_SIZE / 2) {
            let val = (buffer[i * 2 + 1] as u16) << 8 | buffer[i * 2] as u16;
            unsafe { data_port.write(val); }
        }

        wait_ready()?;
        Ok(())
    }

    pub fn read_blocks(device: bool, lba: u64, count: u32, buffer: &mut [u8]) -> Result<(), AtaError> {
        if buffer.len() < count as usize * SECTOR_SIZE {
            return Err(AtaError::IoError);
        }

        let mut offset = 0;
        for i in 0..count as u64 {
            let sector = &mut buffer[offset..offset + SECTOR_SIZE];
            let sector_array: &mut [u8; SECTOR_SIZE] = sector.try_into().unwrap();
            read_sector(device, lba + i, sector_array)?;
            offset += SECTOR_SIZE;
        }

        Ok(())
    }

    pub fn write_blocks(device: bool, lba: u64, count: u32, buffer: &[u8]) -> Result<(), AtaError> {
        if buffer.len() < count as usize * SECTOR_SIZE {
            return Err(AtaError::IoError);
        }

        let mut offset = 0;
        for i in 0..count as u64 {
            let sector = &buffer[offset..offset + SECTOR_SIZE];
            let sector_array: &[u8; SECTOR_SIZE] = sector.try_into().unwrap();
            write_sector(device, lba + i, sector_array)?;
            offset += SECTOR_SIZE;
        }

        Ok(())
    }

    pub fn get_device_info(device: bool) -> Option<DiskGeometry> {
        unsafe {
            if device {
                if PRIMARY_SLAVE.present {
                    Some(PRIMARY_SLAVE.geometry)
                } else {
                    None
                }
            } else {
                if PRIMARY_MASTER.present {
                    Some(PRIMARY_MASTER.geometry)
                } else {
                    None
                }
            }
        }
    }
}

pub mod partitions {
    use super::*;

    pub const MBR_SIGNATURE: u16 = 0xaa55;

    #[derive(Copy, Clone)]
    pub struct PartitionEntry {
        pub status: u8,
        pub start_head: u8,
        pub start_sector: u8,
        pub start_cylinder: u8,
        pub partition_type: u8,
        pub end_head: u8,
        pub end_sector: u8,
        pub end_cylinder: u8,
        pub start_lba: u32,
        pub sectors: u32,
    }

    impl PartitionEntry {
        pub fn is_active(&self) -> bool {
            self.status == 0x80
        }

        pub fn is_empty(&self) -> bool {
            self.partition_type == 0 && self.sectors == 0
        }
    }

    pub struct Mbr {
        pub bootstrap: [u8; 436],
        pub disk_id: u32,
        pub partitions: [PartitionEntry; 4],
        pub signature: u16,
    }

    impl Mbr {
        pub fn read(device: bool) -> Result<Mbr, AtaError> {
            let mut buffer = [0u8; 512];
            
            ata_driver::read_sector(device, 0, &mut buffer)?;
            
            let mut mbr = Mbr {
                bootstrap: [0; 436],
                disk_id: 0,
                partitions: [PartitionEntry {
                    status: 0,
                    start_head: 0,
                    start_sector: 0,
                    start_cylinder: 0,
                    partition_type: 0,
                    end_head: 0,
                    end_sector: 0,
                    end_cylinder: 0,
                    start_lba: 0,
                    sectors: 0,
                }; 4],
                signature: 0,
            };
            
            mbr.signature = (buffer[510] as u16) | ((buffer[511] as u16) << 8);
            
            if mbr.signature != MBR_SIGNATURE {
                return Err(AtaError::NoMedia);
            }
            
            let disk_id_offset = 440;
            mbr.disk_id = (buffer[disk_id_offset] as u32)
                | ((buffer[disk_id_offset + 1] as u32) << 8)
                | ((buffer[disk_id_offset + 2] as u32) << 16)
                | ((buffer[disk_id_offset + 3] as u32) << 24);
            
            for (i, part) in mbr.partitions.iter_mut().enumerate() {
                let offset = 446 + i * 16;
                part.status = buffer[offset];
                part.start_head = buffer[offset + 1];
                part.start_sector = buffer[offset + 2];
                part.start_cylinder = buffer[offset + 3];
                part.partition_type = buffer[offset + 4];
                part.end_head = buffer[offset + 5];
                part.end_sector = buffer[offset + 6];
                part.end_cylinder = buffer[offset + 7];
                part.start_lba = (buffer[offset + 8] as u32)
                    | ((buffer[offset + 9] as u32) << 8)
                    | ((buffer[offset + 10] as u32) << 16)
                    | ((buffer[offset + 11] as u32) << 24);
                part.sectors = (buffer[offset + 12] as u32)
                    | ((buffer[offset + 13] as u32) << 8)
                    | ((buffer[offset + 14] as u32) << 16)
                    | ((buffer[offset + 15] as u32) << 24);
            }
            
            Ok(mbr)
        }
    }
}