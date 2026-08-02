//! Serial output over the first UART (COM1).
//!
//! The VGA buffer is what a user sees; this is what the *host* sees. QEMU runs
//! with `-serial stdio`, so `serial_println!` lands in the terminal — that is
//! how the test harness reports results.

use core::fmt::{self, Write};
use spin::{Lazy, Mutex};
use uart_16550::backend::PioBackend;
use uart_16550::spec::registers::IER;
use uart_16550::{Config, Uart16550Tty};

const COM1: u16 = 0x3F8;

static SERIAL1: Lazy<Mutex<Uart16550Tty<PioBackend>>> = Lazy::new(|| {
    let config = Config {
        // The kernel only ever writes to COM1. Leaving the default DATA_READY
        // interrupt on would raise IRQ4 on host keystrokes, and there is no
        // handler for that vector.
        interrupts: IER::empty(),
        ..Config::DEFAULT
    };

    // SAFETY: COM1 is the architectural base port of the first UART, its eight
    // registers are reachable from it, and nothing else in the kernel drives it.
    let port = unsafe { Uart16550Tty::new_port(COM1, config) };
    Mutex::new(port.expect("COM1 initialisation failed"))
});

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    // An interrupt handler that printed while this lock was held would
    // deadlock, so mask interrupts for the duration.
    x86_64::instructions::interrupts::without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("printing to serial failed");
    });
}

/// Prints to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*))
    };
}

/// Prints to the host through the serial interface, with a trailing newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}
