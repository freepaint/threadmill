use crate::task::Task;
use std::time::Duration;
use until::UntilExt;

mod executor;

pub struct ThreadPool {}

impl ThreadPool {
	pub fn new() -> Self {
		Self::new_max(num_cpus::get())
	}

	pub fn new_max(thread_count: usize) -> Self {
		let (tx, rx) = flume::unbounded();

		let handles = (0..thread_count)
			.map(|_| std::thread::spawn(gen_executor(rx.clone())))
			.collect::<Vec<_>>();

		Self
	}
}

// This got its own function for readability
fn gen_executor(rx: flume::Receiver<Task>) -> impl FnOnce() -> () {
	|| loop {
		// DO async tasks
		for task in rx.try_iter().do_for(Duration::from_millis(100)) {}
		if rx.is_empty() {
			std::thread::park_timeout(Duration::from_secs(30));
		}
	}
}
