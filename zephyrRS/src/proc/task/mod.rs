use crate::{println, serial_println};
use alloc::boxed::Box;
use core::sync::atomic::{AtomicU64, Ordering};
use core::task::{Context, Poll};
use core::{future::Future, pin::Pin};

pub mod executor;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct TaskID(u64);

impl TaskID {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        TaskID(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct TaskPriority(u8);

impl TaskPriority {
    fn new(prio: u8) -> Self {
        TaskPriority(prio)
    }

    fn value(self) -> u8 {
        self.0
    }
    
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskState {
    Running,
    Paused,
}

pub struct Task {
    id: TaskID,
    priority: TaskPriority,
    state: TaskState,
    future: Pin<Box<dyn Future<Output = ()>>>,
}

impl Task {
    pub fn new(future: impl Future<Output = ()> + 'static, priority: u8) -> Task {
        Task {
            id: TaskID::new(),
            priority: TaskPriority::new(priority),
            state: TaskState::Paused,
            future: Box::pin(future),
        }
    }

    fn poll(&mut self, cx: &mut Context) -> Poll<()> {
        self.future.as_mut().poll(cx)
    }
}


pub async fn yield_now() {
    struct YieldNow {
        polled_once: bool,
    }

    impl Future for YieldNow {
        type Output = ();
 
        fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.polled_once {
                Poll::Ready(())
            } else {
                self.polled_once = true;
                cx.waker().wake_by_ref();
                Poll::Pending
            }
        }
    }

    YieldNow { polled_once: false }.await
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

pub async fn task_c() {
    let mut a: u32 = 0;
    let mut b: u8 = 0;
    loop {
        if a == 100_000_000 {
            println!("Process C running. {}% complete.", b);
            a = 0;
            b += 2;

            if b == 100 {
                println!("Process C complete.");
                break;
            }

        }
        a += 5;
    }
}

