use crate::task::Task;

pub struct SyncTask<F: FnOnce() + Send> {
	inner: Option<F>,
}

impl<F: FnOnce() + Send> Task for SyncTask<F> {
	fn exec(&mut self, _: Box<dyn FnOnce() + Send + Sync>) {
		self.inner.take().expect("This task may only be called once")();
	}
}

impl<F: FnOnce() + Send> From<F> for SyncTask<F> {
	fn from(fun: F) -> Self {
		Self { inner: Some(fun) }
	}
}
