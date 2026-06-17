use spin::Mutex;

pub const MAX_IRQS: usize = 256;
pub const MAX_DMA_CHANNELS: usize = 8;
pub const MAX_IOPORTS: usize = 1024;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ResourceType {
    Irq,
    Dma,
    IoPort,
    MemoryRange,
    PciBar,
}

impl ResourceType {
    pub fn name(&self) -> &'static str {
        match self {
            ResourceType::Irq => "irq",
            ResourceType::Dma => "dma",
            ResourceType::IoPort => "ioport",
            ResourceType::MemoryRange => "mem",
            ResourceType::PciBar => "pci_bar",
        }
    }
}

#[derive(Copy, Clone)]
pub struct Resource {
    pub r#type: ResourceType,
    pub start: u64,
    pub end: u64,
    pub owner: [u8; 32],
    pub owner_len: usize,
    pub exclusive: bool,
}

impl Resource {
    pub fn new(r#type: ResourceType, start: u64, end: u64, owner: &str, exclusive: bool) -> Self {
        let mut o = [0u8; 32];
        let len = core::cmp::min(owner.len(), 31);
        o[..len].copy_from_slice(owner.as_bytes());
        Resource { r#type, start, end, owner: o, owner_len: len, exclusive }
    }

    pub fn owner_str(&self) -> &str {
        core::str::from_utf8(&self.owner[..self.owner_len]).unwrap_or("?")
    }

    pub fn contains(&self, val: u64) -> bool {
        val >= self.start && val <= self.end
    }

    pub fn overlaps(&self, other: &Resource) -> bool {
        self.r#type == other.r#type && self.start <= other.end && other.start <= self.end
    }
}

pub struct ResourceManager {
    pub resources: [Option<Resource>; 256],
    pub count: usize,
}

impl ResourceManager {
    pub const fn new() -> Self {
        ResourceManager { resources: [None; 256], count: 0 }
    }

    pub fn register(&mut self, resource: Resource) -> bool {
        if self.count >= 256 {
            return false;
        }

        if resource.exclusive {
            for i in 0..self.count {
                if let Some(existing) = &self.resources[i] {
                    if existing.exclusive && existing.overlaps(&resource) {
                        println!("Conflit: {} {:#x}-{:#x} avec {:#x}-{:#x} ({})",
                            resource.r#type.name(), resource.start, resource.end,
                            existing.start, existing.end, existing.owner_str());
                        return false;
                    }
                }
            }
        }

        self.resources[self.count] = Some(resource);
        self.count += 1;
        true
    }

    pub fn unregister(&mut self, owner: &str) -> usize {
        let mut removed = 0;
        let mut i = 0;
        while i < self.count {
            if let Some(ref res) = self.resources[i] {
                if res.owner_str() == owner {
                    self.resources[i] = None;
                    for j in i..self.count - 1 {
                        self.resources[j] = self.resources[j + 1];
                    }
                    self.count -= 1;
                    removed += 1;
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        removed
    }

    pub fn find(&self, r#type: ResourceType, start: u64) -> Option<&Resource> {
        for i in 0..self.count {
            if let Some(res) = &self.resources[i] {
                if res.r#type == r#type && res.start == start {
                    return Some(res);
                }
            }
        }
        None
    }

    pub fn find_by_owner(&self, owner: &str) -> alloc::vec::Vec<&Resource> {
        let mut result = alloc::vec::Vec::new();
        for i in 0..self.count {
            if let Some(res) = &self.resources[i] {
                if res.owner_str() == owner {
                    result.push(res);
                }
            }
        }
        result
    }

    pub fn request_irq(&mut self, irq: u8, owner: &str) -> bool {
        for i in 0..self.count {
            if let Some(existing) = &self.resources[i] {
                if existing.r#type == ResourceType::Irq && existing.contains(irq as u64) && existing.exclusive {
                    return false;
                }
            }
        }
        let res = Resource::new(ResourceType::Irq, irq as u64, irq as u64, owner, true);
        self.register(res)
    }

    pub fn release_irq(&mut self, irq: u8, owner: &str) -> bool {
        for i in 0..self.count {
            if let Some(ref res) = self.resources[i] {
                if res.r#type == ResourceType::Irq && res.start == irq as u64 && res.owner_str() == owner {
                    self.resources[i] = None;
                    for j in i..self.count - 1 {
                        self.resources[j] = self.resources[j + 1];
                    }
                    self.count -= 1;
                    return true;
                }
            }
        }
        false
    }

    pub fn request_ioports(&mut self, start: u16, end: u16, owner: &str) -> bool {
        let res = Resource::new(ResourceType::IoPort, start as u64, end as u64, owner, true);
        self.register(res)
    }

    pub fn list(&self) {
        println!("RESSOURCES:");
        println!("TYPE     DEBUT      FIN        PROPRIETAIRE");
        println!("-------- ---------- ---------- --------------------------------");
        for i in 0..self.count {
            if let Some(ref res) = self.resources[i] {
                println!("{:8} {:#010x} {:#010x} {}", res.r#type.name(), res.start, res.end, res.owner_str());
            }
        }
    }
}

pub static RESOURCE_MANAGER: Mutex<ResourceManager> = Mutex::new(ResourceManager::new());

pub fn init_resource() {
    println!("Ressources: gestionnaire de ressources systeme");
}

pub mod dma {
    use super::*;

    #[derive(Copy, Clone)]
    pub struct DmaChannel {
        pub channel: u8,
        pub allocated: bool,
        pub owner: [u8; 32],
        pub owner_len: usize,
        pub mode: u8,
        pub address: u32,
        pub count: u16,
    }

    impl DmaChannel {
        pub const fn new(channel: u8) -> Self {
            DmaChannel {
                channel,
                allocated: false,
                owner: [0; 32],
                owner_len: 0,
                mode: 0,
                address: 0,
                count: 0,
            }
        }
    }

    pub struct DmaController {
        pub channels: [DmaChannel; MAX_DMA_CHANNELS],
    }

    impl DmaController {
        pub const fn new() -> Self {
            DmaController {
                channels: [
                    DmaChannel::new(0), DmaChannel::new(1), DmaChannel::new(2),
                    DmaChannel::new(3), DmaChannel::new(4), DmaChannel::new(5),
                    DmaChannel::new(6), DmaChannel::new(7),
                ],
            }
        }

        pub fn allocate(&mut self, channel: u8, owner: &str) -> bool {
            if channel as usize >= MAX_DMA_CHANNELS {
                return false;
            }
            if self.channels[channel as usize].allocated {
                return false;
            }
            let c = &mut self.channels[channel as usize];
            let len = core::cmp::min(owner.len(), 31);
            c.owner[..len].copy_from_slice(owner.as_bytes());
            c.owner_len = len;
            c.allocated = true;
            true
        }

        pub fn free(&mut self, channel: u8) -> bool {
            if channel as usize >= MAX_DMA_CHANNELS {
                return false;
            }
            let c = &mut self.channels[channel as usize];
            if !c.allocated {
                return false;
            }
            c.allocated = false;
            c.owner = [0; 32];
            c.owner_len = 0;
            true
        }
    }

    pub static DMA: Mutex<DmaController> = Mutex::new(DmaController::new());
}

pub mod pnp {
    

    pub const PNP_VENDOR_ID: u32 = 0x0000;

    pub struct PnpDevice {
        pub vendor: u16,
        pub product: u16,
        pub serial: u32,
        pub irq: u8,
        pub iobase: [u16; 4],
        pub drq: u8,
    }

    impl PnpDevice {
        pub const fn new() -> Self {
            PnpDevice { vendor: 0, product: 0, serial: 0, irq: 0, iobase: [0; 4], drq: 0 }
        }
    }
}