use core::arch::{asm, naked_asm};
use spin;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};
use lazy_static::lazy_static;
use alloc::sync::Arc;
use spin::RwLock;
use crate::sync::{Socket, Message, MESSAGE_TYPE_KEY};
use crate::{println, serial_println, syscall, print, hlt_loop};
use crate::boot::gdt;
use crate::proc::process;
use crate::mem::memory;
use x86_64::structures::idt::PageFaultErrorCode;
use x86_64::registers::control::Cr2;
use pic8259::ChainedPics;
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
use spin::Mutex;
use x86_64::instructions::port::Port;
//use api::syscall as apisys;

// Use the lazy_static macro to initialize a static InterruptDescriptorTable.
// This ensures that the IDT is only initialized once.
lazy_static! {
    // Define a static Interrupt Descriptor Table (IDT)
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new(); // Create a new IDT

        // Set the handler for the breakpoint interrupt
        idt.breakpoint.set_handler_fn(breakpoint_handler);

        // Set handlers for various fault types, specifying the stack index for each
        // This is done within an unsafe block because setting handlers directly manipulates memory
        unsafe {
            idt.double_fault.set_handler_fn(double_fault_handler).set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
            idt.page_fault.set_handler_fn(page_fault_handler).set_stack_index(gdt::PAGE_FAULT_IST_INDEX);
            idt.general_protection_fault.set_handler_fn(general_protection_fault_handler).set_stack_index(gdt::GENERAL_PROTECTION_FAULT_IST_INDEX);

            // Set handlers for hardware interrupts like Timer and Keyboard
            idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_handler_naked).set_stack_index(gdt::TIMER_INTERRUPT_INDEX);
            idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler).set_stack_index(gdt::KEYBOARD_INTERRUPT_INDEX);
        }
        idt // Return the populated IDT
    };
}

// Initialize a static socket for keyboard interrupts using lazy_static.
// This ensures that the socket is only initialized once and can be shared safely.
lazy_static! {
    // Define a static Arc<RwLock<Socket>> for keyboard interrupts
    static ref KEYBOARD_SOCKET: Arc<RwLock<Socket>> =
        Arc::new(RwLock::new(Socket::Empty)); // Initialize it as empty
}
/// The keyboard interrupt handler will send messages to this.
pub fn keyboard_socket() -> Arc<RwLock<Socket>> {
    KEYBOARD_SOCKET.clone()
}

// Function to load the IDT into the CPU's interrupt controller.
// This is usually called during the OS initialization process.
pub fn init_idt() {
    IDT.load(); // Load the IDT into the CPU
}


/// The `ISF` struct represents the saved CPU state for a thread.
/// It is used for context switching, exception handling, and interrupt handling.
/// Each field corresponds to a CPU register in x86-64 architecture.
/// For more details on CPU registers, refer to: https://wiki.osdev.org/CPU_Registers_x86-64
#[derive(Clone, Debug)]
#[repr(C)] // Ensure C-compatible layout
pub struct ISF {
    // General-purpose registers
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,
    pub r12: usize,
    pub r11: usize,
    pub r10: usize,
    pub r9: usize,
    pub r8: usize,
    pub rbp: usize, // Base pointer
    pub rsi: usize, // Source index for string operations
    pub rdi: usize, // Destination index for string operations

    // Accumulator, counter, data, and base registers
    pub rdx: usize,
    pub rcx: usize,
    pub rbx: usize,
    pub rax: usize,

    // Exception stack frame pushed by the CPU on interrupt
    pub instruction_pointer: usize,     // Instruction pointer: holds the address of the next instruction to be executed
    pub code_segment: usize,      // Code segment: holds the segment selector for the code segment
    pub flags: usize,  // Processor flags: holds the state of the processor flags
    pub stack_pointer: usize,     // Stack pointer: holds the top address of the stack
    pub stack_segment: usize,      // Stack segment: holds the segment selector for the stack segment

    // Additional fields may be pushed by the CPU to align the stack
}
// The ISF struct is crucial for multitasking and handling interrupts or exceptions.
// When a context switch occurs, the operating system saves the current state of the CPU 
// registers into a ISF struct for the thread that is being preempted. 
// Later, when the thread is scheduled to run again, the OS restores the CPU state from 
// this ISF struct. This allows the thread to continue execution as if it was never interrupted.







/// Number of bytes needed to store a ISF struct
pub const INTERRUPT_CONTEXT_SIZE: usize = 20 * 8;

/// The `timer_handler` function is the interrupt service routine (ISR) for timer interrupts.
/// It is responsible for scheduling the next thread to run and updating the system state.
/// This function is marked as `extern "C"` to ensure that it follows the C calling convention,
/// as it will be called by assembly code.
///
/// # Arguments
/// - `context_addr`: The address of the saved context for the current thread.
///
/// # Returns
/// - Returns the stack pointer of the next thread to be scheduled.
extern "C" fn timer_handler(context_addr: usize) -> usize {
    // The process scheduler decides which thread should be scheduled next.
    // `schedule` returns the stack pointer of the next thread to switch to.
    let next_stack = process::schedule(context_addr);

    // Monitor the current thread if it exists.
    // This could involve updating statistics, checking for timeouts, etc.
    if let Some(thread) = process::RUNNING_THREAD.read().as_ref() {
        process::monitor(thread);
    }

    // Notify the Programmable Interrupt Controller (PIC) that the interrupt has been processed.
    // This is necessary to receive further interrupts.
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }

    // Return the stack pointer of the next thread to be scheduled.
    next_stack
}


/// The `launch_thread` function is responsible for switching the CPU context to a new thread.
/// It uses inline assembly to manually set the CPU registers and stack pointer.
/// This function is a critical part of the context-switching mechanism.
///
///  Sets the stack pointer (RSP) to the address of the new ISF struct. This allows the subsequent pop instructions to restore the saved register values.
///  Restores the general-purpose registers (R15, R14, ..., RAX) from the stack.
///  Enables interrupts by setting the interrupt flag (IF) using the sti instruction. This allows the CPU to respond to further interrupts.
///  Executes iretq, which returns from the interrupt and transfers control to the new thread by setting the instruction pointer (RIP) to the saved value.
///
/// # Arguments
/// - `context_addr`: The address of the saved context for the thread that will be launched.
///
/// # Returns
/// - This function does not return; it transfers control to the new thread.
pub fn launch_thread(context_addr: usize) -> ! {
    // Inline assembly to perform the context switch.
    unsafe {
        asm!("mov rsp, rdi", // Set the stack pointer (RSP) to the address of the new ISF

             "pop r15", // Restore general-purpose registers
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
             "pop rcx",
             "pop rbx",
             "pop rax",

             "sti", // Enable interrupts

             "iretq", // Return from interrupt, effectively launching the new thread
             
             in("rdi") context_addr, // Pass the context address as an argument
             
             options(noreturn)); // This function will not return
    }
}



/// The `interrupt_wrap` macro is a utility for wrapping interrupt handler functions.
/// It generates a wrapper function that saves and restores the CPU state, allowing
/// the actual handler function to be written in Rust.
///
/// # Usage
/// ```rust
/// interrupt_wrap!(handler_function => wrapper_function);
/// ```
/// Disables interrupts using the cli instruction.
/// Pushes all general-purpose registers onto the stack to save their current state.
/// Sets the RDI register to the current stack pointer. This is because the C calling convention expects the first argument in RDI.
/// Calls the actual interrupt handler function ($func).
/// Checks the returned stack pointer (stored in RAX). If it's zero, the stack is unchanged; otherwise, it updates the stack pointer.
/// Pops all the saved registers off the stack to restore their state.
/// Enables interrupts using the sti instruction.
/// Returns from the interrupt using the iretq instruction.
///
/// # Arguments
/// - `$func`: The actual interrupt handler function.
/// - `$wrapper`: The name of the generated wrapper function.
#[macro_export]
macro_rules! interrupt_wrap {
    ($func: ident => $wrapper:ident) => {
        /// A naked function that serves as the entry point for the interrupt.
        /// This function is responsible for saving and restoring the CPU state.
        #[unsafe(naked)]
        pub extern "x86-interrupt" fn $wrapper (_stack_frame: InterruptStackFrame) {
            // Naked functions must consist of a single naked_asm! block.
            // Modern nightly replaced asm!+options(noreturn) inside naked fns
            // with the dedicated naked_asm! macro.
            naked_asm!(
                    // Disable interrupts
                    "cli",
                    // Push general-purpose registers onto the stack
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

                    // Set RDI to the stack pointer (C calling convention)
                    "mov rdi, rsp",
                    // Call the actual handler function
                    "call {handler}",

                    // Check the returned stack pointer (in RAX)
                    "cmp rax, 0",
                    "je 2f", // If RAX is zero, keep the current stack
                    "mov rsp, rax",
                     "2:",

                    // Pop general-purpose registers from the stack
                    "pop r15",
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
                    "pop rcx",
                    "pop rbx",
                    "pop rax",
                    // Enable interrupts
                    "sti",
                    // Return from interrupt
                    "iretq",
                handler = sym $func,
            );
        }
    };
}

// Example usage
interrupt_wrap!(timer_handler => timer_handler_naked);


/// Handles breakpoint exceptions.
///
/// This function is triggered when a breakpoint (`int3`) instruction is executed.
/// It prints the state of the CPU at the time of the exception.
extern "x86-interrupt" fn breakpoint_handler(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}


// Check that execution continues after a breakpoint exception
#[test_case]
fn test_breakpoint_exception() {
    // invoke a breakpoint exception
    x86_64::instructions::interrupts::int3();
}


/// Handles double fault exceptions.
///
/// A double fault occurs when an exception is triggered while trying to call an exception handler.
/// This is usually a critical situation that is unrecoverable.
/// The function prints the state of the CPU and then panics.
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame, _error_code: u64) -> !
{
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

/// Handles page fault exceptions.
///
/// A page fault occurs when a program tries to access a page that is not currently in memory.
/// This function checks the type of page fault and tries to handle it accordingly.
extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame,error_code: PageFaultErrorCode,) {
    // Read the accessed virtual address from the CR2 register
    let accessed_virtaddr = Cr2::read();

    if error_code == (PageFaultErrorCode::PROTECTION_VIOLATION |
                      PageFaultErrorCode::CAUSED_BY_WRITE |
                      PageFaultErrorCode::USER_MODE) {

        serial_println!("EXCEPTION: PAGE FAULT");
        serial_println!("Accessed Address: {:?}", accessed_virtaddr);
        serial_println!("Error Code: {:?}", error_code);
        serial_println!("{:#?}", stack_frame);
        if let Err(msg) = memory::allocate_missing_ondemand_frame(accessed_virtaddr) {
            println!("Page fault error: {}", msg);
            serial_println!("Page fault error: {}", msg);
            //hlt_loop();
        }

    } else {
        // Print the fault details BEFORE exiting the thread — exit_current_thread
        // context-switches away, so anything after it is unreachable.
        serial_println!("EXCEPTION: PAGE FAULT");
        serial_println!("Accessed Address: {:?}", accessed_virtaddr);
        serial_println!("Error Code: {:?}", error_code);
        serial_println!("{:#?}", stack_frame);
        println!("EXCEPTION: PAGE FAULT");
        println!("Accessed Address: {:?}", accessed_virtaddr);
        println!("Error Code: {:?}", error_code);
        println!("{:#?}", stack_frame);

        if let Some(thread) = process::RUNNING_THREAD.read().as_ref() {
           serial_println!("Exiting thread {}", thread.get_thread_id());
           process::exit_current_thread(thread.context_mut());
        }
        hlt_loop();
    }
}

/// Handles general protection fault exceptions.
///
/// A general protection fault occurs for various protection violations in the CPU,
/// such as executing a privileged instruction while in user mode.
/// This function prints the state of the CPU and then panics.
extern "x86-interrupt" fn general_protection_fault_handler(stack_frame: InterruptStackFrame,_error_code: u64) {
    if let Some(thread) = process::RUNNING_THREAD.read().as_ref() {
        process::monitor(thread);
    }
    panic!("EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}", stack_frame);
}



// Offset for the first Programmable Interrupt Controller (PIC)
pub const PIC_1_OFFSET: u8 = 32;

// Offset for the second PIC, which is chained to the first
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

// Mutex-protected chained PICs
// spin::Mutex is used because standard locking mechanisms might not be available in a no_std environment
// ChainedPics::new is unsafe because it directly manipulates hardware
pub static PICS: spin::Mutex<ChainedPics> = spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// Enum representing hardware interrupts
///
/// This enum maps hardware interrupts to their corresponding indices in the Interrupt Descriptor Table (IDT).
/// For example, the Timer interrupt is mapped to the index `PIC_1_OFFSET`, and the Keyboard interrupt is mapped to `PIC_1_OFFSET + 1`.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

impl InterruptIndex {
    /// Converts the enum to a u8
    ///
    /// This is useful for operations that require the interrupt index as a byte.
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    /// Converts the enum to a usize
    ///
    /// This is useful for operations that require the interrupt index as a usize, such as array indexing.
    pub fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}
interrupt_wrap!(keyboard_handler_inner => keyboard_interrupt_handler);
extern "C" fn keyboard_handler_inner(context_addr: usize)
                                     -> usize
{


    lazy_static! {
        static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
            Mutex::new(Keyboard::new(layouts::Us104Key, ScancodeSet1,
                HandleControl::Ignore)
            );
    }

    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(0x60);

    let mut returning = true; // Back to original thread?

    let scancode: u8 = unsafe { port.read() };
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => {
                    print!("{}", character);
                    let (thread1, thread2) =
                        KEYBOARD_SOCKET.write().send_message(None,Message::Packet(MESSAGE_TYPE_KEY, character as u64, 0)); // send message to redezvous, bin/main will pick it up
                    // thread1 should be scheduled to run next
                    if let Some(t) = thread2 {
                        process::schedule_thread(t);
                    }
                    if let Some(t) = thread1 {
                        process::schedule_thread(t);
                        returning = false;
                    }
                },
                DecodedKey::RawKey(key) => print!("{:?}", key),
            }
        }
    }

    let next_context = if returning {context_addr} else {
        // Schedule a different thread to run
        process::schedule(context_addr)
    };

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
    next_context
}
