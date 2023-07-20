use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use crate::println;
use crate::print;
use core::{pin::Pin, task::{Poll,Context}};
use futures_util::{task::AtomicWaker, stream::{Stream, StreamExt}};
use pc_keyboard::{layouts, DecodedKey, HandleControl, Keyboard, ScancodeSet1};

// Represents a stream of scancodes
pub struct ScancodeStream {
    _private: (),
}


static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

// adds a scancode to the queue of scancodes
// pub(crate) so only lib.rs can access 
pub(crate) fn add_scancode(scancode: u8) {
    // if the initiliased, try add it to the queue
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        // if the queue is full, print a warning message
        if let Err(_) = queue.push(scancode) {
            println!("SCANCODE QUEUE FULL");
        } else {
            // if the scancode was successfully added, wake up the ScancodeStream
            WAKER.wake();
        }
    }else {
        println!("SCANCODE QUEUE UNINITIALISED");
    }
}


impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE.try_init_once(|| ArrayQueue::new(100)).expect("ScancodeStream::new called more than once");
        ScancodeStream { _private: () }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;
    
    // impl the poll_next function to fetch the next item from the stream
    fn poll_next(self: Pin<&mut Self>, cx:&mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE.try_get().expect("uninitialised");

        // try pop a scancode from the queue
        if let Ok(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        // if empty, register the waker and try again
        WAKER.register(&cx.waker());
        match queue.pop() {
            Ok(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            },
            Err(crossbeam_queue::PopError) => Poll::Pending,
        }
    }

}

// translate scancodes into key presses
pub async fn output_keypress() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(layouts::Uk105Key, ScancodeSet1, HandleControl::Ignore);

    // for each scancode in the queue
    while let Some(scancode) = scancodes.next().await {
        //print!("Scancode: {}",scancode);
        if let Ok(Some(key_press)) = keyboard.add_byte(scancode) {
            //print!("key event: {:?}",key_press);
            if let Some(key) = keyboard.process_keyevent(key_press) {
                //println!("Decoded key: {:?}", key);
                match key {
                    DecodedKey::Unicode(char) => {
                        match char {
                            '\u{8}' => {
                                //print!("bspace");
                                // get current position of cursor
                                let pos = crate::vga_buffer::WRITER.lock().get_position();
                                
                                // check if the cursor is not at the start of the line 
                                if pos.0  > 0 {
                                    // move the cursor back by one column 
                                    crate::vga_buffer::WRITER.lock().set_position(pos.0 - 1, pos.1);

                                    // overwrite the char 
                                    print!(" ");
                                    
                                    // move cursor back 

                                    crate::vga_buffer::WRITER.lock().set_position(pos.0 - 1, pos.1);
                                }
                            },
                            _ => print!("{}", char),
                        }    
                    },
                    DecodedKey::RawKey(key) => print!("{:?}",key),
                    }
                }
            }
        }
    }

