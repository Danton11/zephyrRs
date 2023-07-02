#![no_std]
#![cfg_attr(test,no_main)]
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(crate::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"
#![allow(non_snake_case)] 
#![feature(abi_x86_interrupt)]

use core::panic::PanicInfo;

pub mod serial;
pub mod vga_buffer;
pub mod interrupts;

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
    serial_println!("Running {} tests", tests.len()); // number of tests to be run
    
    for test in tests { // run each test
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> !{
    serial_println!("[failed]\n");
    serial_println!("Error: {}\n",info);
    exit_qemu(QemuExitCode::Failed);
    loop{}
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

//init interrupt handler
pub fn init(){
    interrupts::init_idt();
}

#[cfg(test)]
#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();
    test_main();
    loop{}
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}

// simple vga test
#[test_case]
fn test_println_simple() {
    println!("test_println_simple output"); 
}


// simple vga test
#[test_case]
fn test_println_many() {
    for _ in 0..200{
        println!("test_println_many output"); 
    }
}


