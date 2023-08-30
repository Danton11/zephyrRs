/*!
# Memory Management and Paging Utilities for a Kernel

This module provides a set of utilities and abstractions for managing memory and paging in a kernel environment, especially designed for x86_64 architectures. The tools here can be utilized for initializing and managing page tables, allocating and deallocating memory frames, handling page faults, and dealing with virtual-to-physical memory mappings.

## Key Components:

1. **MemoryInfo**: A structure holding important information about the boot state of the system, including boot info, physical memory offset, a frame allocator, and the kernel's level 4 page table.

2. **PageFrameAllocator**: An allocator for physical memory frames. It leverages the memory map provided by the bootloader to keep track of used and free frames.

3. **OffsetPageTable**: A utility for mapping virtual addresses to physical addresses and vice versa.

4. **FrameAllocator**: An interface for allocating and deallocating physical memory frames.

5. **Mapping Utilities**: Functions to map and unmap pages, allocate user-space stacks, and handle page faults.

6. **Debugging and Testing**: Utilities to print memory layout, perform checks, and benchmark memory allocation performance.

## How to Use:

1. Call `init` with the boot info to initialize the memory system. This sets up the page tables, memory map, and allocates the initial heap.
 
2. Use the `PageFrameAllocator` to allocate and deallocate physical memory frames as needed.

3. Use `OffsetPageTable` to manage and manipulate the virtual memory mappings.

4. `allocate_pages` and `deallocate_page` can be used to allocate and free pages in virtual memory.

5. Handle page faults and on-demand memory allocations with the provided utilities.

6. Use `test_alloc_times` to benchmark the performance of the memory allocation system.

**Note**: Many of these functions are unsafe and should be used with caution. Ensure that you have a good understanding of virtual memory, paging, and the specific requirements and quirks of the x86_64 architecture before manipulating these utilities.

*/


use crate::{println, serial_println};
use bootloader::{bootinfo::{MemoryMap, MemoryRegionType}, BootInfo};
use x86_64::{structures::paging::{FrameAllocator, Mapper, mapper::MapToError,OffsetPageTable, Page, PageTable, PhysFrame, Size4KiB, page::PageRangeInclusive, PageSize, page_table::PageTableEntry, Translate,},PhysAddr, VirtAddr,};
use crate::mem::allocator;
use alloc::{vec, borrow::ToOwned};
use alloc::string::String;
use core::fmt;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::PageTableFlags;
use core::arch::asm;
use crate::MEMORYLOGGER;
use crate::proc::process;
use lazy_static::lazy_static;
pub mod memory_logger;
use memory_logger::{MemoryLogger, MemoryRegion};

const USER_STACK_PAGE_INDEX: [u8;3] = [5,0,0];

/**
MemoryInfo stores information about the kernel's memory environment. 
It has references to boot information, the physical memory offset, 
the frame allocator, and the kernel's top-level page table.

The static variable MEMORY_INFO is used to store this information for global access.
*/
struct MemoryInfo {
    boot_info: &'static BootInfo,
    phys_memory_offset: VirtAddr,
    frame_allocator: PageFrameAllocator,
    kernel_page_tables: &'static mut PageTable
}

// useful struct to make init cleaner
static mut MEMORY_INFO: Option<MemoryInfo> = None;
///- `init`: This function initializes an `OffsetPageTable` which can translate virtual addresses to physical addresses and vice versa. It requires the `physical_memory_offset` which indicates the difference between the physical and virtual address of a page.
pub fn init(boot_info: &'static BootInfo) {
        interrupts::without_interrupts(|| {
        let mut memory_size = 0;

        println!("-------------------------------Memory Layout ----------------");
        println!("{:<20} {:<20} {:<20} {:<15}", "Region Type", "Start Address", "End Address", "Size");
        println!("-----------------------------------------------");
        serial_println!("-----------------------------------------Memory Layout --------------------------------------");
        serial_println!("{:<20} {:<20} {:<20} {:<15}", "Region Type", "Start Address", "End Address", "Size (bytes) ");
        serial_println!("---------------------------------------------------------------------------------------------");

        for region in boot_info.memory_map.iter() {
            let start_addr = region.range.start_addr();
            let end_addr = region.range.end_addr();
            let region_size = end_addr - start_addr;
            memory_size += region_size;


            println!("{:<20?} {:<#20X} {:<#20X} {:<15}", region.region_type, start_addr, end_addr, region_size);
            serial_println!("{:<20?}                {:<#20X} {:<#20X} {:<15}", region.region_type, start_addr, end_addr, region_size);
        }



        println!("-----------------------------------------------");
        println!("Total Memory Size: {}", memory_size);
        println!("-----------------------------------------------");
        serial_println!("-------------------------------------------------------------------------------");
        serial_println!("Total Memory Size: {}", memory_size);
        serial_println!("-------------------------------------------------------------------------------");

        

        let phys_memory_offset = VirtAddr::new(boot_info.physical_memory_offset);

        let level_4_table = unsafe {active_level_4_table(phys_memory_offset)};

        let mut mapper = unsafe {OffsetPageTable::new(level_4_table, phys_memory_offset)};
        let mut frame_allocator = unsafe { PageFrameAllocator::init(&boot_info.memory_map, phys_memory_offset)  };


        allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialization failed");

        MEMORYLOGGER.lock().setTotalMemory(memory_size as usize);
        MEMORYLOGGER.lock().log_stats();
        // Store boot_info for later calls
        unsafe { MEMORY_INFO = Some(MemoryInfo {
            boot_info,
            phys_memory_offset,
            frame_allocator,
            kernel_page_tables: level_4_table
        }) };
    });
}


fn region_type_to_str(region_type: &MemoryRegionType) -> &'static str {
    match *region_type {
        MemoryRegionType::Usable => "Usable",
        MemoryRegionType::Reserved => "Reserved",
        MemoryRegionType::FrameZero => "FrameZero",
        MemoryRegionType::PageTable => "FrameZero",
        MemoryRegionType::Kernel => "FrameZero",
        MemoryRegionType::KernelStack => "FrameZero",
        MemoryRegionType::BootInfo => "FrameZero",
        MemoryRegionType::Bootloader => "FrameZero",
        // ... add other variants here ...
        _ => "Unknown",
    }
}

fn format_region_type(region_type: String, width: usize) -> String {
    let padding = width.saturating_sub(region_type.len());
    region_type.to_owned() + &" ".repeat(padding.to_owned())
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


/// Copies a set of page tables to create a new identical mapping.
///
/// # Arguments
///
/// * `level_4_table`: The root (level 4) page table to copy from.
///
/// # Returns
///
/// Returns a tuple containing a pointer to the new level 4 page table and its physical address.
fn copy_pagetables(level_4_table: &PageTable) -> (*mut PageTable, u64) {
    // Create a new level 4 page table
    let (table_ptr, table_physaddr) = create_pagetable();
    let table = unsafe { &mut *table_ptr };

    /// Recursively copies pages from one table to another.
    ///
    /// # Arguments
    ///
    /// * `physical_memory_offset`: The offset to convert physical addresses to virtual addresses.
    /// * `from_table`: The source page table.
    /// * `to_table`: The destination page table.
    /// * `level`: The current level of the page table hierarchy.
    fn copy_pages_rec(
        physical_memory_offset: VirtAddr,
        from_table: &PageTable,
        to_table: &mut PageTable,
        level: u16,
    ) {
        // Iterate through each entry in the source table
        for (i, entry) in from_table.iter().enumerate() {
            // Skip unused entries
            if !entry.is_unused() {
                if level == 1 || entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                    // If we are at level 1 or the entry points to a huge page,
                    // directly map the frame.
                    to_table[i].set_addr(entry.addr(), entry.flags());
                } else {
                    // Otherwise, create a new table for the next level down
                    let (new_table_ptr, new_table_physaddr) = create_pagetable();
                    let to_table_m1 = unsafe { &mut *new_table_ptr };

                    // Update the entry in the destination table to point to the new table
                    to_table[i].set_addr(PhysAddr::new(new_table_physaddr), entry.flags());

                    // Obtain a reference to the corresponding table in the source mapping
                    let from_table_m1 = {
                        let virt = physical_memory_offset + entry.addr().as_u64();
                        unsafe { &*virt.as_ptr() }
                    };

                    // Recursively copy entries from the source table to the new table
                    copy_pages_rec(physical_memory_offset, from_table_m1, to_table_m1, level - 1);
                }
            }
        }
    }

    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Start the recursive copy from the root (level 4) page table
    copy_pages_rec(memory_info.phys_memory_offset, level_4_table, table, 4);

    // Return the new level 4 table pointer and its physical address
    return (table_ptr, table_physaddr);
}

pub fn active_pagetable_ptr() -> *mut PageTable {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};
    let virt = memory_info.phys_memory_offset + active_pagetable_physaddr();
    virt.as_mut_ptr()
}



//-----------
/**
Manages the allocation and deallocation of physical frames of memory.
Uses the memory map provided by the bootloader to determine which frames are usable.
Contains functions like allocate_frame, deallocate_frame, usable_frames, etc., to manage frames.
*/

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct PageFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize, 
    frame_usage: [bool;  100000],
    phys_memory_offset: VirtAddr,
}

///- `BootInfoFrameAllocator`: This frame allocator uses the memory map provided by the bootloader to track which frames are available for allocation. It filters out all memory regions that are marked as `USABLE` and provides an iterator over all usable frames. Each time a frame is allocated, it increments the `next` index to keep track of the next frame to allocate.


impl PageFrameAllocator {
    ///- `init`: This method initializes the `BootInfoFrameAllocator`. It is marked as `unsafe` because the caller must ensure that the provided memory map is correct.
    /// Create a FrameAllocator from the passed memory map.
    ///
    /// This function is unsafe because the caller must guarantee that the passed
    /// memory map is valid. The main requirement is that all frames that are marked
    /// as `USABLE` in it are really unused.
    pub unsafe fn init(memory_map: &'static MemoryMap, phys_memory_offset: VirtAddr ) -> Self {
        let _frame_count = memory_map.iter().count();
        //println!("num of entries: {}", frame_count);
        PageFrameAllocator {
            memory_map,
            next: 0,
            frame_usage: [false; 100000],
            phys_memory_offset
        }
    }

    pub fn total_frames(&self) -> usize {
    // get usable regions from memory map
        let regions = self.memory_map.iter();
        let usable_regions = regions.filter(|r| r.region_type == MemoryRegionType::Usable);
        // sum up the size of each region and divide by the size of a frame to get the number of frames
        let total_frames: usize = usable_regions.map(|r| (r.range.end_addr() - r.range.start_addr()) as usize / 4096).sum();
        total_frames
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

    // method to translate a physical address to a virtual one
    fn translate_addr_phys_to_virt(&self, addr: u64) -> VirtAddr {
        self.phys_memory_offset + addr
    }

    // method to translate a virtual address to a physical one
    fn translate_addr_virt_to_phys(&self, addr: VirtAddr) -> u64 {
        addr.as_u64() - self.phys_memory_offset.as_u64()
    }

    fn deallocate_frame(&mut self, frame: PhysFrame) {
        // Convert frame address to frame number
        let frame_num = frame.start_address().as_u64() as usize / 4096;

        //serial_println!("frame_address: {:?}", frame.start_address());
        // Check if the frame is within the bounds of the frame_usage array
        if frame_num < self.frame_usage.len() {
            // Mark frame as free
            self.frame_usage[frame_num] = false;

            // If the frame is before the current 'next' index, update 'next'
            if frame_num < self.next {
                self.next = frame_num;
            }


        } else {
            serial_println!("Error: Frame number {} is out of bounds", frame_num);
        }
        MEMORYLOGGER.lock().log_deallocation(MemoryRegion {start_address: frame.start_address().as_u64(), end_address: (frame.start_address() + frame.size()).as_u64()}, frame.size() as usize, true);
    }
}

///- `allocate_frame`: This method allocates a new frame of memory by returning the next usable frame from the memory map. After each allocation, it increments the `next` index to point to the next usable frame.
unsafe impl FrameAllocator<Size4KiB> for PageFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        match frame {
            Some(frame) if self.is_frame_free(frame) => {
                self.mark_frame_as_used(frame);
                self.next += 1;
                MEMORYLOGGER.lock().log_allocation(MemoryRegion {start_address: frame.start_address().as_u64(), end_address: (frame.start_address() + frame.size()).as_u64()}, frame.size() as usize, true);
                Some(frame)
            }
            _ => { 
                serial_println!("no free frames...");
                None 
            },
        }
    }
}

/**
When the CPU switches from one process to another, or from user-space to kernel-space, 
the page tables need to be switched out as well.
This ensures that the running code (be it a process or the kernel) accesses the correct 
memory locations in the context of its own virtual address space.

1. kernel_mode():
This function switches the active page table to the kernel's page table. 
This is particularly useful when:

The OS is done executing a user-space program and needs to return to kernel mode.
An interrupt or system call occurs, requiring the CPU to jump from user-space to kernel-space.

When such a transition is made, the kernel needs to see its own memory layout and not the layout of the user-space program. Thus, it "switches" to its own page table using this function.
*/
pub fn kernel_mode() {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};
    let phys_addr = (memory_info.kernel_page_tables as *mut PageTable as u64)
        - memory_info.phys_memory_offset.as_u64();
    switch_to_pagetable(phys_addr);
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


/// Recursively free all pages and page tables, including the page
/// table at the given table_physaddr.
fn free_pages_rec(physical_memory_offset: VirtAddr,
                  frame_allocator: &mut PageFrameAllocator,
                  physaddr: PhysAddr,
                  level: u16) {

    if level == 0 {
        // A frame, not a table
        frame_allocator.deallocate_frame(
            PhysFrame::containing_address(physaddr));
        return;
    }

    let table = unsafe{&mut *(physical_memory_offset
                              + physaddr.as_u64())
                       .as_mut_ptr() as &mut PageTable};
    for entry in table.iter() {
        if !entry.is_unused() {
            if level == 1 || entry.flags().contains(PageTableFlags::HUGE_PAGE) {
                // Maps a frame, not a page table
                if entry.flags().contains(PageTableFlags::PRESENT |
                                          PageTableFlags::WRITABLE |
                                          PageTableFlags::USER_ACCESSIBLE)  {
                    // A user frame => deallocate
                    frame_allocator.deallocate_frame(
                        entry.frame().unwrap());
                }
            } else {
                // A page table
                free_pages_rec(physical_memory_offset,
                               frame_allocator,
                               entry.addr(),
                               level - 1);
            }
        }
    }
    // Free page table
    frame_allocator.deallocate_frame(
        PhysFrame::from_start_address(physaddr).unwrap());
}

/// Free all user-accessible pages and the page table frames
///
/// Note: Must not be called to free the current page tables
///       Switch to kernel pagetable before calling
pub fn free_user_pagetables(level_4_physaddr: u64) {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};

    free_pages_rec(memory_info.phys_memory_offset,&mut memory_info.frame_allocator,PhysAddr::new(level_4_physaddr),4);
}




/// Creates on-demand pages for user space in the memory.
///
/// # Arguments
///
/// * `level_4_table_ptr`: The pointer to the level 4 page table.
/// * `start_addr`: The starting virtual address for the pages.
/// * `size`: The size of the memory region to map.
///
/// # Returns
///
/// Returns a Result indicating success or failure.
pub fn create_user_ondemand_pages(level_4_table_ptr: u64,start_addr: VirtAddr,size: u64) -> Result<(), MapToError<Size4KiB>> {
    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };
    let frame_allocator = &mut memory_info.frame_allocator;

    // Get a mutable reference to the level 4 page table
    let l4_table: &mut PageTable = unsafe {
        &mut *(memory_info.phys_memory_offset + level_4_table_ptr).as_mut_ptr()
    };

    // Create a new OffsetPageTable mapper
    let mut mapper = unsafe { OffsetPageTable::new(l4_table, memory_info.phys_memory_offset) };

    // Calculate the range of pages to be mapped
    let page_range = {
        let end_addr = start_addr + size - 1u64;
        let start_page = Page::containing_address(start_addr);
        let end_page = Page::containing_address(end_addr);
        Page::range_inclusive(start_page, end_page)
    };

    // Allocate a frame for the pages
    let frame = frame_allocator
        .allocate_frame()
        .ok_or(MapToError::FrameAllocationFailed)?;

    // Map each page in the range to the allocated frame
    for page in page_range {
        unsafe {
            mapper.map_to_with_table_flags(
                page,
                frame,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
                frame_allocator
            )?.flush();
        }
    }

    // Make the first page in the range writable, effectively 'owning' the frame
    unsafe {
        mapper.update_flags(
            page_range.start,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE
        )
        .map_err(|_| MapToError::FrameAllocationFailed)?
        .flush();  // Update the page table
    }

    // Return success
    Ok(())
}

// Function to allocate a user stack
pub fn allocate_user_stack(level_4_table: *mut PageTable) -> Result<(u64, u64), &'static str> {
    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Dereference the level 4 page table pointer
    let mut table = unsafe { &mut *level_4_table };

    // Traverse the page table hierarchy to reach the level 1 table
    for index in USER_STACK_PAGE_INDEX {
        let entry = &mut table[index as usize];
        if entry.is_unused() {
            // If the page is unused, create a new page table
            let (_new_table_ptr, new_table_physaddr) = create_pagetable();
            entry.set_addr(
                PhysAddr::new(new_table_physaddr),
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
            );
        }
        table = unsafe { &mut *(memory_info.phys_memory_offset + entry.addr().as_u64()).as_mut_ptr() };
    }

    // Generate a unique starting index for searching for an empty slot
    let n_start = process::unique_id(); // Will be modulo 64 soon
    const N: usize = 10; // Number of pages for the user stack

    // Search for an empty slot in the level 1 table
    for i in 0..64 {
        let n = ((n_start + i) % 64) as usize;

        // Check if N consecutive pages are unused
        if (n * 8 + 1..n * 8 + 1 + N).all(|index| table[index].is_unused()) {
            // Allocate the pages for the user stack
            for j in 1..=N {
                let entry = &mut table[n * 8 + j];
                let frame = memory_info.frame_allocator.allocate_frame().ok_or("Failed to allocate frame")?;
                entry.set_addr(
                    frame.start_address(),
                    PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
                );
            }

            // Calculate the virtual addresses for the top and bottom of the user stack
            let slot_address: u64 = ((USER_STACK_PAGE_INDEX[0] as u64) << 39)
                + ((USER_STACK_PAGE_INDEX[1] as u64) << 30)
                + ((USER_STACK_PAGE_INDEX[2] as u64) << 21)
                + (((n * 8) as u64) << 12);

            return Ok((slot_address + 4096, slot_address + ((N + 1) as u64) * 4096));
        }
    }

    // If all slots are full, return an error
    Err("All thread stack slots full")
}

// Function to deallocate a user stack
pub fn free_user_stack(stack_end: VirtAddr) -> Result<(), &'static str> {
    // Get the address in the last page of the stack
    let addr = stack_end - 1u64;
    let table = active_level_1_table_containing(addr);

    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Calculate the index range for the stack pages
    let iend = usize::from(addr.p1_index());

    // Deallocate the stack pages and clear the entries
    for index in ((iend - 10)..=iend).rev() {
        let entry = &mut table[index];

        // Check if the page is writable (i.e., it has a unique frame)
        if entry.flags().contains(PageTableFlags::WRITABLE) {
            // Deallocate the frame
            memory_info.frame_allocator.deallocate_frame(entry.frame().unwrap());
        }
        // Clear the page table entry
        entry.set_flags(PageTableFlags::empty());
    }

    Ok(())
}

// Function to create a new page table and return its pointer and physical address
fn create_pagetable() -> (*mut PageTable, u64) {
    // Get the global memory information
    let mem_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Allocate a frame for the level 4 table
    let level_4_table_frame = mem_info.frame_allocator.allocate_frame().unwrap();
    let phys = level_4_table_frame.start_address(); // Physical address of the frame
    let virt = mem_info.phys_memory_offset + phys.as_u64(); // Convert to kernel virtual address
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    // Zero out the new page table
    unsafe { (*page_table_ptr).zero(); }

    // Return the pointer and physical address
    (page_table_ptr, phys.as_u64())
}

// Function to allocate and map pages for a given virtual address range
pub fn allocate_pages_mapper(frame_allocator: &mut impl FrameAllocator<Size4KiB>,mapper: &mut impl Mapper<Size4KiB>,start_addr: VirtAddr,size: u64,flags: PageTableFlags,) -> Result<(), MapToError<Size4KiB>> {
    // Calculate the range of pages to allocate
    let pages = {
        let end_addr = start_addr + size - 1u64;
        let start_page = Page::containing_address(start_addr);
        let end_page = Page::containing_address(end_addr);
        Page::range_inclusive(start_page, end_page)
    };

    // Allocate and map each page in the range
    for page in pages {
        let frame = frame_allocator.allocate_frame().ok_or(MapToError::FrameAllocationFailed)?;
        unsafe {
            mapper.map_to(page, frame, flags, frame_allocator)?.flush();
        };
    }

    Ok(())
}

// Function to create a kernel-only page table by copying the existing kernel page tables
pub fn create_kernel_only_pagetable() -> (*mut PageTable, u64) {
    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Copy the existing kernel page tables
    copy_pagetables(memory_info.kernel_page_tables)
}

// Function to create a user page table by getting the active level 4 table
pub fn create_user_pagetable() -> *mut PageTable {
    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Get the active level 4 table
    let table = unsafe { active_level_4_table(memory_info.phys_memory_offset) };

    // Return the pointer to the table
    table as *mut PageTable
}

// Function to get the active level 1 page table containing a given virtual address
fn active_level_1_table_containing(addr: VirtAddr) -> &'static mut PageTable {
    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Start with the active page table
    let mut table = unsafe { &mut (*active_pagetable_ptr()) };

    // Traverse the page table hierarchy to find the level 1 table
    for index in [addr.p4_index(), addr.p3_index(), addr.p2_index()] {
        let entry = &mut table[index];
        table = unsafe { &mut *(memory_info.phys_memory_offset + entry.addr().as_u64()).as_mut_ptr() };
    }

    table
}

// Function to allocate a frame for a given virtual address on-demand
pub fn allocate_missing_ondemand_frame(addr: VirtAddr) -> Result<(), &'static str> {
    // Get the level 1 table containing the address
    let table = active_level_1_table_containing(addr);
    let entry = &mut table[addr.p1_index()];

    // Check for unexpected flags
    if entry.flags() != (PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE) {
        println!("Unexpected flags: {:?} addr: {:?}", entry.flags(), addr);
        return Err("Error: Unexpected table flags");
    }

    // Allocate a new frame and update the page table entry
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };
    let frame = memory_info.frame_allocator.allocate_frame()
        .ok_or("Could not allocate frame")?;

    entry.set_addr(frame.start_address(),
                   PageTableFlags::PRESENT |
                   PageTableFlags::WRITABLE |
                   PageTableFlags::USER_ACCESSIBLE);

    Ok(())
}

// Function to allocate pages for a given virtual address range using a specified level 4 table
pub fn allocate_pages(level_4_table: *mut PageTable, start_addr: VirtAddr, size: u64, flags: PageTableFlags) -> Result<(), MapToError<Size4KiB>> {
    // Get the global memory information
    let memory_info = unsafe { MEMORY_INFO.as_mut().unwrap() };

    // Create a new OffsetPageTable mapper
    let mut mapper = unsafe { OffsetPageTable::new(&mut *level_4_table, memory_info.phys_memory_offset) };

    // Use the mapper to allocate and map the pages
    allocate_pages_mapper(&mut memory_info.frame_allocator, &mut mapper, start_addr, size, flags)
}

// Function to deallocate a range of pages
pub fn deallocate_page(mapper: &mut impl Mapper<Size4KiB>, address: u64, size: usize) {
    // Calculate the range of pages to deallocate
    let pages: PageRangeInclusive<Size4KiB> = {
        let start = Page::containing_address(VirtAddr::new(address));
        let end = Page::containing_address(VirtAddr::new(address + (size as u64) - 1));
        Page::range_inclusive(start, end)
    };

    // Unmap each page in the range
    for page in pages {
        if let Ok((_frame, mapping)) = mapper.unmap(page) {
            mapping.flush();
        } else {
            serial_println!("cannot deallocate {:?}", page);
        }
    }
}


fn time_stamp_counter() -> u64 {
    let counter: u64;
    unsafe{
        asm!("rdtsc",
             "shl rdx, 32", // High bits in EDX
             "or rdx, rax", // Low bits in EAX
             out("rdx") counter,
             out("rax") _, // Clobbers RAX
             options(pure, nomem, nostack)
        );
    }
    counter
}



pub fn test_alloc_times() {
    let memory_info = unsafe {MEMORY_INFO.as_mut().unwrap()};
    let mut alloc = &mut memory_info.frame_allocator;

    const N: usize = 800;
    let count1 = time_stamp_counter();

    for _ in 0..10 {
        // Allocate frames
        let mut frames = vec![];
        for _ in 0..N {
            let frame = alloc.allocate_frame();
            frames.push(frame);
        }

        // Free them all again
        for opt_frame in frames {
            if let Some(frame) = opt_frame {
                alloc.deallocate_frame(frame);
            }
        }
    }
    let count2 = time_stamp_counter();
    println!("Clock ticks: {} M", (count2 - count1) / 1000000);
    serial_println!("Clock ticks: {} M", (count2 - count1) / 1000000);
}



