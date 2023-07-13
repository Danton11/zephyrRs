pub struct BumpAllocator {
    heap_start: usize,
    heap_end: usize,
    next:     usize,
    allocations: usize,

}
