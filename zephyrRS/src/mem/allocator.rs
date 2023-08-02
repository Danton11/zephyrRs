use crate::{println, serial_println};
use fixed_size_block::FixedSizeBlockAllocator;
use linked_list_allocator::LockedHeap;
use x86_64::{structures::paging::{mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,},VirtAddr,};




pub const HEAP_START: usize = 0x4444_4444_0000;
pub const HEAP_SIZE: usize = 200 * 1024;

pub mod fixed_size_block;

#[global_allocator]
static ALLOCATOR: Locked<FixedSizeBlockAllocator> = Locked::new(FixedSizeBlockAllocator::new());

pub fn init_heap(mapper: &mut impl Mapper<Size4KiB>,frame_allocator: &mut impl FrameAllocator<Size4KiB>,) -> Result<(), MapToError<Size4KiB>> {
    let pages = { // start and end pages for the heap, then all pages inbetween (range_inclusive)
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64; //  We want an inclusive bound (the address of the last byte of the heap), so we subtract 1
        let heap_start_page = Page::containing_address(heap_start); // convert into Page types
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page) // create range
    };

    // map all pages in the page range to a present and writable page using the FrameAllocator
    for page in pages {
        let frame = frame_allocator.allocate_frame().ok_or(MapToError::FrameAllocationFailed)?; // create a physical page to be mapped to 
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE; // set the flags for the page
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() }; // create mapping from physical to virtual
                                                                                
    }

    unsafe { ALLOCATOR.lock().init(HEAP_START, HEAP_SIZE) }

    println!("Initialised Heap...");
    serial_println!("Initialised Heap...");
    Ok(())
}

pub struct Locked<A> {
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {
            inner: spin::Mutex::new(inner),
        }
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
