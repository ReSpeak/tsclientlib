#[macro_use]
extern crate criterion;
extern crate futures;
#[macro_use]
extern crate slog;
extern crate tokio;
extern crate tsproto;

use criterion::{Bencher, Benchmark, Criterion};
use futures::{future, Future, Sink};

use tsproto::packets::*;
use tsproto::*;

mod utils;
use crate::utils::*;

fn send_messages(b: &mut Bencher) {
	tsproto::init().unwrap();
	let local_address = "127.0.0.1:0".parse().unwrap();
	let address = "127.0.0.1:9987".parse().unwrap();

	let logger = {
		let drain = slog::Discard;
		slog::Logger::root(drain, o!())
	};

	let mut rt = tokio::runtime::Runtime::new().unwrap();
	let logger2 = logger.clone();
	let (c, con) = rt.block_on(future::lazy(move || {
		let c = create_client(
			local_address,
			logger2.clone(),
			SimplePacketHandler,
			0,
		);

		info!(logger2, "Connecting");
		utils::connect(logger2.clone(), c.clone(), address)
			.map_err(|e| panic!("Failed to connect ({:?})", e))
			.map(move |con| (c, con))
	})).unwrap();

	let mut header = Header::default();
	header.set_type(PacketType::Command);
	let mut cmd = commands::Command::new("sendtextmessage");
	cmd.push("targetmode", "3");
	let mut i = 0;

	b.iter(|| {
		let mut cmd = cmd.clone();
		cmd.push("msg", format!("Hello {}", i));
		let packet = Packet::new(header.clone(), Data::Command(cmd));
		i += 1;

		let sink = con.as_packet_sink();
		rt.block_on(future::lazy(move || {
			sink.send(packet)
		})).unwrap();
	});
	rt.block_on(future::lazy(move || {
		disconnect(con)
			.map_err(|e| panic!("Failed to disconnect ({:?})", e))
			.and_then(move |_| {
				info!(logger, "Disconnected");
				// Quit client
				drop(c);
				Ok(())
			})
	})).unwrap();
	rt.shutdown_on_idle().wait().unwrap();
}


fn bench_message(c: &mut Criterion) {
	c.bench("message", Benchmark::new("message", send_messages)
		.sample_size(200));
}

criterion_group!(benches, bench_message);
criterion_main!(benches);
