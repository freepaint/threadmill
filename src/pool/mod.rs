use crate::task::Task;
use flume::TryRecvError;
use std::time::Duration;
use until::UntilExt;

pub struct ThreadPool {
	scheduler: Scheduler,
}

#[derive(Clone, Default)]
pub struct Scheduler {
	/// Despite its name, priority only gets a single thread with the single goal of depleting this queue forever
	priority: SchedulerQueue,
	/// This queue processes all tasks single threaded with regular priority
	regular: SchedulerQueue,
	/// This queue processes all heavy work loads with n threads, n = num of cpus
	work: SchedulerQueue,
}

#[derive(Clone)]
struct SchedulerQueue {
	scheduler: flume::Sender<Box<dyn Task>>,
	queue: flume::Receiver<Box<dyn Task>>,
}

impl Default for SchedulerQueue {
	fn default() -> Self {
		let (tx, rx) = flume::unbounded();
		Self {
			scheduler: tx,
			queue: rx,
		}
	}
}

impl ThreadPool {
	pub fn new() -> Self {
		Self::new_max(num_cpus::get())
	}

	pub fn new_max(thread_count: usize) -> Self {
		let scheduler = Scheduler::default();

		// Spawn work threads
		let mut handles = (0..thread_count)
			.map(|_| std::thread::spawn(gen_executor(scheduler.work.clone())))
			.collect::<Vec<_>>();

		// Spawn thread for regular queue
		handles.push(std::thread::spawn(gen_executor(scheduler.regular.clone())));

		// Spawn thread for priority queue
		handles.push(std::thread::spawn(gen_executor(scheduler.priority.clone())));

		Self { scheduler }
	}
}

// This got its own function for readability
fn gen_executor(queue: SchedulerQueue) -> impl FnOnce() {
	move || loop {
		for mut task in queue.queue.iter().do_for(Duration::from_millis(100)) {
			let (tx, rx) = flume::bounded(0);
			task.exec(Box::new(move || {
				tx.send(()).expect("reschedule channel may not be closed")
			}));
			match rx.try_recv() {
				Ok(_) => drop(queue.scheduler.send(task)),
				Err(TryRecvError::Disconnected) => (),
				Err(TryRecvError::Empty) => {
					todo!() // TODO: reschedule this for waiting async
				}
			}
		}
	}
}
