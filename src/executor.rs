use std::{
    future::Future,
    mem::ManuallyDrop,
    sync::{
        mpsc,
        mpsc::{Receiver, SyncSender},
        Arc, Mutex,
    },
    task::{Context, RawWaker, RawWakerVTable, Waker},
};

use futures::{future::BoxFuture, FutureExt};

#[derive(Debug)]
pub struct Executor {
    ready_queue: Receiver<Arc<Task>>,
}

impl Executor {
    pub fn run(&self) {
        while let Ok(task) = self.ready_queue.recv() {
            let mut future = task.future.lock().unwrap();
            let waker = task.clone().waker();
            let mut context = Context::from_waker(&waker);

            let _ = future.as_mut().poll(&mut context);
        }
    }
}

#[derive(Clone, Debug)]
pub struct Spawner {
    task_sender: SyncSender<Arc<Task>>,
}

impl Spawner {
    pub fn spawn(&self, future: impl Future<Output = ()> + Send + 'static) {
        let future = future.boxed();

        let task = Arc::new(Task {
            future: Mutex::new(future),
            spawner: self.clone(),
        });

        self.task_sender.send(task).expect("too many tasks queued");
    }

    pub(crate) fn spawn_task(&self, task: Arc<Task>) {
        self.task_sender.send(task).expect("too many tasks queued");
    }
}

pub(crate) struct Task {
    future: Mutex<BoxFuture<'static, ()>>,
    spawner: Spawner,
}

impl Task {
    const WAKER_VTABLE: RawWakerVTable = {
        fn clone(ptr: *const ()) -> RawWaker {
            let arc: ManuallyDrop<Arc<Task>> =
                ManuallyDrop::new(unsafe { Arc::from_raw(ptr as _) });

            // Increment the inner counter of the arc.
            let _ = ManuallyDrop::new(arc.clone());

            RawWaker::new(ptr, &Task::WAKER_VTABLE)
        }

        fn wake(ptr: *const ()) {
            let arc: Arc<Task> = unsafe { Arc::from_raw(ptr as _) };

            arc.spawner.spawn_task(arc.clone());
        }

        fn wake_by_ref(ptr: *const ()) {
            // Since we don't have ownership of the arc, we shouldn't decrement its inner
            // counter when we're done.
            let arc: ManuallyDrop<Arc<Task>> =
                ManuallyDrop::new(unsafe { Arc::from_raw(ptr as _) });

            arc.spawner
                .spawn_task(ManuallyDrop::into_inner(arc.clone()));
        }

        fn drop(ptr: *const ()) {
            let _: Arc<Task> = unsafe { Arc::from_raw(ptr as _) };
        }

        RawWakerVTable::new(clone, wake, wake_by_ref, drop)
    };

    pub fn waker(self: Arc<Self>) -> Waker {
        unsafe { Waker::from_raw(RawWaker::new(Arc::into_raw(self) as _, &Task::WAKER_VTABLE)) }
    }
}

pub fn new_executor_spawner() -> (Executor, Spawner) {
    const MAX_QUEUED_TASKS: usize = 10_000;

    let (task_sender, ready_queue) = mpsc::sync_channel(MAX_QUEUED_TASKS);

    (Executor { ready_queue }, Spawner { task_sender })
}
