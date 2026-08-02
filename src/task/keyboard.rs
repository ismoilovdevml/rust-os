//! Keyboard input as an async stream.
//!
//! The interrupt handler does the minimum — push a raw scancode into a
//! lock-free queue and wake the reader. Everything expensive (scancode-set
//! decoding, layout mapping, printing) happens in a normal async task where
//! allocating and taking locks is safe.

use crate::{print, println};
use conquer_once::spin::OnceCell;
use core::pin::Pin;
use core::task::{Context, Poll};
use crossbeam_queue::ArrayQueue;
use futures_util::stream::{Stream, StreamExt};
use futures_util::task::AtomicWaker;

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

/// Called from the keyboard interrupt handler. Must not allocate, lock, or
/// print — hence the silent drop when the queue is full or not yet built.
pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get()
        && queue.push(scancode).is_ok()
    {
        WAKER.wake();
    }
}

/// Stream of raw scancodes. Only one may exist at a time.
pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    /// # Panics
    ///
    /// If called more than once — two streams would steal each other's
    /// scancodes and clobber the shared waker.
    pub fn new() -> Self {
        SCANCODE_QUEUE
            .try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new may only be called once");
        Self { _private: () }
    }
}

impl Default for ScancodeStream {
    fn default() -> Self {
        Self::new()
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE
            .try_get()
            .expect("scancode queue not initialised");

        // Fast path: avoid registering a waker when data is already waiting.
        if let Some(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(cx.waker());
        match queue.pop() {
            // A scancode arrived between the check above and the registration.
            Some(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            None => Poll::Pending,
        }
    }
}

/// Decodes scancodes into characters and echoes them to the screen.
pub async fn print_keypresses() {
    use pc_keyboard::{DecodedKey, HandleControl, PS2Keyboard, ScancodeSet1, layouts};

    let mut scancodes = ScancodeStream::new();
    let mut keyboard = PS2Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore,
    );

    while let Some(scancode) = scancodes.next().await {
        let Ok(Some(key_event)) = keyboard.add_byte(scancode) else {
            continue;
        };
        // `RawKey` covers modifiers, function and arrow keys. None of them have
        // a character to echo, and printing their debug name would splice
        // "LShift" into the middle of whatever the user is typing.
        if let Some(DecodedKey::Unicode(character)) = keyboard.process_keyevent(key_event) {
            print!("{character}");
        }
    }

    println!("keyboard stream ended");
}
