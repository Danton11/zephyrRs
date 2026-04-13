use crate::println;
use core::fmt;
use lazy_static::lazy_static;
use spin::Mutex;
use core::arch::asm;
use volatile::Volatile;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::RwLock;
use crate::proc::process;
use crate::sync::Socket;
///Color Enum: The Color enum represents the 16 different colors available in VGA text mode.

///ColorCode Struct: The ColorCode struct is used to store color codes for text foreground and background. It's stored as a single u8, with the high 4 bits representing the background color and the low 4 bits representing the foreground color. It also includes an implementation block with a method for creating a new ColorCode.

///ScreenChar Struct: The ScreenChar struct represents a colored character that can be printed to the screen. It includes an ASCII character (ascii_character) and a color code (color_code).

///Buffer Struct: The Buffer struct represents the VGA text buffer, which is a two-dimensional array of ScreenChar.

///Writer Struct: The Writer struct handles the actual process of writing characters to the screen. It keeps track of the current column position (column_position), the color code used for printing (color_code), and a reference to the buffer where characters are written (buffer).

///Writer Impl: The implementation block for Writer includes methods for writing bytes and strings to the buffer, clearing a row, and moving to a new line.

///Lazy Static: The WRITER is a lazy static, meaning it is a global, lazily initialized object. This allows access to the VGA buffer from different parts of the kernel. It's also protected by a spinlock mutex to ensure only one thread can write to the VGA buffer at a time.

///Print Macros: These are print! and println! macros for writing formatted strings to the VGA buffer. The _print function is the backend for these macros. It writes a formatted string to the VGA buffer without allowing interrupts to ensure the operation is atomic.

///Tests: Finally, there are some test cases that check the functionality of the println macro. One test writes a simple message to the screen, another writes many messages, and the last verifies that the correct output is displayed on the screen.
///
///

#[allow(dead_code)] //incase we dont use a colour
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
// By deriving the Copy, Clone, Debug, PartialEq, and Eq traits, we enable copy semantics for the type and make it printable and comparable.
#[repr(u8)] // each enum variant is stored as a u8
pub enum Color {
    Black = 0,
    Blue = 1,
    Green = 2,
    Cyan = 3,
    Red = 4,
    Magenta = 5,
    Brown = 6,
    LightGray = 7,
    DarkGray = 8,
    LightBlue = 9,
    LightGreen = 10,
    LightCyan = 11,
    LightRed = 12,
    Pink = 13,
    Yellow = 14,
    White = 15,
}

// enum for representing colors for the text buffer

// Here's what each line does:

//#[derive(Debug, Clone, Copy, PartialEq, Eq)]: This line is using the derive attribute to automatically create implementation of the Debug, Clone, Copy, PartialEq, and Eq traits for ColorCode. This means you'll be able to print ColorCode with formatting (thanks to Debug), create copies of ColorCode values (thanks to Clone and Copy), and compare ColorCode values for equality (thanks to PartialEq and Eq).
//#[repr(transparent)]: This attribute ensures that the ColorCode struct has the same memory layout as its single field, u8. This is important when the struct is used in FFI (Foreign Function Interface) or certain systems programming scenarios where the precise memory layout is critical.
//struct ColorCode(u8);: This line defines a tuple struct with a single field of type u8. A tuple struct is similar to a tuple, but it's a distinct type.

//The impl ColorCode block provides an associated function (like a static method in other languages) new that takes two Color arguments and returns a new ColorCode.

//(background as u8) << 4 shifts the bits of background four places to the left, effectively multiplying the value by 16. This is likely because the upper 4 bits of the u8 represent the background color in VGA text mode.
//| (foreground as u8) performs a bitwise OR with the foreground value. This is likely because the lower 4 bits of the u8 represent the foreground color in VGA text mode.
//So ColorCode((background as u8) << 4 | (foreground as u8)) creates a ColorCode that packs both the background and foreground colors into a single u8.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)] // to force the same layout as u8
struct ColorCode(u8); // contains the byte colour

impl ColorCode {
    fn new(foreground: Color, background: Color) -> ColorCode {
        ColorCode((background as u8) << 4 | (foreground as u8))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
} // This structure represents a character that can be drawn on the screen. It includes the ASCII value of the character (ascii_character) and the color code that should be used to display it (color_code). The #[repr(C)] attribute ensures that the structure layout is in the C-style, where the fields are laid out in the order specified.

// These constants represent the dimensions of the text buffer, which are likely the dimensions of the text mode VGA screen (80 columns wide by 25 rows high).
pub const BUFFER_HEIGHT: usize = 25;
pub const BUFFER_WIDTH: usize = 80;

// Buffer: This structure represents the VGA buffer. It's a 2D array (BUFFER_HEIGHT by BUFFER_WIDTH) of ScreenChar. The #[repr(transparent)] attribute indicates that this struct should have the same memory layout as its only field. This is useful for safety when performing operations that depend on the layout like FFI or interfacing with hardware, as in this case where you're directly interfacing with VGA memory.
#[repr(transparent)]
struct Buffer {
    chars: [[Volatile<ScreenChar>; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

pub struct Writer {
    column_position: usize, // the current position in the column, i.e., where the next character will be written.
    row_position: usize,
    cursor_position: (usize, usize),
    color_code: ColorCode,       //  the color code used to draw text.
    buffer: &'static mut Buffer, // a mutable reference to the VGA buffer where text will be written.
}

// Implement methods for the `Writer` struct
impl Writer {
    // A method that writes a byte to the VGA text buffer
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            // If the byte is a newline character
            b'\n' => self.new_line(), // call the `new_line` method
            0x08 => {
                // backspace
                println!("[bspace]");
                if self.column_position > 0 {
                    self.column_position -= 1;
                    let x = self.column_position;
                    let y = BUFFER_HEIGHT - 1;
                    self.buffer.chars[x][y].write(ScreenChar {
                        ascii_character: b' ',
                        color_code: self.color_code,
                    });
                } else if BUFFER_HEIGHT > 1 {
                    self.clear_row(self.column_position);
                }
            }
            0x7f => {
                // ASCII for Delete key
                //let (y,x) = self.get_position();

                // Shift all characters to the right of the cursor to the left.
                // `chars` is indexed [row][col], so row_position is the outer index.
                for i in self.column_position..BUFFER_WIDTH - 1 {
                    let ScreenChar {
                        ascii_character: c, ..
                    } = self.buffer.chars[self.row_position][i + 1].read();
                    self.buffer.chars[self.row_position][i].write(ScreenChar {
                        ascii_character: c,
                        color_code: self.color_code,
                    });
                }
                // Clear the last character on the line
                self.buffer.chars[self.row_position][BUFFER_WIDTH - 1].write(ScreenChar {
                    ascii_character: b' ',
                    color_code: self.color_code,
                });
            }
            byte => {
                // If the current column position is beyond the buffer width
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line(); // start a new line
                }

                // Always write to the last row
                let row = BUFFER_HEIGHT - 1;
                // The column is the current column position
                let col = self.column_position;

                let color_code = self.color_code;
                // Write the character to the buffer at the current position with the current color
                self.buffer.chars[row][col].write(ScreenChar {
                    ascii_character: byte,
                    color_code,
                });
                // Move the column position one step to the right
                self.column_position += 1;
            }
        }
    }

    // A method that starts a new line in the VGA text buffer
    fn new_line(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.chars[row][col].read();
                self.buffer.chars[row - 1][col].write(character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
        self.column_position = 0;
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.chars[row][col].write(blank);
        }
    }
}

impl Writer {
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                // printable ASCII byte or newline
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                // not part of printable ASCII range
                _ => self.write_byte(0xfe),
            }
        }
    }

    pub fn set_position(&mut self, column: usize, row: usize) {
        if row < BUFFER_HEIGHT && column < BUFFER_WIDTH {
            self.row_position = row;
            self.column_position = column;
            self.cursor_position = (column, row);
        } else {
            panic!("Position out of bounds");
        }
    }

    pub fn get_position(&self) -> (usize, usize) {
        (self.column_position, self.row_position)
    }

    pub fn read_char(&self, x: usize, y: usize) -> char {
        // TODO: Check bounds
        self.buffer.chars[y][x].read().ascii_character as char
    }
    pub fn write_char_at(&mut self, x: usize, y: usize, c: char) {
        if x < BUFFER_WIDTH && y < BUFFER_HEIGHT {
            self.buffer.chars[y][x].write(ScreenChar {
                ascii_character: c as u8,
                color_code: self.color_code,
            });
        } else {
            panic!("Write position ({}, {}) out of bounds", x, y);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

lazy_static! {
    pub static ref WRITER: Mutex<Writer> = Mutex::new(Writer{
        column_position: 0,
        row_position: BUFFER_HEIGHT - 1,
        cursor_position: (0,  BUFFER_HEIGHT - 1),
        color_code: ColorCode::new(Color::Green, Color::Black),
        buffer: unsafe {&mut *(0xb8000 as *mut Buffer)},

        // NOTE: We have only one unsafe block. Afterwards, all operations are safe.
    });
}

#[macro_export]
macro_rules! print{
    ($($arg:tt)*) => ($crate::dev::vga_buffer::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n",format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

#[test_case]
fn test_println_simple() {
    println!("test_println_simple output");
}

#[test_case]
fn test_println_many() {
    for _ in 0..200 {
        println!("test_println_many output");
    }
}

#[test_case]
fn test_println_output() {
    use core::fmt::Write;
    use x86_64::instructions::interrupts;

    let s = "Some test string that fits on a single line";
    interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        writeln!(writer, "\n{}", s).expect("writeln failed");
        for (i, c) in s.chars().enumerate() {
            let screen_char = writer.buffer.chars[BUFFER_HEIGHT - 2][i].read();
            assert_eq!(char::from(screen_char.ascii_character), c);
        }
    });
}

pub fn start_listener() -> Arc<RwLock<Socket>> {
    let rend = Arc::new(RwLock::new(Socket::Empty));
    process::spawn_kernel_thread(listener, Vec::from([rend.clone()])); // kernel space
    rend
}
fn listener() {
    loop {
        // Receive
        let err: u64;
        let value: u64;
        unsafe {
            asm!("mov rax, 3", // sys_receive
                 "mov rdi, 0", // handle
                 "syscall",
                 lateout("rax") err,
                 lateout("rsi") value,
                 out("rdi") _,
                 out("rdx") _)
        }
        let ch = char::from_u32(value as u32).unwrap();
//        println!("VGA: {} , {} => {}", err, value, ch);
    }
}
