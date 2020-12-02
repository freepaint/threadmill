#[derive(Default)]
pub struct DeathController(Vec<flume::Sender<()>>);
pub struct DeathToken(flume::Receiver<()>);

impl DeathController {
	pub fn token(&mut self) -> DeathToken {
		let (tx, rx) = flume::bounded(0);
		self.0.push(tx);
		DeathToken(rx)
	}

	pub fn kill(&mut self) {
		for sender in &self.0 {
			while sender.send(()).is_ok() {}
		}
	}
}

impl DeathToken {
	pub fn listen(&self) -> &flume::Receiver<()> {
		&self.0
	}
}
