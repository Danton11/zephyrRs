#![no_std]
#![feature(alloc_error_handler)]

use core::arch::asm;
pub mod syscall;
pub mod mem;
pub mod print;

use core::panic::PanicInfo;
#[panic_handler]
pub fn panic(info: &PanicInfo) -> ! {
    println!("User panic: {}", info);
    syscall::thread_exit();
}

// User program entry point
extern {
    fn main() -> ();
}

#[no_mangle]
pub unsafe extern "sysv64" fn _start() -> ! {
    // Information passed from the operating system
    let heap_start: usize;
    let heap_size: usize;
    asm!("",
         lateout("rax") heap_start,
         lateout("rcx") heap_size,
         options(pure, nomem, nostack)
    );
    mem::init(heap_start, heap_size);

    // Call the user program
    main();

    syscall::thread_exit();
}
