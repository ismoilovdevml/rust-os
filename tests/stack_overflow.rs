//! Verifies that a stack overflow is caught as a double fault instead of
//! triple-faulting and rebooting the machine.
//!
//! This is the test that proves the IST stack in `gdt.rs` is wired up. It needs
//! its own IDT because the real double-fault handler panics, whereas here a
//! double fault *is* the success condition.

#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

use core::panic::PanicInfo;
use rust_os::{QemuExitCode, exit_qemu, gdt, serial_print, serial_println};
use spin::Lazy;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

static TEST_IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    // SAFETY: the index refers to the IST slot initialised by `gdt::init`.
    unsafe {
        idt.double_fault
            .set_handler_fn(test_double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
    }
    idt
});

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    serial_print!("stack_overflow::stack_overflow...\t");

    gdt::init();
    TEST_IDT.load();

    stack_overflow();

    serial_println!("[failed] execution continued after stack overflow");
    exit_qemu(QemuExitCode::Failed);
    rust_os::hlt_loop();
}

/// Recurses until the guard page is hit. The volatile read after the call stops
/// the compiler from turning this into a tail call (which would loop forever
/// without ever growing the stack).
#[allow(unconditional_recursion)]
fn stack_overflow() {
    stack_overflow();
    core::hint::black_box(0);
}

extern "x86-interrupt" fn test_double_fault_handler(
    _stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    serial_println!("[ok]");
    exit_qemu(QemuExitCode::Success);
    rust_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rust_os::test_panic_handler(info)
}
