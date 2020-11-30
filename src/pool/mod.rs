use crate::task::{Task, TaskState};
use std::time::Duration;
use until::UntilExt;

mod executor;

pub struct ThreadPool {}

#[derive(Clone)]
struct Scheduler {
	/// Despite its name, priority only gets a single thread with the single goal of depleting this queue forever
	priority: (flume::Sender<Box<dyn Task>>, flume::Receiver<Box<dyn Task>>),
	/// This queue processes all tasks single threaded with regular priority
	regular: (flume::Sender<Box<dyn Task>>, flume::Receiver<Box<dyn Task>>),
	/// This queue processes all heavy work loads with n threads, n = num of cpus
	work: (flume::Sender<Box<dyn Task>>, flume::Receiver<Box<dyn Task>>),
}

impl ThreadPool {
	pub fn new() -> Self {
		Self::new_max(num_cpus::get())
	}

	pub fn new_max(thread_count: usize) -> Self {
		let scheduler = Scheduler {
			priority: flume::unbounded(),
			regular: flume::unbounded(),
			work: flume::unbounded(),
		};

		// Spawn work threads
		let mut handles = (0..thread_count)
			.map(|_| std::thread::spawn(gen_executor(scheduler.work.1.clone(), scheduler.work.0.clone())))
			.collect::<Vec<_>>();

		// Spawn thread for regular queue
		handles.push(std::thread::spawn(gen_executor(scheduler.regular.1.clone(), scheduler.regular.0.clone())));

		// Spawn thread for priority queue
		handles.push(std::thread::spawn(gen_executor(scheduler.priority.1.clone(), scheduler.priority.0.clone())));

		Self {}
	}
}

// This got its own function for readability
fn gen_executor(queue: flume::Receiver<Box<dyn Task>>, scheduler: flume::Sender<Box<dyn Task>>) -> impl FnOnce() {
	move || loop {
		for mut task in queue.try_iter().do_for(Duration::from_millis(100)) {
			match task.exec() {
				// Reschedule task for next execution
				TaskState::Reschedule => if let Err(_) = scheduler.send(task) {
					// TODO: Log event
					return; // Assuming queue is closed meaning the scheduler has shutdown
				},
				TaskState::Done => {}
			}
		}
		if queue.is_empty() {
			std::thread::park_timeout(Duration::from_secs(30));
		}
	}
}
