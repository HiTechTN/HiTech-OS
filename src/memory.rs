use spin::Mutex;
use x86_64::structures::paging::{
    Page, PageTable, PageTableFlags, PhysFrame,
};
pub use x86_64::structures::paging::page_table::FrameError;
use x86_64::{PhysAddr, VirtAddr};

pub const PAGE_SIZE: usize = 4096;
pub const HUGE_PAGE_SIZE: usize = 2 * 1024 * 1024;

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum MemoryZone {
    Usable,
    Reserved,
    ACPIReclaimable,
    ACPINVS,
}

#[derive(Copy, Clone)]
pub struct MemoryRegion {
    pub start: PhysAddr,
    pub size: usize,
    pub zone: MemoryZone,
}

pub const MAX_REGIONS: usize = 32;

pub struct MemoryMap {
    pub regions: [Option<MemoryRegion>; MAX_REGIONS],
    pub count: usize,
}

impl MemoryMap {
    pub const fn new() -> Self {
        MemoryMap {
            regions: [None; MAX_REGIONS],
            count: 0,
        }
    }

    pub fn add_region(&mut self, start: u64, size: u64, zone: MemoryZone) {
        if self.count < MAX_REGIONS {
            self.regions[self.count] = Some(MemoryRegion {
                start: PhysAddr::new(start),
                size: size as usize,
                zone,
            });
            self.count += 1;
        }
    }

    pub fn total_usable(&self) -> usize {
        let mut total = 0;
        for i in 0..self.count {
            if let Some(ref r) = self.regions[i] {
                if r.zone == MemoryZone::Usable {
                    total += r.size;
                }
            }
        }
        total
    }
}

pub static KERNEL_MEMORY_MAP: Mutex<MemoryMap> = Mutex::new(MemoryMap::new());

pub fn init_paging(boot_info: &bootloader::BootInfo) {
    let mut mem_map = KERNEL_MEMORY_MAP.lock();
    
    for region in boot_info.memory_map.iter() {
        let zone = match region.region_type {
            bootloader::bootinfo::MemoryRegionType::Usable => MemoryZone::Usable,
            bootloader::bootinfo::MemoryRegionType::Reserved => MemoryZone::Reserved,
            bootloader::bootinfo::MemoryRegionType::AcpiReclaimable => MemoryZone::ACPINVS,
            _ => MemoryZone::Reserved,
        };
        mem_map.add_region(region.range.start_addr(), region.range.end_addr() - region.range.start_addr(), zone);
    }
    
    println!("Memoire detectee: {} KB", mem_map.total_usable() / 1024);
}

pub mod paging {
    use super::*;

    pub fn virt_to_phys(vaddr: VirtAddr) -> Option<PhysAddr> {
        let page = Page::containing_address(vaddr);
        let page_table = unsafe { &mut *(0xfffff000 as *mut PageTable) };
        
        let pml4_entry = &page_table[page.p4_index()];
        let pml4_frame = pml4_entry.frame().ok()?;
        let pdpt = unsafe { &*(pml4_frame.start_address().as_u64() as *mut PageTable) };
        let pdpt_entry = &pdpt[page.p3_index()];
        let pdpt_frame = pdpt_entry.frame().ok()?;
        
        if pdpt_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            let offset = page.start_address().as_u64() & 0x3ffffff;
            return Some(PhysAddr::new(pdpt_frame.start_address().as_u64() + offset));
        }

        let pd = unsafe { &*(pdpt_frame.start_address().as_u64() as *mut PageTable) };
        let pd_entry = &pd[page.p2_index()];
        let pd_frame = pd_entry.frame().ok()?;

        if pd_entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            let offset = page.start_address().as_u64() & 0x1fffff;
            return Some(PhysAddr::new(pd_frame.start_address().as_u64() + offset));
        }

        let pt = unsafe { &*(pd_frame.start_address().as_u64() as *mut PageTable) };
        let pt_entry = &pt[page.p1_index()];
        let pt_frame = pt_entry.frame().ok()?;

        Some(pt_frame.start_address())
    }

    pub fn map_page(virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags) -> Result<(), ()> {
        let page = Page::containing_address(virt);
        let frame = PhysFrame::containing_address(phys);
        
        let page_table = unsafe { &mut *(0xfffff000 as *mut PageTable) };
        
        let pml4_index = page.p4_index();
        let pdpt_index = page.p3_index();
        let pd_index = page.p2_index();
        let pt_index = page.p1_index();

        if page_table[pml4_index].frame().is_err() {
            return Err(());
        }
        
        let pdpt = unsafe { &mut *(page_table[pml4_index].frame().unwrap().start_address().as_u64() as *mut PageTable) };
        
        if pdpt[pdpt_index].frame().is_err() {
            return Err(());
        }
        
        let pd = unsafe { &mut *(pdpt[pdpt_index].frame().unwrap().start_address().as_u64() as *mut PageTable) };
        
        if pd[pd_index].frame().is_err() {
            return Err(());
        }
        
        let pt = unsafe { &mut *(pd[pd_index].frame().unwrap().start_address().as_u64() as *mut PageTable) };
        
        pt[pt_index].set_frame(frame, flags);
        
        Ok(())
    }

    pub fn unmap_page(virt: VirtAddr) -> Result<(), ()> {
        let page = Page::containing_address(virt);
        let page_table = unsafe { &mut *(0xfffff000 as *mut PageTable) };
        
        let pml4_entry = page_table[page.p4_index()].frame().map_err(|_| ())?;
        let pdpt = unsafe { &mut *(pml4_entry.start_address().as_u64() as *mut PageTable) };
        let pdpt_entry = pdpt[page.p3_index()].frame().map_err(|_| ())?;

        let pd = unsafe { &mut *(pdpt_entry.start_address().as_u64() as *mut PageTable) };
        let pd_entry = pd[page.p2_index()].frame().map_err(|_| ())?;

        let pt = unsafe { &mut *(pd_entry.start_address().as_u64() as *mut PageTable) };
        pt[page.p1_index()].set_unused();
        
        Ok(())
    }
}

pub fn get_physical_memory_info() -> (u64, u64) {
    let mem_map = KERNEL_MEMORY_MAP.lock();
    let total = mem_map.total_usable() as u64;
    (total / 1024 / 1024, total)
}