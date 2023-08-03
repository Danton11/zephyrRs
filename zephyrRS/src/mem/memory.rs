use crate::{println, serial_println};
use bootloader::{bootinfo::{MemoryMap, MemoryRegionType}, BootInfo};
use x86_64::{structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PhysFrame, Size4KiB, page::PageRangeInclusive,},PhysAddr, VirtAddr,};
use core::sync::atomic::{AtomicU64, Ordering};
use crate::mem::allocator;



//Remember that page tables are used by the MMU (Memory Management Unit) to translate virtual addresses to physical addresses. When a program accesses an address, it provides a virtual address, which the MMU then translates to a physical address. The physical address is then used to access the actual data in memory. The mapping from virtual to physical addresses is done through a set of hierarchical page tables.

///- `init`: This function initializes an `OffsetPageTable` which can translate virtual addresses to physical addresses and vice versa. It requires the `physical_memory_offset` which indicates the difference between the physical and virtual address of a page.

pub fn init(boot_info: &'static BootInfo) {
    use x86_64::instructions::interrupts;
    use x86_64::structures::paging::Translate; // provides translate_addr

    interrupts::without_interrupts(|| {
        let mut memory_size = 0;
        for region in boot_info.memory_map.iter() {
            let start_addr = region.range.start_addr();
            let end_addr = region.range.end_addr();
            memory_size += end_addr - start_addr;
            println!("MEM [{:#016X}-{:#016X}] {:?}\n", start_addr, end_addr, region.region_type);
            serial_println!("MEM [{:#016X}-{:#016X}] {:?}\n", start_addr, end_addr, region.region_type);
        }
        println!("Memory size: {} KB\n", memory_size >> 10);
        serial_println!("Memory size: {} KB\n", memory_size >> 10);

        let phys_memory_offset = VirtAddr::new(boot_info.physical_memory_offset);

        let level_4_table = unsafe {active_level_4_table(phys_memory_offset)};

        // Initialise the memory mapper
        let mut mapper = unsafe {OffsetPageTable::new(level_4_table, phys_memory_offset)};
        let mut frame_allocator = unsafe {
            BootInfoFrameAllocator::init(&boot_info.memory_map)
        };

        let addresses = [
            // the identity-mapped vga buffer page
            0xb8000,
            // some code page
            0x201008,
            // some stack page
            0x0100_0020_1a10,
            // virtual address mapped to physical address 0
            boot_info.physical_memory_offset,
        ];

        for &address in &addresses {
            let virt = VirtAddr::new(address);
            // new: use the `mapper.translate_addr` method
            let phys = mapper.translate_addr(virt);
            println!("{:?} -> {:?}", virt, phys);
            serial_println!("{:?} -> {:?}", virt, phys);
        }

        allocator::init_heap(&mut mapper, &mut frame_allocator)
            .expect("heap initialization failed");
    });
}

///- `active_level_4_table`: This function returns a mutable reference to the level 4 page table currently active in the CPU. It reads the value from the CR3 register (which contains the physical address of the active level 4 table) and converts it to the equivalent virtual address using the provided `physical_memory_offset`.
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read(); // gets the address of the l4 table from the cr3 register

    let phys = level_4_table_frame.start_address(); // actual address of the page table
    let virt = physical_memory_offset + phys.as_u64(); // offsetted address of the page table
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr(); // virtual pointer to l4 table

    &mut *page_table_ptr // unsafe
}

///- `create_example_mapping`: This function maps a provided page to the frame at the physical address `0xb8000` with the `PRESENT` and `WRITABLE` flags. It's used for testing purposes and not safe to use in a real kernel since the frame at `0xb8000` might be already in use.

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize, 
    frame_usage: [bool; 2000],
}

///- `BootInfoFrameAllocator`: This frame allocator uses the memory map provided by the bootloader to track which frames are available for allocation. It filters out all memory regions that are marked as `USABLE` and provides an iterator over all usable frames. Each time a frame is allocated, it increments the `next` index to keep track of the next frame to allocate.

impl BootInfoFrameAllocator {
    ///- `init`: This method initializes the `BootInfoFrameAllocator`. It is marked as `unsafe` because the caller must ensure that the provided memory map is correct.
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        let _frame_count = memory_map.iter().count();
        //println!("num of entries: {}", frame_count);
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
            frame_usage: [false; 2000],
        }
    }

    /// Returns the total size of usable memory.

    pub fn total_usable_size(&self) -> u64 {
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        let total: u64 = usable_regions.map(|r| r.range.end_addr() - r.range.start_addr()).sum();
        total
    }


    fn mark_frame_as_used(&mut self, frame: PhysFrame) {
        let frame_num = frame.start_address().as_u64() as usize / 4096; // convert frame address to frame number
        self.frame_usage[frame_num] = true; // mark frame as used
    }

    fn is_frame_free(&self, frame: PhysFrame) -> bool {
        let frame_num = frame.start_address().as_u64() as usize / 4096; // convert frame address to frame number
        !self.frame_usage[frame_num] // return true if frame is free
    }

    ///- `usable_frames`: This method returns an iterator over all frames marked as `USABLE` in the memory map. It does this by iterating over all regions in the memory map, filtering out regions not marked as `USABLE`, and then converting each region's address range to a list of frame addresses.
    /// Returns an iterator over the usable frames specified in the memory map.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        // map each region to its address range
        let addr_ranges = usable_regions.map(|r| r.range.start_addr()..r.range.end_addr());
        // transform to an iterator of frame start addresses
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        // create `PhysFrame` types from the start addresses
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

///- `allocate_frame`: This method allocates a new frame of memory by returning the next usable frame from the memory map. After each allocation, it increments the `next` index to point to the next usable frame.
unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        match frame {
            Some(frame) if self.is_frame_free(frame) => {
                self.mark_frame_as_used(frame);
                self.next += 1;
                Some(frame)
            }
            _ => { 
                serial_println!("no free frames...");
                None 
            },
        }
    }
}


static NEXT_PAGE_ADDR: AtomicU64 = AtomicU64::new(0x1000);  // Start at 0x1000
const MAX_ADDR: u64 = 0xFFFF_FFFF_FFFF_F000;

use x86_64::structures::paging::PageTableFlags;

// ... rest of your code ...

pub fn allocate_page(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) -> Result<VirtAddr, &'static str> {
    // Allocate a frame of physical memory
    let frame = frame_allocator.allocate_frame().ok_or("Out of memory")?;

    // Generate a virtual page address
    let mut page_addr = NEXT_PAGE_ADDR.load(Ordering::SeqCst);

    // Create a virtual page at the next unused address
    let mut page = Page::containing_address(VirtAddr::new(page_addr));

    // Check if a virtual page at the chosen address already exists
    while mapper.translate_page(page).is_ok() {
        // If it exists, increment the address and try again
        page_addr = NEXT_PAGE_ADDR.fetch_add(0x1000, Ordering::SeqCst);
        // If we have reached the maximum address, return an error
        if page_addr >= MAX_ADDR {
            return Err("No virtual address space left");
        }
        page = Page::containing_address(VirtAddr::new(page_addr));
    }

    // Map the page to the allocated frame
    let page_table_flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    unsafe {
        mapper.map_to(page, frame, page_table_flags, &mut *frame_allocator).map_err(|_| "Failed to create mapping")?;
    }

    // Return the start address of the page
    Ok(page.start_address())
}

pub fn deallocate_page(mapper: &mut impl Mapper<Size4KiB>,address:u64,size:usize){
    let pages: PageRangeInclusive<Size4KiB> = {
        let start = Page::containing_address(VirtAddr::new(address));
        let end   = Page::containing_address(VirtAddr::new(address + (size as u64) -1 ));
        Page::range_inclusive(start, end)
    };

    for page in pages {
        if let Ok((_frame, mapping)) = mapper.unmap(page){
            mapping.flush();
        }else {
            serial_println!("cannot dealloc {:?}", page);
        }
    }
}

