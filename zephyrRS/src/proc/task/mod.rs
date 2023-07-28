use crate::{println, serial_println};
use alloc::boxed::Box;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll};
use core::{future::Future, pin::Pin};

pub mod executor;
pub mod simple_executor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TaskID(u64);

impl TaskID {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        TaskID(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

pub struct Task {
    id: TaskID,
    future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static) -> Task {
        Task {
            id: TaskID::new(),
            future: Box::pin(future),
        }
    }

    fn poll(&mut self, cx: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }
}

pub async fn return_number() -> u32 {
    42
}

pub async fn example_task() {
    let number = return_number().await;
    println!("asyncro number: {}", number);
}

pub async fn task_a() {
    let mut a: u32 = 0;
    let mut b: u8 = 0;
    loop {
        if a == 100_000_000 {
            println!("Process A running. {}% complete.", b);
            a = 0;
            b += 2;

            if b == 100 {
                println!("Process A complete.");
                break;
            }
        }
        a += 5;
    }
}

pub async fn task_b() {
    let mut a: u32 = 0;
    let mut b: u8 = 0;
    loop {
        if a == 100_000_000 {
            println!("Process B running. {}% complete.", b);
            a = 0;
            b += 2;

            if b == 100 {
                println!("Process B complete.");
                break;
            }
        }
        a += 5;
    }
}
