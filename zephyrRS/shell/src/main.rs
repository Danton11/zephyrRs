#![no_std]
#![no_main]

use api::{syscall,println};
use api::syscall::Message;

#[no_mangle]
fn main() {
    println!("Hello world!");
    let handle = syscall::open("/bin").expect("Couldn't open");
    
    syscall::send(handle,syscall::Message::Short(0, '!' as u64, 0)); // sends character to the
    // bin/main function, but sends it to the keyboard_listener rendezvous 
}
