use super::Task;
use alloc::collections::VecDeque;
use core::task::{Waker, RawWaker, RawWakerVTable, Context, Poll};

pub struct SimpleExecutor {
    task_queue: VecDeque<Task>,
}

impl SimpleExecutor {
    pub fn new() -> SimpleExecutor {
        SimpleExecutor { task_queue: VecDeque::new(), }
    }

    pub fn spawn(&mut self, task: Task){
        self.task_queue.push_back(task)
    }

    pub fn run(&mut self) {

        // for each task, create Context within a Waker instance from dummy waker
        // poll with the context, if ready continue, if pending add to the back of the queue
        while let Some(mut task) = self.task_queue.pop_front() {
            let waker = dummy_waker();
            let mut cx = Context::from_waker(&waker);
            match task.poll(&mut cx) {
                Poll::Ready(()) => {}
                Poll::Pending => self.task_queue.push_back(task),
            }
        }
    }
}

fn dummy_raw_waker() -> RawWaker {
    fn no_op(_: *const ()){} // no_op function takes a *const () pointer and does nothing
    fn clone(_: *const ()) -> RawWaker {// clone function also takes a *const () pointer and returns a new RawWaker by calling dummy_raw_waker
        dummy_raw_waker()
    }

    let vtable = &RawWakerVTable::new(clone, no_op, no_op, no_op);
    RawWaker::new(0 as *const (), vtable) // create new RawWaker
}

fn dummy_waker() -> Waker {
    unsafe { Waker::from_raw(dummy_raw_waker()) }
}
