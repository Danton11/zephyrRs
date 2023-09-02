use super::Locked;
use alloc::alloc::{GlobalAlloc, Layout};
use core::{mem, ptr, ptr::NonNull};

// Define ListNode struct for linked list nodes
struct ListNode {
    next_node: Option<&'static mut ListNode>,
}

// Define block sizes (must be powers of 2)
const BLOCK_SIZES: &[usize] = &[8, 16, 32, 64, 128, 256, 512, 1024, 2048];

// Define FixedSizeBlockAllocator struct
pub struct FixedSizeBlockAllocator {
    // Array of head pointers for each block size
    list_heads: [Option<&'static mut ListNode>; BLOCK_SIZES.len()],
    // Fallback allocator for non-standard sizes
    fallback_allocator: linked_list_allocator::Heap,
}

impl FixedSizeBlockAllocator {
    // Constructor for FixedSizeBlockAllocator
    pub const fn new() -> Self {
        const EMPTY_NODE: Option<&'static mut ListNode> = None;
        FixedSizeBlockAllocator {
            list_heads: [EMPTY_NODE; BLOCK_SIZES.len()],
            fallback_allocator: linked_list_allocator::Heap::empty(),
        }
    }

    // Initialize the allocator with heap bounds
    pub unsafe fn init(&mut self, heap_start: usize, heap_size: usize) {
        self.fallback_allocator.init(heap_start, heap_size);
    }

    // Fallback allocation method
    fn fallback_allocation(&mut self, layout: Layout) -> *mut u8 {
        match self.fallback_allocator.allocate_first_fit(layout) {
            Ok(ptr) => ptr.as_ptr(),
            Err(_) => ptr::null_mut(),
        }
    }
}

// Determine the index of the block size list for a given layout
fn find_block_index(layout: &Layout) -> Option<usize> {
    let required_size = layout.size().max(layout.align());
    BLOCK_SIZES.iter().position(|&size| size >= required_size)
}

// Implement the GlobalAlloc trait for FixedSizeBlockAllocator
unsafe impl GlobalAlloc for Locked<FixedSizeBlockAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut allocator = self.lock();

        // Check if the layout fits one of the predefined block sizes
        match find_block_index(&layout) {
            Some(index) => {
                match allocator.list_heads[index].take() {
                    Some(node) => {
                        allocator.list_heads[index] = node.next_node.take();
                        node as *mut ListNode as *mut u8
                    }
                    None => {
                        let layout = Layout::from_size_align(BLOCK_SIZES[index], BLOCK_SIZES[index]).unwrap();
                        allocator.fallback_allocation(layout)
                    }
                }
            }
            None => allocator.fallback_allocation(layout),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let mut allocator = self.lock();

        // Check if the layout fits one of the predefined block sizes
        match find_block_index(&layout) {
            Some(index) => {
                let new_node = ListNode {
                    next_node: allocator.list_heads[index].take(),
                };

                // Ensure the block can hold a ListNode
                assert!(mem::size_of::<ListNode>() <= BLOCK_SIZES[index]);
                assert!(mem::align_of::<ListNode>() <= BLOCK_SIZES[index]);

                let new_node_ptr = ptr as *mut ListNode;
                new_node_ptr.write(new_node);
                allocator.list_heads[index] = Some(&mut *new_node_ptr);
            }
            None => {
                let ptr = NonNull::new(ptr).unwrap();
                allocator.fallback_allocator.deallocate(ptr, layout);
            }
        }
    }
}
