use core::cmp::min;

pub const BLOCK_SIZE: usize = 512;
pub const MAX_BLOCK_DEVICES: usize = 16;
pub const MAX_PARTITIONS_PER_DEVICE: usize = 64;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum BlockDeviceType {
    Ide,
    Ahci,
    RamDisk,
    Virtio,
    Unknown,
}

impl BlockDeviceType {
    pub fn name(&self) -> &'static str {
        match self {
            BlockDeviceType::Ide => "ide",
            BlockDeviceType::Ahci => "ahci",
            BlockDeviceType::RamDisk => "ram",
            BlockDeviceType::Virtio => "virtio",
            BlockDeviceType::Unknown => "unknown",
        }
    }
}

#[derive(Copy, Clone)]
pub struct BlockDevice {
    pub major: u16,
    pub minor: u16,
    pub name: [u8; 32],
    pub name_len: usize,
    pub device_type: BlockDeviceType,
    pub block_size: u32,
    pub total_blocks: u64,
    pub read_only: bool,
    pub removable: bool,
}

impl BlockDevice {
    pub fn new(major: u16, minor: u16, name: &str, device_type: BlockDeviceType) -> Self {
        let mut n = [0u8; 32];
        let len = min(name.len(), 31);
        n[..len].copy_from_slice(name.as_bytes());

        BlockDevice {
            major,
            minor,
            name: n,
            name_len: len,
            device_type,
            block_size: BLOCK_SIZE as u32,
            total_blocks: 0,
            read_only: false,
            removable: false,
        }
    }

    pub fn name_str(&self) -> &str {
        core::str::from_utf8(&self.name[..self.name_len]).unwrap_or("")
    }

    pub fn capacity_mb(&self) -> u64 {
        (self.total_blocks as u64 * self.block_size as u64) / (1024 * 1024)
    }
}

#[derive(Copy, Clone)]
pub struct Partition {
    pub device: u8,
    pub number: u8,
    pub start_lba: u64,
    pub total_sectors: u64,
    pub partition_type: u8,
    pub bootable: bool,
}

impl Partition {
    pub fn new(device: u8, number: u8, start_lba: u64, total_sectors: u64, partition_type: u8) -> Self {
        Partition {
            device,
            number,
            start_lba,
            total_sectors,
            partition_type,
            bootable: false,
        }
    }

    pub fn size_mb(&self) -> u64 {
        self.total_sectors * BLOCK_SIZE as u64 / (1024 * 1024)
    }
}

pub struct BlockIoLayer {
    pub devices: [Option<BlockDevice>; MAX_BLOCK_DEVICES],
    pub device_count: u8,
    pub partitions: [Option<Partition>; MAX_PARTITIONS_PER_DEVICE],
    pub partition_count: u16,
    pub next_major: u16,
}

impl BlockIoLayer {
    pub const fn new() -> Self {
        BlockIoLayer {
            devices: [None; MAX_BLOCK_DEVICES],
            device_count: 0,
            partitions: [None; MAX_PARTITIONS_PER_DEVICE],
            partition_count: 0,
            next_major: 1,
        }
    }

    pub fn register_device(&mut self, name: &str, device_type: BlockDeviceType) -> u8 {
        let major = self.next_major;
        self.next_major += 1;

        let minor = 0;
        let dev = BlockDevice::new(major, minor, name, device_type);

        if (self.device_count as usize) < MAX_BLOCK_DEVICES {
            self.devices[self.device_count as usize] = Some(dev);
            self.device_count += 1;
        }

        major as u8
    }

    pub fn unregister_device(&mut self, major: u8) -> bool {
        for i in 0..self.device_count as usize {
            if let Some(dev) = &self.devices[i] {
                if dev.major == major as u16 {
                    self.devices[i] = None;
                    for j in i..self.device_count as usize - 1 {
                        self.devices[j] = self.devices[j + 1];
                    }
                    self.device_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn add_partition(&mut self, device: u8, number: u8, start_lba: u64, total_sectors: u64, part_type: u8) -> Option<u16> {
        if self.partition_count as usize >= MAX_PARTITIONS_PER_DEVICE {
            return None;
        }

        let part = Partition::new(device, number, start_lba, total_sectors, part_type);
        let index = self.partition_count;
        self.partitions[index as usize] = Some(part);
        self.partition_count += 1;
        Some(index)
    }

    pub fn remove_partition(&mut self, device: u8, number: u8) -> bool {
        for i in 0..self.partition_count as usize {
            if let Some(ref part) = self.partitions[i] {
                if part.device == device && part.number == number {
                    self.partitions[i] = None;
                    for j in i..self.partition_count as usize - 1 {
                        self.partitions[j] = self.partitions[j + 1];
                    }
                    self.partition_count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn get_device(&self, major: u16) -> Option<&BlockDevice> {
        for i in 0..self.device_count as usize {
            if let Some(dev) = &self.devices[i] {
                if dev.major == major {
                    return Some(dev);
                }
            }
        }
        None
    }

    pub fn list_devices(&self) {
        println!("MAJOR MINOR NAME         TYPE        SIZE CAPACITY");
        println!("----- ----- ------------ ----------- ---- --------");
        for i in 0..self.device_count as usize {
            if let Some(dev) = &self.devices[i] {
                println!("{:5} {:5} {:12} {:10} {} {}M",
                    dev.major, dev.minor, dev.name_str(),
                    dev.device_type.name(),
                    if dev.read_only { "ro" } else { "rw" },
                    dev.capacity_mb());
            }
        }

        if self.partition_count > 0 {
            println!("\nPartitions:");
            for i in 0..self.partition_count as usize {
                if let Some(part) = &self.partitions[i] {
                    println!("  dev{}p{}: LBA {} {} sectors ({} MB)",
                        part.device, part.number, part.start_lba,
                        part.total_sectors, part.size_mb());
                }
            }
        }
    }
}

pub static mut BLOCK_IO: BlockIoLayer = BlockIoLayer::new();

pub fn init_block_io() {
    println!("Block I/O: initialise");
}

pub mod requests {
    use super::*;

    #[derive(Copy, Clone)]
    pub struct BioRequest {
        pub device: u8,
        pub start_block: u64,
        pub count: u32,
        pub direction: BioDirection,
        pub buffer: *mut u8,
    }

    #[derive(Copy, Clone, PartialEq, Debug)]
    pub enum BioDirection {
        Read,
        Write,
        Flush,
    }

    impl BioRequest {
        pub fn new(device: u8, start_block: u64, count: u32, direction: BioDirection, buffer: *mut u8) -> Self {
            BioRequest { device, start_block, count, direction, buffer }
        }

        pub fn submit(&self) -> bool {
            match self.direction {
                BioDirection::Read => {
                    for i in 0..self.count {
                        let _block = self.start_block + i as u64;
                        let buf = unsafe {
                            core::slice::from_raw_parts_mut(self.buffer.add(i as usize * BLOCK_SIZE), BLOCK_SIZE)
                        };
                        buf.fill(0);
                    }
                    true
                }
                BioDirection::Write => {
                    true
                }
                BioDirection::Flush => {
                    true
                }
            }
        }
    }

    pub fn submit_device_request(device: u8, sector: u64, count: u32, direction: BioDirection, buffer: *mut u8) -> bool {
        let req = BioRequest::new(device, sector, count, direction, buffer);
        req.submit()
    }
}

pub mod io_scheduler {
    

    pub static IO_QUEUE_ENABLED: spin::Mutex<bool> = spin::Mutex::new(false);

    pub fn enable_io_queue() {
        *IO_QUEUE_ENABLED.lock() = true;
    }

    pub fn disable_io_queue() {
        *IO_QUEUE_ENABLED.lock() = false;
    }

    pub fn io_queue_enabled() -> bool {
        *IO_QUEUE_ENABLED.lock()
    }

    pub struct IOScheduler {
        pub queue_depth: u32,
        pub rq_count: u32,
        pub wq_count: u32,
    }

    impl IOScheduler {
        pub const fn new() -> Self {
            IOScheduler { queue_depth: 32, rq_count: 0, wq_count: 0 }
        }

        pub fn schedule(&self) -> u8 {
            0
        }
    }
}