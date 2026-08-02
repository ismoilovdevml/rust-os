//! A single-threaded, wake-driven executor.
//!
//! Tasks are polled only when something wakes them. When the ready queue is
//! empty the CPU is halted until the next interrupt, so an idle kernel costs
//! nothing — the naive alternative is a loop that polls every task forever.

use super::{Task, TaskId};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::task::Wake;
use core::task::{Context, Poll, Waker};
use crossbeam_queue::ArrayQueue;

pub struct Executor {
    tasks: BTreeMap<TaskId, Task>,
    /// Ids of tasks ready to be polled. Shared with every [`TaskWaker`], which
    /// is why it is an `Arc` and lock-free — wakers are pushed from interrupt
    /// handlers, where blocking on a mutex would deadlock.
    ready_queue: Arc<ArrayQueue<TaskId>>,
    /// Cached wakers, so re-waking a task does not reallocate.
    waker_cache: BTreeMap<TaskId, Waker>,
}

impl Executor {
    pub fn new() -> Self {
        Self {
            tasks: BTreeMap::new(),
            ready_queue: Arc::new(ArrayQueue::new(100)),
            waker_cache: BTreeMap::new(),
        }
    }

    /// Adds a task and marks it ready for its first poll.
    ///
    /// # Panics
    ///
    /// If the same [`TaskId`] is spawned twice, or the ready queue is full.
    pub fn spawn(&mut self, task: Task) {
        let task_id = task.id;
        if self.tasks.insert(task_id, task).is_some() {
            panic!("task with id {task_id:?} already spawned");
        }
        self.ready_queue.push(task_id).expect("ready queue full");
    }

    /// Runs until the machine is powered off. Never returns.
    pub fn run(&mut self) -> ! {
        loop {
            self.run_ready_tasks();
            self.sleep_if_idle();
        }
    }

    fn run_ready_tasks(&mut self) {
        // Destructured up front so the borrow checker sees disjoint fields
        // rather than a whole `&mut self` held across the loop.
        let Self {
            tasks,
            ready_queue,
            waker_cache,
        } = self;

        while let Some(task_id) = ready_queue.pop() {
            // The task may have finished after being queued.
            let Some(task) = tasks.get_mut(&task_id) else {
                continue;
            };
            let waker = waker_cache
                .entry(task_id)
                .or_insert_with(|| TaskWaker::waker(task_id, ready_queue.clone()));

            if task.poll(&mut Context::from_waker(waker)) == Poll::Ready(()) {
                tasks.remove(&task_id);
                waker_cache.remove(&task_id);
            }
        }
    }

    /// Halts until the next interrupt, unless a task was woken in the window
    /// between the last poll and here.
    fn sleep_if_idle(&self) {
        use x86_64::instructions::interrupts;

        // Disable interrupts before the check so that a wake arriving between
        // the check and the `hlt` cannot be missed — `enable_and_hlt` makes the
        // two steps atomic, which is exactly the race this guards against.
        interrupts::disable();
        if self.ready_queue.is_empty() {
            interrupts::enable_and_hlt();
        } else {
            interrupts::enable();
        }
    }
}

impl Default for Executor {
    fn default() -> Self {
        Self::new()
    }
}

/// Pushes its task back onto the ready queue when woken.
struct TaskWaker {
    task_id: TaskId,
    ready_queue: Arc<ArrayQueue<TaskId>>,
}

impl TaskWaker {
    /// Builds a type-erased [`Waker`] for `task_id`.
    fn waker(task_id: TaskId, ready_queue: Arc<ArrayQueue<TaskId>>) -> Waker {
        Waker::from(Arc::new(Self {
            task_id,
            ready_queue,
        }))
    }

    fn wake_task(&self) {
        self.ready_queue
            .push(self.task_id)
            .expect("ready queue full");
    }
}

impl Wake for TaskWaker {
    fn wake(self: Arc<Self>) {
        self.wake_task();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.wake_task();
    }
}
