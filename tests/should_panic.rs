//! Verifies that a failing assertion actually panics.
//!
//! A panic is terminal in a kernel, so this cannot share a binary with other
//! tests — hence `harness = false` in Cargo.toml and the hand-rolled `_start`.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use rust_os::{QemuExitCode, exit_qemu, serial_print, serial_println};

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    should_fail();

    // Only reached if `should_fail` did not panic, which is the failure case.
    serial_println!("[test did not panic]");
    exit_qemu(QemuExitCode::Failed);
    rust_os::hlt_loop();
}

fn should_fail() {
    serial_print!("should_panic::should_fail...\t");
    assert_eq!(0, 1);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    serial_println!("[ok]");
    exit_qemu(QemuExitCode::Success);
    rust_os::hlt_loop();
}
