use super::{align_up, Locked};
use core::{mem, ptr};
use alloc::alloc::{GlobalAlloc, Layout};


// Define ListNode struct for linked list nodes
struct ListNode {
    block_size: usize,
    next_node: Option<&'static mut ListNode>,
}

impl ListNode {
    // Constructor for ListNode
    const fn new(size: usize) -> Self {
        ListNode { block_size: size, next_node: None }
    }

    // Get the starting address of the list node
    fn start_address(&self) -> usize {
        self as *const Self as usize
    }

    // Get the ending address of the list node
    fn end_address(&self) -> usize {
        self.start_address() + self.block_size
    }
}

// Define LinkedListAllocator struct
pub struct LinkedListAllocator {
    head_node: ListNode,
}

impl LinkedListAllocator {
    // Constructor for LinkedListAllocator
    pub const fn new() -> Self {
        Self { head_node: ListNode::new(0) }
    }

    // Initialize the allocator with heap bounds
    pub unsafe fn initialize(&mut self, heap_start: usize, heap_size: usize) {
        self.add_free_block(heap_start, heap_size);
    }

    // Add a free memory block to the linked list
    unsafe fn add_free_block(&mut self, address: usize, size: usize) {
        // Ensure alignment and minimum size
        assert_eq!(align_up(address, mem::align_of::<ListNode>()), address);
        assert!(size >= mem::size_of::<ListNode>());

        // Create and add the new node
        let new_node = ListNode::new(size);
        let node_ptr = address as *mut ListNode;
        node_ptr.write(new_node);
        self.head_node.next_node = Some(&mut *node_ptr);
    }

    // Find a suitable memory block for allocation
    fn find_suitable_block(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        // Initialize current node reference
        let mut current = &mut self.head_node;
        // Iterate through the linked list to find a suitable block
        while let Some(ref mut region) = current.next_node {
            if let Ok(alloc_start) = Self::allocate_from_block(&region, size, align) {
                // Remove the block from the list
                let next = region.next_node.take();
                let ret = Some((current.next_node.take().unwrap(), alloc_start));
                current.next_node = next;
                return ret;
            } else {
                // Move to the next block
                current = current.next_node.as_mut().unwrap();
            }
        }

        // no suitable region found
        None
    }

    // Attempt to allocate memory from a given block
    fn allocate_from_block(block: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let aligned_start = align_up(block.start_address(), align);
        let aligned_end = aligned_start.checked_add(size).ok_or(())?;

        // Check if the block is large enough
        if aligned_end > block.end_address() {
            return Err(());
        }

        // Check if the remaining size is usable
        let remaining_size = block.end_address() - aligned_end;
        if remaining_size > 0 && remaining_size < mem::size_of::<ListNode>() {
            return Err(());
        }

        Ok(aligned_start)
    }

    // Adjust layout for ListNode storage
    fn adjust_layout(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("Alignment adjustment failed")
            .pad_to_align();
        let adjusted_size = layout.size().max(mem::size_of::<ListNode>());
        (adjusted_size, layout.align())
    }
}

// Implement the GlobalAlloc trait for LinkedListAllocator
unsafe impl GlobalAlloc for Locked<LinkedListAllocator> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (required_size, alignment) = LinkedListAllocator::adjust_layout(layout);
        let mut allocator = self.lock();

        if let Some((block, start_address)) = allocator.find_suitable_block(required_size, alignment) {
            let end_address = start_address.checked_add(required_size).expect("Overflow");
            let remaining_size = block.end_address() - end_address;
            if remaining_size > 0 {
                allocator.add_free_block(end_address, remaining_size);
            }
            start_address as *mut u8
        } else {
            ptr::null_mut()
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let (required_size, _) = LinkedListAllocator::adjust_layout(layout);
        self.lock().add_free_block(ptr as usize, required_size);
    }
}

