#![no_std] // Tells the Rust compiler not to link the std library (C)
#![allow(non_snake_case)] 
#![no_main] // Tells the Rust compiler that it does not need to adhere to the std C runtime (i.e calling main)
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(zephyrRS::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"

use core::panic::PanicInfo;
use x86_64::structures::paging::PageTable;
use bootloader::{BootInfo, entry_point};
use alloc::{boxed::Box, vec, vec::Vec, rc::Rc};
use zephyrRS::allocator;

//use core::fmt::Write;
mod vga_buffer; // imports the custom vga module
mod serial;

extern crate alloc; 


entry_point!(kernel_main); // tells the bootloader where entry point is, instead of _start()

// entry point
fn kernel_main(boot_info: &'static BootInfo) -> ! { // ! sets a diverging return value 
    // this function is the entry point
    use zephyrRS::memory;
    use x86_64::{structures::paging::Translate, structures::paging::Page, VirtAddr};
    use x86_64::registers::control::Cr3;
    print!("\x1b[?25h");
    println!("Hello World{}","!"); // this println! uses the macro defined in vga_buffer.rs
    // if the cfg attribute 'test' is set, call the function test_main
    zephyrRS::init(); // call init fn from lib.rs for creating interrupt handler
    
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {memory::BootInfoFrameAllocator::init(&boot_info.memory_map)};

    allocator::init_heap(&mut mapper,&mut frame_allocator).expect("Heap initiliasation failed");

    #[cfg(test)]
    test_main();


    println!("It did not crash!");
    zephyrRS::hlt_loop();
}

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}",info); // prints panic info
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


