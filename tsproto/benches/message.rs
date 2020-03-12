use anyhow::{bail, Error};
use criterion::{criterion_group, criterion_main, Bencher, Benchmark, Criterion};
use slog::{info, o, Logger};
use tsproto_packets::packets::*;

mod utils;
use crate::utils::*;

fn send_messages(b: &mut Bencher) {
	let local_address = "127.0.0.1:0".parse().unwrap();
	let address = "127.0.0.1:9987".parse().unwrap();

	//let logger = Logger::root(slog::Discard, o!());
	let logger = create_logger();

	let mut rt = tokio::runtime::Runtime::new().unwrap();
	let mut con = rt.block_on(async move {
		let mut con = create_client(
			local_address,
			address,
			logger.clone(),
			0,
		).await?;

		info!(logger, "Connecting");
		connect(&mut con).await?;
		Ok::<_, Error>(con)
	}).unwrap();

	let mut i = 0;

	let mut futs = Vec::new();
	b.iter(|| {
		let text = format!("Hello {}", i);
		let packet =
			OutCommand::new::<_, _, String, String, _, _, std::iter::Empty<_>>(
				Direction::C2S,
				PacketType::Command,
				"sendtextmessage",
				vec![("targetmode", "3"), ("msg", &text)].into_iter(),
				std::iter::empty(),
			);
		i += 1;

		rt.block_on(async {
			let fut = con.send_packet(packet).await;
			futs.push(fut);
			/*let mut fut = con.send_packet_with_answer(packet).await;
			tokio::select! {
				_ = &mut fut => {}
				_ = con.wait_disconnect() => {
					bail!("Disconnected");
				}
			};*/
		});
	});

	rt.block_on(async move {
		tokio::select! {
			r = futures::future::join_all(futs) => {
				r.into_iter().collect::<Result<_, _>>()?;
			}
			r = con.wait_disconnect() => {
				r?;
				bail!("Disconnected");
			}
		};
		disconnect(&mut con).await
	}).unwrap();
}

fn bench_message(c: &mut Criterion) {
	c.bench(
		"message",
		Benchmark::new("message", send_messages).sample_size(200),
	);
}

criterion_group!(benches, bench_message);
criterion_main!(benches);
