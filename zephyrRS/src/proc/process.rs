extern crate alloc;
use spin::RwLock;
use lazy_static::lazy_static;
use x86_64::instructions::interrupts;
use x86_64::VirtAddr;
use alloc::{boxed::Box, collections::vec_deque::VecDeque, vec::Vec};
use object::{Object, ObjectSegment};
use crate::boot::interrupts::{CPUContext,INTERRUPT_CONTEXT_SIZE};
use crate::boot::gdt;
use crate::mem::memory;
use crate::{serial_println,println};
use x86_64::structures::paging::PageTableFlags;



// Defining the size of kernel and user stacks
const KERNEL_STACK_SIZE: usize = 4096 * 2;
const USER_STACK_SIZE: usize   = 4096 * 5;
const USER_CODE_START: u64 = 0x5000000;
const USER_CODE_END: u64 = 0x80000000;



// Thread struct represents a single thread in the OS. It contains 
// the kernel stack, user stack, their ends, and the context (register state).
struct Thread {
    kernel_stack: Vec<u8>,
    user_stack: Vec<u8>,
    kernel_stack_end: u64,
    user_stack_end: usize,
    context: u64,
}

lazy_static! {
    // RUNNING is the queue of threads that are currently running.
    // It's wrapped in a RwLock to allow for concurrent access.
    static ref RUNNING: RwLock<VecDeque<Box<Thread>>> = RwLock::new(VecDeque::new());


    // CURR_THREAD is a reference to the thread that is currently running.
    // It's wrapped in a RwLock to allow for concurrent access.
    static ref CURR_THREAD: RwLock<Option<Box<Thread>>> = RwLock::new(None);

}



// spawn_kernel_thread creates a new thread and adds it to the RUNNING.
// The thread will run the provided function once it gets scheduled.
pub fn spawn_kernel_thread(function: fn()->()) {
    let thread = {
        // Initialize the kernel and user stacks for the new thread.
        let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE);
        let kernel_stack_end = (VirtAddr::from_ptr(kernel_stack.as_ptr())+ KERNEL_STACK_SIZE).as_u64();
        let user_stack = Vec::with_capacity(USER_STACK_SIZE);
        let user_stack_end = (VirtAddr::from_ptr(user_stack.as_ptr()) + USER_STACK_SIZE).as_u64() as usize;

        // The context will be placed at the end of the kernel stack.
        let context = kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64;


        // Box the new thread so it can be moved between queues.
        Box::new(Thread {
            kernel_stack,
            user_stack,
            kernel_stack_end,
            user_stack_end: user_stack_end.try_into().unwrap(),
            context})
    };

    // Set the initial register state for the new thread.
    let context = unsafe {&mut *(thread.context as *mut CPUContext)};
    context.rip = function as usize; // Instruction pointer
    context.rsp = thread.user_stack_end; // Stack pointer
    context.rflags = 0x200; // Interrupts enabled

    let (code_selector, data_selector) = gdt::get_kernel_segments();
    context.cs = code_selector.0 as usize;
    //context.ss = data_selector.0 as usize;    

    // Add the new thread to the running queue.
    interrupts::without_interrupts(|| {
        RUNNING.write().push_back(thread);
    }); 
}

pub fn spawn_user_thread(bin: &[u8]) -> Result<usize,&'static str>{
    const MAGIC_BYTES: [u8;4] = [0x7f, b'E',b'L',b'F'];

    if bin[0..4] != MAGIC_BYTES {
        return Err("No ELF binary found");
    }

    if let Ok(obj) = object::File::parse(bin) {
        let entry_point = obj.entry();
        let user_page_table_ptr = memory::create_user_pagetable();

        for segment in obj.segments() {
    	    let segment_address = segment.address() as u64;
            println!("Section {:?} : {:#016X}", segment.name(), segment_address);
            serial_println!("Section {:?} : {:#016X}", segment.name(), segment_address);


            let start_address = VirtAddr::new(segment_address);
            let end_address = start_address + segment.size() as u64;
            if (start_address < VirtAddr::new(USER_CODE_START))
                || (end_address >= VirtAddr::new(USER_CODE_END)) {
                    return Err("ELF segment outside allowed range");
            }


            if memory::allocate_pages(user_page_table_ptr, VirtAddr::new(segment_address),segment.size() as u64,
                           PageTableFlags::PRESENT |
                           PageTableFlags::WRITABLE |
                           PageTableFlags::USER_ACCESSIBLE).is_err() {
                return Err("Failed to allocate memory for thread");
            }

            if let Ok(data)= segment.data() {
                let new_ptr = segment_address as *mut u8;
                for (i,value) in data.iter().enumerate(){
                    unsafe {
                        let ptr = new_ptr.add(i);
                        core::ptr::write(ptr, *value);
                    }
                }
            }

        }
        return Ok(0);
    } 
    Err("Could not parse ELF")

}
// schedule_next is called when a context switch is required.
// It switches out the current thread and schedules the next one from the running queue.
pub fn schedule_next(context_addr: usize) -> usize {
    let mut running_queue = RUNNING.write();
    let mut current_thread = CURR_THREAD.write();

    // If there's a current thread, push it back into the running queue.
    if let Some(mut thread) = current_thread.take() {
        thread.context = context_addr as u64;
        running_queue.push_back(thread);
    }

    // get next thread 
    *current_thread = running_queue.pop_front();
    match current_thread.as_ref() {
        Some(thread) => {
            gdt::set_interrupt_stack_table( //
              gdt::TIMER_INTERRUPT_INDEX as usize,
              VirtAddr::new(thread.kernel_stack_end));
            thread.context as usize
          },
        None => 0  
    }
}
