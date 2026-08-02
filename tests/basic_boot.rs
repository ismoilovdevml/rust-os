//! Verifies the kernel is usable *before* `init()` runs.
//!
//! `main.rs` sets everything up first, so a bug that made `println!` depend on
//! the GDT or the heap would go unnoticed there. This test boots bare.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rust_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

use core::panic::PanicInfo;
use rust_os::println;

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    test_main();
    rust_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rust_os::test_panic_handler(info)
}

#[test_case]
fn println_works_before_init() {
    println!("println output from an uninitialised kernel");
}

#[test_case]
fn println_survives_a_full_screen_of_scrolling() {
    for i in 0..100 {
        println!("line {i}");
    }
}
