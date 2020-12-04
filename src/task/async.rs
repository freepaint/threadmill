use crate::task::{JoinHandle, Task};
use futures_task::{ArcWake, Context, Poll};
use parking_lot::Mutex;
use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

type PinnedFuture = Pin<Box<dyn Future<Output = Box<dyn Any + 'static + Send>> + 'static + Send>>;

pub struct AsyncTask<T: 'static + Send> {
	inner: Mutex<Option<PinnedFuture>>,
	callback: Option<flume::Sender<std::thread::Result<T>>>,
}

impl<T: 'static + Send> AsyncTask<T> {
	pub fn new(task: impl Future<Output = T> + 'static + Send) -> (Self, JoinHandle<T>) {
		let (tx, rx) = flume::bounded(1);
		(
			Self {
				inner: Mutex::new(Some(Box::pin(async {
					Box::new(task.await) as Box<dyn Any + 'static + Send>
				}))),
				callback: Some(tx),
			},
			JoinHandle(rx),
		)
	}
}

pub struct TaskWaker(AtomicPtr<Box<dyn FnOnce() + Send + Sync>>);

impl<T: 'static + Send> Task for AsyncTask<T> {
	fn exec(&mut self, reschedule: Box<dyn FnOnce() + Send + Sync>) {
		let mut lock = self.inner.lock();
		let mut pin = match lock.take() {
			Some(future) => future,
			None => {
				log::error!("Future polled after completion");
				return;
			}
		};
		let waker = TaskWaker::new(reschedule);
		let waker = futures_task::waker(Arc::new(waker));
		let mut ctx = Context::from_waker(&waker);
		match pin.as_mut().poll(&mut ctx) {
			Poll::Ready(result) => {
				if self.callback.is_some() {
					let _ = self.callback.as_ref().unwrap().send(
						result
							.downcast::<T>()
							.map(|t| *t)
							.map_err(|err| Box::new(err) as Box<dyn Any + Send>),
					);
				}
			}
			Poll::Pending => {
				*lock = Some(pin);
			}
		}
	}
}

impl TaskWaker {
	pub fn new(callback: Box<dyn FnOnce() + Send + Sync>) -> Self {
		Self(AtomicPtr::new(Box::leak(Box::new(callback))))
	}
}

impl ArcWake for TaskWaker {
	fn wake_by_ref(arc_self: &Arc<Self>) {
		let ptr = arc_self.0.swap(std::ptr::null_mut(), Ordering::AcqRel);
		let waker = *unsafe { Box::from_raw(ptr) };
		waker();
	}
}

impl Drop for TaskWaker {
	fn drop(&mut self) {
		let ptr = self.0.load(Ordering::Acquire);
		if !ptr.is_null() {
			let _ = unsafe { Box::from_raw(ptr) };
		}
	}
}
