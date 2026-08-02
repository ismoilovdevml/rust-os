//! Exercises the kernel heap: basic allocation, growth, and reuse.

#![no_std]
#![no_main]
#![feature(custom_test_frameworks)]
#![test_runner(rust_os::test_runner)]
#![reexport_test_harness_main = "test_main"]

extern crate alloc;

use alloc::boxed::Box;
use alloc::vec::Vec;
use bootloader::{BootInfo, entry_point};
use core::panic::PanicInfo;
use rust_os::allocator::{self, HEAP_SIZE};
use rust_os::memory::{self, BootInfoFrameAllocator};
use x86_64::VirtAddr;

entry_point!(main);

fn main(boot_info: &'static BootInfo) -> ! {
    rust_os::init();

    // SAFETY: as in `main.rs` — the offset is the bootloader's, and this runs
    // exactly once.
    let mut mapper = unsafe { memory::init(VirtAddr::new(boot_info.physical_memory_offset)) };
    let mut frame_allocator = unsafe { BootInfoFrameAllocator::init(&boot_info.memory_map) };
    allocator::init_heap(&mut mapper, &mut frame_allocator).expect("heap initialisation failed");

    test_main();
    rust_os::hlt_loop();
}

#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    rust_os::test_panic_handler(info)
}

#[test_case]
fn simple_allocation() {
    let a = Box::new(41);
    let b = Box::new(13);
    assert_eq!(*a, 41);
    assert_eq!(*b, 13);
}

#[test_case]
fn large_vec_reallocates_correctly() {
    let n = 1000;
    let v: Vec<u64> = (0..n).collect();
    assert_eq!(v.iter().sum::<u64>(), (n - 1) * n / 2);
}

#[test_case]
fn many_boxes_reuse_freed_memory() {
    // Allocating far more than the heap holds only works if each `Box` is
    // actually returned to the free list when it drops.
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
}

#[test_case]
fn many_boxes_with_a_long_lived_allocation() {
    // The same, but with a live allocation the whole time — this catches an
    // allocator that only ever reuses the most recently freed block.
    let long_lived = Box::new(1);
    for i in 0..HEAP_SIZE {
        let x = Box::new(i);
        assert_eq!(*x, i);
    }
    assert_eq!(*long_lived, 1);
}
