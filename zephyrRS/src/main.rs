#![no_std]
#![allow(non_snake_case)]
#![no_main]
use core::panic::PanicInfo;


#[no_mangle] // don't mangle the name of this function when compiled  (needs to called start)
pub extern "C" fn _start() -> ! { // ! sets a diverging return value 
    // this function is the entry point, since the linker looks for a function
    // named `_start` by default
    loop {}
}


/// This function is called on panic.
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
} 