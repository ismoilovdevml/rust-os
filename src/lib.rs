//! `rust_os` — a small x86_64 kernel.
//!
//! The crate is a library so that the integration tests under `tests/` boot the
//! exact same code `main.rs` boots. Everything here runs in ring 0 with no
//! operating system underneath it.

#![no_std]
#![cfg_attr(test, no_main)]
#![feature(abi_x86_interrupt)]
#![feature(custom_test_frameworks)]
#![test_runner(crate::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

pub mod allocator;
pub mod gdt;
pub mod interrupts;
pub mod memory;
pub mod serial;
pub mod task;
pub mod vga_buffer;

use core::panic::PanicInfo;

/// Brings up the CPU facilities every other subsystem depends on: segmentation,
/// the interrupt table, and the two chained PICs.
///
/// Memory management is deliberately *not* started here — it needs the
/// bootloader's memory map, which only the binary entry point receives.
pub fn init() {
    gdt::init();
    interrupts::init_idt();
    // SAFETY: called once, before interrupts are enabled, with the PIC offsets
    // chosen in `interrupts` so they do not collide with CPU exceptions.
    unsafe { interrupts::PICS.lock().initialize() };
    x86_64::instructions::interrupts::enable();
}

/// Parks the CPU until the next interrupt instead of spinning at 100%.
pub fn hlt_loop() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}

// ---------------------------------------------------------------------------
// Test harness
// ---------------------------------------------------------------------------

/// Anything runnable by [`test_runner`]. Blanket-implemented for every `Fn()`
/// so each test reports its own name without extra boilerplate.
pub trait Testable {
    fn run(&self);
}

impl<T: Fn()> Testable for T {
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

pub fn test_runner(tests: &[&dyn Testable]) {
    serial_println!("Running {} tests", tests.len());
    for test in tests {
        test.run();
    }
    exit_qemu(QemuExitCode::Success);
}

pub fn test_panic_handler(info: &PanicInfo) -> ! {
    serial_println!("[failed]\n");
    serial_println!("Error: {info}\n");
    exit_qemu(QemuExitCode::Failed);
    hlt_loop();
}

/// Status handed to QEMU's `isa-debug-exit` device. QEMU reports
/// `(code << 1) | 1`, so `Success` reaches the shell as exit code 33.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

/// Shuts the emulator down. Returns only if the debug-exit device is absent.
pub fn exit_qemu(exit_code: QemuExitCode) {
    use x86_64::instructions::port::Port;

    // SAFETY: 0xf4 is the iobase configured for `isa-debug-exit` in Cargo.toml.
    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }
}

/// Entry point used when the library itself is under test.
#[cfg(test)]
#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    init();
    test_main();
    hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    test_panic_handler(info)
}
