use crate::{println, serial_println};
use bootloader::{bootinfo::{MemoryMap, MemoryRegionType}, BootInfo};
use x86_64::{structures::paging::{FrameAllocator, Mapper, mapper::MapToError,OffsetPageTable, Page, PageTable, PhysFrame, Size4KiB, page::PageRangeInclusive,},PhysAddr, VirtAddr,};
use crate::mem::allocator;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::PageTableFlags;
use core::arch::asm;


const THREAD_STACK_PAGE_INDEX: [u8;3] = [5,0,0];

struct MemoryInfo {
    boot_info: &'static BootInfo,
    phys_memory_offset: VirtAddr,
    frame_allocator: BootInfoFrameAllocator,
    kernel_page_tables: &'static mut PageTable
}

// useful struct to make init cleaner
static mut MEMORY_INFO: Option<MemoryInfo> = None;


///- `init`: This function initializes an `OffsetPageTable` which can translate virtual addresses to physical addresses and vice versa. It requires the `physical_memory_offset` which indicates the difference between the physical and virtual address of a page.
pub fn init(boot_info: &'static BootInfo) {
        interrupts::without_interrupts(|| {
        let mut memory_size = 0;
        for region in boot_info.memory_map.iter() {
            let start_addr = region.range.start_addr();
            let end_addr = region.range.end_addr();
            memory_size += end_addr - start_addr;
            println!("{:?} - [{:#016X}-{:#016X}]\n",region.region_type, start_addr, end_addr);
            serial_println!("{:?} - [{:#016X}-{:#016X}]\n",region.region_type, start_addr, end_addr);
        }
        println!("Memory size: {} Bytes\n", memory_size);
        serial_println!("Memory size: {} Bytes\n", memory_size);

        let phys_memory_offset = VirtAddr::new(boot_info.physical_memory_offset);

        let level_4_table = unsafe {active_level_4_table(phys_memory_offset)};

        let mut mapper = unsafe {OffsetPageTable::new(level_4_table, phys_memory_offset)};
        let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map)  };


        allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

        // Store boot_info for later calls
        unsafe { MEMORY_INFO = Some(MemoryInfo {
            boot_info,
            phys_memory_offset,
            frame_allocator,
            kernel_page_tables: level_4_table
        }) };
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

/// Copy a set of pagetables
fn copy_pagetables(level_4_table: &PageTable) -> (*mut PageTable, u64) {
    // Create a new level 4 pagetable
    let (table_ptr, table_physaddr) = create_pagetable();
    let table = unsafe {&mut *table_ptr};

    fn copy_pages_rec(physical_memory_offset: VirtAddr,
                      from_table: &PageTable, to_table: &mut PageTable,
                      level: u16) {
        for (i, entry) in from_table.iter().enumerate() {
            if !entry.is_unused() {
                if (level == 1) || entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                    // Maps a frame, not a page table
                    to_table[i].set_addr(entry.addr(), entry.flags());
                } else {
                    // Create a new table at level - 1
                    let (new_table_ptr, new_table_physaddr) = create_pagetable();
                    let to_table_m1 = unsafe {&mut *new_table_ptr};

                    // Point the entry to the new table
                    to_table[i].set_addr(PhysAddr::new(new_table_physaddr),
                                         entry.flags());

                    // Get reference to the input level-1 table
                    let from_table_m1 = {
                        let virt = physical_memory_offset + entry.addr().as_u64();
                        unsafe {& *virt.as_ptr()}
                    };

                    // Copy level-1 entries
                    copy_pages_rec(physical_memory_offset, from_table_m1, to_table_m1, level - 1);
                }
            }
        }
    }

    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};
    copy_pages_rec(memory_info.phys_memory_offset, level_4_table, table, 4);

    return (table_ptr, table_physaddr)
}

pub fn active_pagetable_ptr() -> *mut PageTable {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};
    let virt = memory_info.phys_memory_offset + active_pagetable_physaddr();
    virt.as_mut_ptr()
}



//-----------


//find the first bit to set to 1 in a bitmap
fn first_bit_location(bitmap: u32) -> u32 {
    let i: u32;
    unsafe {
        asm!("bsf eax, ecx",
             in("ecx") bitmap,
             lateout("eax") i,
             options(pure, nomem, nostack));
    }
    i
}
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

pub fn switch_to_pagetable(phys_addr: u64) {
    unsafe {
        asm!("mov cr3, {addr}",
             addr = in(reg) phys_addr);
    }
}
pub fn active_pagetable_physaddr() -> u64 {
    let mut physaddr: u64;
    unsafe {
        asm!("mov {addr}, cr3",
             addr = out(reg) physaddr);
    }
    physaddr
}

pub fn allocate_user_stack(level_4_table: *mut PageTable) -> Result<(u64, u64), &'static str> {
  
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};

    let mut table = unsafe {&mut *level_4_table};
    for index in THREAD_STACK_PAGE_INDEX {
        let entry = &mut table[index as usize];
        if entry.is_unused() {
            // Page not allocated -> Create page table
            let (_new_table_ptr, new_table_physaddr) = create_pagetable();
            entry.set_addr(PhysAddr::new(new_table_physaddr),
                           PageTableFlags::PRESENT |
                           PageTableFlags::WRITABLE |
                           PageTableFlags::USER_ACCESSIBLE);
        }
        table = unsafe {&mut *(memory_info.phys_memory_offset
                               + entry.addr().as_u64()).as_mut_ptr()};
    }

    // Table should now be the level 1 page table
    //
    // Find an unused set of 8 pages. The lowest page is always unused
    // (guard), but the first should be used so look in pages
    // (1 + 8*n) where n=0..64
    //
    // Choose a random n to start looking, and check entries
    // sequentially from there. For now just use process::unique_id
    use crate::proc::process;
    let n_start = process::unique_id(); // Modulo 64 soon
    for i in 0..64 {
        let n = ((n_start + i) % 64) as usize;

        if table[n * 8 + 1].is_unused() {
            // Found an empty slot:
            //  [n * 8] -> Empty (guard)
            //  [n * 8 + 1] -> User stack
            //      ...
            //  [n * 8 + 7] -> User stack

            for j in 1..8 {
                // Allocate user stack frames
                let entry = &mut table[n * 8 + j];

                let frame = memory_info.frame_allocator.allocate_frame()
                    .ok_or("Failed to allocate frame")?;

                entry.set_addr(frame.start_address(),
                               PageTableFlags::PRESENT |
                               PageTableFlags::WRITABLE |
                               PageTableFlags::USER_ACCESSIBLE);
            }

            // Return the virtual addresses of the top of the kernel and user stacks
            let slot_address: u64 = ((THREAD_STACK_PAGE_INDEX[0] as u64) << 39) +
                ((THREAD_STACK_PAGE_INDEX[1] as u64) << 30) +
                ((THREAD_STACK_PAGE_INDEX[2] as u64) << 21) +
                (((n * 8) as u64) << 12);

            return Ok((slot_address + 4096, slot_address + 8 * 4096)); // User stack
        }
    }

    Err("All thread stack slots full")
}


fn create_pagetable() -> (*mut PageTable,u64) {
    let mem_info = unsafe {MEMORY_INFO.as_mut().unwrap()};

    let level_4_table_frame = mem_info.frame_allocator.allocate_frame().unwrap();
    let phys = level_4_table_frame.start_address(); // Physical address
    let virt = mem_info.phys_memory_offset + phys.as_u64(); // Kernel virtual address
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    unsafe {(*page_table_ptr).zero();} // empty the page table

    (page_table_ptr, phys.as_u64())
}

pub fn allocate_pages_mapper(frame_allocator: &mut impl FrameAllocator<Size4KiB>,mapper: &mut impl Mapper<Size4KiB>,start_addr: VirtAddr,size: u64,flags: PageTableFlags) -> Result<(), MapToError<Size4KiB>> {
    let pages = {
        let end_addr = start_addr + size - 1u64;
        let start_page = Page::containing_address(start_addr);
        let end_page = Page::containing_address(end_addr); 
        Page::range_inclusive(start_page, end_page)
    };

    for page in pages {
        let frame = frame_allocator.allocate_frame().ok_or(MapToError::FrameAllocationFailed)?;
        unsafe {
            mapper.map_to(page,frame,flags,frame_allocator)?.flush()
        };
    }

    Ok(())
}
pub fn create_kernel_only_pagetable() -> (*mut PageTable, u64) {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};

    copy_pagetables(memory_info.kernel_page_tables)
}

pub fn create_user_pagetable() -> *mut PageTable {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};
    let table = unsafe {active_level_4_table(memory_info.phys_memory_offset)};
    table as *mut PageTable
}


pub fn allocate_pages(level_4_table: *mut PageTable, start_addr: VirtAddr, size: u64, flags: PageTableFlags) -> Result<(), MapToError<Size4KiB>> {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};
    let mut mapper = unsafe { OffsetPageTable::new(&mut *level_4_table, memory_info.phys_memory_offset)};
    allocate_pages_mapper(&mut memory_info.frame_allocator, &mut mapper, start_addr, size, flags)
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
            serial_println!("cannot deallocate {:?}", page);
        }
    }
}

