//! Paging and physical frame allocation.
//!
//! The bootloader maps all of physical memory at a fixed virtual offset before
//! handing over control, which is what makes [`OffsetPageTable`] usable: any
//! physical address `p` is readable at `p + physical_memory_offset`.

use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::structures::paging::{FrameAllocator, OffsetPageTable, PageTable, PhysFrame, Size4KiB};
use x86_64::{PhysAddr, VirtAddr};

/// Builds a page-table view of the address space the bootloader set up.
///
/// # Safety
///
/// `physical_memory_offset` must be the offset at which the bootloader mapped
/// the complete physical address space, and this must be called only once —
/// two live `OffsetPageTable`s would alias the same mutable page tables.
pub unsafe fn init(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    let level_4_table = unsafe { active_level_4_table(physical_memory_offset) };
    unsafe { OffsetPageTable::new(level_4_table, physical_memory_offset) }
}

/// # Safety
///
/// Same contract as [`init`]; the returned reference is unchecked-mutable and
/// must not be aliased.
unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();
    let virt = physical_memory_offset + level_4_table_frame.start_address().as_u64();

    unsafe { &mut *virt.as_mut_ptr() }
}

/// Hands out frames the bootloader marked `Usable`.
///
/// Deliberately bump-only: frames are never returned. The kernel allocates its
/// heap once at boot and then works out of that, so a free list would be
/// unexercised complexity.
pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize,
}

impl BootInfoFrameAllocator {
    /// # Safety
    ///
    /// `memory_map` must be valid, and every region it marks `Usable` must
    /// really be unused.
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        Self {
            memory_map,
            next: 0,
        }
    }

    /// Every 4 KiB frame in the usable regions, in ascending address order.
    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        self.memory_map
            .iter()
            .filter(|r| r.region_type == MemoryRegionType::Usable)
            .map(|r| r.range.start_addr()..r.range.end_addr())
            .flat_map(|r| r.step_by(4096))
            .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }

    /// Total usable physical memory in bytes, for the boot banner.
    pub fn usable_memory_bytes(&self) -> u64 {
        self.memory_map
            .iter()
            .filter(|r| r.region_type == MemoryRegionType::Usable)
            .map(|r| r.range.end_addr() - r.range.start_addr())
            .sum()
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}
