//! Kernel entry point.
//!
//! The bootloader has already switched the CPU into 64-bit long mode and mapped
//! all of physical memory before calling [`kernel_main`]. Everything from there
//! on is ours.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rust_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::vec::Vec;
use bootloader::{BootInfo, entry_point};
use core::fmt::Arguments;
use core::panic::PanicInfo;
use rust_os::task::{Task, executor::Executor, keyboard};
use rust_os::vga_buffer::{Color, ColorCode, WRITER, print_colored};
use rust_os::{allocator, memory, print, println};
use x86_64::VirtAddr;

const TITLE: ColorCode = ColorCode::new(Color::LightCyan, Color::Black);
const OK: ColorCode = ColorCode::new(Color::LightGreen, Color::Black);
const DIM: ColorCode = ColorCode::new(Color::DarkGray, Color::Black);
const HINT: ColorCode = ColorCode::new(Color::Yellow, Color::Black);
const RULE: &str = "================================================================";

// Type-checks the entry signature against what the bootloader actually passes —
// something a bare `#[no_mangle] fn _start` cannot do.
entry_point!(kernel_main);

fn kernel_main(boot_info: &'static BootInfo) -> ! {
    rust_os::init();

    // SAFETY: the bootloader guarantees this offset maps all of physical
    // memory, and both initialisers run exactly once — here.
    let mut mapper = unsafe { memory::init(VirtAddr::new(boot_info.physical_memory_offset)) };
    let mut frame_allocator =
        unsafe { memory::BootInfoFrameAllocator::init(&boot_info.memory_map) };

    let usable_memory = frame_allocator.usable_memory_bytes();
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialisation failed");

    banner(boot_info.physical_memory_offset, usable_memory);

    #[cfg(test)]
    test_main();

    let mut executor = Executor::new();
    executor.spawn(Task::new(heap_report()));
    executor.spawn(Task::new(keyboard::print_keypresses()));
    executor.run();
}

/// Draws the boot report — and, incidentally, the first real exercise of the
/// VGA driver.
fn banner(physical_memory_offset: u64, usable_memory: u64) {
    x86_64::instructions::interrupts::without_interrupts(|| WRITER.lock().clear_screen());

    print_colored(
        TITLE,
        format_args!(
            "  rust_os v{}  --  an x86_64 kernel written in Rust\n",
            env!("CARGO_PKG_VERSION")
        ),
    );
    print_colored(DIM, format_args!("  {RULE}\n\n"));

    subsystem(
        "GDT + TSS",
        format_args!("double fault runs on its own IST stack"),
    );
    subsystem(
        "IDT",
        format_args!("breakpoint, page fault, GPF, double fault"),
    );
    subsystem(
        "PIC 8259",
        format_args!("timer + PS/2 keyboard IRQs enabled"),
    );
    subsystem(
        "Paging",
        format_args!("physical memory mapped at {physical_memory_offset:#x}"),
    );
    subsystem(
        "Heap",
        format_args!(
            "{} KiB at {:#x}",
            allocator::HEAP_SIZE / 1024,
            allocator::HEAP_START
        ),
    );
    subsystem("Executor", format_args!("cooperative async/await, 2 tasks"));

    println!();
    print!("  Usable RAM: ");
    print_colored(HINT, format_args!("{} MiB\n", usable_memory / 1024 / 1024));
    print_colored(
        HINT,
        format_args!("  Type something -- keystrokes echo below.\n\n"),
    );
}

fn subsystem(label: &str, detail: Arguments) {
    print_colored(OK, format_args!("  [ok] "));
    print!("{label:<11}");
    print_colored(DIM, format_args!("{detail}\n"));
}

/// Runs on the executor and proves the heap works end to end: allocate, grow,
/// drop, and report what the allocator thinks it still holds.
async fn heap_report() {
    let mut numbers: Vec<u64> = (0..1000).collect();
    numbers.reverse();
    let sum: u64 = numbers.iter().sum();
    let peak = allocator::used_bytes();
    drop(numbers);

    print_colored(
        DIM,
        format_args!(
            "  heap check: sum(0..1000) = {sum}, {peak} B at peak, {} B after drop\n\n",
            allocator::used_bytes()
        ),
    );
}

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{info}");
    rust_os::hlt_loop();
}

#[cfg(test)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rust_os::test_panic_handler(info)
}
