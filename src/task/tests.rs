use crate::pool::ThreadPool;
use crate::task::{AsyncTask, SyncTask, Task};

#[test]
fn sync_task() {
	let pool = ThreadPool::new_with_max(1);
	let (task, join) = SyncTask::new(|| 1337);
	pool.scheduler
		.regular
		.scheduler
		.send(Box::new(task) as Box<dyn Task>);
	assert_eq!(join.join().unwrap(), 1337);
}

#[test]
fn async_task() {
	let pool = ThreadPool::new_with_max(1);
	let (task, join) = AsyncTask::new(async { 1337 });
	pool.scheduler
		.regular
		.scheduler
		.send(Box::new(task) as Box<dyn Task>);
	assert_eq!(join.join().unwrap(), 1337);
}
