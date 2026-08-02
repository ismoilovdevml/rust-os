//! Kernel heap.
//!
//! [`init_heap`] maps a fixed virtual range to freshly allocated frames and
//! hands it to a linked-list allocator, which is what makes `Box`, `Vec` and
//! the async executor usable from here on.

use linked_list_allocator::LockedHeap;
use x86_64::VirtAddr;
use x86_64::structures::paging::mapper::MapToError;
use x86_64::structures::paging::{FrameAllocator, Mapper, Page, PageTableFlags, Size4KiB};

/// Start of the kernel heap. An arbitrary address in unused, non-canonical-safe
/// virtual space — recognisable in a fault dump, and far from anything mapped.
pub const HEAP_START: usize = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 1024 * 1024; // 1 MiB

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Maps [`HEAP_START`]`..`[`HEAP_START`]`+`[`HEAP_SIZE`] and initialises the
/// global allocator over it. Must run before the first allocation.
pub fn init_heap(
    mapper: &mut impl Mapper<Size4KiB>,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
) -> Result<(), MapToError<Size4KiB>> {
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        Page::range_inclusive(
            Page::containing_address(heap_start),
            Page::containing_address(heap_end),
        )
    };

    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    for page in page_range {
        let frame = frame_allocator
            .allocate_frame()
            .ok_or(MapToError::FrameAllocationFailed)?;
        // SAFETY: the frame came from the bootloader's usable list and is not
        // mapped anywhere else, and `page` is in an otherwise unused range.
        unsafe { mapper.map_to(page, frame, flags, frame_allocator)?.flush() };
    }

    // SAFETY: the range was just mapped writable and is used by nothing else.
    unsafe { ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE) };

    Ok(())
}

/// Bytes currently handed out by the allocator.
pub fn used_bytes() -> usize {
    ALLOCATOR.lock().used()
}
