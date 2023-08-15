use alloc::{boxed::Box, sync::Arc};
use spin::RwLock;
use core::mem;
use crate::proc::process::Thread;
use crate::syscall;

//When send is called change state from Empty to Sending
//When send is called, Keep sending state, return the calling thread and error (one sending thread
//at a time 
//When send is called on the receiving state, change to empty, return both threads
//


pub const MESSAGE_TYPE_KEY: u64 = 0; 


/// Represents the possible states of a socket during message passing.
pub enum Socket {
    /// The socket is currently not being used.
    Empty,
    /// The socket is being used for sending a message but hasn't been paired with a receiver yet.
    Sending(Option<Box<Thread>>, Message),
    /// The socket is waiting for a message. Optionally restricted to a specific sender thread.
    Receiving(Box<Thread>, Option<u64>),
    /// The socket is being used for both sending a message and then receiving a reply.
    SendReceiving(Box<Thread>, Message),
}


/// Represents the data types that can be part of a message.
pub enum Data {
    /// A simple 64-bit value.
    Value(u64),
    /// A reference to another socket, which allows for creating more complex communication patterns.
    Socket(Arc<RwLock<Socket>>)
}

/// Represents a message that can be sent through the socket.
pub enum Message {
    /// A short message comprising three 64-bit values.
    Short(u64,u64,u64),
    /// A longer message with two pieces of data.
    Long(u64, Data, Data)
}


impl Socket {
    /// Tries to send a message using the socket.
    ///
    /// If the socket is empty, it transitions to the Sending state.
    /// If there's already a receiver waiting, the message is passed and both threads are returned.
    /// If there's another sender, an error is returned to the calling thread.
    ///
    /// Returns a tuple where the first option is the receiving thread and the second option is the sending thread.
    pub fn send_message(&mut self, thread: Option<Box<Thread>>, message: Message) -> (Option<Box<Thread>>, Option<Box<Thread>>) {
        match &*self {
            Socket::Empty => {
                *self = Socket::Sending(thread, message);
                (None, None)
            }
            Socket::Sending(_, _) => {
                if let Some(t) = &thread {
                    t.return_error(1);
                }
                (thread, None)
            }
            Socket::Receiving(_, some_tid) => {
                if let Some(tid) = some_tid {
                    // Restricted to a single thread
                    if let Some(t) = &thread {
                        if t.get_thread_id() != *tid {
                            t.return_error(7);
                            return (thread, None);
                        }
                        // else keep going
                    } else {
                        return (thread, None);
                    }
                }

                if let Socket::Receiving(rec_thread, _) = mem::replace(self, Socket::Empty) {
                    rec_thread.return_message(message);
                    if let Some(ref t) = thread {
                        t.return_error(0);
                    }
                    return (Some(rec_thread), thread);
                }
                (None, None) // This should never be reached
            }
            Socket::SendReceiving(_, _) => {
                // Signal error to thread: Can't have two sending threads
                if let Some(t) = &thread {
                    t.return_error(8);
                }
                (thread, None)
            }
        }
    }

    /// Tries to set the socket to a receiving state.
    ///
    /// If the socket is empty, it transitions to the Receiving state.
    /// If there's a message already waiting to be received, the message is passed to the thread.
    ///
    /// Returns a tuple where the first option is the receiving thread and the second option is the sending thread
    pub fn receive(&mut self, thread: Box<Thread>)-> (Option<Box<Thread>>, Option<Box<Thread>>) {
        match &*self {
            Socket::Empty => {
                *self = Socket::Receiving(thread, None);
                (None, None)
            }
            Socket::Sending(_, _) => {
                // Complete the message transfer
                if let Socket::Sending(snd_thread, message) = mem::replace(self, Socket::Empty) {
                    thread.return_message(message);
                    if let Some(ref t) = snd_thread {
                        t.return_error(0);
                    }
                    return (Some(thread), snd_thread);
                }
                (None, None) // This should never be reached
            }
            Socket::Receiving(_, _) => {
                // Already receiving
                thread.return_error(syscall::SYSCALL_ERROR_RECV_BLOCKING);
                (Some(thread), None)
            }
            Socket::SendReceiving(_, _) => {
                if let Socket::SendReceiving(snd_thread, message) = mem::replace(self, Socket::Empty) {
                    thread.return_message(message);
                    *self = Socket::Receiving(snd_thread, Some(thread.get_thread_id()));
                    return (Some(thread), None);
                }
                (None, None)
            }
        }
    }

    /// Tries to send a message and then sets the socket to wait for a reply.
    ///
    /// This is useful for request-reply patterns.
    /// Returns a tuple where the first option is the receiving thread and the second option is the sending thread.
    pub fn send_receive(&mut self, thread: Box<Thread>, message: Message)-> (Option<Box<Thread>>, Option<Box<Thread>>) {
        match &*self {
            Socket::Empty => {
                *self = Socket::SendReceiving(thread, message);
                (None, None)
            }
            Socket::Sending(_, _) => {
                // Signal error to thread: Can't have two sending threads
                thread.return_error(syscall::SYSCALL_ERROR_SEND_BLOCKING);
                (Some(thread), None)
            }
            Socket::Receiving(_, some_tid) => {
                if let Some(tid) = some_tid {
                    // Restricted to a single thread
                    if thread.get_thread_id() != *tid {
                        // Wrong thread ID
                        thread.return_error(syscall::SYSCALL_ERROR_RECV_BLOCKING);
                        return (Some(thread), None);
                    }
                }

                // Complete the message transfer
                if let Socket::Receiving(rec_thread, _) = mem::replace(self, Socket::Empty) {
                    rec_thread.return_message(message);

                    // Calling thread waits for a reply
                    *self = Socket::Receiving(thread, Some(rec_thread.get_thread_id()));

                    return (Some(rec_thread), None);
                }
                (None, None) // This should never be reached
            }
            Socket::SendReceiving(_, _) => {
                // Signal error to thread: Can't have two sending threads
                thread.return_error(syscall::SYSCALL_ERROR_SEND_BLOCKING);
                (Some(thread), None)
            }
        }
    }
    /// Returns the current state of the socket.
    pub fn get_state(&self) -> &'static str {
        match self {
            Socket::Empty => "Empty",
            Socket::Sending(_, _) => "Sending",
            Socket::Receiving(_, _) => "Receiving",
            Socket::SendReceiving(_, _) => "SendReceiving",
        }
    }
    /// Peeks into the message if available.
    pub fn peek_message(&self) -> Option<&Message> {
        match self {
            Socket::Sending(_, message) => Some(message),
            _ => None,
        }
    }

    /// Resets the socket to the Empty state.
    pub fn reset(&mut self) {
        *self = Socket::Empty;
    }
}

