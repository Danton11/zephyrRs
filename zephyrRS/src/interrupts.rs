use x86_64::structures::idt::{InterruptDescriptorTable,InterruptStackFrame};
use crate::println;
use lazy_static::lazy_static;

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
        idt.breakpoint.set_handler_fn(breakpoint_handler);
        idt //return the idt with 'static lifetime
    };
}


pub fn init_idt(){
    IDT.load(); // loads the idt for the cpu to use 
}

#[test_case]
fn test_breakpoint_exception() {
    x86_64::instructions::interrupts::int3();
}
 
extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame){
    println!("EXCEPTION: BREAKPOINT\n{:#?}",stack_frame);
}
