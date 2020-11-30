use evc::OperationCache;
use std::time::Instant;

pub struct ExecutionTimeWrapper(Option<Instant>);

enum ETOperation {
	Set(Instant),
	Clear,
}

impl evc::OperationCache for ExecutionTimeWrapper {
	type Operation = ETOperation;

	fn apply_operation(&mut self, operation: Self::Operation) {
		self.0 = match operation {
			ETOperation::Set(inst) => Some(inst),
			ETOperation::Clear => None,
		}
	}
}
