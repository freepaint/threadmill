pub trait Task: Send {
	fn exec(&mut self) -> TaskState;
}

pub enum TaskState {
	Reschedule,
	Done,
}
