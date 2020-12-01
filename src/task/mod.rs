mod r#async;
pub mod sync;

pub use r#async::AsyncTask;
pub use sync::SyncTask;

pub trait Task: Send {
	fn exec(&mut self, reschedule: Box<dyn FnOnce() + Send + Sync>);
}
