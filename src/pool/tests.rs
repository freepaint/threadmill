use crate::task::Task;
use rand::Rng;

#[test]
fn workload() {
	assert!(false, "this test is broken");
	let mut rng = rand::thread_rng();
	let mut load = Polygon::new();
	for _ in 0..10 {
		load.push(rng.gen());
	}
	let pool = super::ThreadPool::new();
	let bar = std::sync::Arc::new(std::sync::Barrier::new(21));
	for i in 0..20 {
		pool.scheduler
			.work
			.scheduler
			.send(Box::new(MathWorker {
				polygon: load.clone(),
				bar: bar.clone(),
				tests: 0,
				hit_count: 0,
				target: 1_000_000,
				id: i,
			}))
			.unwrap();
	}
	bar.wait();
}

type Point = (f64, f64);
type Polygon = Vec<Point>;
struct MathWorker {
	polygon: Polygon,
	tests: u32,
	bar: std::sync::Arc<std::sync::Barrier>,
	hit_count: u32,
	target: u32,
	id: u32,
}

impl Task for MathWorker {
	fn exec(&mut self, rescheduler: Box<dyn FnOnce()>) {
		let steps = 10_000.min(self.target - self.tests);
		let mut rng = rand::thread_rng();
		for _ in 0..steps {
			let point = rng.gen();
			if point_in_poligon(&point, &self.polygon) {
				self.hit_count += 1;
			}
		}
		self.tests += steps;
		if self.tests < self.target {
			println!("[{}] Rescheduling! {}", self.id, self.tests);
			rescheduler();
		} else {
			println!(
				"[{}] Result: {}%",
				self.id,
				self.hit_count as f64 / self.tests as f64 * 100.0
			);
			self.bar.wait();
		}
	}
}

fn point_in_poligon(point: &Point, vertices: &Polygon) -> bool {
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