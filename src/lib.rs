use std::future::Future;

pub use prelude::*;

pub mod prelude {
	pub use crate::pool::ThreadPool;
	pub use crate::task::{AsyncTask, JoinHandle, SyncTask, Task};
}

pub mod pool;
pub mod task;

pub trait Scheduler {
	fn schedule(&self, priority: Priority, task: Box<dyn Task>);
	fn spawn<T: 'static + Send>(&self, priority: Priority, fun: impl FnOnce() -> T + Send);
	fn spawn_async<T: 'static + Send>(&self, priority: Priority, future: impl Future<Output = T>);
}

pub enum Priority {
	/// Low priority for task given, will be processed last
	/// Identical to Normal with default implementation
	/// Single threaded with default implementation
	Low,
	/// Normal priority for task given, will be processed in order given
	/// Identical to Low with default implementation
	/// Single threaded with default implementation
	Normal,
	/// High priority for task given, will be processed first
	/// Single threaded with default implementation
	High,
	/// For bulk amount of tasks
	/// Multi threaded with default implementation, processed in own [`crate::ThreadPool`]
	Bulk,
}
