//! Resolve TeamSpeak server addresses of any kind.

use std::net::{SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use futures::{Async, future, Future, Poll, stream, Stream};
use futures::sync::oneshot;
use rand::{thread_rng, Rng};
use slog::Logger;
use tokio_core::reactor::Handle;
use trust_dns_resolver::{AsyncResolver, Name};

use {Error, Result};

const RESOLVER_ADDR: ([u8; 4], u16) = ([8,8,8,8], 53);
const DEFAULT_PORT: u16 = 9987;
const TSDNS_DEFAULT_PORT: u16 = 41144;
const DNS_PREFIX_TCP: &str = "_tsdns._tcp.";
const DNS_PREFIX_UDP: &str = "_ts3._udp.";
const NICKNAME_LOOKUP_ADDRESS: &str = "https://named.myteamspeak.com/lookup?name=";
// TODO Dynamic timeout with exponential backoff starting at 1s
/// Wait this amount of seconds before giving up.
const TIMEOUT_SECONDS: u64 = 10;
// TODO global resolve timeout of 30s

enum ParseIpResult<'a> {
	Addr(SocketAddr),
	Other(&'a str, Option<u16>),
}

/// Pick the first stream which does not return an error or is empty.
///
/// # Panic
///
/// Panicks when polled without child streams.
struct StreamCombiner<S: Stream> {
	streams: Vec<S>,
}

impl<S: Stream> StreamCombiner<S> {
	fn new(streams: Vec<S>) -> Self {
		Self { streams }
	}
}

impl<S: Stream> Stream for StreamCombiner<S> {
	type Item = S::Item;
	type Error = S::Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		if self.streams.is_empty() {
			panic!("StreamCombiner has no children");
		}
		loop {
			let res = self.streams[0].poll();
			match res {
				Ok(Async::NotReady) => return res,
				Ok(Async::Ready(Some(_))) => {
					if self.streams.len() > 1 {
						// Remove all other self.streams
						self.streams = vec![self.streams.remove(0)];
					}
					return res;
				}
				_ => {
					if self.streams.len() > 1 {
						self.streams.remove(0);
					} else {
						return res;
					}
				}
			}
		}
	}
}

/// Beware that this may be slow because it tries all available methods.
///
/// The following methods are tried:
/// 1. If the address is an ip, the ip is returned
/// 1. Server nickname
/// 1. The SRV record at `_ts3._udp.address`
/// 1. The SRV record at `_tsdns._tcp.address` to get the address of a tsdns
///    server
/// 1. Connect directly to a tsdns server at `address`
/// 1. Directly resolve the address to an ip address
///
/// To get the address of the tsdns server, all subdomains are tried.
/// E.g. for my.cool.subdomain.from.de:
/// - from.de
/// - subdomain.from.de
/// - cool.subdomain.from.de
///
/// If a port is given with `:port`, it overwrites the automatically determined
/// port.
pub fn resolve(logger: Logger, handle: Handle, address: &str) -> impl Stream<Item = SocketAddr, Error = Error> {
	let addr;
	let port;
	match parse_ip(address) {
		Ok(ParseIpResult::Addr(res)) => {
			let res: Box<Stream<Item=_, Error=_>> = Box::new(stream::once(Ok(res)));
			return res;
		}
		Ok(ParseIpResult::Other(a, p)) => {
			addr = a.to_string();
			port = p;
		}
		Err(res) => return Box::new(stream::once(Err(res))),
	}

	let p = port.clone();
	let nickname_res = (|| -> Box<Stream<Item=_, Error=_>> {
		if !addr.contains('.') && addr != "localhost" {
			// Could be a server nickname
			Box::new(resolve_nickname(&addr).map(move |mut addr| {
				if let Some(port) = p {
					addr.set_port(port);
				}
				addr
			}))
		} else {
			Box::new(stream::once(Err(format_err!("Not a valid nickname").into())))
		}
	})();

	let (resolver, background) = match AsyncResolver::from_system_conf() {
		Ok(r) => r,
		Err(e) => return Box::new(stream::once(Err(e.into()))),
	};
	handle.spawn(background);
	let resolve = resolver.clone();
	let address = addr.clone();
	let srv_res = stream::futures_ordered(Some(future::lazy(move || -> Result<_> {
		let resolver = resolve.clone();
		let addr = address;
		// Try to get the address by an SRV record
		let prefix = Name::from_str(DNS_PREFIX_UDP).map_err(|e| format_err!("Canot parse udp domain prefix ({:?})", e))?;
		let mut name = Name::from_str(&addr).map_err(|e| format_err!("Cannot parse domain ({:?})", e))?;
		name.set_fqdn(true);

		Ok(resolve_srv(resolver.clone(), &prefix.append_name(&name)))
	}))).flatten();

	let address = addr.clone();
	let tsdns_srv_res = stream::futures_ordered(Some(future::lazy(move || {
		let addr = address;
		// Try to get the address of a tsdns server by an SRV record
		let prefix = Name::from_str(DNS_PREFIX_TCP).map_err(|e| format_err!("Canot parse tcp domain prefix ({:?})", e))?;
		let mut name = match Name::from_str(&addr) {
			Ok(r) => r,
			Err(e) => {
				bail!("Cannot parse domain ({:?})", e);
			}
		};
		name.set_fqdn(true);

		// Split domain to a list of subdomains
		let streams = (2..name.num_labels()).map(|i| {
			let name = name.trim_to(i as usize);
			let addr = addr.clone();
			let prefix = prefix.clone();
			stream::futures_ordered(Some(resolve_srv(resolver.clone(), &prefix.append_name(&name)).into_future()
				.map_err(|(e, _)| Error::from(e))
				.and_then(move |(srv, _)| {
				if let Some(srv) = srv {
					// Got tsdns server
					let port = port.clone();
					Ok(resolve_tsdns(srv, &addr).map(move |mut addr| {
						if let Some(port) = port {
							// Overwrite port if it was specified
							addr.set_port(port);
						}
						addr
					}))
				} else {
					Err(format_err!("Got no tsdns SRV record").into())
				}
			}))).flatten()
		}).collect();

		// Pick the first srv record of the first server that answers
		Ok(StreamCombiner::new(streams))
	}))).flatten();

	let address = addr.clone();
	let tsdns_direct_res = stream::futures_ordered(Some(future::lazy(move || {
		let addr = address;
		let mut name = match Name::from_str(&addr) {
			Ok(r) => r,
			Err(e) => bail!("Cannot parse domain ({:?})", e),
		};
		name.set_fqdn(true);

		// Try connecting to the tsdns service directly
		let streams = (2..name.num_labels()).map(|i| {
			let name = name.trim_to(i as usize);
			let addr = addr.clone();
			let port = port.clone();
			stream::futures_ordered(Some(resolve_hostname(name.to_string(), TSDNS_DEFAULT_PORT).map(move |srv| {
				let port = port.clone();
				Box::new(resolve_tsdns(srv, &addr).map(move |mut addr| {
					if let Some(port) = port {
						addr.set_port(port);
					}
					addr
				}))
			}).collect().map(StreamCombiner::new))).flatten()
		}).collect();

		// Pick the first answering server
		Ok(StreamCombiner::new(streams))
	}))).flatten();

	let last_res = stream::futures_ordered(Some(
		// Interpret as normal address and resolve with system resolver
		Ok(resolve_hostname(addr, port.unwrap_or(DEFAULT_PORT))) as Result<_>
	)).flatten();

	let streams = vec![
		nickname_res,
		Box::new(srv_res),
		Box::new(tsdns_srv_res),
		Box::new(tsdns_direct_res),
		Box::new(last_res),
	];

	Box::new(StreamCombiner::new(streams))
	// TODO timeout returns error on stream
		//.select2(Timeout::new(Duration::from_secs(TIMEOUT_SECONDS), &handle)
			//.expect("Failed to create Timeout"))
}

fn parse_ip(address: &str) -> Result<ParseIpResult> {
	let mut addr = address;
	let mut port = None;
	if let Some(pos) = address.rfind(':') {
		// Either with port or IPv6 address
		if address.find(':').unwrap() == pos {
			// Port is appended
			addr = &address[..pos];
			port = Some(&address[pos..]);
			if addr.chars().all(|c| c.is_digit(10) || c == '.') {
				// IPv4 address
				return Ok(ParseIpResult::Addr((addr, DEFAULT_PORT).to_socket_addrs()?.next()
					.ok_or_else(|| format_err!("Cannot parse IPv4 address"))?));
			}
		} else if let Some(pos_bracket) = address.rfind(']') {
			if pos_bracket < pos {
				// IPv6 address and port
				return Ok(ParseIpResult::Addr(address.to_socket_addrs()?.next()
					.ok_or_else(|| format_err!("Cannot parse IPv6 address"))?));
			} else {
				// IPv6 address
				return Ok(ParseIpResult::Addr((address, DEFAULT_PORT).to_socket_addrs()?.next()
					.ok_or_else(|| format_err!("Cannot parse IPv6 address"))?));
			}
		} else {
			// IPv6 address
			return Ok(ParseIpResult::Addr((address, DEFAULT_PORT).to_socket_addrs()?.next()
				.ok_or_else(|| format_err!("Cannot parse IPv6 address"))?));
		}
	} else if address.chars().all(|c| c.is_digit(10) || c == '.') {
		// IPv4 address
		return Ok(ParseIpResult::Addr((address, DEFAULT_PORT).to_socket_addrs()?.next()
			.ok_or_else(|| format_err!("Cannot parse IPv4 address"))?));
	}
	let port = if let Some(port) = port.map(|p| p.parse()
		.map_err(|e| format_err!("Cannot parse port ({:?})", e))) {
		Some(port?)
	} else {
		None
	};
	Ok(ParseIpResult::Other(addr, port))
}

pub fn resolve_nickname(nickname: &str) -> impl Stream<Item = SocketAddr, Error = Error> {
	stream::once(unimplemented!())
}

pub fn resolve_tsdns(server: SocketAddr, addr: &str) -> impl Stream<Item = SocketAddr, Error = Error> {
	stream::once(unimplemented!())
}

fn resolve_srv(resolver: AsyncResolver, addr: &Name)
	-> impl Stream<Item = SocketAddr, Error = Error> {
	stream::futures_ordered(Some(
		resolver.lookup_srv(addr).from_err::<Error>().map(|lookup| -> Box<Stream<Item = _, Error = Error>> {
			let mut entries = Vec::new();
			let mut max_prio = if let Some(e) = lookup.iter().next() {
				e.priority()
			} else {
				return Box::new(stream::once(Err(format_err!("Found no SRV entry").into())));
			};

			// Move all SRV records into entries and only retain the ones with the
			// lowest priority.
			for srv in lookup.iter() {
				if srv.priority() > max_prio {
					max_prio = srv.priority();
					entries.clear();
					entries.push(srv);
				} else if srv.priority() == max_prio {
					entries.push(srv);
				}
			}

			// Select by weight
			let mut sorted_entries = Vec::new();
			while !entries.is_empty() {
				let weight: u32 = entries.iter().map(|e| e.weight() as u32).sum();
				let mut rng = thread_rng();
				let mut w = rng.gen_range(0, weight + 1);
				if w == 0 {
					// Pick the first entry with weight 0
					if let Some(i) = entries.iter().position(|e| e.weight() == 0) {
						sorted_entries.push(entries.remove(i));
					}
				}
				for i in 0..entries.len() {
					let weight = entries[i].weight() as u32;
					if w <= weight {
						sorted_entries.push(entries.remove(i));
					}
					w -= weight;
				}
			}

			let entries: Vec<_> = sorted_entries.drain(..).map(|e| {
				let addr = e.target().to_string();
				let port = e.port();
				resolve_hostname(addr, port)
			}).collect();
			Box::new(stream::iter_ok::<_, Error>(entries).flatten())
		})
	)).flatten()
}

fn resolve_hostname(name: String, port: u16) -> impl Stream<Item = SocketAddr, Error = Error> {
	stream::futures_ordered(Some(
		blocking_to_future(move || (name.as_str(), port).to_socket_addrs().map_err(Error::from))
			.from_err::<Error>().flatten()))
		.map(|addrs| stream::futures_ordered(addrs.map(Ok)))
		.flatten()
}

fn blocking_to_future<R: 'static + Send, F: FnOnce() -> R + 'static + Send>(f: F)
	-> impl Future<Item = R, Error = oneshot::Canceled> {
	future::lazy(|| {
		let (send, recv) = oneshot::channel();
		thread::spawn(move || {
			send.send(f())
		});
		recv
	})
}
