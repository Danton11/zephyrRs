use x86_64::{VirtAddr, PhysAddr};
use x86_64::instructions::interrupts;
use x86_64::structures::paging::{PageTableFlags, PageTable};
use spin::RwLock;
use lazy_static::lazy_static;
extern crate alloc;
use alloc::{boxed::Box, collections::vec_deque::VecDeque, vec::Vec, sync::Arc, string::String};
use core::arch::asm;
use crate::{println, serial_println};
use crate::boot::interrupts::{ISF, INTERRUPT_CONTEXT_SIZE, keyboard_socket};
use crate::boot::gdt;
use crate::mem::memory;
use crate::syscall;
use crate::sync::Socket;
use crate::sync::{Message,Data};
use core::fmt;
use crate::ID_SOCKET;
use object::{Object, ObjectSegment};

// Constants for stack sizes and memory regions
pub const KERNEL_STACK_SIZE: usize = 4096 * 3;  // Size of the kernel stack in bytes (12 KiB)
pub const USER_STACK_SIZE: usize = 4096 * 10;   // Size of the user stack in bytes (40 KiB)
pub const USER_CODE_START: u64 = 0x5000000;     // Starting address for user code
const USER_CODE_END: u64 = 0x90000000;          // Ending address for user code
const USER_HEAP_START: u64 = 0x280_0060_0000;   // Starting address for user heap
const USER_HEAP_SIZE: u64 = 4194304;            // Size of the user heap in bytes (4 MiB)

// Global state for thread management
lazy_static! {
    // Queue containing runnable threads that wait to become the running thread
    // in a RwLock to make thread safe (spin lock)
    static ref WAIT_QUEUE: RwLock<VecDeque<Box<Thread>>> =
        RwLock::new(VecDeque::new());
    
    // Current executing thread
    // in a RwLock spin lock to make thread safe
    pub static ref RUNNING_THREAD: RwLock<Option<Box<Thread>>> = RwLock::new(None);
    
    // Counter for generating unique IDs
    static ref COUNTER: RwLock<u64> = RwLock::new(0);
}

// Function to generate a unique ID
pub fn unique_id() -> u64 {
    interrupts::without_interrupts(|| {
        let mut counter = COUNTER.write();
        *counter += 1;
        *counter
    })
}

// Process struct definition
pub struct Process {
    // Physical address of the page table for this process
    page_table_physaddr: u64,
    
    // File descriptors for the process
    sockets: Vec<Option<Arc<RwLock<Socket>>>>,
    
    // Mounted filesystems for the process
    mounts: Arc<RwLock<Vec<(String, Arc<RwLock<Socket>>)>>>,
}

// Enum to distinguish between Kernel and User threads
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ThreadType {
    Kernel,  // Kernel-level thread
    User,    // User-level thread
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
    /// Used to store the Context of a thread on switch
    kernel_stack: Vec<u8>,

    /// The end address of the kernel stack. Given that stacks grow downwards in memory, 
    /// this represents the top of the stack, and it's where new items would be pushed.
    /// Actual address that is placed in the TSS
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
        let context = unsafe {&mut *(self.context as *mut ISF)};
        let kernel_stack_start = self.kernel_stack_end - (KERNEL_STACK_SIZE as u64);
        let user_stack_start = self.user_stack_end - (USER_STACK_SIZE as u64);
        let contextRip = context.instruction_pointer;
        let contextRsp = context.stack_pointer;

        serial_println!("---------------- Thread Details ----------------");
        serial_println!("Thread ID:              {}", self.thread_id);
        serial_println!("Thread Type:            {:?}", self.thread_type);
        serial_println!("RIP:                    {:#016X}", contextRip);
        serial_println!("Kernel Stack  {:#016X} - {:#016X}: ({} bytes)",  kernel_stack_start, self.kernel_stack_end, KERNEL_STACK_SIZE);
        serial_println!("ISF Address:        {:#016X}", self.context);
        serial_println!("Thread Stack {:#016X} - {:#016X} ({} bytes)", user_stack_start, self.user_stack_end, USER_STACK_SIZE);
        serial_println!("RSP:                    {:#016X}", contextRsp);
        serial_println!("-----------------------------------------------");
    }

    pub fn get_sockets(&self, id: u64) -> Option<Arc<RwLock<Socket>>> {
            self.process.read().sockets.get(id as usize).unwrap_or(&None).as_ref().map(|rv| rv.clone()) // Option<Arc<>>
    }

    pub fn take_socket(&self, id: u64)-> Option<Arc<RwLock<Socket>>> {
        self.process.write().sockets.get_mut(id as usize).map_or(None, |elem| elem.take())
    }

    /// Add a socket to the process, returning the handle
    pub fn give_socket(&self, socket: Arc<RwLock<Socket>>) -> u64 {
        // Lock the sockets
        let sockets = &mut self.process.write().sockets;

        // Find empty handle slot
        for (pos, handle) in sockets.iter().enumerate() {
            if handle.is_none() {
                // Found empty slot => Store socket
                sockets[pos] = Some(socket);
                return pos as u64;
            }
        }
        // All full => Add new handle
        sockets.push(Some(socket));
        (sockets.len() - 1) as u64
    }

    // Functions to manipulate and retrieve the context (saved state) of a thread.
    fn context(&self) -> &ISF {
        unsafe {& *(self.context as *const ISF)}
    }

    pub fn context_mut(&self) -> &mut ISF {
        unsafe {&mut *(self.context as *mut ISF)}
    }
    
    pub fn set_context(&mut self, context_ptr: *mut ISF) {
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
            Message::Packet(data1, data2, data3) => {
                context.rdi = data1 as usize;
                context.rsi = data2 as usize;
                context.rdx = data3 as usize;
            },
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
    RUNNING_THREAD.write().take()
}

pub fn set_current_thread(thread: Box<Thread>) {
    // Replace the current thread
    let old_current = RUNNING_THREAD.write().replace(thread);
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
        // Find if there is an empty sockets slots
        if let Some(index) = self.sockets.iter().position(
            |handle| handle.is_none()) {
            self.sockets[index] = Some(rv);
            return index;
        }
        // No free slot -> Add one
        self.sockets.push(Some(rv));
        self.sockets.len() - 1
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
        let context = unsafe {&mut *(self.context as *mut ISF)};
        let kernel_stack_start = self.kernel_stack_end - (KERNEL_STACK_SIZE as u64);
        let user_stack_start = self.user_stack_end - (USER_STACK_SIZE as u64);
        let contextRip = context.instruction_pointer;
        let contextRsp = context.stack_pointer;
        serial_println!("\n
===========================================
Thread ID: {} , {:?}
===========================================
Kernel Stack:
    Start: {:#016X}
    End:   {:#016X}
    ISF Address: {:#016X}
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
    ISF Address: {:#016X}
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


// Function to spawn a new kernel thread.
// This function initializes the thread's context and adds it to the scheduling queue.
pub fn spawn_kernel_thread(function: fn()->(), mut sockets: Vec<Arc<RwLock<Socket>>>) -> u64 {
    // Create a new thread object
    let new_thread = {
        // Allocate kernel stack and user stack within the same memory region
        let kernel_stack = Vec::with_capacity(KERNEL_STACK_SIZE + USER_STACK_SIZE);
        let kernel_stack_start = VirtAddr::from_ptr(kernel_stack.as_ptr());
        let kernel_stack_end = (kernel_stack_start + KERNEL_STACK_SIZE).as_u64();
        let user_stack_end = kernel_stack_end + (USER_STACK_SIZE as u64);

        // Generate a unique thread ID
        let uid = unique_id();

        // Initialize the thread object
        Box::new(Thread {
            thread_id: uid,
            process: Arc::new(RwLock::new(Process { 
                page_table_physaddr: 0, 
                sockets: sockets.drain(..).map(|h| Some(h)).collect(), 
                mounts: Arc::new(RwLock::new(Vec::new()))
            })),
            kernel_stack,
            kernel_stack_end,
            context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
            user_stack_end,
            page_table_phys: 0,
            thread_type: ThreadType::Kernel,
        })
    };

    // Get a mutable ref to thread;s ISF
    let context = new_thread.context_mut();

    // Set the instruction pointer to the function to be executed by the thread
    // provides the thread with a function to run
    context.instruction_pointer = function as usize;

    // Set processor flags; 0x200 enables interrupts
    context.flags = 0x200;

    // Set segment selectors for code and data
    let (code_selector, data_selector) = gdt::get_kernel_segments();
    context.code_segment = code_selector.0 as usize;
    context.stack_segment = data_selector.0 as usize;

    // Set the stack pointer to the end of the user stack
    context.stack_pointer = new_thread.user_stack_end as usize;

    // Get the thread ID for return
    let thread_id = new_thread.thread_id;

    // Monitor the new thread (for debugging or logging)
    monitor(&new_thread);

    // Print thread details (for debugging or logging)
    new_thread.print_details();

    // Add the new thread to the scheduling queue
    schedule_thread(new_thread);

    // Return the unique thread ID
    thread_id
}

// Function to handle the scheduling of threads, ensuring each gets a fair share of CPU time.
pub fn schedule_thread(thread: Box<Thread>) {
    // Turn off interrupts while modifying process table
    // so that we don't get any iterrupt during this
    interrupts::without_interrupts(|| {WAIT_QUEUE.write().push_front(thread);});
}


// The `switch_pagetable_and_execute` function is responsible for temporarily switching to a different page table,
// executing a closure, and then switching back to the original page table.
// This is useful for operations that need to be performed in a different address space.
//
// The function takes two arguments:
// 1. `page_table_physaddr`: The physical address of the new page table to switch to.
// 2. `func`: The closure to execute while the new page table is active.
//
// The function returns the result of the closure execution.
//
// Type Parameters:
// - F: The type of the closure.
// - R: The return type of the closure.
fn switch_pagetable_and_execute<F, R>(page_table_physaddr: u64, func: F) -> R where F: FnOnce() -> R {
    // Store the physical address of the currently active page table.
    // This will be used to restore the original state.
    let original_page_table = memory::active_pagetable_physaddr();

    // Switch to the new page table specified by `page_table_physaddr`.
    memory::switch_to_pagetable(page_table_physaddr);

    // Execute the closure `func` while the new page table is active.
    let result = func();

    // Restore the original page table after the closure has been executed.
    memory::switch_to_pagetable(original_page_table);

    // Return the result of the closure execution.
    result
}


// A struct representing the parameters required to spawn a user thread.
pub struct Params { 
    // A vector containing file descriptors represented as sockets. 
    // This allows for shared access to file descriptors among threads.
    pub sockets: Vec<Arc<RwLock<Socket>>>,
    // A mapping of mount points (e.g., "/mnt/disk1") to their corresponding sockets.
    // This provides a mechanism for threads to access shared mount points.
    pub mounts: Arc<RwLock<Vec<(String, Arc<RwLock<Socket>>)>>> 
}


fn validate_binary(bin: &[u8]) -> bool {
    const MAGIC_BYTES: [u8; 4] = [0x7f, b'E', b'L', b'F'];
    // Check if the provided binary starts with the expected ELF magic bytes.
    if bin[0..4] != MAGIC_BYTES {
        false
    } else {
        true
    }
}


// Parses the ELF binary using the object crate
fn parse_elf_binary(bin: &[u8]) -> Result<object::File<'_>, &'static str> {
    object::File::parse(bin).map_err(|_| "Could not parse ELF")
}

/// Allocates heap for the user process.
///
/// # Arguments
///
/// * `user_pt_phys` - The physical address of the user's page table.
///
/// # Returns
///
/// * `Result<(), &'static str>` - Returns Ok if the heap is successfully allocated, otherwise returns an Err.
fn allocate_user_heap(user_pt_phys: u64) -> Result<(), &'static str> {
    // Define the start and end addresses of the user heap.
    // These should be constants defined in your memory management module.
    let heap_start = VirtAddr::new(USER_HEAP_START);
    let heap_size = USER_HEAP_SIZE;

    // Create on-demand pages for the heap.
    // This function should mark a range of virtual addresses as "on-demand"
    // so that actual pages will be allocated when first accessed.
    if memory::create_user_ondemand_pages(user_pt_phys, heap_start, heap_size).is_err() {
        return Err("Couldn't allocate on-demand pages for user heap");
    }

    Ok(())
}



/// Load an ELF binary into memory.
///
/// # Arguments
///
/// * `elf_file`: The ELF file to be loaded.
/// * `user_page_table_phys`: The physical address of the user's page table.
/// * `user_page_table_ptr`: The pointer to the user's page table.
///
/// # Returns
///
/// * `Result<(), &'static str>`: Returns `Ok(())` if successful, otherwise returns an error message.
fn load_binary_into_memory(elf_file: &object::File, user_page_table_phys: u64, user_page_table_ptr: *mut PageTable) -> Result<(), &'static str> {
    // Get the entry point of the ELF binary.
    let entry_point = elf_file.entry();

    // Iterate over each segment in the ELF binary.
    for segment in elf_file.segments() {
        let segment_start_addr = segment.address() as u64;
        let start_virt_addr = VirtAddr::new(segment_start_addr);
        let end_virt_addr = start_virt_addr + segment.size() as u64;

        // Validate that the segment's memory range is within the allowed user code range.
        if start_virt_addr < VirtAddr::new(USER_CODE_START) || end_virt_addr >= VirtAddr::new(USER_CODE_END) {
            return Err("ELF segment outside allowed range");
        }

        // Reserve memory for the segment in the user's page table.
        if memory::allocate_pages(
            user_page_table_ptr,
            start_virt_addr,
            segment.size() as u64, // Size in bytes
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE
        )
        .is_err()
        {
            return Err("Could not allocate memory");
        }

        // Switch to the user's page table (i.e., switch to user mode).
        memory::switch_to_pagetable(user_page_table_phys);

        // Retrieve the segment data.
        if let Ok(segment_data) = segment.data() {
            // Validate the segment data size.
            if segment_data.len() > segment.size() as usize {
                return Err("ELF data length > segment size");
            } else if !segment_data.is_empty() {
                // Copy the segment data into the allocated memory.
                let dest_ptr = segment_start_addr as *mut u8;
                for (index, value) in segment_data.iter().enumerate() {
                    unsafe {
                        let write_ptr = dest_ptr.add(index);
                        core::ptr::write(write_ptr, *value);
                    }
                }
            }
        } else {
            return Err("Could not get segment data");
        }
    }
    Ok(())
}


/// Initializes a new thread.
///
/// # Arguments
///
/// * `entry_point` - The function that the thread will execute.
/// * `user_pt_phys` - The physical address of the user's page table.
/// * `params` - Additional parameters for thread initialization.
///
/// # Returns
///
/// * `Result<Thread, &'static str>` - Returns a new `Thread` object if successful, otherwise returns an error.
fn initialise_thread(entry_point: u64, user_pt_phys: u64, user_pt_ptr: *mut PageTable, params: Params) -> Result<u64, &'static str> {
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
        let (_user_stack_start, user_stack_end) = memory::allocate_user_stack(user_pt_ptr)?;
        println!("Thread {} allocated user stack from {:#x} to {:#x}", uid, _user_stack_start as u64, user_stack_end as u64);
        serial_println!("Thread {} allocated user stack from {:#x} to {:#x}", uid, _user_stack_start as u64, user_stack_end as u64);

        // Extract file descriptors for the thread.
        let mut sockets = params.sockets;

        // Construct the thread object.
        Box::new(Thread {
            thread_id:  uid,
            process: Arc::new(RwLock::new(Process {page_table_physaddr: user_pt_phys, sockets: sockets.drain(..).map(|h| Some(h)).collect(), mounts: params.mounts})),
            page_table_phys: user_pt_phys,
            kernel_stack,
            kernel_stack_end,
            user_stack_end,
            context: kernel_stack_end - INTERRUPT_CONTEXT_SIZE as u64,
            // User stack needs new pages, not allocated on the kernel heap
            thread_type: ThreadType::User,
            
        })
    };

    // Update the execution context to point to the ELF binary's entry point.
    let context = new_thread.context_mut();
    context.instruction_pointer = entry_point as usize;
    context.flags = 0x0200; // Interrupt enable

    // Set the code and data segment selectors for user mode execution.
    let (code_selector, data_selector) = gdt::get_user_segments();
    context.code_segment = code_selector.0 as usize; // Code segment flags
    context.stack_segment = data_selector.0 as usize; // Without this we get a GPF

    
    // Set the stack pointer to the end of the allocated user stack. (start when growing up)
    context.stack_pointer = new_thread.user_stack_end as usize;

    // Pass memory details to the thread through registers.
    context.rax = USER_HEAP_START as usize;
    context.rcx = USER_HEAP_SIZE as usize;

    monitor(&new_thread);
    // Print thread details for debugging purposes.
    let thread_id = new_thread.thread_id;
    new_thread.print_details();


    // Add the new thread to the scheduler for execution.
    schedule_thread(new_thread);

    Ok(thread_id)

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
pub fn spawn_user_thread(bin_file: &[u8],params: Params) -> Result<u64, &'static str> {
    // https://en.wikipedia.org/wiki/Executable_and_Linkable_Format
    // The magic bytes are the first four bytes in an ELF file.

        // Validate the binary format
        if !validate_binary(bin_file) {
            return Err("Invalid binary format");
        }

           // Parse the binary using object crate
        let obj_result = parse_elf_binary(bin_file);
        if obj_result.is_err() {
            return Err("Failed to parse ELF");
        }
        let obj = obj_result.unwrap();

        // Create a user pagetable that includes only kernel pages.
        let (user_page_table_ptr, user_page_table_physaddr) = memory::create_kernel_only_pagetable();
        //serial_println!("Thread allocated page table at physical address {:#x}", user_page_table_physaddr);

        // Allocate user heap memory. This is memory reserved for dynamic allocations 
        // during the execution of the user thread (e.g., when `malloc` is called).
        allocate_user_heap(user_page_table_physaddr)?;



        // Switch to the user pagetable and setup the memory segments 
        // based on the parsed ELF object.
        return switch_pagetable_and_execute(user_page_table_physaddr, || {
            load_binary_into_memory(&obj, user_page_table_physaddr, user_page_table_ptr)?;
           
            let new_thread_id = initialise_thread(obj.entry(),  user_page_table_physaddr, user_page_table_ptr, params);
            match new_thread_id {
                    Ok(new_id) => {
                        // Return the thread ID as Result<u64, &str>
                        Ok(new_id)
                    },
                    Err(e) => {
                        // Forward the error
                        Err(e)
                    }
                }
        }).map_err(|e| "Error creating user thread");
}

// Function to create a new thread by duplicating the current thread's state.
pub fn fork_current_thread(current_context: &mut ISF) {

    // Check if there's a currently running thread.
    if let Some(current_thread) = RUNNING_THREAD.read().as_ref() {

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
            let new_context = unsafe {&mut *(new_thread.context as *mut ISF)};
            *new_context = current_context.clone();

            // Setup the new thread's stack pointer and reset the registers for the forked context.
            new_context.stack_pointer = new_thread.user_stack_end as usize;
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
            WAIT_QUEUE.write().push_back(new_thread);
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
pub fn exit_current_thread(_current_context: &mut ISF) {
    // Remove the current thread from the global state.
    {
        let mut current_thread = RUNNING_THREAD.write();

        if let Some(_thread) = current_thread.take() {

            WAIT_QUEUE.write().retain(|t| t.thread_id != _thread.thread_id);

            //monitor(&_thread);
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


/// Helper function to update the current thread's context and move it to the back of the wait queue.
fn update_current_thread(context_addr: usize, waiting_queue: &mut VecDeque<Box<Thread>>, current_thread: &mut Option<Box<Thread>>) {
    if let Some(thread) = current_thread.take() {
        monitor(&thread);
        if thread.thread_type != ThreadType::Kernel {
            serial_println!("[SCHEDULER] - Thread {}: Page table at {:#x}, Kernel stack at {:#x}", thread.thread_id, thread.page_table_phys, thread.kernel_stack_end);
        }
        let mut updated_thread = thread;
        updated_thread.context = context_addr as u64;
        updated_thread.page_table_phys = memory::active_pagetable_physaddr();
        waiting_queue.push_back(updated_thread); // push the thread that is being switched out to
        // the back of the queue
    }
}

/// Schedules the next thread to run.
///
/// This function is called by the timer interrupt handler to switch the currently running thread.
/// It updates the context of the currently running thread and moves it to the back of the queue.
/// Then, it pops the next thread from the front of the queue and prepares it for execution.
///
/// # Arguments
///
/// * `context_addr` - The address of the saved context of the currently running thread.
///
/// # Returns
///
/// * `usize` - The address of the saved context of the next thread to run.
pub fn schedule(context_addr: usize) -> usize {
    // Lock the running queue and the current thread for writing.
    let mut waiting_queue = WAIT_QUEUE.write();
    let mut running_thread = RUNNING_THREAD.write();

    // Update the current thread and move it to the back of waiting queue.
    update_current_thread(context_addr, &mut waiting_queue, &mut running_thread);

    // Pop the next thread from the front of the running queue.
    *running_thread = waiting_queue.pop_front();

    // Prepare the next thread for execution.
    match running_thread.as_ref() {
        Some(thread) => {
            // Set the interrupt stack (kernel stack) for the next scheduled thread.
            gdt::set_interrupt_stack_table(gdt::TIMER_INTERRUPT_INDEX as usize,
                VirtAddr::new(thread.kernel_stack_end)); // context of next thread

            // If this isn't a kernel thread, switch to its page table.
            if thread.page_table_phys != 0 {
                memory::switch_to_pagetable(thread.page_table_phys);
            }

            // Return the saved context address for the next thread.
            thread.context as usize
        },
        None => 0  // If there's no thread to schedule, return 0. i.e, before the first thread is
        // spawned but there is still a timer interrupt
    }
}




/// Opens a file or resource specified by a path and returns a handle to it.
///
/// This function is called to open a file or resource specified by the given path.
/// It checks if the path is mounted and if so, returns a handle to the resource.
///
/// # Arguments
///
/// * `current_context` - The current execution context.
/// * `path` - The path of the file or resource to open.
///
/// # Returns
///
/// * `Result<usize, usize>` - Returns a handle to the opened resource if successful, otherwise returns an error code.
pub fn open_path(current_context: &mut ISF, path: &str) -> Result<usize, usize> {
    // Check if there is a currently running thread
    if let Some(current_thread) = RUNNING_THREAD.read().as_ref() {
        println!("[!] - Thread {} opening {}", current_thread.thread_id, path);

        // Lock the process for writing
        let mut process = current_thread.process.write();

        // Check if the path is mounted
        let option = if let Some((_mount, soc)) = process.mounts.read().iter().find(|&(mount, soc)| mount == path) {
            // Clone the socket if the path is mounted
            Some(soc.clone())
        } else {
            // Return None if the path is not mounted
            None
        };

        // If the path is mounted, add a handle and return it
        if let Some(sock) = option {
            let handle = process.add_handle(sock.clone());
            return Ok(handle);
        } else {
            // Return an error code if the path is not mounted
            return Err(7);
        }
    }
    // Return an error code if there is no currently running thread
    Err(0)
}


// Inline function to get the current stack pointer for kernel mode.
#[inline]
fn get_current_stack_pointer() -> usize {
    let rsp: usize;
    unsafe {
        // Inline assembly to move the value of the stack pointer register (rsp) into the variable rsp.
        asm!("mov {}, rsp", out(reg) rsp);
    }
    rsp
}

// Function to get the current user stack pointer from a saved context.
fn get_current_user_stack_pointer_from_context(context: &ISF) -> usize {
    context.stack_pointer
}

// Inline function to get the current stack pointer for user mode.
#[inline]
fn get_current_user_stack_pointer() -> usize {
    let rsp: usize;
    unsafe {
        // Inline assembly to move the value of the stack pointer register (rsp) into the variable rsp.
        asm!("mov {}, rsp", out(reg) rsp);
    }
    rsp
}

// Function to monitor the stack usage of a thread.
pub fn monitor(thread: &Thread) {
    match thread.thread_type {
        // For Kernel threads
        ThreadType::Kernel => {
            // Special handling for thread IDs 1 and 2 (if needed)
            if thread.thread_id == 1 || thread.thread_id == 2 {
                // Do nothing for these specific threads
            } else {
                // Get the current stack pointer
                let current_stack_ptr = get_current_stack_pointer();
                // Calculate stack usage
                let stack_usage = thread.kernel_stack_end as isize - current_stack_ptr as isize;
                // Log the stack usage
                serial_println!(
                    "[STACK_MONITOR][KERNEL][Thread {}] Stack Usage: {} bytes", 
                    thread.thread_id, 
                    stack_usage
                );
            }
        },
        // For User threads
        ThreadType::User => {
            // Get the saved context of the thread
            let context = thread.context_mut();
            // Get the user stack pointer from the saved context
            let user_stack_ptr = get_current_user_stack_pointer_from_context(context);
            // Calculate stack usage
            let stack_usage = thread.user_stack_end as isize - user_stack_ptr as isize;
            // Calculate stack usage as a percentage of the total stack size
            let stack_percentage = (stack_usage as f64 / (4096.0 * 10.0)) * 100.0;
            // Log the stack usage and its percentage
            serial_println!(
                "[STACK_MONITOR][USER][Thread {}] Stack Usage: {} bytes ({}%)" , 
                thread.thread_id, 
                stack_usage, stack_percentage
            );
        },
    }
}

