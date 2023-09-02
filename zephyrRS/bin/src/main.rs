#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
use core::format_args;
use alloc::string::String;
use core::arch::asm;
extern crate alloc;
use alloc::{boxed::Box};
use api::{println, syscall, syscall::Message};
//use core::sync::atomic::{AtomicUsize, Ordering, AtomicU64};

fn recv_id () -> u64 {
    let thread_id = match syscall::receive(2).unwrap(){ // recv the ID of the thread through the ID_SENDING socket
        Message::Packet(value, _, _) => value,
            _ => 0
    };
    thread_id
}


//__________________________________________________________________________
pub extern "C" fn recursive_thread() {
    let thread_id = recv_id();
    println!("[thread {}]: Starting recursive function", thread_id);
    
    // Start the recursion
    recursive_function(0, thread_id);
    
    // Notify the main thread that this thread has completed
    syscall::send(3, Message::Packet(thread_id as u64, 0, 0)).unwrap();
    syscall::thread_exit();
}


fn recursive_function(depth: usize, thread_id: u64) {
    // Print the current depth to monitor the recursion
    if depth % 50 == 0 { // print every 50th depth to avoid flooding the console
        println!("[thread {}]: Recursion depth {}", thread_id, depth);
    }
    
    // Artificially use up some stack space
    let _array: [u64; 1000] = [0; 1000]; // 100 include_bytes!("
    
    syscall::thread_yield();
    // Recursive call
    if depth < 500 { // Limiting the depth to prevent infinite recursion
        recursive_function(depth + 10 , thread_id);
    } 
}
pub extern "C" fn recursive_too_deep_thread() {
    let thread_id = recv_id();
    println!("[thread {}]: Starting recursive function", thread_id);
    
    // Start the recursion
    recursive_too_deep_function(0, thread_id);
    
    // Notify the main thread that this thread has completed
    syscall::send(3, Message::Packet(thread_id as u64, 0, 0)).unwrap();
    syscall::thread_exit();
}

fn recursive_too_deep_function(depth: usize, thread_id: u64) {
    // Print the current depth to monitor the recursion
    if depth % 50 == 0 { // print every 50th depth to avoid flooding the console
        println!("[thread {}]: Recursion depth {}", thread_id, depth);
    }
    
    // Artificially use up some stack space
    let _array: [u64; 1000] = [0; 1000]; // 100 include_bytes!("
    
    syscall::thread_yield();
    // Recursive call
    if depth < 500 { // Limiting the depth to prevent infinite recursion
        recursive_too_deep_function(depth + 1 , thread_id);
    } 
}
//Thread Stack 0x00028000021000 - 0x0002800002B000 (thread 3)
//Thead Stacl  0x00028000031000 - 0x0002800003B000 (thread 6)
pub extern "C" fn write_to_kernel_stack(){

    let thread_id = recv_id();
    for i in 0..5{
        println!("[thread {}]: {}", thread_id, i);
        syscall::thread_yield();
    }

    // Address of the parent thread's stack
    const PARENT_THREAD_STACK_START: *mut u8 = 0x00444444450280 as *mut u8;
    const PARENT_THREAD_STACK_END: *mut u8 = 0x0044444445A280 as *mut u8;


    
    // Attempt to write to the parent thread's stack
    unsafe {
        // Make sure to not write beyond the stack limits
        if PARENT_THREAD_STACK_START.offset(4) < PARENT_THREAD_STACK_END {
            // Write a value to the parent's stack
            *PARENT_THREAD_STACK_START.offset(4) = 42;

            // Read the value back
            let read_value = *PARENT_THREAD_STACK_START.offset(4);
            println!("[thread {}]: Read value: {}", 6, read_value);
        } else {
            println!("[thread {}]: Attempt to write out of bounds", 6);
        }
    }

    syscall::send(3, Message::Packet(thread_id as u64, 0, 0)).unwrap();
}

pub extern "C" fn tester(){

    let thread_id = recv_id();
    for i in 0..10{
        println!("[thread {}]: {}", thread_id, i);
        syscall::thread_yield();
    }



    syscall::send(3, Message::Packet(thread_id as u64, 0, 0)).unwrap();
}


pub extern "C" fn null_safety_demo() {
    let x: Option<i32> = Some(5);
    let y: Option<i32> = None;

    // You must explicitly handle the None case
    match x {
        Some(val) => println!("Got value: {}", val),
        None => println!("Got nothing"),
    }
}
pub extern "C" fn heap_alloc() {
    let _thread_id = recv_id();
    let _ptr = Box::new(42);
    
    // Memory is automatically freed when `ptr` goes out of scope
    // No manual `free` is required

    // Uncommenting the next line would result in a compile-time error
    // let another_ptr = ptr;  // Error: use of moved value

    // Memory is automatically freed here, at the end of scope
}

//____________________________________________________________________________

//_______DANGLING POINTER EXAMPLE_________
//// This won't compile
//pub extern "C" fn thread_func() {
//    let thread_id = recv_id();
//    let s1 = String::from("hello");
//    let s2 = s1;  // Ownership transferred
//    
//    println!("{}", s1);  // Compile-time error
//}
//
//

//-------Borrow error example---------
//pub extern "C" fn thread_func() {
//    let thread_id = recv_id();
//    let mut x = 5;
//    let y = &mut x;  // Mutable borrow
//    // let z = &x;  // This would be a compile-time error
//}
//



#[no_mangle] 
fn main() {
    let _main_thread_id = recv_id();
    //println!("I have received my ID: {} ", main_thread_id);
    println!("\n\nUser land program usage: 
Input: h - (show these prompts again)\n
Input: b - (calls 3 threads incrementing an individual counter)\n
Input: r - (calls a function that recurs and allocated some space in  the stack)\n
Input: m - (calls a function that attempts to write to a kernel stack (malicious!)\n 
Input: a - (atomically incremenet a counter)\n 
Input: e - (allocate some space on the heap and then free)\n 
        ");

    

    loop{
        let message = syscall::receive(0).unwrap(); // recv from keyboard interrupt handler
        let value = match message {
            Message::Packet(_, value, _) => value,
            _ => 0
        };
        let ch = char::from_u32(value as u32).unwrap();

        if ch == 'x' {
            println!("[!] - Exiting");
            break;
        } else if ch == 'b' {
            call_basic_threads(3);
        } else if ch == 'r' {
            call_recursive_threads();
        } else if ch == 't' {
            call_recursive_too_deep_threads()
        } else if ch == 'm' {
            call_malicious_thread();
        } else if ch == 'e' {
            call_heap_alloc();
        } else if ch == 'n'{
            null_safety_demo();
        } else if ch == '1' || ch == '2' || ch == '3' || ch == '4' {
            let ch_value = ch as u64 - '0' as u64;  // Convert char to its ASCII value and then to u64
            println!("{}", ch_value);
            call_basic_threads(ch_value);
        }
        else if ch == 'h' {
            println!("\n\nUser land program usage: 
Input: h - (show these prompts again)\n
Input: b - (calls 3 threads incrementing an individual counter)\n
Input: r - (calls a function that recurs and allocated some space in  the stack)\n
Input: m - (calls a function that attempts to write to a kernel stack (malicious!)\n 
Input: a - (atomically incremenet a counter)\n 
Input: e - (allocate some space on the heap and then free)\n 
        ");

        }
        
        syscall::send(1, message).unwrap(); // send to vga_listener
    }
}

// double free example
fn call_heap_alloc(){
    let tid_result = syscall::thread_spawn(heap_alloc);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Packet(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };

}

fn call_malicious_thread() {
    let tid_result = syscall::thread_spawn(write_to_kernel_stack);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Packet(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };
}

fn call_recursive_too_deep_threads() {
    let tid_result = syscall::thread_spawn(recursive_too_deep_thread);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Packet(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };
    match syscall::receive(3) {
        Ok(Message::Packet(thread_id, _, _)) => {
            println!("[main]: Thread {} has completed", thread_id);
        },
        _ => {
            println!("[main]: Unexpected message");
        }
    }

}
fn call_recursive_threads() {
    let tid_result = syscall::thread_spawn(recursive_thread);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Packet(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };
    match syscall::receive(3) {
        Ok(Message::Packet(thread_id, _, _)) => {
            println!("[main]: Thread {} has completed", thread_id);
        },
        _ => {
            println!("[main]: Unexpected message");
        }
    }

}

fn call_basic_threads(num: u64) {
    for _ in 0..num {
            let tid_result = syscall::thread_spawn(tester);
            let tid = match tid_result {
                Ok(thread_id) => {
                    println!("[!] - Thread spawned with ID: {}", thread_id);
                    syscall::send(2, Message::Packet(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
                    thread_id
                },
                Err(error_code) => {
                    println!("[!] - Failed to spawn thread. Error code: {}", error_code);
                    return; // or handle the error in another way
                }
            };
        }

    
    
// Wait for the two threads to complete
    for _ in 0..num {
        match syscall::receive(3) {
            Ok(Message::Packet(thread_id, _, _)) => {
                println!("[main]: Thread {} has completed", thread_id);
            },
            _ => {
                println!("[main]: Unexpected message");
            }
        }
    }
}
