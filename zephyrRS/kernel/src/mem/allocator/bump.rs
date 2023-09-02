use super::{align_up, Locked};
use alloc::alloc::{GlobalAlloc, Layout};
use core::ptr;

// Define the BumpAllocator struct
pub struct BumpAllocator {
    heap_start: usize, // Starting address of the heap
    heap_end: usize,   // Ending address of the heap
    next_free: usize,  // Next free address for allocation
    allocation_count: usize, // Number of active allocations
}

impl BumpAllocator {
    /// Creates a new empty bump allocator.
    pub const fn new() -> Self {
        BumpAllocator {
            heap_start: 0,
            heap_end: 0,
            next_free: 0,
            allocation_count: 0,
        }
    }

    
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.heap_start = heap_start;
        self.heap_end = heap_start.saturating_add(heap_size);
        self.next_free = heap_start;
    }
}

// Implement the GlobalAlloc trait for BumpAllocator
unsafe impl GlobalAlloc for Locked<BumpAllocator> {
    // Implement the alloc function
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut bump = self.lock(); // Lock and get a mutable reference

        // Calculate the starting address for the new allocation
        let alloc_start = align_up(bump.next_free, layout.align());
        
        // Calculate the ending address for the new allocation
        let alloc_end = match alloc_start.checked_add(layout.size()) {
            Some(end) => end,
            None => return ptr::null_mut(), // Overflow
        };

        // Check if the allocation fits into the heap
        if alloc_end > bump.heap_end {
            ptr::null_mut() // Out of memory
        } else {
            bump.next_free = alloc_end;
            bump.allocation_count += 1;
            alloc_start as *mut u8
        }
    }

    // Implement the dealloc function
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        let mut bump = self.lock(); // Lock and get a mutable reference

        bump.allocation_count -= 1;
        
        // Reset the next_free pointer if there are no active allocations
        if bump.allocation_count == 0 {
            bump.next_free = bump.heap_start;
        }
    }
}
