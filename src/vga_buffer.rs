//! VGA text-mode driver for the 80x25 buffer mapped at `0xb8000`.
//!
//! Each cell is two bytes: an ASCII code point and a colour byte (high nibble
//! background, low nibble foreground). The hardware may reorder or elide plain
//! stores to that region, so every access goes through volatile reads/writes.

use core::fmt;
use core::ptr;
use spin::{Lazy, Mutex};

pub const BUFFER_HEIGHT: usize = 25;
pub const BUFFER_WIDTH: usize = 80;

const VGA_BUFFER_ADDR: usize = 0xb8000;

/// CRT controller ports. Registers are selected by writing an index to the
/// first port, then read or written through the second.
const CRTC_INDEX: u16 = 0x3d4;
const CRTC_DATA: u16 = 0x3d5;
const CURSOR_LOCATION_HIGH: u8 = 0x0e;
const CURSOR_LOCATION_LOW: u8 = 0x0f;

/// The 16 colours the VGA text mode can display.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
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

/// A packed foreground/background pair, in the layout the hardware expects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct ColorCode(u8);

impl ColorCode {
    pub const fn new(foreground: Color, background: Color) -> Self {
        Self((background as u8) << 4 | (foreground as u8))
    }
}

/// One character cell, laid out exactly as the hardware reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
struct ScreenChar {
    ascii_character: u8,
    color_code: ColorCode,
}

#[repr(transparent)]
struct Buffer {
    chars: [[ScreenChar; BUFFER_WIDTH]; BUFFER_HEIGHT],
}

impl Buffer {
    fn read(&self, row: usize, col: usize) -> ScreenChar {
        // SAFETY: indexing bounds-checks first, and the cell is a valid,
        // aligned MMIO location for the lifetime of the kernel.
        unsafe { ptr::read_volatile(&raw const self.chars[row][col]) }
    }

    fn write(&mut self, row: usize, col: usize, ch: ScreenChar) {
        // SAFETY: as above.
        unsafe { ptr::write_volatile(&raw mut self.chars[row][col], ch) }
    }
}

/// A cursor over the VGA buffer. Text fills the screen from the top and only
/// scrolls once the last row is reached, the way a terminal behaves.
pub struct Writer {
    row_position: usize,
    column_position: usize,
    color_code: ColorCode,
    buffer: &'static mut Buffer,
}

/// The single screen writer. Every `print!` funnels through this lock.
///
/// Built lazily rather than as a plain `static`: const evaluation rejects a
/// reference conjured from an integer address, and it is only valid at runtime
/// anyway.
pub static WRITER: Lazy<Mutex<Writer>> = Lazy::new(|| {
    Mutex::new(Writer {
        row_position: 0,
        column_position: 0,
        color_code: ColorCode::new(Color::LightGray, Color::Black),
        // SAFETY: 0xb8000 is the fixed VGA text buffer, mapped by the
        // bootloader. This closure runs once, so no other `&mut` can exist.
        buffer: unsafe { &mut *(VGA_BUFFER_ADDR as *mut Buffer) },
    })
});

impl Writer {
    /// Sets the colour used for subsequent writes.
    pub fn set_color(&mut self, color_code: ColorCode) {
        self.color_code = color_code;
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),
            byte => {
                if self.column_position >= BUFFER_WIDTH {
                    self.new_line();
                }

                let color_code = self.color_code;
                self.buffer.write(
                    self.row_position,
                    self.column_position,
                    ScreenChar {
                        ascii_character: byte,
                        color_code,
                    },
                );
                self.column_position += 1;
            }
        }
    }

    /// Writes a string, substituting `■` for anything outside printable ASCII —
    /// VGA text mode has no notion of UTF-8.
    pub fn write_string(&mut self, s: &str) {
        for byte in s.bytes() {
            match byte {
                0x20..=0x7e | b'\n' => self.write_byte(byte),
                _ => self.write_byte(0xfe),
            }
        }
        // Once per string rather than per byte: four port writes per character
        // would dominate the cost of printing.
        self.update_cursor();
    }

    /// Points the blinking hardware cursor at the next write position.
    ///
    /// Without this it stays at the top-left corner forever, sitting in the
    /// middle of the text instead of trailing it.
    fn update_cursor(&self) {
        use x86_64::instructions::port::Port;

        let position = (self.row_position * BUFFER_WIDTH + self.column_position) as u16;
        let mut index: Port<u8> = Port::new(CRTC_INDEX);
        let mut data: Port<u8> = Port::new(CRTC_DATA);

        // SAFETY: the CRT controller's index/data pair is the architectural way
        // to address cursor registers, and no other code touches these ports.
        unsafe {
            index.write(CURSOR_LOCATION_LOW);
            data.write((position & 0xff) as u8);
            index.write(CURSOR_LOCATION_HIGH);
            data.write((position >> 8) as u8);
        }
    }

    /// Blanks the whole screen and returns the cursor to the top-left.
    pub fn clear_screen(&mut self) {
        for row in 0..BUFFER_HEIGHT {
            self.clear_row(row);
        }
        self.row_position = 0;
        self.column_position = 0;
        self.update_cursor();
    }

    fn new_line(&mut self) {
        if self.row_position + 1 < BUFFER_HEIGHT {
            self.row_position += 1;
        } else {
            self.scroll_up();
        }
        self.column_position = 0;
    }

    /// Shifts every row up by one and blanks the last, leaving the cursor on
    /// the bottom row.
    fn scroll_up(&mut self) {
        for row in 1..BUFFER_HEIGHT {
            for col in 0..BUFFER_WIDTH {
                let character = self.buffer.read(row, col);
                self.buffer.write(row - 1, col, character);
            }
        }
        self.clear_row(BUFFER_HEIGHT - 1);
    }

    fn clear_row(&mut self, row: usize) {
        let blank = ScreenChar {
            ascii_character: b' ',
            color_code: self.color_code,
        };
        for col in 0..BUFFER_WIDTH {
            self.buffer.write(row, col, blank);
        }
    }
}

impl fmt::Write for Writer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_string(s);
        Ok(())
    }
}

/// Prints to the VGA text buffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

/// Prints to the VGA text buffer, with a trailing newline.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;

    // The timer and keyboard handlers print too. Without masking interrupts, an
    // interrupt landing between `lock()` and the unlock would deadlock the CPU.
    x86_64::instructions::interrupts::without_interrupts(|| {
        WRITER.lock().write_fmt(args).unwrap();
    });
}

/// Prints in a specific colour, restoring the previous colour afterwards.
pub fn print_colored(color_code: ColorCode, args: fmt::Arguments) {
    use core::fmt::Write;

    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut writer = WRITER.lock();
        let previous = writer.color_code;
        writer.set_color(color_code);
        writer.write_fmt(args).unwrap();
        writer.set_color(previous);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::fmt::Write;

    #[test_case]
    fn println_does_not_panic() {
        println!("println_does_not_panic output");
    }

    #[test_case]
    fn println_scrolls_without_panicking() {
        for _ in 0..200 {
            println!("scroll test");
        }
    }

    #[test_case]
    fn println_lands_in_the_buffer() {
        let s = "a single line that fits on screen";

        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut writer = WRITER.lock();
            writeln!(writer, "\n{s}").unwrap();

            // The trailing newline moved the cursor one row past the text.
            let row = writer.row_position - 1;
            for (i, c) in s.chars().enumerate() {
                let screen_char = writer.buffer.read(row, i);
                assert_eq!(char::from(screen_char.ascii_character), c);
            }
        });
    }

    #[test_case]
    fn non_ascii_is_replaced() {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut writer = WRITER.lock();
            writer.write_string("\nsalom, olam — ø");

            // The em dash and `ø` must both collapse to the fallback glyph.
            let row = writer.row_position;
            let fallback_count = (0..BUFFER_WIDTH)
                .filter(|&col| writer.buffer.read(row, col).ascii_character == 0xfe)
                .count();
            assert!(fallback_count >= 2);
        });
    }

    #[test_case]
    fn cursor_stops_at_the_last_row_and_scrolls() {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut writer = WRITER.lock();
            writer.clear_screen();
            assert_eq!(writer.row_position, 0);

            for _ in 0..BUFFER_HEIGHT * 2 {
                writer.write_string("x\n");
            }

            // Past the bottom the cursor pins to the last row and the screen
            // scrolls instead — otherwise this would write out of bounds.
            assert_eq!(writer.row_position, BUFFER_HEIGHT - 1);
        });
    }

    #[test_case]
    fn a_full_line_wraps_to_the_next_row() {
        x86_64::instructions::interrupts::without_interrupts(|| {
            let mut writer = WRITER.lock();
            writer.clear_screen();

            // Written byte by byte: the unit-test kernel never initialises a
            // heap, so nothing here may allocate.
            for _ in 0..BUFFER_WIDTH {
                writer.write_byte(b'a');
            }
            assert_eq!(writer.column_position, BUFFER_WIDTH);

            writer.write_byte(b'z');
            assert_eq!(writer.row_position, 1);
            assert_eq!(writer.column_position, 1);
        });
    }
}
