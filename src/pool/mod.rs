use crate::task::Task;

mod executor;

pub struct ThreadPool {}

impl ThreadPool {
	pub fn new() -> Self {
		Self::new_max(num_cpus::get())
	}

	pub fn new_max(thread_count: usize) -> Self {
		let (tx, rx) = flume::unbounded();

		let threads = vec![];
		std::thread::spawn(gen_executor(rx.clone()));
	}
}

// This got its own function for readability
fn gen_executor(rx: flume::Receiver<Task>) -> impl FnOnce() -> () {
	|| loop {
		// DO async tasks
		for task in rx.try_iter().take(10) {} // Use until iter I just made later here because take sucks in that case
	}
}
