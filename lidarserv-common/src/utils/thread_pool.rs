use crate::span;
use crossbeam_channel::Select;
use std::cell::UnsafeCell;
use std::pin::Pin;
use std::thread::JoinHandle;
use std::{mem, thread};
use tracy_client::set_thread_name;

/// A pool of multiple threads, that processes concurrent tasks.
pub struct Threads {
    threads: Vec<Thread>,
}

struct Thread {
    join_handle: JoinHandle<()>,
    tasks_sender: crossbeam_channel::Sender<Message>,
}

struct Message {
    task: TaskPtr,
    executor: unsafe fn(TaskPtr, usize) -> (),
}

struct Task<Func, Arg, Ret> {
    function: Func,
    args: Vec<UnsafeCell<Option<Arg>>>,
    done_senders: Vec<crossbeam_channel::Sender<Ret>>,
}

#[derive(Clone)]
struct TaskPtr(*const ());
unsafe impl Send for TaskPtr {}

#[must_use]
pub struct ExecuteJoinHandle<Data, Arg, Ret> {
    // Suppress "field is never read" warning.
    // This field is indeed never read, it is just here to control, when the Task is dropped.
    // The thread pool has a pointer to this task. Since the Drop implementation of ExecuteJoinHandle
    // synchronizes with the end of the task execution, this ensures, that that pointer is valid
    // until the task is done.
    #[allow(dead_code)]
    task: Pin<Box<Task<Data, Arg, Ret>>>,
    done_receivers: Vec<crossbeam_channel::Receiver<Ret>>,
}

pub struct ExecuteJoinIter<Data, Arg, Ret> {
    handle: ExecuteJoinHandle<Data, Arg, Ret>,
}

impl Threads {
    /// Creates a new thread pool with the given number of threads.
    pub fn new(nr_threads: usize) -> Self {
        Threads {
            threads: (0..nr_threads)
                .into_iter()
                .map(|thread_id| {
                    let (tasks_sender, tasks_receiver) = crossbeam_channel::bounded(1);
                    let join_handle =
                        thread::spawn(move || Self::thread(thread_id, nr_threads, tasks_receiver));
                    Thread {
                        join_handle,
                        tasks_sender,
                    }
                })
                .collect(),
        }
    }

    /// Executes a concurrent task.
    ///
    /// The given closure is passed into the thread pool and concurrently executed by all threads.
    /// Each thread will pass its thread id, a number between `0` and `num_threads - 1` to the closure.
    /// Additionally, an argument is passed to the closure, that is looked up in the args parameter by the thread id.
    ///
    /// Unlike vanilla std::sync::Thread, the closure does not need to be static. So, it can borrow
    /// variables from the stack of thee caller. All borrows will last until join is called on the
    /// returned [ExecuteJoinHandle].
    pub fn execute_with_args<Func, Arg, Ret>(
        &mut self,
        args: Vec<Arg>,
        func: Func,
    ) -> ExecuteJoinHandle<Func, Arg, Ret>
    where
        Func: Sync + Fn(usize, Arg) -> Ret,
        Arg: Send,
        Ret: Send,
    {
        // must provide an argument for each thread
        assert_eq!(args.len(), self.num_threads());

        // channels to send back the results
        // and synchronize on the completion of the task
        let mut done_senders = Vec::with_capacity(self.num_threads());
        let mut done_receivers = Vec::with_capacity(self.num_threads());
        for _ in 0..self.num_threads() {
            let (done_sender, done_receiver) = crossbeam_channel::bounded(1);
            done_senders.push(done_sender);
            done_receivers.push(done_receiver);
        }

        // create task
        let task: Pin<Box<Task<Func, Arg, Ret>>> = Box::pin(Task {
            function: func,
            args: args
                .into_iter()
                .map(|val| UnsafeCell::new(Some(val)))
                .collect(),
            done_senders,
        });

        // task ptr points to task,
        // but the generic types (Func and Arg) are removed.
        let task_ptr = task.to_task_pointer();

        // Function pointer to function, that can turn the task_ptr back to a Task<Func, Arg> and
        // execute the task.
        let executor = Task::<Func, Arg, Ret>::execute_pointer;

        // send task to each thread
        for thread in &self.threads {
            let task = task_ptr.clone();
            thread
                .tasks_sender
                .send(Message { task, executor })
                .unwrap();
        }

        ExecuteJoinHandle {
            task,
            done_receivers,
        }
    }

    /// Executes a concurrent task.
    ///
    /// The given closure is passed into the thread pool and concurrently executed by all threads.
    /// Each thread will pass its thread id, a number between `0` and `num_threads - 1` to the closure.
    ///
    /// Unlike vanilla std::sync::Thread, the closure does not need to be static. So, it can borrow
    /// variables from the stack of thee caller. All borrows will last until join is called on the
    /// returned [ExecuteJoinHandle].
    pub fn execute<Func, Ret>(
        &mut self,
        func: Func,
    ) -> ExecuteJoinHandle<impl Fn(usize, ()) -> Ret, (), Ret>
    where
        Func: Sync + Fn(usize) -> Ret,
        Ret: Send,
    {
        let nr_args = self.num_threads();
        let args = vec![(); nr_args];
        self.execute_with_args(args, move |thread_id, _arg| func(thread_id))
    }

    /// Returns the number of threads in the pool.
    pub fn num_threads(&self) -> usize {
        self.threads.len()
    }

    fn thread(thread_id: usize, nr_threads: usize, receiver: crossbeam_channel::Receiver<Message>) {
        let thread_name = format!("pool thread {} of {}", thread_id + 1, nr_threads);
        set_thread_name(&thread_name);
        for message in receiver.iter() {
            let Message { task, executor } = message;

            // unsafe: pointer is still valid since it is only invalidated once the task is completed.
            unsafe {
                executor(task, thread_id);
            }
        }
    }
}

impl Drop for Threads {
    fn drop(&mut self) {
        let threads = mem::take(&mut self.threads);
        for thread in threads {
            let Thread {
                join_handle,
                tasks_sender,
            } = thread;
            drop(tasks_sender); // closing the commands channel will make the thread stop.
            join_handle.join().unwrap();
        }
    }
}

impl<Func, Arg, Ret> Task<Func, Arg, Ret>
where
    Func: Fn(usize, Arg) -> Ret,
{
    pub fn execute(&self, thread_id: usize) {
        let argument = unsafe { self.args[thread_id].get().as_mut().unwrap().take().unwrap() };
        let s = span!("thread pool: execute");
        let result = (self.function)(thread_id, argument);
        drop(s);
        self.done_senders[thread_id].send(result).unwrap();
    }

    fn to_task_pointer(&self) -> TaskPtr {
        TaskPtr(self as *const Self as *const ())
    }

    /// Safety: pointer must be valid.
    unsafe fn execute_pointer(task: TaskPtr, thread_id: usize) {
        let ptr = task.0 as *const Self;
        let me = &*ptr; // unsafe: pointer is valid until task completes
        me.execute(thread_id);
    }
}

impl<Task, Arg, Ret> ExecuteJoinHandle<Task, Arg, Ret> {
    fn join_impl(&mut self) -> Vec<Ret> {
        mem::take(&mut self.done_receivers)
            .into_iter()
            .map(|receiver| receiver.recv().unwrap())
            .collect()
    }

    pub fn join(mut self) -> Vec<Ret> {
        self.join_impl()
    }

    pub fn into_iter(self) -> ExecuteJoinIter<Task, Arg, Ret> {
        ExecuteJoinIter { handle: self }
    }
}

impl<Task, Arg, Ret> Drop for ExecuteJoinHandle<Task, Arg, Ret> {
    fn drop(&mut self) {
        self.join_impl();
    }
}

impl<Task, Arg, Ret> Iterator for ExecuteJoinIter<Task, Arg, Ret> {
    type Item = Ret;

    fn next(&mut self) -> Option<Self::Item> {
        if self.handle.done_receivers.is_empty() {
            return None;
        }
        let mut sel = Select::new();
        for r in &self.handle.done_receivers {
            sel.recv(r);
        }
        let op = sel.select();
        let index = op.index();
        let result = op.recv(&self.handle.done_receivers[index]).unwrap();
        self.handle.done_receivers.swap_remove(index);
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::Threads;
    use std::sync::atomic::{AtomicI32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_pool() {
        let mut pool = Threads::new(4);
        let counter = Arc::new(AtomicI32::new(0));
        let counter2 = Arc::clone(&counter);
        pool.execute(move |_| {
            counter2.fetch_add(1, Ordering::AcqRel);
        })
        .join();
        assert_eq!(counter.load(Ordering::Acquire), 4);
    }

    #[test]
    fn test_thread_id() {
        let mut pool = Threads::new(4);
        let shared = Arc::new(vec![
            AtomicI32::new(0),
            AtomicI32::new(0),
            AtomicI32::new(0),
            AtomicI32::new(0),
        ]);
        let shared2 = Arc::clone(&shared);
        pool.execute(move |thread_id| {
            shared2[thread_id].fetch_add(1, Ordering::AcqRel);
        })
        .join();
        assert_eq!(shared[0].load(Ordering::Acquire), 1);
        assert_eq!(shared[1].load(Ordering::Acquire), 1);
        assert_eq!(shared[2].load(Ordering::Acquire), 1);
        assert_eq!(shared[3].load(Ordering::Acquire), 1);
    }

    #[test]
    fn test_lifetime() {
        let mut pool = Threads::new(4);
        let counter = AtomicI32::new(0);

        // the closure borrows `counter`
        let join_handle = pool.execute(|_id| {
            counter.fetch_add(1, Ordering::AcqRel);
        });

        // the borrow ends, when `join` is called.
        join_handle.join();

        // after the call to join, we are allowed to drop or mutate the borrowed variables.
        // if we dropped it before the `join`, this would not compile.
        // (todo: Is it possible to test, that something does not compile? Maybe via some
        //   procedural macro?)
        drop(counter);
    }

    #[test]
    fn test_args() {
        let mut pool = Threads::new(4);
        let sum = AtomicI32::new(0);

        let args = vec![1, 2, 4, 8];
        pool.execute_with_args(args, |_, arg| {
            sum.fetch_add(arg, Ordering::AcqRel);
        })
        .join();

        let sum = sum.into_inner();
        assert_eq!(sum, 15);
    }

    #[test]
    fn test_return_values() {
        let mut pool = Threads::new(4);
        let results = pool.execute(|thread_id| thread_id * 2).join();
        assert_eq!(results, vec![0, 2, 4, 6]);
    }
}
