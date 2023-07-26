#![no_std]
#![cfg_attr(test,no_main)]
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(crate::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"
#![allow(non_snake_case)] 
#![feature(abi_x86_interrupt)]
#![feature(const_mut_refs)]
extern crate alloc;

use core::panic::PanicInfo;

pub mod dev;
pub mod boot;
pub mod mem;
pub mod proc;

use dev::vga_buffer;

//init interrupt handler
pub fn init(){
    boot::gdt::init();
    boot::interrupts::init_idt();
    unsafe {boot::interrupts::PICS.lock().initialize()}; // initialise PIC ( hardware interrupts)
    x86_64::instructions::interrupts::enable(); // allow interrupts to reach CPU
}


pub trait Testable {
    fn run(&self) -> ();
} 


impl<T> Testable for T 
where T: Fn(),{
    fn run(&self){
        serial_print!("{}...\t", core::any::type_name::<T>()); // type_name gets the name of the function/test being run
        self(); //invoke the test
        serial_println!("[ok]");
    }
}

//custom test_runner
pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> !{
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n",info);
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}
    
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode){
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt(); // wrapper for asm hlt
    }
}

#[cfg(test)]
use bootloader::{entry_point, BootInfo};

#[cfg(test)]
entry_point!(test_kernel_main);


#[cfg(test)]
fn test_kernel_main(_boot_info: &'static BootInfo) -> ! {
    init();
    test_main();
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}


