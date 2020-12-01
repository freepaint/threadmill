use crate::task::Task;
use futures_task::{ArcWake, Context, Poll};
use parking_lot::Mutex;
use std::any::Any;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::sync::Arc;

type PinnedFuture = Pin<Box<dyn Future<Output = Box<dyn Any + 'static + Send>> + 'static + Send>>;

pub struct AsyncTask {
	inner: Mutex<Option<PinnedFuture>>,
}

pub struct TaskWaker(AtomicPtr<Box<dyn FnOnce() + Send + Sync>>);

impl Task for AsyncTask {
	fn exec(&mut self, reschedule: Box<dyn FnOnce() + Send + Sync>) {
		let mut lock = self.inner.lock();
		let mut pin = match lock.take() {
			Some(future) => future,
			None => return, // TODO: polled complete future, announce error with log
		};
		let waker = TaskWaker::new(reschedule);
		let waker = futures_task::waker(Arc::new(waker));
		let mut ctx = Context::from_waker(&waker);
		match pin.as_mut().poll(&mut ctx) {
			Poll::Ready(_val) => {
				// TODO: Handle return value
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
