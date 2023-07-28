use crate::println;
use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::{
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PhysFrame, Size4KiB,
    },
    PhysAddr, VirtAddr,
};

//Remember that page tables are used by the MMU (Memory Management Unit) to translate virtual addresses to physical addresses. When a program accesses an address, it provides a virtual address, which the MMU then translates to a physical address. The physical address is then used to access the actual data in memory. The mapping from virtual to physical addresses is done through a set of hierarchical page tables.

///- `init`: This function initializes an `OffsetPageTable` which can translate virtual addresses to physical addresses and vice versa. It requires the `physical_memory_offset` which indicates the difference between the physical and virtual address of a page.

/// Initialize a new OffsetPageTable.
///
/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must be only called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = active_level_4_table(physical_memory_offset);
    println!("Initialised page tables...");
    OffsetPageTable::new(level_4_table, physical_memory_offset)
}

///- `active_level_4_table`: This function returns a mutable reference to the level 4 page table currently active in the CPU. It reads the value from the CR3 register (which contains the physical address of the active level 4 table) and converts it to the equivalent virtual address using the provided `physical_memory_offset`.

/// This function is unsafe because the caller must guarantee that the
/// complete physical memory is mapped to virtual memory at the passed
/// `physical_memory_offset`. Also, this function must be only called once
/// to avoid aliasing `&mut` references (which is undefined behavior).
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read(); // gets the address of the l4 table from the cr3 register

    let phys = level_4_table_frame.start_address(); // actual address
    let virt = physical_memory_offset + phys.as_u64(); // offsetted address
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr(); // pointer to l4 table

    &mut *page_table_ptr // unsafe
}

///- `create_example_mapping`: This function maps a provided page to the frame at the physical address `0xb8000` with the `PRESENT` and `WRITABLE` flags. It's used for testing purposes and not safe to use in a real kernel since the frame at `0xb8000` might be already in use.

/// Creates an example mapping for the given page to frame `0xb8000`.
pub fn create_example_mapping(
    page: Page,
    mapper: &mut OffsetPageTable,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) {
    use x86_64::structures::paging::PageTableFlags as Flags;

    let frame = PhysFrame::containing_address(PhysAddr::new(0xb8000));
    let flags = Flags::PRESENT | Flags::WRITABLE;

    let map_to_result = unsafe {
        // FIXME: this is not safe, we do it only for testing
        mapper.map_to(page, frame, flags, frame_allocator)
    };
    map_to_result.expect("map_to failed").flush();
}

///- `EmptyFrameAllocator`: This is a dummy frame allocator that never allocates any frames. It's used when you don't have a physical memory manager yet or for testing purposes.
/// A FrameAllocator that always returns `None`.
pub struct EmptyFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for EmptyFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        None
    }
}

/// A FrameAllocator that returns usable frames from the bootloader's memory map.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
    frame_usage: [bool; 1000],
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
        let frame_count = memory_map.iter().count();
        println!("num of entries: {}", frame_count);

        BootInfoFrameAllocator {
            memory_map,
            next: 0,
            frame_usage: [false; 1000],
        }
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
            _ => None,
        }
    }
}
