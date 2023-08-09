#![no_std]
#![no_main]
#![feature(alloc_error_handler)]

use core::panic::PanicInfo;
use core::alloc;
use linked_list_allocator::LockedHeap;
use core::arch::asm;
use core::format_args;
use core::fmt;

//pub mod serial;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[alloc_error_handler]
fn alloc_error_handler(layout: alloc::Layout) -> ! {
    panic!("allocation error: {:?}", layout)
}

struct Writer {}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            asm!("mov rax, 2", // syscall  write function
                 "syscall",
                 in("rdi") s.as_ptr(), // First argument
                 in("rsi") s.len());// Second argument
        }
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    Writer{}.write_fmt(args).unwrap();
}

macro_rules! print {
    ($($arg:tt)*) => {
        _print(format_args!($($arg)*));
    };
}

macro_rules! println {
    () => (print!("\n"));
    ($fmt:expr) => (print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => (print!(
        concat!($fmt, "\n"), $($arg)*));
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("Userland panic: {}", info);
    // syscalls::thread_exit();
    unsafe {
        asm!("mov rax, 1", // exit_current_thread syscall
             "syscall");
    }
    loop {}
}


///
fn thread_spawn(func: extern "C" fn() -> ()) -> Result<u64, u64> {
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
             // New thread never leaves this asm block
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
extern "C" fn test() {
    println!("Hello from forked user thread !");

    for i in 1..11 {
        println!("Thread (2) : {}", i);
        for _ in 1..10000000 {
            unsafe { asm!("nop");}
        }
    }
}

#[no_mangle]
pub unsafe extern "sysv64" fn _start() -> ! {
    // Information passed from the operating system
    let heap_start: usize;
    let heap_size: usize;
    asm!("",
         lateout("rax") heap_start,
         lateout("rcx") heap_size,
         options(pure, nomem, nostack)
    );
    println!("Heap start {:#016X}, size: {} bytes ({} Mb)", heap_start, heap_size, heap_size / (1024 * 1024));

    let tid = thread_spawn(test).unwrap();

    for i in 1..10 {
        println!("Thread (1) {}", i);
        for i in 1..10000000 {
            unsafe { asm!("nop");}
        }
    }

    ALLOCATOR.lock().init(heap_start, heap_size);

    loop{
        println!("Calling sys_read");
        let err: u64;
        let value: u64;
        asm!("mov rax, 3", // sys_receive
             "mov rdi, 0", // handle
             "syscall",
             lateout("rax") err,
             lateout("rdi") value);
        let ch = char::from_u32(value as u32).unwrap();
        println!("Received: {} , {} => {}", err, value, ch);
        if ch == 'x' {
            println!("Exiting");
            break;
        }
    }

    asm!("mov rax, 1", // exit_current_thread syscall
         "syscall");
    loop{}
}
