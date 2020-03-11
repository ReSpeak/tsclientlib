use anyhow::Error;
use criterion::{criterion_group, criterion_main, Bencher, Benchmark, Criterion};
use slog::{info, o, Logger};

mod utils;
use crate::utils::*;

fn one_connect(b: &mut Bencher) {
	let local_address = "127.0.0.1:0".parse().unwrap();
	let address = "127.0.0.1:9987".parse().unwrap();

	let logger = Logger::root(slog::Discard, o!());

	let mut rt = tokio::runtime::Runtime::new().unwrap();

	b.iter(|| {
		let logger = logger.clone();
		rt.block_on(async move {
			// The TS server does not accept the 3rd reconnect from the same port
			// so we create a new client for every connection.
			let mut con = create_client(
				local_address,
				address,
				logger.clone(),
				0,
			).await?;

			info!(logger, "Connecting");
			connect(&mut con).await?;
			info!(logger, "Disconnecting");
			disconnect(&mut con).await?;
			Ok::<_, Error>(())
		})
		.unwrap();
	});
}

fn bench_connect(c: &mut Criterion) {
	c.bench("connect", Benchmark::new("connect", one_connect).sample_size(20));
}

criterion_group!(benches, bench_connect);
criterion_main!(benches);
