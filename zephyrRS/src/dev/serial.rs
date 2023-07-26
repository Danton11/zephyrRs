use uart_16550::SerialPort;
use spin::Mutex;
use lazy_static::lazy_static;

lazy_static!{ // static writer instance and only called once
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial_port = unsafe {SerialPort::new(0x3F8)}; // SerialPort::new expects the address of first IO port
        serial_port.init();
        Mutex::new(serial_port)
    };
}

// for ease of use, create some macros; serial_print and serial_println

#[doc(hidden)]
pub fn _print(args: ::core::fmt::Arguments){
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(||{
        SERIAL1.lock().write_fmt(args).expect("Printing to serial failed");    
    });
    
    
}

// prints to host machine through serial
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::dev::serial::_print(format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt,"\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}
