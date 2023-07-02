#![no_std] // Tells the Rust compiler not to link the std library (C)
#![allow(non_snake_case)] 
#![no_main] // Tells the Rust compiler that it does not need to adhere to the std C runtime (i.e calling main)
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(zephyrRS::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"

use core::panic::PanicInfo;
//use core::fmt::Write;

mod vga_buffer; // imports the custom vga module
mod serial;

// entry point
#[no_mangle] // don't mangle the name of this function when compiled  (needs to called start)
pub extern "C" fn _start() -> ! { // ! sets a diverging return value 
    // this function is the entry point, since the linker looks for a function

    println!("Hello World{}","!"); // this println! uses the macro defined in vga_buffer.rs

    // if the cfg attribute 'test' is set, call the function test_main
    #[cfg(test)]
    test_main();

    loop {}
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


