use anyhow::Error;
use criterion::{criterion_group, criterion_main, Bencher, Benchmark, Criterion};
use slog::{info, o, Logger};
use tsproto_packets::packets::*;

mod utils;
use crate::utils::*;

fn send_messages(b: &mut Bencher) {
	let local_address = "127.0.0.1:0".parse().unwrap();
	let address = "127.0.0.1:9987".parse().unwrap();

	//let logger = Logger::root(slog::Discard, o!());
	let logger = create_logger(true);

	let mut rt = tokio::runtime::Runtime::new().unwrap();
	let mut con = rt.block_on(async move {
		let mut con = create_client(
			local_address,
			address,
			logger.clone(),
			3,
		).await?;

		info!(logger, "Connecting");
		connect(&mut con).await?;
		Ok::<_, Error>(con)
	}).unwrap();

	let mut i = 0;

	let mut last_id = None;
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
			con.wait_until_can_send().await.unwrap();
			last_id = Some(con.send_packet(packet).unwrap());
		});
	});

	rt.block_on(async move {
		if let Some(id) = last_id {
			info!(con.logger, "Waiting for {:?}", id);
			con.wait_for_ack(id).await?;
		}
		tokio::select! {
			_ = tokio::time::delay_for(tokio::time::Duration::from_secs(1)) => {
			}
			_ = con.wait_disconnect() => {
				anyhow::bail!("Disconnected");
			}
		};

		info!(con.logger, "Disconnecting");
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
