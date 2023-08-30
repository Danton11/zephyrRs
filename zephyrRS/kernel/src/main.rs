#![no_std] // Tells the Rust compiler not to link the std library (C)
#![allow(non_snake_case)]
#![no_main] // Tells the Rust compiler that it does not need to adhere to the std C runtime (i.e calling main)
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(kernel::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"use core::panic::PanicInfo;

use bootloader::{entry_point, BootInfo};
use spin::RwLock;
use core::panic::PanicInfo;
use kernel::mem::memory;
use kernel::syscall::{self, SYSCALL_YEILD};
use kernel::dev::vga_buffer;
use kernel::boot::interrupts::{self, Context};
use kernel::{println, serial_println, QemuExitCode};
use kernel::proc::process;
use kernel::sync::Socket;
use lazy_static::lazy_static;
extern crate alloc; 
use alloc::vec::Vec;
use alloc::sync::Arc;
use kernel::sync::Message;
use core::arch::asm;

use x86_64::instructions::port::Port;
use alloc::string::String;



entry_point!(kernel_main);

fn kernel_thread_main() {
    let keyboard_listener = interrupts::keyboard_socket();
    let vga_listener = vga_buffer::start_listener();
    let resources = [keyboard_listener.clone(),vga_listener.clone(), kernel::ID_SOCKET.clone(), kernel::FIN_SOCKET.clone(), kernel::PROC_FIN_SOCKET.clone()]; // list of listeners for a process to have
    //
    //

    

    let proc = process::spawn_user_thread(include_bytes!("../../user/bin"),process::Params { sockets: resources.to_vec(), mounts: Arc::new(RwLock::new(Vec::new()))});

    let proc_id = match proc {
        Ok(val) => val,
        Err(e) => {
            println!("Error spawning user thread: {}", e);
            return; // or handle the error in another way
        }
    };
     
    kernel::ID_SOCKET.write().send_message(None, Message::Short(proc_id,0, 0));
//
//    for _ in 0..1000000000{
//        unsafe {asm!("nop");};
//
//    }
//
//    serial_println!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
//    let proc = process::spawn_user_thread(include_bytes!("../../user/bin"),process::Params { sockets: resources.to_vec(), mounts: Arc::new(RwLock::new(Vec::new()))});
//
//    let proc_id = match proc {
//        Ok(val) => val,
//        Err(e) => {
//            println!("Error spawning user thread: {}", e);
//            return; // or handle the error in another way
//        }
//    };
//     
//    kernel::ID_SOCKET.write().send_message(None, Message::Short(proc_id,0, 0));
//
    
    kernel::hlt_loop();
}

// entry point
fn kernel_main(boot_info: &'static BootInfo) -> ! {
    // ! sets a diverging return value
     // this function is the entry point

    println!("[] - Setting up ZephyrRS!"); // this println! uses the macro defined in vga_buffer.rs
    serial_println!("\n\n[] - Setting up ZephyrRS!");

    // if the cfg attribute 'test' is set, call the function test_main
    kernel::init(); // call init fn from lib.rs for creating gdt, idt and mem
    println!("Successfully initialised Kernel");
    serial_println!("[] - Successfully initialised Kernel");   
    memory::init(boot_info);

    // Set up system calls
    syscall::init();
    println!("Successfully initialised syscall functionality");
    serial_println!("Successfully initialised syscall functionality");

    
    #[cfg(test)]
    test_main();
    
    process::spawn_kernel_thread(kernel_thread_main, Vec::new());




    kernel::hlt_loop();
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
    kernel::test_panic_handler(info)
}

// practice test_case
#[test_case]
fn trivial_assertion() {
    assert_eq!(1, 1); // assertion
}
