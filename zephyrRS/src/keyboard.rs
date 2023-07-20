use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use crate::println;


static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
// pub(crate) so only lib.rs can access 
pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if let Err(_) = queue.push(scancode) {
            println!("SCANCODE QUEUE FULL");
        }
    }else {
        println!("SCANCODE QUEUE UNINITIALISED");
    }
}

