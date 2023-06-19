#[allow(dead_code)] //incase we dont use a colour
#[derive(Debug, Clone, Copy, PartialEq, Eq)] // By deriving the Copy, Clone, Debug, PartialEq, and Eq traits, we enable copy semantics for the type and make it printable and comparable.
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
const BUFFER_HEIGHT: usize = 25;
const BUFFER_WIDTH: usize = 80;

// Buffer: This structure represents the VGA buffer. It's a 2D array (BUFFER_HEIGHT by BUFFER_WIDTH) of ScreenChar. The #[repr(transparent)] attribute indicates that this struct should have the same memory layout as its only field. This is useful for safety when performing operations that depend on the layout like FFI or interfacing with hardware, as in this case where you're directly interfacing with VGA memory.
#[repr(transparent)]
struct Buffer {
    chars: [[ScreenChar; BUFFER_WIDTH]; BUFFER_HEIGHT],
}


pub struct Writer {
    column_position: usize,  // the current position in the column, i.e., where the next character will be written.
    color_code: ColorCode,   //  the color code used to draw text.
    buffer: &'static mut Buffer, // a mutable reference to the VGA buffer where text will be written.
}

// Implement methods for the `Writer` struct
impl Writer {
    // A method that writes a byte to the VGA text buffer
    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            // If the byte is a newline character
            b'\n' => self.new_line(),  // call the `new_line` method
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
                self.buffer.chars[row][col] = ScreenChar {
                    ascii_character: byte,
                    color_code,
                };
                // Move the column position one step to the right
                self.column_position += 1;
            }
        }
    }

    // A method that starts a new line in the VGA text buffer
    fn new_line(&mut self) {/}
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
}

pub fn print_something() {
    let mut writer = Writer {
        column_position: 0,
        color_code: ColorCode::new(Color::Yellow, Color::Black),
        buffer: unsafe { &mut *(0xb8000 as *mut Buffer) }, // 0xb8000 is the address of the VGA buffer in memory
    };

    writer.write_byte(b'H');
    writer.write_string("ello ");
    writer.write_string("Wörld!");
}