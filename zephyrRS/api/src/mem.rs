extern crate alloc;
use linked_list_allocator::LockedHeap;
use crate::println;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

pub fn init(heap_start: usize, heap_size: usize) {
    println!("Heap start {:#016X}, size: {} bytes ({} Mb)", heap_start, heap_size, heap_size / (1024 * 1024));
    unsafe {ALLOCATOR.lock().init(heap_start, heap_size);}
}

// Allocator error handler
#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::alloc::Layout) -> ! {
    panic!("Could not allocate memory to user thread: {:?}", layout)
}
