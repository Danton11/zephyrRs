use core::arch::asm;
use crate::println;



// Constant representing the message type key.
pub const MESSAGE_TYPE_KEY: u64 = 0;

// Enum representing different types of messages.
pub enum Message {
    // Short message type with three u64 data fields.
    Short(u64, u64, u64),
}

// Implement methods for the Message enum.
impl Message {
    // Method to convert a Message enum into a tuple of four u64 values.
    // Returns a Result containing the tuple if successful, or an error code otherwise.
    pub fn to_values(&self) -> Result<(u64, u64, u64, u64), u64> {
        match self {
            // If the message is of type Short, destructure it into its components.
            Message::Short(data1, data2, data3) => {
                // Return the components as a tuple, prefixed with a 0 to indicate the Short message type.
                Ok((0, *data1, *data2, *data3))
            },
            // For other message types, return an error.
            _ => Err(0)
        }
    }

    // Method to create a Message enum from four u64 values.
    // Currently, it only supports creating Short messages.
    pub fn from_values(_ctrl: u64, data1: u64, data2: u64, data3: u64) -> Message {
        // Create and return a Short message with the given data fields.
        Message::Short(data1, data2, data3)
    }
}


// Function to spawn a new thread.
// Takes a function pointer as an argument, which will be the entry point for the new thread.
// Returns a Result containing the thread ID if successful, or an error code otherwise.
pub fn thread_spawn(func: extern "C" fn() -> ()) -> Result<u64, u64> {
    // Initialize variables to hold the thread ID and error code.
    let mut threadid: u64 = 0;
    let mut error: u64 = 0;

    // Execute inline assembly to perform the syscall for thread spawning.
    unsafe {
        asm!("mov rax, 0", // Set rax to 0 to indicate fork_current_thread syscall.
             "syscall",    // Perform the syscall.
             
             // Check if rax is 0, which indicates no error.
             "cmp rax, 0",
             "jnz 2f",      // Jump to label 2 if there's an error.
             
             // Check if rdi is 0, which indicates a new thread.
             "cmp rdi, 0",
             "jnz 2f",      // Jump to label 2 if not a new thread.
             
             // Call the function pointed to by r8 (func) for the new thread.
             "call r8",
             
             // Set rax to 1 to indicate exit_current_thread syscall.
             "mov rax, 1",
             "syscall",    // Perform the syscall.
             
             "2:",         // Label 2: Error handling.
             
             // Input and output operands for the inline assembly.
             in("r8") func,
             lateout("rax") error,
             lateout("rdi") threadid);
    }

    // Check if there was an error during the syscall.
    if error != 0 {
        return Err(error); // Return the error code.
    }

    // Return the thread ID.
    Ok(threadid)
}

// Function to exit the current thread.
// This function never returns; it ends the current thread.
pub fn thread_exit() -> ! {
    // Execute inline assembly to perform the syscall for thread exit.
    unsafe {
        asm!("mov rax, 1", // Set rax to 1 to indicate exit_current_thread syscall.
             "syscall");   // Perform the syscall.
    }

    // Loop indefinitely as this thread should now be terminated.
    loop {}
}


// Function to receive a message from a file descriptor.
// Takes a file descriptor as an argument.
// Returns a Result containing a Message if successful, or an error code otherwise.
pub fn receive(socket: u64) -> Result<Message, u64> {
    // Initialize variables to hold the error code and message data.
    let mut error_code: u64;
    let (message_data1, message_data2, message_data3): (u64, u64, u64);

    // Execute inline assembly to perform the syscall for receiving a message.
    unsafe {
        asm!("mov rax, 3", // Set rax to 3 to indicate sys_receive syscall.
             "syscall",
             in("rdi") socket,
             lateout("rax") error_code,
             lateout("rdi") message_data1,
             lateout("rsi") message_data2,
             lateout("rdx") message_data3,
             out("rcx") _,
             out("r11") _);
    }

    // Check if there was an error during the syscall.
    if error_code == 0 {
        return Ok(Message::Short(message_data1, message_data2, message_data3));
    }

    // Return the error code.
    Err(error_code)
}

// Function to send a message to a file descriptor.
// Takes a file descriptor and a Message as arguments.
// Returns a Result indicating success or failure.
pub fn send(socket: u32, message: Message) -> Result<(), u64> {
    // Match on the type of message to send.
    match message {
        Message::Short(message_data1, message_data2, message_data3) => {
            // Initialize a variable to hold the error code.
            let mut error_code: u64;

            // Execute inline assembly to perform the syscall for sending a message.
            unsafe {
                asm!("syscall",
                     in("rax") 4 + ((socket as u64) << 32),
                     in("rdi") message_data1,
                     in("rsi") message_data2,
                     in("rdx") message_data3,
                     lateout("rax") error_code,
                     out("rcx") _,
                     out("r11") _);
            }

            // Check if there was an error during the syscall.
            if error_code == 0 {
                return Ok(());
            }

            // Return the error code.
            Err(error_code)
        },
        // If the message type is not supported, return an error.
        _ => return Err(0)
    }
}
/// Send a message and wait for a message back from the same thread.
/// Takes a file descriptor and a Message as arguments.
/// Returns a Result containing a Message if successful, or an error code otherwise.
pub fn send_receive(file_descriptor: u32, message: Message) -> Result<Message, u64> {
    // Convert the message to its constituent values.
    let (control_value, data1, data2, data3) = message.to_values()?;

    // Initialize variables to hold the error code and output message data.
    let mut error_code: u64;
    let (control_value_out, data1_out, data2_out, data3_out): (u64, u64, u64, u64);

    // Execute inline assembly to perform the syscall for sending and receiving a message.
    unsafe {
        // The 'syscall' instruction is used to make a system call.
        // 'rax' register contains the syscall number.
        // 'rdi', 'rsi', 'rdx' are used to pass arguments to the syscall.
        asm!("syscall",
             // Input operands
             in("rax") 5 | control_value | ((file_descriptor as u64) << 32), // syscall number and control value
             in("rdi") data1, // First data value
             in("rsi") data2, // Second data value
             in("rdx") data3, // Third data value

             // Output operands
             lateout("rax") control_value_out, // Output control value
             lateout("rdi") data1_out,        // Output first data value
             lateout("rsi") data2_out,        // Output second data value
             lateout("rdx") data3_out,        // Output third data value

             // Clobbered registers
             out("rcx") _,
             out("r11") _);
    }

    // Extract the error code from the control value.
    error_code = control_value_out & 0xFF;

    // Check if there was an error during the syscall.
    if error_code == 0 {
        return Ok(Message::from_values(control_value_out, data1_out, data2_out, data3_out));
    }

    // Return the error code.
    Err(error_code)
}

/// Open a file or device specified by the path.
/// Takes a string path as an argument.
/// Returns a Result containing a file descriptor if successful, or an error code otherwise.
pub fn open(file_path: &str) -> Result<u32, u64> {
    // Initialize variables to hold the error code and file descriptor.
    let mut error_code: u64;
    let mut file_descriptor: u32;

    // Execute inline assembly to perform the syscall for opening a file or device.
    unsafe {
        // 'mov rax, 6' sets the syscall number to 6 for the 'open' operation.
        // 'syscall' triggers the system call.
        // 'rdi' and 'rsi' are used to pass the file path and its length to the syscall.
        asm!("mov rax, 6", // Set syscall number to 6 (open)
             "syscall",
             
             // Input operands
             in("rdi") file_path.as_ptr(), // Pointer to the file path
             in("rsi") file_path.len(),    // Length of the file path

             // Output operands
             out("rax") error_code,        // Output error code
             lateout("rdi") file_descriptor, // Output file descriptor

             // Clobbered registers
             out("rcx") _,
             out("r11") _);
    }

    // Check if there was an error during the syscall.
    if error_code == 0 {
        Ok(file_descriptor)
    } else {
        Err(error_code)
    }
}

/// Yield the current thread's execution.
pub fn thread_yield() {
    // Inline assembly for syscall
    unsafe {
        // 'syscall' triggers the system call.
        // 'rax' register contains the syscall number 9 for yielding the thread.
        asm!("syscall",
             in("rax") 9, // Set syscall number to 9 (thread_yield)
             
             // Clobbered registers
             out("rcx") _,
             out("r11") _);
    }
}

/// Send a message and wait for a specific response.
///
/// # Arguments
///
/// * `file_descriptor`: The file descriptor to send and receive messages.
/// * `data1`, `data2`, `data3`: The data to send.
/// * `expected_data1`: The expected first data value in the received message.
///
/// # Returns
///
/// * `Result<(u64, u64, u64), u64>`: On success, returns the received data. On failure, returns an error code.
pub fn send_and_wait_for_receive(file_descriptor: u32, data1: u64, data2: u64, data3: u64, expected_data1: Option<u64>) -> Result<(u64, u64, u64), u64> {
    const MAX_RETRIES: usize = 100;
    let mut retry_count = 0;

    loop {
        // Attempt to send and receive a message.
        let send_receive_result = send_receive(file_descriptor, Message::Short(data1, data2, data3));

        match send_receive_result {
            // If rendezvous is blocked, retry.
            Err(1) | Err(2) => {
                retry_count += 1;
                if retry_count > MAX_RETRIES {
                    // Exceeded maximum retries, return an error.
                    return Err(2);
                }

                // Introduce a delay before retrying.
                // Ideally, this should be replaced with a proper syscall for short delays.
                for _ in 0..10000 {
                    unsafe { asm!("nop") }; // No-operation instruction for delay
                }
                continue; // Retry
            }
            // If received a message, check if it matches the expected data.
            Ok(Message::Short(received_data1, received_data2, received_data3)) => {
                if let Some(expected_rd1) = expected_data1 {
                    if received_data1 != expected_rd1 {
                        return Err(received_data1);
                    }
                }
                return Ok((received_data1, received_data2, received_data3));
            }
            // For all other cases, return an error.
            _ => return Err(0),
        }
    }
}
