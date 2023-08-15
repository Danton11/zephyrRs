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

pub extern "C" fn tester(){
    //let thread_id = match syscall::receive(2).unwrap(){ // recv the ID of the thread through the ID_SENDING socket
    //    Message::Short(value, _, _) => value,
    //        _ => 0
    //};

    let thread_id = recv_id();
    for i in 0..5{
        println!("[thread {}]: {}", thread_id, i);
        syscall::thread_yield();
    }

}

#[no_mangle] 
fn main() {
    let main_thread_id = recv_id();
    println!("I have received my ID: {} ", main_thread_id);
    
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
        }
        syscall::send(1, message).unwrap(); // send to vga_listener
    }
}

