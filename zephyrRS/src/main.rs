#![no_std] // Tells the Rust compiler not to link the std library (C)
#![allow(non_snake_case)]
#![no_main] // Tells the Rust compiler that it does not need to adhere to the std C runtime (i.e calling main)
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(zephyrRS::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"use core::panic::PanicInfo;

use bootloader::{entry_point, BootInfo};
use core::panic::PanicInfo;
use zephyrRS::mem::memory;
use zephyrRS::syscall;
use zephyrRS::{println, serial_println};
use zephyrRS::proc::process;
extern crate alloc; 


entry_point!(kernel_main);

fn kernel_thread_main() {

    println!("Starting kernel thread!");
    let _ = process::spawn_user_thread(include_bytes!("../user/hello"));
    //process::spawn_user_thread(include_bytes!("../user/hello"));

    zephyrRS::hlt_loop();
}

// entry point
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // ! sets a diverging return value
     // this function is the entry point

    println!("[] - Setting up ZephyrRS!"); // this println! uses the macro defined in vga_buffer.rs
    serial_println!("\n\n[] - Setting up ZephyrRS!");

    // if the cfg attribute 'test' is set, call the function test_main
    zephyrRS::init(); // call init fn from lib.rs for creating gdt, idt and mem
    println!("Successfully initialised Kernel");
    serial_println!("[] - Successfully initialised Kernel");   
    memory::init(boot_info);

    // Set up system calls
    syscall::init();
    
    #[cfg(test)]
    test_main();

    process::spawn_kernel_thread(kernel_thread_main);

    zephyrRS::hlt_loop();
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info); // prints panic info
    loop {}
}

// our panic handler in test mode
#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    zephyrRS::test_panic_handler(info)
}

// practice test_case
#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1); // assertion
}
