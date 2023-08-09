use alloc::{boxed::Box, sync::Arc};
use spin::RwLock;
use core::mem;
use crate::proc::process::Thread;
use crate::syscall;

//When send is called change state from Empty to Sending
//When send is called, Keep sending state, return the calling thread and error (one sending thread
//at a time 
//When send is called on the receiving state, change to empty, return both threads
pub enum Rendezvous {
    Empty,
    Sending(Option<Box<Thread>>, Message),
    Receiving(Box<Thread>)
}
pub enum Data {
    Value(u64),
    Rendezvous(Arc<RwLock<Rendezvous>>)
}
pub enum Message {
    Short(u64,u64,u64),
    Long(u64, Data, Data)
}


impl Rendezvous {
    pub fn send_message(&mut self, thread: Option<Box<Thread>>, message: Message) -> (Option<Box<Thread>>, Option<Box<Thread>>) {
        match &*self {
            Rendezvous::Empty => {
                *self = Rendezvous::Sending(thread, message);
                (None, None)
            }
            Rendezvous::Sending(_, _) => {
                if let Some(t) = &thread {
                    t.return_error(1);
                }
                (thread, None)
            }
            Rendezvous::Receiving(_) => {
                if let Rendezvous::Receiving(receiving_thread) = mem::replace(self, Rendezvous::Empty) {
                    receiving_thread.return_message(message);
                    if let Some(ref t) = thread {
                        t.return_error(0);
                    }
                    return (Some(receiving_thread), thread);
                }
                (None, None) // This should never be reached
            }
        }
    }

    pub fn receive(&mut self, thread: Box<Thread>)
                   -> (Option<Box<Thread>>, Option<Box<Thread>>) {
        match &*self {
            Rendezvous::Empty => {
                *self = Rendezvous::Receiving(thread);
                (None, None)
            }
            Rendezvous::Sending(_, _) => {
                // Complete the message transfer
                if let Rendezvous::Sending(snd_thread, message) = mem::replace(self, Rendezvous::Empty) {
                    thread.return_message(message);
                    if let Some(ref t) = snd_thread {
                        t.return_error(0);
                    }
                    return (Some(thread), snd_thread);
                }
                (None, None) // This should never be reached
            }
            Rendezvous::Receiving(_) => {
                // Already receiving
                thread.return_error(syscall::SYSCALL_ERROR_RECV_BLOCKING);
                (Some(thread), None)
            }
        }
    }
}

