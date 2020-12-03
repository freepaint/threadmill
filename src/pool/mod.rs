mod death;
#[cfg(test)]
mod tests;

use crate::pool::death::{DeathController, DeathToken};
use crate::task::Task;
use flume::select::SelectError;
use flume::{Selector, TryRecvError};
use log::debug;
use rand::Rng;
use std::fmt::{Display, Formatter};
use std::thread::JoinHandle;
use std::time::Duration;

type WatchdogCallback = (Box<dyn Task>, flume::Receiver<()>);

pub struct ThreadPool {
	scheduler: Scheduler,
	handles: Vec<JoinHandle<()>>,
	death_con: DeathController,
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
		debug!("New Queue created");
		Self {
			scheduler: tx,
			queue: rx,
		}
	}
}

impl ThreadPool {
	pub fn new() -> Self {
		Self::new_with_max(num_cpus::get())
	}

	pub fn new_with_max(thread_count: usize) -> Self {
		let tp_id = rand::thread_rng().gen::<u32>();
		let scheduler = Scheduler::default();
		let mut death_con = DeathController::default();

		// Spawn work threads
		let (callback, worker_callback) = flume::unbounded();
		let worker_scheduler = scheduler.work.clone();
		let mut handles = (0..thread_count)
			.map(|i| {
				debug!("Spawning work executor {}", i);
				std::thread::Builder::new()
					.name(format!("{}-Worker{{id={}}}", tp_id, i))
					.spawn(gen_executor(
						worker_scheduler.clone(),
						callback.clone(),
						death_con.token(),
					))
					.unwrap()
			})
			.collect::<Vec<_>>();

		// Spawn thread for regular queue
		let (callback, regular_callback) = flume::unbounded();
		let regular_scheduler = scheduler.regular.clone();
		debug!("Spawning regular executor");
		handles.push(
			std::thread::Builder::new()
				.name(format!("{}-Executor{{Regular}}", tp_id))
				.spawn(gen_executor(
					scheduler.regular.clone(),
					callback,
					death_con.token(),
				))
				.unwrap(),
		);

		// Spawn thread for priority queue
		let (callback, priority_callback) = flume::unbounded();
		let priority_scheduler = scheduler.priority.clone();
		debug!("Spawning priority executor");
		handles.push(
			std::thread::Builder::new()
				.name(format!("{}-Executor{{Priority}}", tp_id))
				.spawn(gen_executor(
					scheduler.priority.clone(),
					callback,
					death_con.token(),
				))
				.unwrap(),
		);

		let dtc = death_con.token();
		debug!("Spawning Watchdog");
		handles.push(
			std::thread::Builder::new()
				.name(format!("{}-Watchdog", tp_id))
				.spawn(move || {
					run_watchdog(
						vec![
							(worker_callback, worker_scheduler),
							(regular_callback, regular_scheduler),
							(priority_callback, priority_scheduler),
						],
						dtc,
					)
				})
				.unwrap(),
		);

		debug!(
			"New ThreadPool created, ThreadPool{{id={},size={}}}",
			tp_id, thread_count
		);
		Self {
			scheduler,
			handles,
			death_con,
		}
	}
}

impl Drop for ThreadPool {
	fn drop(&mut self) {
		debug!("Sending Death notification...");
		self.death_con.kill();
		while let Some(handle) = self.handles.pop() {
			debug!("Joining thread...");
			let _ = handle.join();
		}
		debug!("ThreadPool shutdown");
	}
}

fn run_watchdog(queues: Vec<(flume::Receiver<WatchdogCallback>, SchedulerQueue)>, dt: DeathToken) {
	log::debug!("Started watchdog");
	let mut queues = queues
		.into_iter()
		.zip(0usize..)
		.map(|((rcv, sched), id)| (id, Some(rcv), sched))
		.collect::<Vec<_>>();
	let mut tasks = Vec::<(Box<dyn Task>, flume::Receiver<()>, usize)>::new();
	let mut timeout_c = 1;
	loop {
		let mut selector = flume::Selector::new().recv(dt.listen(), |_| Handle::Death);
		let mut any = false;
		for queue in queues.iter().filter(|(_, rcv, _)| rcv.is_some()) {
			let id = queue.0;
			selector = selector.recv(&queue.1.as_ref().unwrap(), move |new_task| match new_task {
				Ok((task, rcv)) => Handle::NewTask(task, rcv, id),
				Err(_) => Handle::RemoveQueue(queue.0),
			});
			any = true;
		}
		if !any && tasks.is_empty() {
			debug!("Watchdog has no work left, Exiting...");
			return;
		}
		for task in tasks.iter().zip(0..) {
			let id = task.1;
			selector = selector.recv(&(task.0).1, move |res| match res {
				Ok(_) => Handle::Reschedule(id),
				Err(_) => Handle::RemoveTask(task.1),
			});
		}
		let handle = selector.wait_timeout(Duration::from_millis(50 * timeout_c));
		let handle = match handle {
			Ok(handle) => {
				timeout_c = 1;
				handle
			}
			Err(_) => {
				log::debug!("Watchdog Timeout");
				timeout_c = (timeout_c + 1).min(1000);
				continue;
			}
		};
		debug!("Received Handler, Handle{{{}}}", handle);
		match handle {
			Handle::NewTask(task, rcv, sched_id) => {
				tasks.push((task, rcv, sched_id));
				debug!("Received new Task, Scheduler{{id={}}}", sched_id);
			}
			Handle::Reschedule(task_id) => {
				let (task, _, sched_id) = tasks.remove(task_id);
				let (_, _, sched) = queues.get(sched_id).unwrap();
				let _ = sched.scheduler.send(task);
				debug!(
					"Rescheduling task, Task{{id={}}} -> Scheduler{{id={}}}",
					task_id, sched_id
				);
			}
			Handle::RemoveTask(task_id) => {
				let _ = tasks.remove(task_id);
				debug!("Removed task, Task{{id={}}}", task_id);
			}
			Handle::RemoveQueue(queue_id) => {
				queues.get_mut(queue_id).unwrap().1 = None;
				debug!("Removed Scheduler, Scheduler{{id={}}}", queue_id);
			}
			Handle::Death => {
				debug!("Watchdog received Death Request, Exiting...");
				return;
			}
		}
	}

	enum Handle {
		NewTask(Box<dyn Task>, flume::Receiver<()>, usize),
		Reschedule(usize),
		RemoveTask(usize),
		RemoveQueue(usize),
		Death,
	}

	impl Display for Handle {
		fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
			match self {
				Handle::NewTask(_, _, id) => write!(f, "NewTask(id={})", id),
				Handle::Reschedule(id) => write!(f, "Reschedule(id={})", id),
				Handle::RemoveTask(id) => write!(f, "RemoveTask(id={})", id),
				Handle::RemoveQueue(id) => write!(f, "RemoveQueue(id={})", id),
				Handle::Death => write!(f, "Death"),
			}
		}
	}
}

// This got its own function for "readability"
fn gen_executor(
	queue: SchedulerQueue,
	watchdog: flume::Sender<WatchdogCallback>,
	dt: DeathToken,
) -> impl FnOnce() {
	move || {
		while let Some(Ok(mut task)) = Selector::new()
			.recv(dt.listen(), |_| None)
			.recv(&queue.queue, Some)
			.wait()
		{
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
