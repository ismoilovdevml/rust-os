//! Global Descriptor Table and Task State Segment.
//!
//! Long mode barely uses segmentation, but the TSS is still the only way to
//! give a fault its own stack. Without that, a stack overflow would push the
//! double-fault frame onto the already-exhausted stack, escalate to a triple
//! fault, and reset the machine. The IST entry below is what turns an
//! unrecoverable reboot into a diagnosable panic.

use spin::Lazy;
use x86_64::VirtAddr;
use x86_64::instructions::segmentation::{CS, Segment};
use x86_64::instructions::tables::load_tss;
use x86_64::structures::gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector};
use x86_64::structures::tss::TaskStateSegment;

/// Interrupt Stack Table slot reserved for the double-fault handler.
pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

const STACK_SIZE: usize = 4096 * 5;

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        // No allocator exists this early, so the fault stack is a static. It is
        // never written through this reference — only its address is taken —
        // and the CPU is the sole user once the TSS is loaded.
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

        let stack_start = VirtAddr::from_ptr(&raw const STACK);
        // x86 stacks grow downwards, so the CPU wants the *end* address.
        stack_start + STACK_SIZE as u64
    };
    tss
});

struct Selectors {
    code_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code_selector = gdt.append(Descriptor::kernel_code_segment());
    let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors {
            code_selector,
            tss_selector,
        },
    )
});

/// Loads the GDT, then points `cs` and the task register at its new entries.
pub fn init() {
    GDT.0.load();
    // SAFETY: both selectors index descriptors in the GDT loaded on the line
    // above, and the code segment is a valid ring-0 kernel segment.
    unsafe {
        CS::set_reg(GDT.1.code_selector);
        load_tss(GDT.1.tss_selector);
    }
}
