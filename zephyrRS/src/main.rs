#![no_std] // Tells the Rust compiler not to link the std library (C)
#![allow(non_snake_case)] 
#![no_main] // Tells the Rust compiler that it does not need to adhere to the std C runtime (i.e calling main)
#![feature(custom_test_frameworks)] // enables custom test frameworks
#![test_runner(zephyrRS::test_runner)] // specifies the test runner
#![reexport_test_harness_main = "test_main"] // re-exports the test harness main as "test_main"

use core::panic::PanicInfo;
use bootloader::{BootInfo, entry_point};
use zephyrRS::task::executor::Executor;
use zephyrRS::task::keyboard;
use zephyrRS::{allocator};
use zephyrRS::task::{Task, simple_executor::SimpleExecutor};
use zephyrRS::memory;
use x86_64::VirtAddr;

//use core::fmt::Write;
mod vga_buffer; // imports the custom vga module
mod serial;

extern crate alloc; 


entry_point!(kernel_main); // tells the bootloader where entry point is, instead of _start()

// entry point
fn kernel_main(boot_info: &'static BootInfo) -> ! { // ! sets a diverging return value 
    // this function is the entry point
    
    println!("Setting up kernel{}","!:"); // this println! uses the macro defined in vga_buffer.rs
    // if the cfg attribute 'test' is set, call the function test_main
    zephyrRS::init(); // call init fn from lib.rs for creating interrupt handler
    
    let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {memory::BootInfoFrameAllocator::init(&boot_info.memory_map)};

    allocator::init_heap(&mut mapper,&mut frame_allocator).expect("Heap initiliasation failed"); // init the heap using mapper and BootInfoFrameAllocator


    #[cfg(test)]
    test_main();


    println!("Successfully initialised Kernel");


    let mut executor = Executor::new(); // SimpleExecutor is made with empty queue

    executor.1.spawn(Task::new(keyboard::output_keypress()));
    //executor.1.spawn(Task::new(task_a())); // wrap the future from example_task in Task, which pins it on the heap, 'spawn' adds it the queue
    //executor.1.spawn(Task::new(task_b())); // wrap the future from example_task in Task, which pins it on the heap, 'spawn' adds it the queue

    executor.0.run(); // pop the task, create rawwaker for task, call the poll method, check if
    // Poll::ready, if not add to back of the queue, else return 

}

async fn return_number() -> u32 {
    42
}

async fn example_task() {
    let number = return_number().await;
    println!("asyncro number: {}", number);
    serial_println!("asyncro number: {}",number);
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

async fn task_a() {
    let mut a: u32 = 0;
    let mut b: u8 = 0;
    loop {
        if a == 100_000_000 {
            println!("Process A running. {}% complete.", b);
            a = 0;
            b += 1;

            if b == 100 {
                println!("Process A complete.");
                break;
            }
        }
        a += 1;
    }
}

async fn task_b() {
    let mut a: u32 = 0;
    let mut b: u8 = 0;
    loop {
        if a == 100_000_000 {
            println!("Process B running. {}% complete.", b);
            a = 0;
            b += 1;

            if b == 100 {
                println!("Process B complete.");
                break;
            }
        }
        a += 1;
    }
}
