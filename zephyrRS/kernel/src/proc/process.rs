use x86_64::{VirtAddr, PhysAddr};
use x86_64::instructions::interrupts;
use x86_64::structures::paging::PageTableFlags;
use spin::RwLock;
use lazy_static::lazy_static;
extern crate alloc;
use alloc::{boxed::Box, collections::vec_deque::VecDeque, vec::Vec, sync::Arc, string::String};
use core::arch::asm;
use crate::{println, serial_println};
use crate::boot::interrupts::{Context, INTERRUPT_CONTEXT_SIZE, keyboard_socket};
use crate::boot::gdt;
use crate::mem::memory;
use crate::syscall;
use crate::sync::Socket;
use crate::sync::{Message,Data};
use core::fmt;
use crate::ID_SOCKET;

//use core::ptr;
use object::{Object, ObjectSegment};

/// Size of the kernel stack for each thread, in bytes
pub const KERNEL_STACK_SIZE: usize = 4096 * 3;

/// Size of the user stack for each user thread, in bytes
pub const USER_STACK_SIZE: usize = 4096 * 10;
/// Lowest address that user code can be loaded into
pub const USER_CODE_START: u64 = 0x5000000;
/// Exclusive upper limit for user code
const USER_CODE_END: u64 = 0x90000000;

const USER_HEAP_START: u64 = 0x280_0060_0000;
const USER_HEAP_SIZE: u64 = 4 * 1024 * 1024;



lazy_static! {
    // queue that contains moveable boxes of Threads
    static ref RUNNING: RwLock<VecDeque<Box<Thread>>> =
        RwLock::new(VecDeque::new());

    pub static ref CURR_THREAD: RwLock<Option<Box<Thread>>> = RwLock::new(None);
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
    //A Process has a page_table_physaddr, which is the physical address of its page table, allowing it to have its own separate virtual memory space.
    page_table_physaddr: u64,
    fdescriptor: Vec<Option<Arc<RwLock<Socket>>>>,
    mounts: Arc<RwLock<Vec<(String, Arc<RwLock<Socket>>)>>>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
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
    process: Arc<RwLock<Process>>,

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
    // Various getter methods to access the thread's properties.
    pub fn get_thread_id(&self) -> u64 {
        self.thread_id
    }
    pub fn get_process(&self) -> &Arc<RwLock<Process>> {
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

    // Display function to print the details of a thread for debugging purposes.
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
        serial_println!("Kernel Stack  {:#016X} - {:#016X}: ({} bytes)",  kernel_stack_start, self.kernel_stack_end, KERNEL_STACK_SIZE);
        serial_println!("Context Address:        {:#016X}", self.context);
        serial_println!("Thread Stack {:#016X} - {:#016X} ({} bytes)", user_stack_start, self.user_stack_end, USER_STACK_SIZE);
        serial_println!("RSP:                    {:#016X}", contextRsp);
        serial_println!("-----------------------------------------------");
    }

    pub fn get_fdescriptor(&self, id: u64) -> Option<Arc<RwLock<Socket>>> {
            self.process.read().fdescriptor.get(id as usize).unwrap_or(&None).as_ref().map(|rv| rv.clone()) // Option<Arc<>>
    }

    pub fn take_socket(&self, id: u64)-> Option<Arc<RwLock<Socket>>> {
        self.process.write().fdescriptor.get_mut(id as usize).map_or(None, |elem| elem.take())
    }

    /// Add a socket to the process, returning the handle
    pub fn give_socket(&self, socket: Arc<RwLock<Socket>>) -> u64 {
        // Lock the fdescriptor
        let fdescriptor = &mut self.process.write().fdescriptor;

        // Find empty handle slot
        for (pos, handle) in fdescriptor.iter().enumerate() {
            if handle.is_none() {
                // Found empty slot => Store socket
                fdescriptor[pos] = Some(socket);
                return pos as u64;
            }
        }
        // All full => Add new handle
        fdescriptor.push(Some(socket));
        (fdescriptor.len() - 1) as u64
    }

    // Functions to manipulate and retrieve the context (saved state) of a thread.
    fn context(&self) -> &Context {
        unsafe {& *(self.context as *const Context)}
    }

    fn context_mut(&self) -> &mut Context {
        unsafe {&mut *(self.context as *mut Context)}
    }
    
    pub fn set_context(&mut self, context_ptr: *mut Context) {
        self.context = context_ptr as u64;
    }
    // The function 'return_error' sets an error code in the thread's context, 
    // while 'return_message' sets a message in the thread's context.
    // 
    pub fn return_error(&self, error_code: usize) {
        self.context_mut().rax = error_code;
    }

    pub fn return_message(&self, message: Message) {
        let context = self.context_mut();
        context.rax = 0; // No error
        match message {
            Message::Short(data1, data2, data3) => {
                context.rdi = data1 as usize;
                context.rsi = data2 as usize;
                context.rdx = data3 as usize;
            },
            Message::Long(data1, data2, data3) => {
                context.rdi = data1 as usize;

                context.rsi = match data2 {
                    Data::Value(value) => value,
                    Data::Socket(rdv) => {
                        context.rax |= (syscall::MESSAGE_DATA2_RDV |
                                        syscall:: MESSAGE_LONG) as usize;
                        self.give_socket(rdv)
                    }
                } as usize;

                context.rdx = match data3 {
                    Data::Value(value) => value,
                    Data::Socket(rdv) => {
                        context.rax |= (syscall::MESSAGE_DATA3_RDV |
                                        syscall::MESSAGE_LONG) as usize;
                        self.give_socket(rdv)
                    }
                } as usize;   
            }
        }
    }
//    pub fn memory_usage(&self) -> (u64, u64) {
//        let stack_usage = self.user_stack_end - self.get_current_stack_pointer();
//        let heap_usage: u64 = self.heap_allocations.read().iter().sum();
//        
//        (stack_usage, heap_usage)
//    }

    fn get_current_stack_pointer(&self) -> u64 {
        // This method depends on your architecture and how you've set up your system.
        // For x86_64, you'd typically use the RSP register:
        let rsp: u64;
        unsafe { asm!("mov {}, rsp", out(reg) rsp) };
        rsp
    }
}

// Functions to manage the currently running thread.
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

// When a Process is dropped (no longer in use), ensure the associated memory structures are cleaned up.
impl Drop for Process {
    fn drop(&mut self) {
        if self.page_table_physaddr == memory::active_pagetable_physaddr() {
            memory::kernel_mode();
        }
        memory::free_user_pagetables(self.page_table_physaddr);
        println!("[!] - Process with page table at {:#x} deallocated its user page tables", self.page_table_physaddr);
    }
}

impl Process {
    // This method allows adding a new socket handle to a process's file descriptor table.
    fn add_handle(&mut self, rv: Arc<RwLock<Socket>>) -> usize {
        // Find if there is an empty fdescriptor slots
        if let Some(index) = self.fdescriptor.iter().position(
            |handle| handle.is_none()) {
            self.fdescriptor[index] = Some(rv);
            return index;
        }
        // No free slot -> Add one
        self.fdescriptor.push(Some(rv));
        self.fdescriptor.len() - 1
    }
}


// Implement the Display trait for Thread, allowing it to be printed in a human-readable format.
impl Drop for Thread {
    fn drop(&mut self) {
        memory::free_user_stack(VirtAddr::new(self.user_stack_end));

        monitor(&self);
        println!("[!] - Thread {} deallocated user stack at {:#x}", self.thread_id, self.user_stack_end);
        serial_println!("[!] - Thread {} deallocated user stack at {:#x}", self.thread_id, self.user_stack_end);
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
        serial_println!("\n
===========================================
Thread ID: {} , {:?}
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
               self.thread_id, self.thread_type, contextRip,

               kernel_stack_start, self.kernel_stack_end,

               self.context,
               
               user_stack_start, self.user_stack_end,
               
               contextRsp);
        write!(f, "\
===========================================
Thread ID: {} , {:?}
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
               self.thread_id, self.thread_type, contextRip,

               kernel_stack_start, self.kernel_stack_end,

               self.context,
               
               user_stack_start, self.user_stack_end,
               
               contextRsp)
    }
}

// Function to spawn a new kernel thread, initializing its context and adding it to the scheduling queue.
// create a kernel thread within the kernel stack space
pub fn spawn_kernel_thread(function: fn()->(), mut fdescriptor: Vec<Arc<RwLock<Socket>>>) -> u64 {
    let  new_thread = {
        let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE + USER_STACK_SIZE);
        let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
        let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();
        let user_stack_end = kernel_stack_end + (USER_STACK_SIZE as u64);

        let uid = unique_id();
        Box::new(Thread {
            thread_id: uid,
            process: Arc::new(RwLock::new(Process { page_table_physaddr: 0, fdescriptor: fdescriptor.drain(..).map(|h| Some(h)).collect(), mounts: Arc::new(RwLock::new(Vec::new()))})),
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
    
    monitor(&new_thread);
    //println!("New kernel thread {}", new_thread);
    new_thread.print_details();

    schedule_thread(new_thread);
    thread_id
}

// Function to handle the scheduling of threads, ensuring each gets a fair share of CPU time.
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

// A struct representing the parameters required to spawn a user thread.
pub struct Params { 
    // A vector containing file descriptors represented as sockets. 
    // This allows for shared access to file descriptors among threads.
    pub fdescriptor: Vec<Arc<RwLock<Socket>>>,
    // A mapping of mount points (e.g., "/mnt/disk1") to their corresponding sockets.
    // This provides a mechanism for threads to access shared mount points.
    pub mounts: Arc<RwLock<Vec<(String, Arc<RwLock<Socket>>)>>> 
}

// Function to spawn a user thread, which involves setting up its memory, parsing the ELF binary, and initializing its context.
// // Spawns a user-level thread by:
// 1. Setting up its memory.
// 2. Parsing the given ELF binary.
// 3. Initializing its execution context.
//
// `bin`: A byte slice representing the ELF binary.
// `params`: Parameters required for spawning the thread.
//
// Returns: 
// - `Ok(u64)`: Successfully created thread with its unique ID.
// - `Err(&'static str)`: An error occurred during the spawning process.
pub fn spawn_user_thread(bin: &[u8],params: Params) -> Result<u64, &'static str> {
    // https://en.wikipedia.org/wiki/Executable_and_Linkable_Format
    // The magic bytes are the first four bytes in an ELF file.
    const MAGIC_BYTES: [u8; 4] = [0x7f, b'E', b'L', b'F'];

    // Check if the provided binary starts with the expected ELF magic bytes.
    if bin[0..4] != MAGIC_BYTES {
        return Err("ELF FILE NOT FOUND");
    }
    
    if let Ok(obj) = object::File::parse(bin) {

        // Create a user pagetable that includes only kernel pages.
        let (user_page_table_ptr, user_page_table_physaddr) = memory::create_kernel_only_pagetable();
        serial_println!("Thread allocated page table at physical address {:#x}", user_page_table_physaddr);

        // Allocate user heap memory. This is memory reserved for dynamic allocations 
        // during the execution of the user thread (e.g., when `malloc` is called).
        if memory::create_user_ondemand_pages(
            user_page_table_physaddr,
            VirtAddr::new(USER_HEAP_START),
            USER_HEAP_SIZE).is_err() {
            return Err("Couldn't allocate on-demand pages");
        }

        // Switch to the user pagetable and setup the memory segments 
        // based on the parsed ELF object.
        return with_pagetable(user_page_table_physaddr, || {

            let entry_point = obj.entry();

            // Iterate over each segment in the ELF binary.
            for segment in obj.segments() {
                let segment_address = segment.address() as u64;

                let start_address = VirtAddr::new(segment_address);
                let end_address = start_address + segment.size() as u64;

                // Verify that the segment's memory range is within the allowed user code range.
                // For example, if USER_CODE_START is 0x400000, and USER_CODE_END is 0x800000,
                // then any segment outside this range would be rejected.
                if (start_address < VirtAddr::new(USER_CODE_START))
                    || (end_address >= VirtAddr::new(USER_CODE_END)) {
                        return Err("ELF segment outside allowed range");
                    }

                // Reserve memory for the segment in the pagetable.
                if memory::allocate_pages(user_page_table_ptr,
                                          start_address,
                                          segment.size() as u64, // Size in bytes
                                          PageTableFlags::PRESENT |
                                          PageTableFlags::WRITABLE |
                                          PageTableFlags::USER_ACCESSIBLE).is_err() {
                    return Err("Could not allocate memory");
                }


                // Activate the user pagetable for the current CPU. (switch to user mode) 
                memory::switch_to_pagetable(user_page_table_physaddr);

                // Retrieve and validate segment data.
                if let Ok(data) = segment.data() {
                    if data.len() > segment.size() as usize {
                        return Err("ELF data length > segment size");
                    } else if data.len() > 0 {
                        // Copy the segment data to its respective address in memory.
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


            let uid = unique_id();  // Generate a unique ID for the thread.

            // Create the new Thread struct
            let new_thread = {
                // Allocate a kernel stack for the thread. This is to stave the user stack on
                // interrupt call or system call when we need to switch into kernel mode...
                let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE);
                let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
                let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();

                // Allocate a user stack for the thread. 
                // The user stack is used when the thread executes in user mode.
                let (_user_stack_start, user_stack_end) = memory::allocate_user_stack(user_page_table_ptr)?;
                println!("Thread {} allocated user stack from {:#x} to {:#x}", uid, _user_stack_start as u64, user_stack_end as u64);
                serial_println!("Thread {} allocated user stack from {:#x} to {:#x}", uid, _user_stack_start as u64, user_stack_end as u64);

                // Extract file descriptors for the thread.
                let mut fdescriptor = params.fdescriptor;

                // Construct the thread object.
                Box::new(Thread {
                    thread_id:  uid,
                    // Create a new process associated with the thread.
                    process: Arc::new(RwLock::new(Process {page_table_physaddr: user_page_table_physaddr, fdescriptor: fdescriptor.drain(..).map(|h| Some(h)).collect(), mounts: params.mounts})),
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

            // Update the execution context to point to the ELF binary's entry point.
            let context = new_thread.context_mut();
            context.rip = entry_point as usize;
            context.rflags = 0x0200; // Interrupt enable

            // Set the code and data segment selectors for user mode execution.
            let (code_selector, data_selector) = gdt::get_user_segments();
            context.cs = code_selector.0 as usize; // Code segment flags
            context.ss = data_selector.0 as usize; // Without this we get a GPF

            
            // Set the stack pointer to the end of the allocated user stack. (start when growing up)
            context.rsp = new_thread.user_stack_end as usize;

            // Pass memory details to the thread through registers.
            context.rax = USER_HEAP_START as usize;
            context.rcx = USER_HEAP_SIZE as usize;

            monitor(&new_thread);
            // Print thread details for debugging purposes.
            let thread_id = new_thread.thread_id;
            new_thread.print_details();


            // Add the new thread to the scheduler for execution.
            schedule_thread(new_thread);
            return Ok(thread_id);
        });
    }
    return Err("Could not parse ELF");
}

// Function to create a new thread by duplicating the current thread's state.
pub fn fork_current_thread(current_context: &mut Context) {

    // Check if there's a currently running thread.
    if let Some(current_thread) = CURR_THREAD.read().as_ref() {

        // Fetch the active page table pointer.
        let page_table_ptr = memory::active_pagetable_ptr();


        // Allocate a new user stack for the forked thread.
        if let Ok((_user_stack_start, user_stack_end)) = memory::allocate_user_stack(page_table_ptr) {
            let new_thread = {
                // Create a new kernel stack for the forked thread.
                let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE);
                let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
                let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();

                // Clone the state of the current thread to create a new thread.
                Box::new(Thread {
                    thread_id: unique_id(),
                    process: current_thread.process.clone(), // Use the same process state (shared).
                    page_table_phys: current_thread.page_table_phys, // Use the same page table (shared).
                    kernel_stack,
                    kernel_stack_end,
                    user_stack_end,
                    context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
                    thread_type: current_thread.thread_type,
                })
            };

            // Copy the context of the current thread to the new thread.
            let new_context = unsafe {&mut *(new_thread.context as *mut Context)};
            *new_context = current_context.clone();

            // Setup the new thread's stack pointer and reset the registers for the forked context.
            new_context.rsp = new_thread.user_stack_end as usize;
            new_context.rax = 0; 
            new_context.rdi = 0; 
            // Update the current thread's registers to indicate successful fork.
            current_context.rax = 0; 
            current_context.rdi = new_thread.thread_id as usize;

            let _tid = new_thread.thread_id;
            // Print details of the new thread.
            new_thread.print_details();
            
            monitor(&new_thread);
            // Add the new thread to the running queue.
            RUNNING.write().push_back(new_thread);
        } else {
            // Handle memory allocation error for the user stack.
            println!("err");
            current_context.rax = syscall::SYSCALL_ERROR_MEMALLOC; // Error code
        }
    } else {
        println!("err 2 ");
        current_context.rax = 2; 
    }
}

// Function to terminate the execution of the current thread and remove it from the scheduling queue.
pub fn exit_current_thread(_current_context: &mut Context) {
    // Remove the current thread from the global state.
    {
        let mut current_thread = CURR_THREAD.write();

        if let Some(_thread) = current_thread.take() {
            monitor(&_thread);
        }
    }
    // Wait for the next timer interrupt. This halts the CPU until the next interrupt.
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

    // If there's a currently running thread, move it to the back of the scheduling queue.
    if let Some(thread) = current_thread.take() {
        monitor(&thread);
        // Log thread scheduling details if it's not a kernel thread.
        if thread.thread_type != ThreadType::Kernel {
            serial_println!("[!] - Scheduling thread {}: Using page table at {:#x} and kernel stack at {:#x}", thread.thread_id, thread.page_table_phys, thread.kernel_stack_end);
        } 
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
    // Fetch the next thread to run from the front of the queue.
    *current_thread = running_queue.pop_front();

    match current_thread.as_ref() {
        Some(thread) => {
            // Set the interrupt stack for the next scheduled thread.
            gdt::set_interrupt_stack_table(
                gdt::TIMER_INTERRUPT_INDEX as usize,
                VirtAddr::new(thread.kernel_stack_end)  // Point to the end of the stack.
            );

            // If this isn't a kernel thread, switch to its page table.
            if thread.page_table_phys != 0 {
                memory::switch_to_pagetable(thread.page_table_phys);
            }

            // Return the context address for the next thread to be loaded by the interrupt handler.
            thread.context as usize
        },
        None => 0  // If there's no thread to schedule, return 0.
    }
}

pub fn open_path(current_context: &mut Context,path: &str) -> Result<usize, usize> {
    if let Some(current_thread) = CURR_THREAD.read().as_ref() {
        println!("[!] - Thread {} opening {}", current_thread.thread_id, path);

        let mut process = current_thread.process.write();

        let option = if let Some((_mount, rend)) = process.mounts.read().iter().find(|&(mount, _rend)| mount == path) {
                Some(rend.clone())
            } else {
                None
            };

        if let Some(rv) = option {
            let handle = process.add_handle(rv.clone());
            return Ok(handle);
        } else {
            return Err(7);
        }
    }
    Err(0)
}


pub fn allocate_memory_chunk(pages_required: u64,max_physical_address: u64) -> Result<(VirtAddr, PhysAddr), usize> {
    // Get the current active thread.
    if let Some(current_thread) = CURR_THREAD.read().as_ref() {
        println!("[!] - Thread {} requesting {} pages", current_thread.get_thread_id(), pages_required);

        // Fetch a virtual address for an available chunk of pages.
        let chunk_start_addr = match memory::find_available_page_chunk(
            current_thread.page_table_phys) {
            Some(address) => address,
            None => return Err(syscall::SYSCALL_ERROR_MEMALLOC)
        };
        if max_physical_address != 0 {
            // If the user specifies a max physical address, 
            // we need to allocate consecutive frames.
            let physical_start_addr = match memory::create_sequential_pages(
                current_thread.page_table_phys,
                chunk_start_addr,
                pages_required,
                max_physical_address) {
                Ok(phys_addr) => phys_addr,
                Err(_) => return Err(syscall::SYSCALL_ERROR_MEMALLOC)
            };

            return Ok((chunk_start_addr, physical_start_addr));
        } else {
            // If no specific physical address range is required,
            // allocate frames on an on-demand basis.
            
            if memory::create_user_ondemand_pages(
                current_thread.page_table_phys,
                chunk_start_addr,
                pages_required).is_err() {
                return Err(syscall::SYSCALL_ERROR_MEMALLOC);
            }

            // Since we're not ensuring sequential physical addresses, 
            // we return 0 for the physical address.
            return Ok((chunk_start_addr, PhysAddr::new(0)));
        }
    }
    // Return an error if no active thread is found.
    Err(syscall::SYSCALL_ERROR_MEMALLOC)
}

#[inline]
fn get_current_stack_pointer() -> usize {
    let rsp: usize;
    unsafe {
        asm!("mov {}, rsp", out(reg) rsp);
    }
    rsp
}


fn get_current_user_stack_pointer_from_context(context: &Context) -> usize {
    context.rsp
}


#[inline]
fn get_current_user_stack_pointer() -> usize {
    let rsp: usize;
    unsafe {
        asm!("mov {}, rsp", out(reg) rsp);
    }
    rsp
}


pub fn monitor(thread: &Thread) {
    match thread.thread_type {
        ThreadType::Kernel => {
            if thread.thread_id == 1 || thread.thread_id == 2{
                
            }else {
            let current_stack_ptr = get_current_stack_pointer();
            let stack_usage = thread.kernel_stack_end as isize - current_stack_ptr as isize;
            serial_println!(
                "[STACK_MONITOR][KERNEL][Thread {}] Stack Usage: {} bytes", 
                thread.thread_id, 
                stack_usage
                );
            }
        },
        ThreadType::User => {
            let context = thread.context_mut(); // Assuming this gives you the saved context
            let user_stack_ptr = get_current_user_stack_pointer_from_context(context);
            //serial_println!("Direct user_stack_ptr: {:#x}", user_stack_ptr);
            let stack_usage = thread.user_stack_end as isize - user_stack_ptr as isize;
            serial_println!(
                "[STACK_MONITOR][USER][Thread {}] Stack Usage: {} bytes", 
                thread.thread_id, 
                stack_usage
            );
        },
    }
}

