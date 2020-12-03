use crate::task::{JoinHandle, Task};
use std::panic::UnwindSafe;

pub struct SyncTask<F: FnOnce() -> T + Send + UnwindSafe, T: Send = ()> {
	inner: Option<F>,
	callback: Option<flume::Sender<std::thread::Result<T>>>,
}

impl<F: FnOnce() -> T + Send + UnwindSafe, T: Send> SyncTask<F, T> {
	pub fn new(fun: F) -> (Self, JoinHandle<T>) {
		let (tx, rx) = flume::bounded(1);
		(
			Self {
				inner: Some(fun),
				callback: Some(tx),
			},
			JoinHandle(rx),
		)
	}
}

impl<F: FnOnce() -> T + Send + UnwindSafe, T: Send> Task for SyncTask<F, T> {
	fn exec(&mut self, _: Box<dyn FnOnce() + Send + Sync>) {
		let fun = self.inner.take().expect("This task may only be called once");
		let result = std::panic::catch_unwind(move || fun());
		if self.callback.is_some() {
			let _ = self.callback.as_ref().unwrap().send(result);
		}
	}
}

impl<F: FnOnce() -> T + Send + UnwindSafe, T: Send> From<F> for SyncTask<F, T> {
	fn from(fun: F) -> Self {
		Self {
			inner: Some(fun),
			callback: None,
		}
	}
}
