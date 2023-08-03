#![no_std] // Tells the Rust compiler not to link the std library (C)
#![allow(non_snake_case)]
#![no_main] // Tells the Rust compiler that it does not need to adhere to the std C runtime (i.e calling main)
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(zephyrRS::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"

use bootloader::{entry_point, BootInfo};
use zephyrRS::hlt_loop;
use core::panic::PanicInfo;
use x86_64::VirtAddr;
use zephyrRS::dev::keyboard;
use zephyrRS::mem::allocator;
use zephyrRS::mem::memory;
use zephyrRS::proc::task::executor::Executor;
use zephyrRS::proc::task::{example_task, task_a, task_b, task_c, Task};
use zephyrRS::{println, serial_println};
use zephyrRS::proc::process;
use core::arch::asm;

extern crate alloc;

entry_point!(kernel_main); // tells the bootloader where entry point is, instead of _start()

fn kernel_thread(){
    serial_println!("Kernel Thread!");
    process::spawn_kernel_thread(test_kernel_fn2);
    loop {
        println!("[[ 1 ]]");
        x86_64::instructions::hlt();
    }
}


fn test_kernel_fn2() {
    println!("Hello from kernel function 2!");

    loop {
        println!("       [[ 2 ]]");
        x86_64::instructions::hlt();
    }
}
// entry point
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // ! sets a diverging return value
     // this function is the entry point

    println!("[] - Setting up ZephyrRS!"); // this println! uses the macro defined in vga_buffer.rs
    serial_println!("\n\n[] - Setting up ZephyrRS!");
    // if the cfg attribute 'test' is set, call the function test_main
    zephyrRS::init(); // call init fn from lib.rs for creating gdt, idt and mem
    
    
    memory::init(boot_info);

    #[cfg(test)]
    test_main();

    println!("Successfully initialised Kernel");
    serial_println!("[] - Successfully initialised Kernel");
    
    //process::spawn_kernel_thread(kernel_thread);
    //process::spawn_user_thread(include_bytes!("../user/exec"));

    hlt_loop();
}

/// This function is called on panic.
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
