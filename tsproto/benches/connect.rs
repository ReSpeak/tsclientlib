use anyhow::{Error, Result};
use criterion::{criterion_group, criterion_main, Bencher, Benchmark, Criterion};
use once_cell::sync::Lazy;
use tracing::{info, warn};
use tsproto::client::Client;
use tsproto::connection::StreamItem;

mod utils;
use crate::utils::*;

static TRACING: Lazy<()> = Lazy::new(|| tracing_subscriber::fmt().with_test_writer().init());

async fn wait_channellistfinished(con: &mut Client) -> Result<()> {
	con.filter_items(|con, i| {
		Ok(match i {
			StreamItem::S2CInit(packet) => {
				con.hand_back_buffer(packet.into_buffer());
				None
			}
			StreamItem::C2SInit(packet) => {
				con.hand_back_buffer(packet.into_buffer());
				None
			}
			StreamItem::Command(packet) => {
				if packet.data().data().name == "channellistfinished" {
					Some(())
				} else {
					None
				}
			}
			StreamItem::Error(error) => {
				warn!(%error, "Got connection error");
				None
			}
			i => {
				warn!(got = ?i, "Unexpected packet, waiting for channellistfinished");
				None
			}
		})
	})
	.await?;
	Ok(())
}

fn one_connect(b: &mut Bencher) {
	//Lazy::force(&TRACING);

	let local_address = "127.0.0.1:0".parse().unwrap();
	let address = "127.0.0.1:9987".parse().unwrap();

	let mut rt = tokio::runtime::Runtime::new().unwrap();

	b.iter(|| {
		rt.block_on(async move {
			// The TS server does not accept the 3rd reconnect from the same port
			// so we create a new client for every connection.
			let mut con = create_client(local_address, address, 0).await?;

			connect(&mut con).await?;
			info!("Connected");
			// Wait until channellistfinished
			wait_channellistfinished(&mut con).await?;

			info!("Disconnecting");
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
