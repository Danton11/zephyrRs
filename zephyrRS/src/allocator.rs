use x86_64::{structures::paging::{mapper::MapToError, FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB,},VirtAddr,};
use alloc::alloc::{GlobalAlloc, Layout};
use bump::BumpAllocator;

pub const HEAP_START: usize = 0x4444_4444_0000;
pub const HEAP_SIZE: usize = 100 * 1024;


pub mod bump;
#[global_allocator]
static ALLOCATOR: Locked<BumpAllocator> = Locked::new(BumpAllocator::new());

pub fn init_heap(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>,) -> Result<(), MapToError<Size4KiB>>{
    let pages = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE - 1u64; //  We want an inclusive bound (the address of the last byte of the heap), so we subtract 1
        let heap_start_page = Page::containing_address(heap_start); // convert into Page types
        let heap_end_page = Page::containing_address(heap_end);
        Page::range_inclusive(heap_start_page, heap_end_page) // create range
    };
    

    // map all pages in the page range to a present and writable page using the FrameAllocator
    for page in pages {
        let frame = frame_allocator.allocate_frame().ok_or(MapToError::FrameAllocationFailed)?;
        let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
        unsafe {mapper.map_to(page, frame, flags, frame_allocator)?.flush()}; // crate active
        // mapping
    }

    unsafe{ALLOCATOR.lock().init(HEAP_START,HEAP_SIZE)}

    Ok(())
}

pub struct Locked<A>{
    inner: spin::Mutex<A>,
}

impl<A> Locked<A> {
    pub const fn new(inner: A) -> Self {
        Locked {inner: spin::Mutex::new(inner),}
    }

    pub fn lock(&self) -> spin::MutexGuard<A> {
        self.inner.lock()
    }
}


fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}
