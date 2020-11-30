pub trait Task {
	fn exec(&mut self) -> TaskState;
}

pub enum TaskState {
	Reschedule,
	Done,
}
