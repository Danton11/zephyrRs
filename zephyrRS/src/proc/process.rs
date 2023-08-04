use x86_64::VirtAddr;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::PageTableFlags;
use spin::RwLock;
use lazy_static::lazy_static;
extern crate alloc;
use alloc::{boxed::Box, collections::vec_deque::VecDeque, vec::Vec};
use core::arch::asm;
use crate::{println, serial_println};
use crate::boot::interrupts::{Context, INTERRUPT_CONTEXT_SIZE};
use crate::boot::gdt;
use crate::mem::memory;
use core::fmt;
//use core::ptr;
use object::{Object, ObjectSegment};

/// Size of the kernel stack for each thread, in bytes
const KERNEL_STACK_SIZE: usize = 4096 * 2;

/// Size of the user stack for each user thread, in bytes
const USER_STACK_SIZE: usize = 4096 * 5;
/// Lowest address that user code can be loaded into
const USER_CODE_START: u64 = 0x5000000;
/// Exclusive upper limit for user code
const USER_CODE_END: u64 = 0x80000000;


lazy_static! {
    // queue that contains moveable boxes of Threads
    static ref RUNNING: RwLock<VecDeque<Box<Thread>>> =
        RwLock::new(VecDeque::new());

    static ref CURR_THREAD: RwLock<Option<Box<Thread>>> = RwLock::new(None);
}


struct Thread {
    /// Thread ID
    thread_id: usize,
    kernel_stack: Vec<u8>,
    kernel_stack_end: u64,
    context: u64,
    user_stack: Vec<u8>,
    page_table_phys: u64, // pointer to each threads user stack
}


// Allow thread details to outputted to screen
impl fmt::Display for Thread {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let context = unsafe {&mut *(self.context as *mut Context)};
        let kernel_stack_start = VirtAddr::from_ptr(self.kernel_stack.as_ptr()).as_u64();
        let user_stack_start = VirtAddr::from_ptr(self.user_stack.as_ptr()).as_u64();
        let contextRip = context.rip;
        let contextRsp = context.rsp;

        write!(f, "\
thread_id: {}, rip: {:#016X}
    Kernel stack: {:#016X} - {:#016X} Context: {:#016X}
    Thread stack: {:#016X} - {:#016X} RSP: {:#016X}",
               self.thread_id, contextRip,
               kernel_stack_start,
               kernel_stack_start + (KERNEL_STACK_SIZE as u64),
               self.context,
               user_stack_start,
               user_stack_start + (USER_STACK_SIZE as u64),
               contextRsp)
    }
}

// create a kernel thread within the kernel stack space
pub fn spawn_kernel_thread(function: fn()->()) -> usize {
    let  new_thread = {
        let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE);
        let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
        let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();

        Box::new(Thread {
            thread_id: 0,
            kernel_stack,
            kernel_stack_end,
            context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
            user_stack: Vec::with_capacity(USER_STACK_SIZE),
            page_table_phys: 0,
        })
    };

    let context = unsafe {&mut *(new_thread.context as *mut Context)};
    context.rip = function as usize;

    unsafe {
        asm!{
            "pushf",
            "pop rax", // Get RFLAGS in RAX
            lateout("rax") context.rflags,
        }
    }

    context.cs = 8;
    context.rsp = (VirtAddr::from_ptr(new_thread.user_stack.as_ptr()) + USER_STACK_SIZE).as_u64() as usize;

    let thread_id = new_thread.thread_id;

    println!("New kernel thread {}", new_thread);

    // Turn off interrupts while modifying thread table
    interrupts::without_interrupts(|| {
        RUNNING.write().push_back(new_thread);
    });
    thread_id
}

/// Wrapper which runs a closure with a specified page table
///
/// Ensures that the original page table is restored after the
/// closure finishes.
fn with_pagetable<F, R>(page_table_physaddr: u64, func: F) -> R where
    F: FnOnce() -> R {
    // Store the page table and switch back before returning
    let original_page_table = memory::active_pagetable_physaddr();

    // Switch to the new user page table
    //
    // Note: We don't need to turn off interrupts because
    // schedule_next() saves the page table for each thread. This
    // thread temporarily has a different page table to the other
    // threads.
    memory::switch_to_pagetable(page_table_physaddr);

    let result = func();

    // Switch back to original page table
    memory::switch_to_pagetable(original_page_table);

    result
}
pub fn spawn_user_thread(bin: &[u8]) -> Result<usize, &'static str> {
    // https://en.wikipedia.org/wiki/Executable_and_Linkable_Format
    // Check the header
    const ELF_MAGIC: [u8; 4] = [0x7f, b'E', b'L', b'F'];

    if bin[0..4] != ELF_MAGIC {
        return Err("Expected ELF binary");
    }
    // Use the object crate to parse the ELF file
    // https://crates.io/crates/object
    if let Ok(obj) = object::File::parse(bin) {

        // Create a user pagetable with only kernel pages
        let (user_page_table_ptr, user_page_table_physaddr) =
            memory::create_kernel_only_pagetable();

        // No interrupts while using new page table
        return interrupts::without_interrupts(|| {
            memory::switch_to_pagetable(user_page_table_physaddr);

            let entry_point = obj.entry();
            println!("Entry point: {:#016X}", entry_point);

            for segment in obj.segments() {
                let segment_address = segment.address() as u64;

                println!("Section {:?} : {:#016X}", segment.name(), segment_address);

                if let Ok(data) = segment.data() {
                    println!("  len : {}", data.len());

                    memory::allocate_pages(user_page_table_ptr,
                                           VirtAddr::new(segment_address), // Start address
                                           data.len() as u64, // Size (bytes)
                                           PageTableFlags::PRESENT |
                                           PageTableFlags::WRITABLE |
                                           PageTableFlags::USER_ACCESSIBLE);

                    // Copy data
                    let dest_ptr = segment_address as *mut u8;
                    for (i, value) in data.iter().enumerate() {
                        unsafe {
                            let ptr = dest_ptr.add(i);
                            core::ptr::write(ptr, *value);
                        }
                    }
                } else {
                    return Err("Could not get segment data");
                }
            }

            // Create the new Thread struct
            let new_thread = {
                let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE);
                let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
                let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();

                Box::new(Thread {
                    thread_id: 0,
                    page_table_phys: user_page_table_physaddr,
                    kernel_stack,
                    // Note that stacks move backwards, so SP points to the end
                    kernel_stack_end,
                    // Push a Context struct on the kernel stack
                    context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
                    // User stack needs new pages, not allocated on the kernel heap
                    user_stack: Vec::new()
                })
            };

            // Cast context address to Context struct
            let context = unsafe {&mut *(new_thread.context as *mut Context)};

            context.rip = entry_point as usize;

            // Set flags
            context.rflags = 0x0200; // Interrupt enable

            let (code_selector, data_selector) = gdt::get_user_segments();
            context.cs = code_selector.0 as usize; // Code segment flags
            context.ss = data_selector.0 as usize; // Without this we get a GPF

            // Allocate pages for the user stack
            const USER_STACK_START: u64 = 0x5200000;

            memory::allocate_pages(user_page_table_ptr,
                                   VirtAddr::new(USER_STACK_START), // Start address
                                   USER_STACK_SIZE as u64, // Size (bytes)
                                   PageTableFlags::PRESENT |
                                   PageTableFlags::WRITABLE |
                                   PageTableFlags::USER_ACCESSIBLE);

            // Note: Need to point to the end of the allocated region
            //       because the stack moves down in memory
            context.rsp = (USER_STACK_START as usize) + USER_STACK_SIZE;

            let tid = new_thread.thread_id;

            println!("New Thread {}", new_thread);

            RUNNING.write().push_back(new_thread);

            return Ok(tid);
        });
    }
    return Err("Could not parse ELF");
}

// called each tick in timer_interrupt_handler
pub fn schedule_next(context: &Context) -> usize {
    let mut running = RUNNING.write();
    let mut curr_thread = CURR_THREAD.write();

    if let Some(thread) = curr_thread.take() {
        let mut proc_mut = thread;

        proc_mut.context = (context as *const Context) as u64;

        // add current thread to the back of the queue after storing it's context
        running.push_back(proc_mut);
    }
    *curr_thread= running.pop_front();

    match curr_thread.as_ref() {
        Some(thread) => {

            gdt::set_interrupt_stack_table(
                gdt::TIMER_INTERRUPT_INDEX as usize,

                VirtAddr::new(thread.kernel_stack_end));
            
            if thread.page_table_phys != 0 { // not kernel 
                memory::switch_to_pagetable(thread.page_table_phys);
            }
            thread.context as usize
        },
        None => 0
    }
}
