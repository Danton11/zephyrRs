use crate::serial_println;

use super::{Task, TaskID, TaskState};
use alloc::task::Wake;
use alloc::{collections::BTreeMap, sync::Arc};
use core::task::{Context, Poll, Waker};
use crossbeam_queue::ArrayQueue;
use spin::Mutex;
use alloc::vec::Vec;

pub struct Executor {
    tasks: Arc<Mutex<BTreeMap<TaskID, Arc<Mutex<Task>>>>>,
    task_queues: Arc<Vec<Arc<ArrayQueue<TaskID>>>>,
    waker_cache: BTreeMap<TaskID, Waker>,
    total_tasks: u64,
    task_per_priority: [u64; 6],
}

pub struct Spawner {
    tasks: Arc<Mutex<BTreeMap<TaskID, Arc<Mutex<Task>>>>>,
    task_queues: Arc<Vec<Arc<ArrayQueue<TaskID>>>>,
}

// The idea is that the wakers push the ID of the woken task to the queue. The executor sits on the receiving end of the queue, retrieves the woken tasks by their ID from the tasks map, and then runs them.

impl Executor {
    pub fn new() -> (Self, Spawner) {
        let mut task_queues = Vec::new();
        for _ in 0..10{
            task_queues.push(Arc::new(ArrayQueue::new(100)));
        }

        let tasks = Arc::new(Mutex::new(BTreeMap::new()));
        let task_queues = Arc::new(task_queues);

        serial_println!("Initialised Executor...");
        (
            Executor {
                tasks: Arc::clone(&tasks),
                task_queues: Arc::clone(&task_queues),
                waker_cache: BTreeMap::new(),
                total_tasks: 0,
                task_per_priority: [0,0,0,0,0,0],
            },
            Spawner {
                tasks: tasks,
                task_queues: Arc::clone(&task_queues),
            },
        )

    }

    pub fn spawn(&mut self, task: Task) {
        let taskid = task.id;
        let priority = task.priority.value();
        let taskToSpawn = Arc::new(Mutex::new(task));
        if self.tasks.lock().insert(taskid, Arc::clone(&taskToSpawn)).is_some() {
            panic!("duplicate task id in queue");
        }
        if self.task_queues[priority as usize].push(taskid).is_err() {
            panic!("queue full");
        
        }

        self.total_tasks += 1;
        self.task_per_priority[priority as usize] += 1;
    }

    fn run_tasks(&mut self) {
        let Self {
            tasks,
            task_queues,
            waker_cache,
            ..
        } = self;

        for task_queue in task_queues.iter().rev() {
            while let Ok(task_id) = task_queue.pop() {
                let mut tasks_locked = tasks.lock();
                let task_status = if let Some(task) = tasks_locked.get_mut(&task_id) {
                    let priority = task.lock().priority.value();
                    let waker = waker_cache
                        .entry(task_id)
                        .or_insert_with(|| TaskWaker::new(task_id, &task_queues, priority));
                    let mut context = Context::from_waker(waker);
                    let task_status = task.lock().poll(&mut context);
                    task_status
                } else {
                    continue; // task not found, so continue to the next iteration
                };

                match task_status {
                    Poll::Ready(()) => {
                        tasks_locked.remove(&task_id);
                        waker_cache.remove(&task_id);
                    }
                    Poll::Pending => {}
                }
            }
        }
    }

        // Method to get the total number of tasks
    pub fn total_tasks(&self) -> u64 {
        self.total_tasks
    }

    // Method to get the number of tasks for a given priority level
    pub fn num_tasks_per_priority(&self, priority: usize) -> u64 {
        self.task_per_priority[priority as usize]
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
        if self.task_queues.iter().all(|queue| queue.is_empty()) {
            enable_and_hlt();
        } else {
            interrupts::enable();
        }
    }

// Pauses the task with the given ID.
    // This method should be safe to call even if the task is already paused or has been killed.
    pub fn pause(&mut self, id: TaskID) -> Result<(), &'static str> {
        let mut tasks = self.tasks.lock();
        if let Some(task) = tasks.get_mut(&id) {
            task.lock().state = TaskState::Paused;
            Ok(())
        } else {
            Err("Task not found")
        }
    }

    // Resumes the task with the given ID.
    // This method should be safe to call even if the task is not paused or has been killed.
    pub fn resume(&mut self, id: TaskID) -> Result<(), &'static str> {
        let mut tasks = self.tasks.lock();
        if let Some(task) = tasks.get_mut(&id) {
            task.lock().state = TaskState::Running;
            Ok(())
        } else {
            Err("Task not found")
        }
    }

    // Kills the task with the given ID.
    // This method should be safe to call even if the task has already been killed.
    pub fn kill(&mut self, id: TaskID) -> Result<(), &'static str> {
        let mut tasks = self.tasks.lock();
        if tasks.remove(&id).is_some() {
            Ok(())
        } else {
            Err("Task not found")
        }
    }

    //
    pub fn kill_all(&mut self) {
        self.tasks.lock().clear();
    }
}

impl Spawner {
    pub fn spawn(&self, task: Task) {
        let taskid = task.id;
        let priority = task.priority.value();
        let task = Arc::new(Mutex::new(task));

        // Insert the task into the task map. If a task with the same ID already exists, panic.
        if self.tasks.lock().insert(taskid, Arc::clone(&task)).is_some() {
            panic!("duplicate task id in queue");
        }

        // Add the task's ID to the task queue
        self.task_queues[priority as usize].push(taskid).expect("queue full");
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

    fn new(task_id: TaskID, task_queues: &Vec<Arc<ArrayQueue<TaskID>>>, priority: u8) -> Waker {
        let task_queue = Arc::clone(&task_queues[priority as usize]);
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
