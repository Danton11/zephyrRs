use core::arch::asm;
use crate::println;


pub const MESSAGE_TYPE_KEY: u64 = 0; 
pub enum Message {
    Short(u64,u64,u64),
    Long, 
}

impl Message {
    pub fn to_values(&self)-> Result<(u64, u64, u64, u64), u64> {
        match self {
            Message::Short(data1, data2, data3) => {
                Ok((0, *data1, *data2, *data3))
            },
            _ => Err(0)
        }
    }
    pub fn from_values(_ctrl: u64, data1: u64, data2: u64, data3: u64)-> Message {
        Message::Short(data1, data2, data3)
    }
}


pub fn thread_spawn(func: extern "C" fn() -> ()) -> Result<u64, u64> {
    let mut tid: u64 = 0;
    let mut errcode: u64 = 0;
    unsafe {
        asm!("mov rax, 0", // fork_current_thread syscall
             "syscall",
             // rax = 0 indicates no error
             "cmp rax, 0",
             "jnz 2f",
             // rdi = 0 for new thread
             "cmp rdi, 0",
             "jnz 2f",
             // New thread
             "call r8",
             "mov rax, 1", // exit_current_thread syscall
             "syscall",
             "2:",
             in("r8") func,
             lateout("rax") errcode,
             lateout("rdi") tid);
    }
    if errcode != 0 {
        return Err(errcode);
    }
    Ok(tid)
}

pub fn thread_exit() -> ! {
    unsafe {
        asm!("mov rax, 1", // exit_current_thread syscall
             "syscall");
    }
    loop{}
}

pub fn receive(fd: u64) -> Result<Message,u64> {
    let mut err: u64;
    let (data1, data2, data3): (u64, u64, u64);
    unsafe {
        asm!("mov rax, 3", // sys_receive
             "syscall",
             in("rdi") fd,
             lateout("rax") err,
             lateout("rdi") data1,
             lateout("rsi") data2,
             lateout("rdx") data3,
             out("rcx") _,
             out("r11") _);
    }
    if err == 0 {
        return Ok(Message::Short(data1, data2, data3));
    }
    Err(err)
}

pub fn send(fd: u32, value: Message) -> Result<(), u64> {
    match value {
        Message::Short(data1, data2, data3) => {
            let err: u64;
            unsafe {
                asm!("syscall",
                     in("rax") 4 + ((fd as u64) << 32),
                     in("rdi") data1,
                     in("rsi") data2,
                     in("rdx") data3,
                     lateout("rax") err,
                     out("rcx") _,
                     out("r11") _);
            }
            //println!("{}",err);
            if err == 0 {
                return Ok(());
            }
            Err(err)
        },
        _ => return Err(0)
    }
}
/// Send a message and wait for a message back from the same thread
pub fn send_receive(fd: u32,message: Message) -> Result<Message, u64> {
    let (ctrl, data1, data2, data3) = message.to_values()?;

    let err: u64;
    let (ctrl_out, data1_out, data2_out, data3_out): (u64, u64, u64, u64);
    unsafe {
        asm!("syscall",
             in("rax") 5 | ctrl | ((fd as u64) << 32),
             in("rdi") data1,
             in("rsi") data2,
             in("rdx") data3,
             lateout("rax") ctrl_out,
             lateout("rdi") data1_out,
             lateout("rsi") data2_out,
             lateout("rdx") data3_out,
             out("rcx") _,
             out("r11") _);
    }
    let err = ctrl_out & 0xFF;
    if err == 0 {
        return Ok(Message::from_values(ctrl_out,data1_out, data2_out, data3_out));
    }
    Err(err)
}

pub fn open(path: &str) -> Result<u32, u64> {
    let error: u64;
    let fd: u32;
    unsafe {
        asm!("mov rax, 6", // syscall function
             "syscall",
             in("rdi") path.as_ptr(), // First argument
             in("rsi") path.len(), // Second argument
             out("rax") error,
             lateout("rdi") fd,
             out("rcx") _,
             out("r11") _);
    }
    if error == 0 {
        Ok(fd)
    } else {
        Err(error)
    }
}

pub fn thread_yield() {
    unsafe{
        asm!("syscall",
             in("rax") 9,
             out("rcx") _,
             out("r11") _);
    }
} 

pub fn send_and_wait_for_receive(fd: u32,data1: u64,data2: u64,data3: u64,expect_rdata1: Option<u64>) -> Result<(u64, u64, u64), u64> {
    const MAX_RETRIES: usize = 100;

    let mut retry = 0;
    loop {
        // Try sending
        let result = send_receive(fd,Message::Short(data1, data2, data3));

        match result {
            Err(1) |
            Err(2) => {
                // Rendezvous blocked => Wait and try again

                retry += 1;
                if retry > MAX_RETRIES {
                    // Give up
                    return Err(2 as u64);
                }

                // Delay. Should have a syscall for short delay
                for _i in 0..10000 {
                    unsafe{asm!("nop")};
                }
                continue; // Go around for another try
            }
            Ok(Message::Short(rdata1, rdata2, rdata3)) => {
                if let Some(rd1) = expect_rdata1 {
                    // Filter on first argument
                    if rdata1 != rd1 {
                        return Err(rdata1);
                    }
                }
                return Ok((rdata1, rdata2, rdata3));
            }
            _ => return Err(0),
        }
    }
}
