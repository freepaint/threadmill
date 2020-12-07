use crate::pool::TaskPool;
use crate::task::{AsyncTask, SyncTask, Task};
use crate::{Priority, Scheduler};

#[test]
fn sync_task() {
	let pool = TaskPool::new_with_max(1);
	let join = pool.spawn(Priority::Normal, || 1337);
	assert_eq!(join.join().unwrap(), 1337);
}

#[test]
fn async_task() {
	let pool = TaskPool::new_with_max(1);
	let join = pool.spawn_async(Priority::Normal, async { 1337 });
	assert_eq!(join.join().unwrap(), 1337);
}

#[test]
fn sync_task_old() {
	let pool = TaskPool::new_with_max(1);
	let (task, join) = SyncTask::new(|| 1337);
	pool.schedule(Priority::Normal, task);
	assert_eq!(join.join().unwrap(), 1337);
}

#[test]
fn async_task_old() {
	let pool = TaskPool::new_with_max(1);
	let (task, join) = AsyncTask::new(async { 1337 });
	pool.schedule(Priority::Normal, task);
	assert_eq!(join.join().unwrap(), 1337);
}
