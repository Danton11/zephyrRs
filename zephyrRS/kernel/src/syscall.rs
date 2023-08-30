use core::{arch::asm, slice, str};
use core::mem::drop;
use crate::{println,print};
use crate::proc::process;
use crate::boot::gdt;
use crate::boot::interrupts;
use crate::boot::interrupts::Context;
use crate::sync::{Socket,Message, Data};


// Constants for Model Specific Registers (MSRs) related to syscall handling
const MSR_STAR: usize = 0xc0000081;
const MSR_LSTAR: usize = 0xc0000082;
const MSR_FMASK: usize = 0xc0000084;
const MSR_KERNEL_GS_BASE: usize = 0xC0000102;

// Offset for kernel stack during syscalls. This is to ensure syscalls can be interrupted.
const SYSCALL_KERNEL_STACK_OFFSET: u64 = 1024;


// Error codes for system calls
pub const SYSCALL_ERROR_SEND_BLOCKING: usize = 1;
pub const SYSCALL_ERROR_RECV_BLOCKING: usize = 2;
pub const SYSCALL_ERROR_INVALID_HANDLE: usize = 3;
pub const SYSCALL_ERROR_MEMALLOC: usize = 9;
pub const MESSAGE_LONG: u64 = 2 << 8;
pub const MESSAGE_DATA2_RDV: u64 = 2 << 9;
const MESSAGE_DATA2_TYPE: u64 = MESSAGE_DATA2_RDV; // Bit mask
const MESSAGE_DATA2_MOVE: u64 = 2 << 10;

pub const MESSAGE_DATA3_RDV: u64 = 2 << 11;
const MESSAGE_DATA3_TYPE: u64 = MESSAGE_DATA3_RDV; // Bit mask
const MESSAGE_DATA3_MOVE: u64 = 2 << 12;

pub const SYSCALL_FORK: u64 = 2;
pub const SYSCALL_EXIT: u64 = 2;
pub const SYSCALL_WRITE: u64 = 2;
pub const SYSCALL_RECV: u64 = 3;
pub const SYSCALL_SEND: u64 = 4;
pub const SYSCALL_YEILD: u64 = 5;
/// Initialize syscall handling by setting up necessary Model Specific Registers (MSRs)
/// and enabling the System Call Extensions (SCE) for syscall/sysret opcodes.
pub fn init() {
    let handler_addr = handle_syscall as *const () as u64;
    //https://wiki.osdev.org/Model_Specific_Registers
    unsafe {
        // Enable System Call Extensions 
        asm!("mov ecx, 0xC0000080",
             "rdmsr",
             "or eax, 1",
             "wrmsr");

        asm!("xor rdx, rdx",
             "mov rax, 0x300",
             "wrmsr",
             in("rcx") MSR_FMASK);


        asm!("mov rdx, rax",
             "shr rdx, 32",
             "wrmsr",
             in("rax") handler_addr,
             in("rcx") MSR_LSTAR);


        asm!(
            "xor rax, rax",
            "mov rdx, 0x230008", 
            "wrmsr",
            in("rcx") MSR_STAR);


        asm!(
            "mov eax, edx",
            "shr rdx, 32", 
            "wrmsr",
            in("rcx") MSR_KERNEL_GS_BASE,
            in("rdx") gdt::tss_addr()
        );
    }
}

/// Naked syscall handler function which sets up the required environment for processing syscalls.
/// It saves the current context and prepares the stack to make a transition from user space to kernel space.
#[naked]
extern "C" fn handle_syscall() {
    unsafe {
        asm!(
            // switch to a syscall specific stack
            // - https://github.com/redox-os/kernel/blob/master/src/arch/x86_64/interrupt/syscall.rs#L65
            // - https://www.felixcloutier.com/x86/swapgs

            "swapgs", // Put the TSS address into GS (stored in syscalls::init)
            "mov gs:{tss_temp}, rsp", // Save user space stack pointer

            "mov rsp, gs:{tss_timer}", // load kernel stack pointer
            "sub rsp, {ks_offset}", // Use a different location than timer interrupt

            // Create an Exception stack frame
            "sub rsp, 8", 
            "push gs:{tss_temp}",
            "swapgs", // Put TSS address back

            "push r11", // Caller's RFLAGS
            "sub rsp, 8",  // CS
            "push rcx", // Caller's RIP

            // Push all of the context registers to this new stack
            "push rax",
            "push rbx",
            "push rcx",
            "push rdx",

            "push rdi",
            "push rsi",
            "push rbp",
            "push r8",

            "push r9",
            "push r10",
            "push r11",
            "push r12",

            "push r13",
            "push r14",
            "push r15",

            // Call the rust dispatch_syscall function
            "mov r8, rdx", 
            "mov rcx, rsi",
            "mov rdx, rdi",
            "mov rsi, rax",
            "mov rdi, rsp",
            "call {dispatcher}",

            "pop r15", // restore callee-saved registers
            "pop r14",
            "pop r13",

            "pop r12",
            "pop r11",
            "pop r10",
            "pop r9",

            "pop r8",
            "pop rbp",
            "pop rsi",
            "pop rdi",

            "pop rdx",
            "pop rcx", // pop user return pointer
            "pop rbx", 
            "pop rax",

            "add rsp, 24", // Pop new syscall stack
            "pop rsp", // Restore user stack pointer
            
            "cmp rcx, {user_code_start}",
            "jl 2f",
            "sysretq", // switch control to user stack

            "2:",
            "push r11",
            "popf", // Set RFLAGS
            "jmp rcx", // Jump to kernel code
            dispatcher = sym dispatch_syscall,
            tss_timer = const(0x24 + gdt::TIMER_INTERRUPT_INDEX * 8),
            tss_temp = const(0x24 + gdt::SYSCALL_TEMP_INDEX * 8),
            ks_offset = const(SYSCALL_KERNEL_STACK_OFFSET),
            user_code_start = const(process::USER_CODE_START),
            options(noreturn));
    }
}

/// Dispatcher function that handles syscalls based on their ID.
/// It redirects to the appropriate syscall handler function after setting up the execution environment.
extern "C" fn dispatch_syscall(context_ptr: *mut Context, syscall_id: u64, arg1: u64, arg2: u64, arg3: u64) {
    // Dereference the context pointer to get a mutable reference to the Context struct.
    let context = unsafe { &mut *context_ptr };

    // Determine the appropriate code and data segment selectors based on the instruction pointer (RIP).
    // If the RIP is below the USER_CODE_START, we are in kernel mode; otherwise, we are in user mode.
    let (code_selector, data_selector) = 
        if context.rip < process::USER_CODE_START as usize {
            gdt::get_kernel_segments() // Switching threads may overwrite permission registers, so get kernel segments.
        } else {
            gdt::get_user_segments() // We are in user mode, so get user segments.
        };

    // Update the code segment (CS) and stack segment (SS) in the context.
    context.cs = code_selector.0 as usize;
    context.ss = data_selector.0 as usize;

    // Dispatch to the appropriate syscall handler based on the syscall ID.
    // The syscall ID is masked with 0xFF to get the actual ID value.
    match syscall_id & 0xFF {
        0 => process::fork_current_thread(context),  // Fork the current thread
        1 => process::exit_current_thread(context),  // Exit the current thread
        2 => sys_write(arg1 as *const u8, arg2 as usize),  // Write to a file descriptor
        3 => sys_receive(context_ptr, arg1),  // Receive a message
        4 => sys_send(context_ptr, syscall_id, arg1, arg2, arg3),  // Send a message
        5 => sys_send(context_ptr, syscall_id, arg1, arg2, arg3),  // Send a message (duplicate case, consider removing)
        6 => sys_open(context_ptr, arg1 as *const u8, arg2 as usize),  // Open a file
        9 => sys_yield(context_ptr),  // Yield the processor
        _ => println!("Unknown syscall {:?} {} {} {}", context_ptr, syscall_id, arg1, arg2)  // Unknown syscall
    }
}

/// Sends a message using a socket.
/// 
/// # Arguments
/// 
/// * `socket`: The Socket index to send the message to.
/// * `value`: The message to send, encapsulated in a `Message` enum.
/// 
/// # Returns
/// 
/// * `Ok(())` if the message was successfully sent.
/// * `Err(err)` if an error occurred, where `err` is the error code.
pub fn send(socket: u32, value: Message) -> Result<(), u64> {
    // Match the type of message to send
    match value {
        // If the message is of type `Message::Short`
        Message::Short(data1, data2, data3) => {
            // Variable to hold the error code returned by the syscall
            let err: u64;
            
            // Perform the syscall using inline assembly
            unsafe {
                asm!("syscall",
                     in("rax") 4 + ((socket as u64) << 32),  // RAX holds the syscall number and socket
                     in("rdi") data1,  // RDI holds the first data field
                     in("rsi") data2,  // RSI holds the second data field
                     in("rdx") data3,  // RDX holds the third data field
                     lateout("rax") err,  // RAX will hold the error code after syscall
                     out("rcx") _,  // RCX is clobbered by syscall
                     out("r11") _);  // R11 is clobbered by syscall
            }
            
            // Print the error code for debugging purposes
            println!("err: {}", err);
            
            // Check if the syscall was successful
            if err == 0 {
                return Ok(());  // No error, return Ok
            }
            
            // Return the error code if syscall failed
            Err(err)
        },
        // For other types of messages, return an error
        _ => return Err(0)
    }
}

/// System call to write a string to the console.
/// Given a pointer to a string and its size, this syscall prints the string to the console.
extern "C" fn sys_write(ptr: *const u8, size:usize) {
    if size == 0 {
        return;
    }

    let slice = unsafe {slice::from_raw_parts(ptr, size)};

    if let Ok(s) = str::from_utf8(slice) {
        print!("{}",s);
    }
}


/// System call to receive a message from a socket.
///
/// # Arguments
///
/// * `context_ptr`: Pointer to the context of the current thread.
/// * `handle`: The handle of the socket to receive from. Or index of the socket in the list
fn sys_receive(context_ptr: *mut Context, handle: u64) {
    // Attempt to take the current thread from the scheduler
    if let Some(mut thread) = process::take_current_thread() {
        // Store the thread ID of the current thread
        let current_thread_id = thread.get_thread_id();
        
        // Update the context of the current thread
        thread.set_context(context_ptr); // This ensures that all subsequent operations within the system call are working with the most up-to-date context.

        // Attempt to get the socket from the thread's socket list
        if let Some(rdv) = thread.get_sockets(handle) {
            // Perform the receive operation on the socket
            // This returns two threads: one to be started immediately, and one to be scheduled
            let (thread1, thread2) = rdv.write().receive(thread);

            // Flag to indicate whether the current thread should return immediately
            let mut returning = false;
            // After receiving the data, the function may schedule other threads for execution. If the current thread is involved in the rendezvous, it is updated and set as the current thread; otherwise, it's scheduled for later execution.
            // If the current thread is not involved in the rendezvous, a context switch occurs to move to the next scheduled thread.
            // Iterate through the two threads returned by the receive operation
            for new_thread in [thread2, thread1] {
                if let Some(t) = new_thread {
                    if t.get_thread_id() == current_thread_id {
                        // If the thread is the current thread, set it as the current thread and prepare to return
                        process::set_current_thread(t);
                        returning = true;
                    } else {
                        // Otherwise, schedule the thread for later execution
                        process::schedule_thread(t);
                    }
                }
            }

            // If the current thread is not returning, schedule the next thread and switch context
            if !returning {
                let new_context_addr = process::schedule_next(context_ptr as usize);
                interrupts::launch_thread(new_context_addr);
            }
        } else {
            // If the handle is invalid, return an error
            thread.return_error(SYSCALL_ERROR_INVALID_HANDLE);
            process::set_current_thread(thread);
        }
    }
}



/// `sys_send` is responsible for sending messages or data from one thread to another.
/// It is part of the operating system kernel and is invoked by user-space programs.
/// The function performs several key tasks:
/// - Extracts the current thread and updates its context.
/// - Retrieves the socket associated with a given handle.
/// - Sends the message through the socket.
/// - Manages thread scheduling based on the IPC.
fn sys_send(context_ptr: *mut Context, syscall_id: u64, data1: u64, data2: u64, data3: u64) {
    // Extract the current thread
    let handle = syscall_id >> 32;
    if let Some(mut thread) = process::take_current_thread() {
        let current_tid = thread.get_thread_id();
        // Update the thread's context
        thread.set_context(context_ptr);

        // Retrieve the socket associated with the handle
        if let Some(rdv) = thread.get_sockets(handle) {
            // Prepare the message to be sent
            
        // Prepare the message to be sent
        let message = if syscall_id & MESSAGE_LONG == 0 {
            Message::Short(data1, data2, data3)
        } else {
            // Long message
            // Prepare the message based on the syscall arguments and flags
            let message = Message::Long(
                data1,
                // Check the type of data2 and prepare accordingly
                if syscall_id & MESSAGE_DATA2_TYPE == MESSAGE_DATA2_RDV {
                    // Moving or copying a handle
                    if let Some(rdv) = thread.get_sockets(data2) {
                        Data::Socket(rdv)
                    } else {
                        // Invalid handle, return an error
                        thread.return_error(SYSCALL_ERROR_INVALID_HANDLE);
                        process::set_current_thread(thread);
                        return;
                    }
                } else {
                    Data::Value(data2)
                },
                // Check the type of data3 and prepare accordingly
                if syscall_id & MESSAGE_DATA3_TYPE == MESSAGE_DATA3_RDV {
                    if let Some(rdv) = thread.get_sockets(data3) {
                        Data::Socket(rdv)
                    } else {
                        // Invalid handle, return an error
                        thread.return_error(SYSCALL_ERROR_INVALID_HANDLE);
                        process::set_current_thread(thread);
                        return;
                    }
                } else {
                    Data::Value(data3)
                }
            );
            // If the message is valid, remove any handles that are being moved
            if (syscall_id & MESSAGE_DATA2_TYPE == MESSAGE_DATA2_RDV) && (syscall_id & MESSAGE_DATA2_MOVE != 0) {
                let _ = thread.take_socket(data2);
            }
            if (syscall_id & MESSAGE_DATA3_TYPE == MESSAGE_DATA3_RDV) && (syscall_id & MESSAGE_DATA3_MOVE != 0) {
                let _ = thread.take_socket(data3);
            }
            message
        };

            // Send the message and get the threads involved in the rendezvous
            let (thread1, thread2) = match syscall_id & 0xFF {
                4 => rdv.write().send_message(Some(thread), message),
                5 => rdv.write().send_receive(thread, message),
                _ => panic!("Internal error")
            };

            // Determine which threads should be scheduled or returned
            let mut returning = false;
            for maybe_thread in [thread2, thread1] {
                if let Some(t) = maybe_thread {
                    if t.get_thread_id() == current_tid {
                        // The current thread is involved, so return it
                        process::set_current_thread(t);
                        returning = true;
                    } else {
                        // Schedule the other thread for execution
                        process::schedule_thread(t);
                    }
                }
            }

            // If the original thread is not returning, switch context
            if !returning {
                let new_context_addr = process::schedule_next(context_ptr as usize);
                interrupts::launch_thread(new_context_addr);
            }
        } else {
            // Handle is missing, return an error
            thread.return_error(SYSCALL_ERROR_INVALID_HANDLE);
            process::set_current_thread(thread);
        }
    }
}

fn sys_yield(context_ptr: *mut Context) {
    let next_stack = process::schedule_next(context_ptr as usize);
    interrupts::launch_thread(next_stack);
}

fn sys_open(context_ptr: *mut Context,ptr: *const u8,len: usize) {

    let context = unsafe {&mut (*context_ptr)};

    // Check input length
    if len == 0 {
        context.rax = 5;
        return;
    }
    // Convert raw pointer to a slice
    let u8_slice = unsafe {slice::from_raw_parts(ptr, len)};

    if let Ok(path_string) = str::from_utf8(u8_slice) {
        match process::open_path(context, &path_string) {
            Ok(handle) => {
                context.rax = 0; // No error
                context.rdi = handle; // Return handle
            }
            Err(error_code) => {
                context.rax = error_code;
            }
        }
    } else {
        // Bad utf8 conversion
        context.rax = 6;
    }
}
