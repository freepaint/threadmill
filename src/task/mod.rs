pub trait Task: Send {
	fn exec(&mut self, rescheduler: Box<dyn FnOnce()>);
}
