#![no_std] 
#![no_main]

use core::panic::PanicInfo;

mod vga_buffer;

static SALOM: &[u8] = b"Hello Rust";

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let vga_buffer = 0xb800 as *mut u8;
    for (i, &byte) in SALOM.iter().enumerate()  {
        unsafe{
            *vga_buffer.offset(i as isize *2) = byte;
            *vga_buffer.offset(i as isize * 2+1) = 0xb;
        }        
    }
    loop {}
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    loop {}
}