#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
use core::format_args;
use api::{println, syscall, syscall::Message};
use core::arch::asm;


#[no_mangle]
fn main() {
    loop{
        let message = syscall::receive(0).unwrap();
        let value = match message {
            Message::Short(value, _, _) => value,
            _ => 0
        };
        let ch = char::from_u32(value as u32).unwrap();
        println!("Received: {} => {}", value, ch);
        if ch == 'x' {
            println!("Exiting");
            break;
        }
        syscall::send(1, message).unwrap();
    }
}
