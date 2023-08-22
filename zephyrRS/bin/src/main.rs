#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
use core::format_args;
use api::{println, syscall, syscall::Message};
//use core::sync::atomic::{AtomicUsize, Ordering, AtomicU64};

fn recv_id () -> u64 {
    let thread_id = match syscall::receive(2).unwrap(){ // recv the ID of the thread through the ID_SENDING socket
        Message::Short(value, _, _) => value,
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
    syscall::send(3, Message::Short(thread_id as u64, 0, 0)).unwrap();
    syscall::thread_exit();
}

fn recursive_function(depth: usize, thread_id: u64) {
    // Print the current depth to monitor the recursion
    if depth % 50 == 0 { // print every 50th depth to avoid flooding the console
        println!("[thread {}]: Recursion depth {}", thread_id, depth);
    }
    
    // Artificially use up some stack space
    let _array: [u8; 100] = [0; 100]; // 100 include_bytes!("
    
    syscall::thread_yield();
    // Recursive call
    if depth < 500 { // Limiting the depth to prevent infinite recursion
        recursive_function(depth + 10, thread_id);
    } 
}


pub extern "C" fn tester(){
    let thread_id = recv_id();
    for i in 0..5{
        println!("[thread {}]: {}", thread_id, i);
        syscall::thread_yield();
    }
    syscall::send(3, Message::Short(thread_id as u64, 0, 0)).unwrap();
}


//____________________________________________________________________________





#[no_mangle] 
fn main() {
    let main_thread_id = recv_id();
    println!("I have received my ID: {} ", main_thread_id);
    

    loop{
        let message = syscall::receive(0).unwrap(); // recv from keyboard interrupt handler
        let value = match message {
            Message::Short(_, value, _) => value,
            _ => 0
        };
        let ch = char::from_u32(value as u32).unwrap();

        if ch == 'x' {
            println!("[!] - Exiting");
            break;
        } else if ch == 'b' {
            call_basic_threads();
        } else if ch == 'r' {
            call_recursive_threads();
        }
        
        syscall::send(1, message).unwrap(); // send to vga_listener
    }
}

fn call_recursive_threads() {
    let tid_result = syscall::thread_spawn(recursive_thread);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Short(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };
    match syscall::receive(3) {
        Ok(Message::Short(thread_id, _, _)) => {
            println!("[main]: Thread {} has completed", thread_id);
        },
        _ => {
            println!("[main]: Unexpected message");
        }
    }

}

fn call_basic_threads() {
    let tid_result = syscall::thread_spawn(tester);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Short(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };

   let tid_result = syscall::thread_spawn(tester);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Short(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };


// Wait for the two threads to complete
    for _ in 0..2 {
        match syscall::receive(3) {
            Ok(Message::Short(thread_id, _, _)) => {
                println!("[main]: Thread {} has completed", thread_id);
            },
            _ => {
                println!("[main]: Unexpected message");
            }
        }
    }

    let tid_result = syscall::thread_spawn(tester);
    let tid = match tid_result {
        Ok(thread_id) => {
            println!("[!] - Thread spawned with ID: {}", thread_id);
            syscall::send(2, Message::Short(thread_id as u64, 0, 0)); // send to handle 2, which is the ID_SENDING socket
            thread_id
        },
        Err(error_code) => {
            println!("[!] - Failed to spawn thread. Error code: {}", error_code);
            return; // or handle the error in another way
        }
    };
    // Wait for the two threads to complete
    for _ in 0..1 {
        match syscall::receive(3) {
            Ok(Message::Short(thread_id, _, _)) => {
                println!("[main]: Thread {} has completed", thread_id);
            },
            _ => {
                println!("[main]: Unexpected message");
            }
        }
    }

}
