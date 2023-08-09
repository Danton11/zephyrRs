use x86_64::VirtAddr;
use x86_64::instructions::interrupts;
use x86_64::structures::paging::PageTableFlags;
use spin::RwLock;
use lazy_static::lazy_static;
extern crate alloc;
use alloc::{boxed::Box, collections::vec_deque::VecDeque, vec::Vec, sync::Arc};
use core::arch::asm;
use crate::{println, serial_println};
use crate::boot::interrupts::{Context, INTERRUPT_CONTEXT_SIZE, keyboard_rendezvous};
use crate::boot::gdt;
use crate::mem::memory;
use crate::syscall;
use crate::sync::Rendezvous;
use crate::sync::Message;
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

const USER_HEAP_START: u64 = 0x280_0060_0000;
const USER_HEAP_SIZE: u64 = 4 * 1024 * 1024;
lazy_static! {
    // queue that contains moveable boxes of Threads
    static ref RUNNING: RwLock<VecDeque<Box<Thread>>> =
        RwLock::new(VecDeque::new());

    static ref CURR_THREAD: RwLock<Option<Box<Thread>>> = RwLock::new(None);
    static ref COUNTER: RwLock<u64> = RwLock::new(0);
}

pub fn unique_id() -> u64 {
    interrupts::without_interrupts(|| {
        let mut counter = COUNTER.write();
        *counter += 1;
        *counter
    })
}

pub struct Process {
    page_table_physaddr: u64,
    handles: Vec<Arc<RwLock<Rendezvous>>>
}

#[derive(Debug, Copy, Clone)]
pub enum ThreadType {
    Kernel,
    User,
}

pub struct Thread {
    /// A unique identifier for the thread. This ID is typically assigned sequentially
    /// and can be used to differentiate threads from one another.
    thread_id: u64,

    /// A reference to the parent process of the thread. This provides context for the
    /// thread's execution and access to shared resources with other threads of the same process.
    process: Arc<Process>,

    /// The memory region reserved for the thread's kernel stack. The kernel stack is used 
    /// for operations that occur in kernel mode, separate from the user stack.
    kernel_stack: Vec<u8>,

    /// The end address of the kernel stack. Given that stacks grow downwards in memory, 
    /// this represents the top of the stack, and it's where new items would be pushed.
    kernel_stack_end: u64,

    /// The memory address where the thread's context is stored. The context includes
    /// information like register values which are necessary to resume the thread's 
    /// execution after it has been paused (e.g., during a context switch).
    context: u64,

    /// The end address of the user stack. Similar to the kernel stack end, but for 
    /// operations that the thread performs in user mode. New items are pushed onto the 
    /// user stack at this address.
    user_stack_end: u64,

    /// A pointer to the physical memory location of the page table that manages memory
    /// translations for this thread. This is essential for virtual memory and ensuring 
    /// the thread accesses the right memory locations.
    page_table_phys: u64,

    /// Indicates the type of the thread (e.g., Kernel or User). This can be used to 
    /// determine the privileges and operations the thread is allowed to perform.
    thread_type: ThreadType,
}




impl Thread {
    pub fn get_thread_id(&self) -> u64 {
        self.thread_id
    }
    pub fn get_process(&self) -> &Arc<Process> {
        &self.process
    }

    pub fn get_kernel_stack(&self) -> &Vec<u8> {
        &self.kernel_stack
    }

    pub fn get_kernel_stack_end(&self) -> u64 {
        self.kernel_stack_end
    }

    pub fn get_context(&self) -> u64 {
        self.context
    }

    pub fn get_user_stack_end(&self) -> u64 {
        self.user_stack_end
    }

    pub fn get_page_table_phys(&self) -> u64 {
        self.page_table_phys
    }

    pub fn get_thread_type(&self) -> &ThreadType {
        &self.thread_type
    }

    pub fn print_details(&self) {
        let context = unsafe {&mut *(self.context as *mut Context)};
        let kernel_stack_start = self.kernel_stack_end - (KERNEL_STACK_SIZE as u64);
        let user_stack_start = self.user_stack_end - (USER_STACK_SIZE as u64);
        let contextRip = context.rip;
        let contextRsp = context.rsp;

        serial_println!("---------------- Thread Details ----------------");
        serial_println!("Thread ID:              {}", self.thread_id);
        serial_println!("Thread Type:            {:?}", self.thread_type);
        serial_println!("RIP:                    {:#016X}", contextRip);
        serial_println!("Kernel Stack ({} bytes): {:#016X} - {:#016X}", KERNEL_STACK_SIZE, kernel_stack_start, self.kernel_stack_end);
        serial_println!("Context Address:        {:#016X}", self.context);
        serial_println!("Thread Stack ({} bytes): {:#016X} - {:#016X}", USER_STACK_SIZE, user_stack_start, self.user_stack_end);
        serial_println!("RSP:                    {:#016X}", contextRsp);
        serial_println!("-----------------------------------------------");
    }

    pub fn get_handles(&self, id: u64) -> Option<Arc<RwLock<Rendezvous>>> {
        self.process.handles.get(id as usize).map(|r| r.clone())    
    }

    fn context(&self) -> &Context {
        unsafe {& *(self.context as *const Context)}
    }

    fn context_mut(&self) -> &mut Context {
        unsafe {&mut *(self.context as *mut Context)}
    }
    
    pub fn set_context(&mut self, context_ptr: *mut Context) {
        self.context = context_ptr as u64;
    }
    
    pub fn return_error(&self, error_code: usize) {
        self.context_mut().rax = error_code;
    }

    pub fn return_message(&self, message: Message) {
        let context = self.context_mut();
        context.rax = 0; // No error
        match message {
            // sysret call takes the IP from RCX and RFLAGs from R11
            Message::Short(value) => { context.rdi = value;}, // place message value in the rdi register
            Message::Long => {context.rdi = 42;} // placeholder
        }
    }
}

pub fn take_current_thread() -> Option<Box<Thread>> {
    CURR_THREAD.write().take()
}

pub fn set_current_thread(thread: Box<Thread>) {
    // Replace the current thread
    let old_current = CURR_THREAD.write().replace(thread);
    if let Some(t) = old_current {
        schedule_thread(t); // make sure current thread gets reshceduled
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        if self.page_table_physaddr == memory::active_pagetable_physaddr() {
            memory::switch_to_kernel_pagetable();
        }
        memory::free_user_pagetables(self.page_table_physaddr);
    }
}


impl Drop for Thread {
    fn drop(&mut self) {
        memory::free_user_stack(VirtAddr::new(self.user_stack_end));
    }
}


// Allow thread details to outputted to screen
impl fmt::Display for Thread {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let context = unsafe {&mut *(self.context as *mut Context)};
        let kernel_stack_start = self.kernel_stack_end - (KERNEL_STACK_SIZE as u64);
        let user_stack_start = self.user_stack_end - (USER_STACK_SIZE as u64);
        let contextRip = context.rip;
        let contextRsp = context.rsp;

        write!(f, "\
===========================================
Thread ID: {}
===========================================
Kernel Stack:
    Start: {:#016X}
    End:   {:#016X}
    Context Address: {:#016X}
-------------------------------------------
User Stack:
    Start: {:#016X}
    End:   {:#016X}
    RSP:   {:#016X}
-------------------------------------------
Execution:
    RIP: {:#016X}
===========================================
",
               self.thread_id, contextRip,

               kernel_stack_start, self.kernel_stack_end,

               self.context,
               
               user_stack_start, self.user_stack_end,
               
               contextRsp)
    }
}

// create a kernel thread within the kernel stack space
pub fn spawn_kernel_thread(function: fn()->()) -> u64 {
    let  new_thread = {
        let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE + USER_STACK_SIZE);
        let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
        let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();
        let user_stack_end = kernel_stack_end + (USER_STACK_SIZE as u64);


        Box::new(Thread {
            thread_id: unique_id(),
            process: Arc::new(Process{page_table_physaddr: 0, handles: Vec::new() }),
            kernel_stack,
            kernel_stack_end,
            context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
            user_stack_end,
            page_table_phys: 0,
            thread_type: ThreadType::Kernel,

        })
    };

    // Cast context address to Context struct
    let context = new_thread.context_mut();

    // Set the instruction pointer
    context.rip = function as usize;

    // Set flags
    context.rflags = 0x200;

    // Set segment selector flags
    let (code_selector, data_selector) = gdt::get_kernel_segments();
    context.cs = code_selector.0 as usize;
    context.ss = data_selector.0 as usize;

    // The kernel thread has its own stack
    // Note: Need to point to the end of the memory region
    //       because the stack moves down in memory
    context.rsp = new_thread.user_stack_end as usize;

    let thread_id = new_thread.thread_id;

    println!("New kernel thread {}", new_thread);

    schedule_thread(new_thread);
    thread_id
}

pub fn schedule_thread(thread: Box<Thread>) {
    // Turn off interrupts while modifying process table
    interrupts::without_interrupts(|| {
        RUNNING.write().push_front(thread);
    });
}

/// Ensures that the original page table is restored after the closure finishes.
fn with_pagetable<F, R>(page_table_physaddr: u64, func: F) -> R where
    F: FnOnce() -> R {
    // Store the page table and switch back before returning
    let original_page_table = memory::active_pagetable_physaddr();

    // Switch to the new user page table
    //
    // Note: We don't need to turn off interrupts because
    // schedule_next() saves th;e page table for each thread. This
    // thread temporarily has a different page table to the other
    // threads.
    memory::switch_to_pagetable(page_table_physaddr);

    let result = func();

    // Switch back to original page table
    memory::switch_to_pagetable(original_page_table);

    result
}

pub fn spawn_user_thread(bin: &[u8]) -> Result<u64, &'static str> {
    // https://en.wikipedia.org/wiki/Executable_and_Linkable_Format
    // Check the header
    const MAGIC_BYTES: [u8; 4] = [0x7f, b'E', b'L', b'F'];

    if bin[0..4] != MAGIC_BYTES {
        return Err("ELF FILE NOT FOUND");
    }
    
    // Use the object crate to parse the ELF file
    // https://crates.io/crates/object
    if let Ok(obj) = object::File::parse(bin) {

        // Create a user pagetable with only kernel pages
        let (user_page_table_ptr, user_page_table_physaddr) =
            memory::create_kernel_only_pagetable();

        // Allocate user heap
        memory::create_user_ondemand_pages(
            user_page_table_ptr,
            VirtAddr::new(USER_HEAP_START),
            USER_HEAP_SIZE);

        return with_pagetable(user_page_table_physaddr, || {

            let entry_point = obj.entry();
            println!("Entry point: {:#016X}", entry_point);

            for segment in obj.segments() {
                let segment_address = segment.address() as u64;

                println!("Section {:?} : {:#016X} size {}",
                         segment.name(), segment_address, segment.size());

                let start_address = VirtAddr::new(segment_address);
                let end_address = start_address + segment.size() as u64;

                // Check if data is in allowed range
                if (start_address < VirtAddr::new(USER_CODE_START))
                    || (end_address >= VirtAddr::new(USER_CODE_END)) {
                        return Err("ELF segment outside allowed range");
                    }

                // Allocate memory in the pagetable
                if memory::allocate_pages(user_page_table_ptr,
                                          start_address,
                                          segment.size() as u64, // Size (bytes)
                                          PageTableFlags::PRESENT |
                                          PageTableFlags::WRITABLE |
                                          PageTableFlags::USER_ACCESSIBLE).is_err() {
                    return Err("Could not allocate memory");
                }
                memory::switch_to_pagetable(user_page_table_physaddr);

                if let Ok(data) = segment.data() {
                    println!(" data len : {}", data.len());
                    if data.len() > segment.size() as usize {
                        return Err("ELF data length > segment size");
                    } else if data.len() > 0 {
                        // Copy data
                        let dest_ptr = segment_address as *mut u8;
                        for (i, value) in data.iter().enumerate() {
                            unsafe {
                                let ptr = dest_ptr.add(i);
                                core::ptr::write(ptr, *value);
                            }
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
                
                let (_user_stack_start, user_stack_end) = memory::allocate_user_stack(user_page_table_ptr)?;

                Box::new(Thread {
                    thread_id: unique_id(),
                    // Create a new process
                    process: Arc::new(Process {page_table_physaddr: user_page_table_physaddr, handles: Vec::from([keyboard_rendezvous()])}),
                    page_table_phys: user_page_table_physaddr,
                    kernel_stack,
                    // Note that stacks move backwards, so SP points to the end
                    kernel_stack_end,
                    user_stack_end,
                    // Push a Context struct on the kernel stack
                    context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
                    // User stack needs new pages, not allocated on the kernel heap
                    thread_type: ThreadType::User,
                    
                })
            };

            // Cast context address to Context struct
            let context = new_thread.context_mut();

            context.rip = entry_point as usize;

            // Set flags
            context.rflags = 0x0200; // Interrupt enable

            let (code_selector, data_selector) = gdt::get_user_segments();
            context.cs = code_selector.0 as usize; // Code segment flags
            context.ss = data_selector.0 as usize; // Without this we get a GPF

            // Note: Need to point to the end of the allocated region
            //       because the stack moves down in memory
            context.rsp = new_thread.user_stack_end as usize;

            // Modify the context to pass information to the new thread
            context.rax = USER_HEAP_START as usize;
            context.rcx = USER_HEAP_SIZE as usize;

            let thread_id = new_thread.thread_id;

            new_thread.print_details();
            schedule_thread(new_thread);


            return Ok(thread_id);
        });
    }
    return Err("Could not parse ELF");
}

pub fn fork_current_thread(current_context: &mut Context) {

    if let Some(current_thread) = CURR_THREAD.read().as_ref() {

        // Allocate user stack
        let page_table_ptr = memory::active_pagetable_ptr();
        if let Ok((_user_stack_start, user_stack_end)) = memory::allocate_user_stack(page_table_ptr) {
            let new_thread = {
                // Create a new kernel stack
                let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE);
                let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
                let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();

                Box::new(Thread {
                    thread_id: unique_id(),
                    process: current_thread.process.clone(), // Shared state
                    page_table_phys: current_thread.page_table_phys, // Shared page table
                    kernel_stack,
                    kernel_stack_end,
                    user_stack_end,
                    context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
                    thread_type: current_thread.thread_type,
                })
            };

            let new_context = unsafe {&mut *(new_thread.context as *mut Context)};
            *new_context = current_context.clone();

            // Set new stack pointer
            new_context.rsp = new_thread.user_stack_end as usize;

            // Set return values in rax
            new_context.rax = 0; // No error
            new_context.rdi = 0; // Indicates that this is the new thread
            current_context.rax = 0; // No error
            current_context.rdi = new_thread.thread_id as usize;

            let _tid = new_thread.thread_id;
            new_thread.print_details();
            
            RUNNING.write().push_back(new_thread);
        } else {
            // Failed to allocate user stack
            current_context.rax = syscall::SYSCALL_ERROR_MEMALLOC; // Error code
        }
    } else {
        // Somehow no current thread
        current_context.rax = 2; // Error code
    }
}

pub fn exit_current_thread(_current_context: &mut Context) {
    // Remove current thread
    {
        let mut current_thread = CURR_THREAD.write();

        if let Some(_thread) = current_thread.take() {
            // Free user stack pages

            // If this is the last thread in this process, free shared
            // memory and page tables

            // Drop thread, free kernel stack
        }
    }
    // Can't return from this syscall, so this thread now waits for a
    // timer interrupt to switch context.
    unsafe {
        asm!("sti",
             "2:",
             "hlt",
             "jmp 2b");
    }
}
pub fn schedule_next(context_addr: usize) -> usize {

    let mut running_queue = RUNNING.write();
    let mut current_thread = CURR_THREAD.write();

    if let Some(thread) = current_thread.take() {
        // Put the current thread to the back of the queue

        // Update the stack pointer
        let mut thread_mut = thread;

        // Store context location. This should almost always be in the same
        // location on the kernel stack. The exception is the
        // first time a context switch occurs from the original kernel
        // stack to the first kernel thread stack.
        thread_mut.context = context_addr as  u64;

        // Save the page table. This is to enable context
        // switching during functions which manipulate page tables
        // for example new_user_thread
        thread_mut.page_table_phys = memory::active_pagetable_physaddr();

        running_queue.push_back(thread_mut);
    } 
    *current_thread = running_queue.pop_front();

    match current_thread.as_ref() {
        Some(thread) => {
            // Set the kernel stack for the next interrupt
            gdt::set_interrupt_stack_table(
                gdt::TIMER_INTERRUPT_INDEX as usize,
                // Note: Point to the end of the stack
                VirtAddr::new(thread.kernel_stack_end));

            if thread.page_table_phys != 0 {
                // Change page table
                // Note: zero for kernel thread
                memory::switch_to_pagetable(thread.page_table_phys);
            }

            // Point the stack to the new context
            // (which is usually stored on the kernel stack)
            thread.context as usize
        },
        None => 0
    }
}
