mod r#async;
pub mod sync;
#[cfg(test)]
mod tests;

use std::any::Any;

pub use r#async::AsyncTask;
pub use sync::SyncTask;

pub trait Task: Send {
	fn exec(&mut self, reschedule: Box<dyn FnOnce() + Send + Sync>);
}

pub struct JoinHandle<T: Send>(flume::Receiver<std::thread::Result<T>>);

impl<T: Send> JoinHandle<T> {
	pub fn join(self) -> std::thread::Result<T> {
		match self.0.recv() {
			Ok(result) => result.map_err(|err| Box::new(err) as Box<dyn Any + Send>),
			Err(err) => Err(Box::new(err) as Box<dyn Any + Send>),
		}
	}

	pub async fn join_async(self) -> std::thread::Result<T> {
		match self.0.recv_async().await {
			Ok(result) => result.map_err(|err| Box::new(err) as Box<dyn Any + Send>),
			Err(err) => Err(Box::new(err) as Box<dyn Any + Send>),
		}
	}

	pub fn alive(&self) -> bool {
		!self.0.is_disconnected()
	}

	pub fn result_available(&self) -> bool {
		!self.0.is_empty()
	}
}
