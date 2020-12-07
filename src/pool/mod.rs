mod death;
#[cfg(test)]
mod tests;

use crate::{task, Priority, Scheduler, Task};
use log::debug;
use std::future::Future;
use std::panic::UnwindSafe;

type WatchdogCallback = (Box<dyn Task>, flume::Receiver<()>);

/// [`TaskPool`]s provide and manage threads to schedule sync and async tasks, although sync tasks may never block.
/// There are 3 Priorities that are implemented with this design, [`Normal`], [`High`] and [`Bulk`] ([`Low`] maps to [`Normal`])
///
/// ```
/// use threadmill::pool::TaskPool;
/// use threadmill::{Scheduler, Priority};
///
/// // This spawns a TaskPool with n + 3 threads, while n equals to the count of logical cores.
/// let pool = TaskPool::new();
/// // Now we spawn a task with normal priority to perform some *very hard* math
/// let join_handle = pool.spawn(Priority::Normal, || 13 * 100 + 37);
/// // We can either join a task with `join` (Sync) or `join_async` (async)
/// assert_eq!(join_handle.join().unwrap(), 1337);
/// ```
/// [`JoinHandle`]s can also be safely dropped, the task will execute to end and drop all resources afterwards.
/// ```
/// use threadmill::pool::TaskPool;
/// use threadmill::{Priority, Scheduler};
///
/// let pool = TaskPool::new();
/// // This time we need to process a lot of *immense* computation, so we can spawn a lot of tasks and
/// let sum = (0..42)
///     .map(|_| pool.spawn(Priority::Bulk, || 1u32))
///     .collect::<Vec<_>>() // We collect here to force all tasks to be spawned first
///     .into_iter()
///     .map(|join_handle| join_handle.join().unwrap())
///     .sum::<u32>();
/// assert_eq!(sum, 42);
/// ```
///
/// [`Low`]: ../enum.Priority.html
/// [`Normal`]: ../enum.Priority.html
/// [`High`]: ../enum.Priority.html
/// [`Bulk`]: ../enum.Priority.html
/// [`JoinHandle`]: ../task/struct.JoinHandle.html
pub struct TaskPool {
	scheduler: TaskScheduler,
	handles: Vec<std::thread::JoinHandle<()>>,
	death_con: death::DeathController,
}

///
#[derive(Clone, Default)]
pub struct TaskScheduler {
	priority: SchedulerQueue,
	regular: SchedulerQueue,
	work: SchedulerQueue,
}

#[derive(Clone)]
pub struct SchedulerQueue {
	enqueuer: flume::Sender<Box<dyn Task>>,
	dequeuer: flume::Receiver<Box<dyn Task>>,
}

impl TaskPool {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn new_with_max(thread_count: usize) -> Self {
		use rand::Rng;

		let tp_id = rand::thread_rng().gen::<u16>();
		let scheduler = TaskScheduler::default();
		let mut death_con = death::DeathController::default();

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

impl Scheduler for TaskPool {
	fn schedule(&self, priority: Priority, task: impl Task + 'static + Send) {
		self.scheduler.schedule(priority, task)
	}

	fn spawn<T: 'static + Send>(
		&self,
		priority: Priority,
		fun: impl FnOnce() -> T + 'static + Send + UnwindSafe,
	) -> crate::JoinHandle<T> {
		self.scheduler.spawn(priority, fun)
	}

	fn spawn_async<T: 'static + Send>(
		&self,
		priority: Priority,
		future: impl Future<Output = T> + 'static + Send,
	) -> crate::JoinHandle<T> {
		self.scheduler.spawn_async(priority, future)
	}
}

impl Default for TaskPool {
	fn default() -> Self {
		Self::new_with_max(num_cpus::get())
	}
}

impl Drop for TaskPool {
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

impl Scheduler for TaskScheduler {
	fn schedule(&self, priority: Priority, task: impl Task + 'static) {
		let task = Box::new(task) as Box<dyn Task>;
		match priority {
			Priority::Low | Priority::Normal => self.regular.enqueuer.send(task).expect("No ThreadPool"),
			Priority::High => self.priority.enqueuer.send(task).expect("No ThreadPool"),
			Priority::Bulk => self.work.enqueuer.send(task).expect("No ThreadPool"),
		}
	}

	fn spawn<T: 'static + Send>(
		&self,
		priority: Priority,
		fun: impl FnOnce() -> T + 'static + Send + UnwindSafe,
	) -> crate::JoinHandle<T> {
		let (task, join_handle) = task::SyncTask::new(fun);
		self.schedule(priority, task);
		join_handle
	}

	fn spawn_async<T: 'static + Send>(
		&self,
		priority: Priority,
		future: impl Future<Output = T> + 'static + Send,
	) -> crate::JoinHandle<T> {
		let (task, join_handle) = task::AsyncTask::new(future);
		self.schedule(priority, task);
		join_handle
	}
}

impl Default for SchedulerQueue {
	fn default() -> Self {
		let (tx, rx) = flume::unbounded();
		debug!("New Queue created");
		Self {
			enqueuer: tx,
			dequeuer: rx,
		}
	}
}

fn gen_executor(
	queue: SchedulerQueue,
	watchdog: flume::Sender<WatchdogCallback>,
	dt: death::DeathToken,
) -> impl FnOnce() {
	use flume::{Selector, TryRecvError};

	move || {
		while let Some(Ok(mut task)) = Selector::new()
			.recv(dt.listen(), |_| None)
			.recv(&queue.dequeuer, Some)
			.wait()
		{
			let (tx, rx) = flume::bounded(1);
			task.exec(Box::new(move || {
				tx.send(()).expect("reschedule channel may not be closed")
			}));
			match rx.try_recv() {
				Ok(_) => drop(queue.enqueuer.send(task)),
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

fn run_watchdog(queues: Vec<(flume::Receiver<WatchdogCallback>, SchedulerQueue)>, dt: death::DeathToken) {
	use std::fmt::{Display, Formatter};
	use std::time::Duration;

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
				let _ = sched.enqueuer.send(task);
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
