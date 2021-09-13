use anyhow::Error;
use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use tracing::info;
use tsproto_packets::packets::*;

mod utils;
use crate::utils::*;

fn send_messages(b: &mut Bencher) {
	create_logger(false);

	let local_address = "127.0.0.1:0".parse().unwrap();
	let address = "127.0.0.1:9987".parse().unwrap();

	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut con = rt
		.block_on(async move {
			let mut con = create_client(local_address, address, 0).await?;

			info!("Connecting");
			connect(&mut con).await?;
			Ok::<_, Error>(con)
		})
		.unwrap();

	let mut i = 0;

	let mut last_id = None;
	b.iter(|| {
		let text = format!("Hello {}", i);
		let mut packet =
			OutCommand::new(Direction::C2S, Flags::empty(), PacketType::Command, "sendtextmessage");
		packet.write_arg("targetmode", &"3");
		packet.write_arg("msg", &text);
		i += 1;

		rt.block_on(async {
			con.wait_until_can_send().await.unwrap();
			last_id = Some(con.send_packet(packet.into_packet()).unwrap());
		});
	});

	rt.block_on(async move {
		if let Some(id) = last_id {
			info!("Waiting for {:?}", id);
			con.wait_for_ack(id).await?;
		}
		tokio::select! {
			_ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
			}
			_ = con.wait_disconnect() => {
				anyhow::bail!("Disconnected");
			}
		};

		info!("Disconnecting");
		disconnect(&mut con).await
	})
	.unwrap();
}

fn bench_message(c: &mut Criterion) { c.bench_function("message", send_messages); }

criterion_group! {
	name = benches;
	config = Criterion::default().sample_size(200);
	targets = bench_message
}
criterion_main!(benches);
