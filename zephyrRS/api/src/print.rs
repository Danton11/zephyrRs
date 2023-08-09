use core::arch::asm;
use core::format_args;
use core::fmt;

struct Writer {}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            asm!("mov rax, 2", // syscall function
                 "syscall",
                 in("rdi") s.as_ptr(), // First argument
                 in("rsi") s.len()); // Second argument
        }
        Ok(())
    }
}

pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    Writer{}.write_fmt(args).unwrap();
}

#[macro_export]
macro_rules! _print {
    ($($arg:tt)*) => ($crate::print::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::debug_print!("\n"));
    ($($arg:tt)*) => ($crate::_print!("{}\n", format_args!($($arg)*)));
}
