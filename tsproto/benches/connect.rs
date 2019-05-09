#[macro_use]
extern crate criterion;
extern crate futures;
#[macro_use]
extern crate slog;
extern crate tokio;
extern crate tsproto;

use criterion::{Bencher, Benchmark, Criterion};
use futures::{future, Future};

mod utils;
use crate::utils::*;

fn connect(b: &mut Bencher) {
	let local_address = "127.0.0.1:0".parse().unwrap();
	let address = "127.0.0.1:9987".parse().unwrap();

	let logger = {
		let drain = slog::Discard;
		//use slog::Drain;
		//let decorator = slog_term::TermDecorator::new().build();
		//let drain = slog_term::CompactFormat::new(decorator).build().fuse();
		//let drain = slog_async::Async::new(drain).build().fuse();
		slog::Logger::root(drain, o!())
	};

	let mut rt = tokio::runtime::Runtime::new().unwrap();

	b.iter(|| {
		let logger = logger.clone();
		rt.block_on(future::lazy(move || {
			// The TS server does not accept the 3rd reconnect from the same port
			// so we create a new client for every connection.
			let c = create_client(
				local_address,
				logger.clone(),
				SimplePacketHandler,
				0,
			);

			info!(logger, "Connecting");
			let logger2 = logger.clone();
			let c2 = c.clone();
			utils::connect(logger.clone(), c.clone(), address)
				.map_err(|e| panic!("Failed to connect ({:?})", e))
				.and_then(move |con| {
					info!(logger, "Disconnecting");
					disconnect(&c2, con)
						.map_err(|e| panic!("Failed to disconnect ({:?})", e))
				})
				.and_then(move |_| {
					info!(logger2, "Disconnected");
					// Quit client
					drop(c);
					Ok(())
				})
		}))
		.unwrap();
	});
	rt.shutdown_on_idle().wait().unwrap();
}

fn bench_connect(c: &mut Criterion) {
	c.bench(
		"connect",
		Benchmark::new("connect", connect).sample_size(20),
	);
}

criterion_group!(benches, bench_connect);
criterion_main!(benches);
