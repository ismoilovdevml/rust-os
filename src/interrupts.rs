//! Interrupt Descriptor Table, CPU exception handlers, and the 8259 PICs.

use crate::{gdt, hlt_loop, println};
use core::sync::atomic::{AtomicU64, Ordering};
use pic8259::ChainedPics;
use spin::{Lazy, Mutex};
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Vectors 0..32 belong to CPU exceptions, so the PICs are remapped above them.
/// Leaving the BIOS defaults in place would make a timer tick indistinguishable
/// from a double fault.
pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = PIC_1_OFFSET + 8;

/// SAFETY (of the eventual `initialize()`): the offsets above are free vectors.
pub static PICS: Mutex<ChainedPics> =
    Mutex::new(unsafe { ChainedPics::new(PIC_1_OFFSET, PIC_2_OFFSET) });

/// Hardware interrupts, numbered by their remapped vector.
#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard,
}

impl InterruptIndex {
    fn as_u8(self) -> u8 {
        self as u8
    }

    fn as_usize(self) -> usize {
        usize::from(self.as_u8())
    }
}

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();

    idt.breakpoint.set_handler_fn(breakpoint_handler);
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt.general_protection_fault
        .set_handler_fn(general_protection_fault_handler);

    // SAFETY: `DOUBLE_FAULT_IST_INDEX` is a valid, initialised IST slot in the
    // TSS loaded by `gdt::init`.
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
    }

    idt[InterruptIndex::Timer.as_u8()].set_handler_fn(timer_interrupt_handler);
    idt[InterruptIndex::Keyboard.as_u8()].set_handler_fn(keyboard_interrupt_handler);

    idt
});

pub fn init_idt() {
    IDT.load();
}

// ---------------------------------------------------------------------------
// CPU exceptions
// ---------------------------------------------------------------------------

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    println!("EXCEPTION: BREAKPOINT\n{stack_frame:#?}");
}

/// Raised when a fault handler itself faults. There is no recovery path — the
/// only alternative to halting is a triple fault, which resets the machine.
extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    _error_code: u64,
) -> ! {
    panic!("EXCEPTION: DOUBLE FAULT\n{stack_frame:#?}");
}

extern "x86-interrupt" fn general_protection_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) {
    panic!("EXCEPTION: GENERAL PROTECTION FAULT ({error_code:#x})\n{stack_frame:#?}");
}

extern "x86-interrupt" fn page_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {error_code:?}");
    println!("{stack_frame:#?}");
    hlt_loop();
}

// ---------------------------------------------------------------------------
// Hardware interrupts
// ---------------------------------------------------------------------------

/// Fires on every PIT tick (~18.2 Hz by default). Counting rather than printing
/// keeps the handler allocation-free and lock-free, and leaves the screen usable.
extern "x86-interrupt" fn timer_interrupt_handler(_stack_frame: InterruptStackFrame) {
    TICKS.fetch_add(1, Ordering::Relaxed);
    notify_end_of_interrupt(InterruptIndex::Timer);
}

/// Timer ticks since boot — the kernel's only clock.
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Reads one scancode from the PS/2 controller and hands it to the async
/// keyboard task. Decoding happens outside the handler so this stays short.
extern "x86-interrupt" fn keyboard_interrupt_handler(_stack_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;

    let mut port: Port<u8> = Port::new(0x60);
    // SAFETY: 0x60 is the PS/2 data port; reading it also clears the condition
    // that raised this interrupt.
    let scancode: u8 = unsafe { port.read() };

    crate::task::keyboard::add_scancode(scancode);
    notify_end_of_interrupt(InterruptIndex::Keyboard);
}

/// Acknowledges the interrupt so the PIC will deliver the next one.
fn notify_end_of_interrupt(index: InterruptIndex) {
    // SAFETY: `index` is one of the vectors registered in the IDT above, so the
    // PIC is not sent an EOI for a line it never raised.
    unsafe {
        PICS.lock().notify_end_of_interrupt(index.as_usize() as u8);
    }
}

#[cfg(test)]
mod tests {
    #[test_case]
    fn breakpoint_exception_is_recoverable() {
        // If the handler were missing, this would escalate to a double fault
        // and the test binary would never reach the next line.
        x86_64::instructions::interrupts::int3();
    }
}
