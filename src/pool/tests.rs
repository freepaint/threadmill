use crate::task::Task;
use rand::Rng;
use std::time::Duration;

#[test]
fn workload() {
	std::env::set_var("RUST_LOG", "debug");
	env_logger::init();

	let mut rng = rand::thread_rng();
	let mut load = Polygon::new();
	for _ in 0..10 {
		load.push(rng.gen());
	}
	let pool = super::ThreadPool::new();
	let (tx, rx) = flume::unbounded();
	for i in 0..5 {
		pool.scheduler
			.work
			.enqueuer
			.send(Box::new(MathWorker {
				polygon: load.clone(),
				bar: tx.clone(),
				tests: 0,
				hit_count: 0,
				target: 10_000,
				id: i,
			}))
			.unwrap();
	}
	log::info!("{}", rx.iter().take(5).sum::<f64>() / 5.0);
}

type Point = (f64, f64);
type Polygon = Vec<Point>;
struct MathWorker {
	polygon: Polygon,
	tests: u32,
	bar: flume::Sender<f64>,
	hit_count: u32,
	target: u32,
	id: u32,
}

impl Task for MathWorker {
	fn exec(&mut self, rescheduler: Box<dyn FnOnce() + Send + Sync>) {
		let steps = 1_000.min(self.target - self.tests);
		let mut rng = rand::thread_rng();
		for _ in 0..steps {
			let point = rng.gen();
			if point_in_polygon(&point, &self.polygon) {
				self.hit_count += 1;
			}
		}
		self.tests += steps;
		if self.tests < self.target {
			log::info!("[{}] Rescheduling! {}", self.id, self.tests);
			std::thread::spawn(move || {
				std::thread::sleep(Duration::from_millis(10));
				rescheduler();
			});
		} else {
			let res = self.hit_count as f64 / self.tests as f64;
			log::info!("[{}] Result: {}%", self.id, res * 100.0);
			self.bar.send(res).unwrap();
		}
	}
}

fn point_in_polygon(point: &Point, vertices: &Polygon) -> bool {
	if vertices.len() < 3 {
		return false;
	}
	let mut is_in_polygon = false;
	let mut last_vert = vertices[vertices.len() - 1];
	for vertex in vertices {
		if is_between(point.1, last_vert.1, vertex.1) {
			let t = (point.1 - last_vert.1) / (vertex.1 - last_vert.1);
			let x = t * (point.0 - last_vert.0) + last_vert.0;
			if x >= point.0 {
				is_in_polygon = !is_in_polygon;
			}
		} else if ((point.1 - last_vert.1).abs() < f64::EPSILON
			&& point.0 < last_vert.0
			&& vertex.1 > point.1)
			|| ((point.1 - vertex.1).abs() < f64::EPSILON && point.0 < vertex.0 && last_vert.1 > point.1)
		{
			is_in_polygon = !is_in_polygon;
		}
		last_vert = vertex.clone();
	}
	is_in_polygon
}

fn is_between(x: f64, a: f64, b: f64) -> bool {
	(x - a) * (x - b) < 0.0
}
