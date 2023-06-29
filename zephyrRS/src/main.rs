#![no_std]
#![allow(non_snake_case)]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;
//use core::fmt::Write;

mod vga_buffer;

/// This function is called on panic.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println!("{}",_info);
    loop {}
} 


#[no_mangle] // don't mangle the name of this function when compiled  (needs to called start)
pub extern "C" fn _start() -> ! { // ! sets a diverging return value 
    // this function is the entry point, since the linker looks for a function

    println!("Hello World{}","!"); // this println! uses the macro defined in vga_buffer.rs

    #[cfg(test)]
    test_main();

    loop {}
}

#[cfg(test)]
fn test_runner(tests: &[&dyn Fn()]) {
    println!("Running {} tests", tests.len());
    for test in tests {
        test();
    }
}

#[test_case]
fn trivial_assertion() {
    print!("trivial assertion... ");
    assert_eq!(1, 1);
    println!("[ok]");
}
