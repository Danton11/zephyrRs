use super::{Task, TaskID};
use alloc::{collections::BTreeMap, sync::Arc};
use core::task::{Waker,Context, Poll};
use crossbeam_queue::ArrayQueue;
use alloc::task::Wake;

pub struct Executor {
    tasks: BTreeMap<TaskID, Task>, // tree map of taskids and tasks
    task_queue: Arc<ArrayQueue<TaskID>>, // queue of taskids
    waker_cache: BTreeMap<TaskID, Waker>, // 
}

// The idea is that the wakers push the ID of the woken task to the queue. The executor sits on the receiving end of the queue, retrieves the woken tasks by their ID from the tasks map, and then runs them. 

impl Executor {
    pub fn new() -> Self {
        Executor { 
            tasks: BTreeMap::new() ,
            task_queue: Arc::new(ArrayQueue::new(200)), // queue of task ids 
            waker_cache: BTreeMap::new(),
        }
    }

    pub fn spawn(&mut self, task:Task){
        let taskid = task.id;
        if self.tasks.insert(task.id,task).is_some() { // btreemap will return a value if you try insert it twice
            panic!("duplicate task id in queue");
        }
        self.task_queue.push(taskid).expect("queue full");
    }
    // The basic idea of this function is similar to our SimpleExecutor: Loop over all tasks in the task_queue, create a waker for each task, and then poll them. However, instead of adding pending tasks back to the end of the task_queue, we let our TaskWaker implementation take care of adding woken tasks back to the queue. The implementation of this waker type will be shown in a moment.
    fn run_tasks(&mut self) {
        let Self {
            tasks,
            task_queue,
            waker_cache,
        } = self;

        while let Ok(task_id) = task_queue.pop() {
            let task = match tasks.get_mut(&task_id) {
                Some(task) => task,
                None => continue, // task finished
            };

            let waker = waker_cache.entry(task_id).or_insert_with(|| TaskWaker::new(task_id, task_queue.clone()));
            let mut context = Context::from_waker(waker);
            match task.poll(&mut context) {
                Poll::Ready(()) => {
                    tasks.remove(&task_id);
                    waker_cache.remove(&task_id);
                }
                Poll::Pending => {}
            }
        }
    }

    // We use destructuring to split self into its three fields to avoid some borrow checker errors. Namely, our implementation needs to access the self.task_queue from within a closure, which currently tries to borrow self completely. This is a fundamental borrow checker issue that will be resolved when RFC 2229 is implemented.

    //For each popped task ID, we retrieve a mutable reference to the corresponding task from the tasks map. Since our ScancodeStream implementation registers wakers before checking whether a task needs to be put to sleep, it might happen that a wake-up occurs for a task that no longer exists. In this case, we simply ignore the wake-up and continue with the next ID from the queue.

    //To avoid the performance overhead of creating a waker on each poll, we use the waker_cache map to store the waker for each task after it has been created. For this, we use the BTreeMap::entry method in combination with Entry::or_insert_with to create a new waker if it doesn’t exist yet and then get a mutable reference to it. For creating a new waker, we clone the task_queue and pass it together with the task ID to the TaskWaker::new function (implementation shown below). Since the task_queue is wrapped into an Arc, the clone only increases the reference count of the value, but still points to the same heap-allocated queue. Note that reusing wakers like this is not possible for all waker implementations, but our TaskWaker type will allow it.
    
    pub fn run(&mut self) -> ! {
        loop {
            self.run_tasks();
            self.sleep_if_idle();
        }
    }

    fn sleep_if_idle(&self) {
        use x86_64::instructions::interrupts::{self, enable_and_hlt};

        interrupts::disable();
        if self.task_queue.is_empty() {
            enable_and_hlt();
        }else {
            interrupts::enable();
        }

        
    }

}

struct TaskWaker {
    task_id: TaskID,
    task_queue: Arc<ArrayQueue<TaskID>>,
}

impl TaskWaker {
    fn wake_task(&self) {
        self.task_queue.push(self.task_id).expect("task_queue full");
    }

    fn new(task_id: TaskID, task_queue: Arc<ArrayQueue<TaskID>>) -> Waker {
        Waker::from(Arc::new(TaskWaker {
            task_id,
            task_queue,
        }))
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
