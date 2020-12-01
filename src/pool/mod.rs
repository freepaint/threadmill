#[cfg(test)]
mod tests;

use crate::task::Task;

use flume::TryRecvError;
use std::time::Duration;
use until::UntilExt;

type WatchdogCallback = (Box<dyn Task>, flume::Receiver<()>);

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
		let (callback, worker_callback) = flume::unbounded();
		let worker_scheduler = scheduler.work.clone();
		let mut handles = (0..thread_count)
			.map(|_| std::thread::spawn(gen_executor(worker_scheduler.clone(), callback.clone())))
			.collect::<Vec<_>>();

		// Spawn thread for regular queue
		let (callback, regular_callback) = flume::unbounded();
		let regular_scheduler = scheduler.regular.clone();
		handles.push(std::thread::spawn(gen_executor(
			scheduler.regular.clone(),
			callback,
		)));

		// Spawn thread for priority queue
		let (callback, priority_callback) = flume::unbounded();
		let priority_scheduler = scheduler.priority.clone();
		handles.push(std::thread::spawn(gen_executor(
			scheduler.priority.clone(),
			callback,
		)));

		std::thread::spawn(move || {
			gen_watchdog(vec![
				(worker_callback, worker_scheduler),
				(regular_callback, regular_scheduler),
				(priority_callback, priority_scheduler),
			])
		});

		Self { scheduler }
	}
}

fn gen_watchdog(queues: Vec<(flume::Receiver<WatchdogCallback>, SchedulerQueue)>) {
	let mut queues = queues
		.into_iter()
		.zip(0usize..)
		.map(|((rcv, sched), id)| (id, Some(rcv), sched))
		.collect::<Vec<_>>();
	let mut tasks = Vec::<(Box<dyn Task>, flume::Receiver<()>, usize)>::new();
	loop {
		let mut selector = flume::Selector::new();
		let mut any = false;
		for queue in queues.iter().filter(|(_, rcv, _)| rcv.is_some()) {
			let id = queue.0;
			selector = selector.recv(&queue.1.as_ref().unwrap(), move |new_task| match new_task {
				Ok((task, rcv)) => Handle::NewTask(task, rcv, id),
				Err(_) => Handle::RemoveQueue(queue.0),
			});
			any = true;
		}
		if any && tasks.is_empty() {
			return;
		}
		for task in tasks.iter().zip(0..) {
			let id = task.1;
			selector = selector.recv(&(task.0).1, move |res| match res {
				Ok(_) => Handle::Reschedule(id),
				Err(_) => Handle::RemoveTask(task.1),
			});
		}
		match selector.wait() {
			Handle::NewTask(task, rcv, sched_id) => {
				tasks.push((task, rcv, sched_id));
			}
			Handle::Reschedule(task_id) => {
				let (task, _, sched_id) = tasks.remove(task_id);
				let (_, _, sched) = queues.get(sched_id).unwrap();
				let _ = sched.scheduler.send(task);
			}
			Handle::RemoveTask(task_id) => drop(tasks.remove(task_id)),
			Handle::RemoveQueue(queue_id) => {
				queues.get_mut(queue_id).unwrap().1 = None;
			}
		}
	}

	enum Handle {
		NewTask(Box<dyn Task>, flume::Receiver<()>, usize),
		Reschedule(usize),
		RemoveTask(usize),
		RemoveQueue(usize),
	}
}

// This got its own function for "readability"
fn gen_executor(queue: SchedulerQueue, watchdog: flume::Sender<WatchdogCallback>) -> impl FnOnce() {
	move || loop {
		for mut task in queue.queue.iter().do_for(Duration::from_millis(100)) {
			let (tx, rx) = flume::bounded(1);
			task.exec(Box::new(move || {
				tx.send(()).expect("reschedule channel may not be closed")
			}));
			match rx.try_recv() {
				Ok(_) => drop(queue.scheduler.send(task)),
				Err(TryRecvError::Disconnected) => (),
				Err(TryRecvError::Empty) => {
					watchdog
						.send((task, rx))
						.expect("Watchdog died, unable to reschedule task");
				}
			}
		}
	}
}
