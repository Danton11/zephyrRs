use crate::hlt_loop;
use crate::{boot::gdt, println,print, serial_println};
use crate::proc::process;
use lazy_static::lazy_static;
use pic8259::ChainedPics;
use spin;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};
use core::sync::atomic::{AtomicBool,Ordering};
use core::arch::asm;

// ------- Constants --------
pub const PIC_1_OFFSET: u8 = 32; // start after exception index (32)
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;
// The size of the context that we're saving on the stack during an interrupt.
// This should be enough to store all the general-purpose registers.
pub const INTERRUPT_CONTEXT_SIZE: usize = 16 * 8;

// ------- Structs and Enums --------

// This struct represents the CPU context that is saved on the stack during an interrupt.
// It includes all the general-purpose registers, as well as the instruction pointer,
// stack pointer, and segment selectors.
// Note: The layout of this struct must match exactly with the assembly code in the wrapper macros.
#[derive(Debug)]
#[repr(packed)]
pub struct CPUContext {
    // These are pushed in the handler function
    pub r15: usize,
    pub r14: usize,
    pub r13: usize,

    pub r12: usize,
    pub r11: usize,
    pub r10: usize,
    pub r9: usize,

    pub r8: usize,
    pub rbp: usize,
    pub rsi: usize,
    pub rdi: usize,

    pub rdx: usize,
    pub rcx: usize,
    pub rbx: usize,
    pub rax: usize,
    // Below is the exception stack frame pushed by the CPU on interrupt
    // Note: For some interrupts (e.g. Page fault), an error code is pushed here
    pub rip: usize,     // Instruction pointer
    pub cs: usize,      // Code segment
    pub rflags: usize,  // Processor flags
    pub rsp: usize,     // Stack pointer
    pub ss: usize,      // Stack segment
    // Here the CPU may push values to align the stack on a 16-byte boundary (for SSE)
}

#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}


// ------- Macros --------

// This macro is used to create interrupt handler functions with a specific prologue and epilogue.
// The prologue saves the state of all general-purpose registers, and the epilogue restores them.
// This ensures that the interrupt handler doesn't unintentionally modify the state of the CPU.
#[macro_export]
macro_rules! wrap {
    ($func: ident => $wrapper:ident) => {
        #[naked]
        pub extern "x86-interrupt" fn $wrapper (_stack_frame: InterruptStackFrame) {
            // Naked functions must consist of a single asm! block
            unsafe{
                asm!(
                // Disable interrupts
                    "cli",
                    // Push registers
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

                    // First argument in rdi with C calling convention
                    "mov rdi, rsp",
                    // Call the hander function
                    "call {handler}",

                    // New stack pointer is in RAX
                    // (C calling convention return value)
                    "cmp rax, 0",
                    "je 2f", // If RAX is zero, keep stack
                    "mov rsp, rax",
                     "2:",

                    // Pop scratch registers
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
                    // Interrupt return
                    "iretq",
                    // Note: Getting the handler pointer here using `sym` operand, because
                    // an `in` operand would clobber a register that we need to save, and we
                    // can't have two asm blocks
                    handler = sym timer_interrupt_handler,
                    options(noreturn)          
                );
                            
            }
        }
    };
}
wrap!(timer_interrupt_handler => timer_handler_naked);



// ------- Lazy Static Initializations --------

// PIC interrupt vector offsets, to avoid conflicts with CPU exceptions (which occupy the first 32 spots).
// This is the Interrupt Descriptor Table. It's a table that tells the CPU 
// what function should be called when a certain interrupt or exception occurs.
lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        // each index set to handler for ISR
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);

        unsafe {
            // We need to use the `set_stack_index` method to specify the stack we prepared for double faults and timer interrupts.
            idt.double_fault.set_handler_fn(double_fault_handler).set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
            idt.general_protection_fault.set_handler_fn(general_protection_fault_handler).set_stack_index(gdt::GENERAL_PROTECTION_FAULT_IST_INDEX);
            idt[InterruptIndex::Timer.as_usize()].set_handler_fn(timer_handler_naked).set_stack_index(gdt::TIMER_INTERRUPT_INDEX); // index 32
        }

        idt.divide_error.set_handler_fn(divide_by_zero_handler);
        idt.invalid_opcode.set_handler_fn(invalid_opcode_handler);

        idt.page_fault.set_handler_fn(page_fault_handler);
        idt[InterruptIndex::Keyboard.as_usize()].set_handler_fn(keyboard_interrupt_handler); // index 33

        idt //return the idt with 'static lifetime
    };
}

// PICS init for hardware
pub static PICS: spin::Mutex<ChainedPics> = spin::Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });


// ------- Public Functions --------

pub fn init_idt() {
    IDT.load(); // loads the idt for the cpu to use
    println!("Initialised IDT...");
    serial_println!("Initialised IDT...");
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}


#[test_case]
fn test_breakpoint_exception() {
    x86_64::instructions::interrupts::int3();
}



// ------- Interrupt Handlers --------

// breakpoint interrupt handler (ISR)
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

// double-fault interrupt handler
extern "x86-interrupt" fn double_fault_handler(stack_frame: InterruptStackFrame,_error_code: u64,) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

// breakpoint interrupt handler (ISR)
extern "x86-interrupt" fn divide_by_zero_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: DIVIDE ERROR\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn invalid_opcode_handler(stack_frame: InterruptStackFrame) {
    panic!("EXCEPTION: INVALID OPCODE at {:#?}", stack_frame);
}


extern "C" fn timer_interrupt_handler(cpuContextAddr: usize) -> usize {
//    print!(".");
    let next_stack = process::schedule_next(cpuContextAddr);
    unsafe {
        PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
    }
    next_stack
}



extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame)
{
    use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};
    use spin::Mutex;
    use x86_64::instructions::port::Port;

    lazy_static! {
        static ref KEYBOARD: Mutex<Keyboard<layouts::Us104Key, ScancodeSet1>> =
            Mutex::new(Keyboard::new(layouts::Us104Key, ScancodeSet1,
                HandleControl::Ignore)
            );
    }


    let mut keyboard = KEYBOARD.lock();
    let mut port = Port::new(0x60);

    let scancode: u8 = unsafe { port.read() };
    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => print!("{}", character),
                DecodedKey::RawKey(key) => print!("{:?}", key),
            }
        }
    }

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

extern "x86-interrupt" fn page_fault_handler(stack_frame: InterruptStackFrame,error_code: PageFaultErrorCode,) {
    use x86_64::registers::control::Cr2; // cr2 registers stores the location of the page fault

    println!("Exception: PAGE FAULT...");
    println!("Accessed Address {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("stack frame: \n{:#?}", stack_frame);
    hlt_loop();
}

extern "x86-interrupt" fn general_protection_fault_handler(stack_frame: InterruptStackFrame, _error_code: u64) {
    panic!("GENERAL PROTECTION FAULT\n{:#?}", stack_frame);
}
