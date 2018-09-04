//! Resolve TeamSpeak server addresses of any kind.
//!
//! Using
//! 1. Server nicknames
//! 1. TSDNS (SRV records)
//! 1. Normal DNS (A and AAAA records)
//! 1. The address directly if it is an ip
//!
//! A port is determined automatically, if it is not specified.

use std::net::{IpAddr, SocketAddr, ToSocketAddrs};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

use futures::{future, Future, stream, Stream};
use futures::future::Either;
use futures::sync::oneshot;
use rand::{thread_rng, Rng};
use slog::Logger;
use tokio_core::reactor::{Handle, Timeout};
use trust_dns_resolver::{AsyncResolver, Name};

use {Error, Result};

const RESOLVER_ADDR: ([u8; 4], u16) = ([8,8,8,8], 53);
const DEFAULT_PORT: u16 = 9987;
const DNS_PREFIX_TCP: &str = "_tsdns._tcp.";
const DNS_PREFIX_UDP: &str = "_ts3._udp.";
const NICKNAME_LOOKUP_ADDRESS: &str = "https://named.myteamspeak.com/lookup?name=";
/// Wait this amount of seconds before giving up.
const TIMEOUT_SECONDS: u64 = 10;
// TODO global resolve timeout of 30s

enum ParseIpResult<'a> {
	Addr(SocketAddr),
	Other(&'a str, Option<u16>),
}

/// Beware that this may be slow because it tries all available methods.
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

	Box::new(stream::futures_ordered(Some(nickname_res.into_future()
		.select2(Timeout::new(Duration::from_secs(TIMEOUT_SECONDS), &handle)
			.expect("Failed to create Timeout"))
		.then(move |r| -> Result<Box<Stream<Item=_, Error=_>>> { match r {
			Ok(Either::A(((Some(addr), stream), _))) =>
				Ok(Box::new(stream::once(Ok(addr)).chain(stream))),
			_ => {
				// Nickname resolution failed
				let (resolver, background) = AsyncResolver::from_system_conf()?;
				handle.spawn(background);

				// Try to get the address by an SRV record
				let prefix = Name::from_str(DNS_PREFIX_UDP).map_err(|e| format_err!("Canot parse udp domain prefix ({:?})", e))?;
				let mut name = Name::from_str(&addr).map_err(|e| format_err!("Cannot parse domain ({:?})", e))?;
				name.set_fqdn(true);

				let srv_res = resolve_srv(resolver.clone(), &prefix.append_name(&name));
				Ok(Box::new(stream::futures_ordered(Some(srv_res.into_future()
					.select2(Timeout::new(Duration::from_secs(TIMEOUT_SECONDS), &handle)
						.expect("Failed to create Timeout"))
					.then(move |r| -> Result<Box<Stream<Item=_, Error=_>>> { match r {
						Ok(Either::A(((Some(addr), stream), _))) =>
							Ok(Box::new(stream::once(Ok(addr)).chain(stream))),
						_ => {
							// Udp SRV resolution failed
							Ok(Box::new(resolve_for_tsdns(logger, handle, addr, port)))
						}
					}})
				)).flatten()))
			}
		}}))).flatten())
}

fn resolve_for_tsdns(logger: Logger, handle: Handle, addr: String, port: Option<u16>)
	-> impl Stream<Item = SocketAddr, Error = Error> {
	// Try to get the address of a tsdns server by an SRV record
	let mut name = match Name::from_str(&addr) {
		Ok(r) => r,
		Err(e) => {
			let res: Box<Stream<Item=_, Error=_>> = Box::new(stream::once(Err(format_err!("Cannot parse domain ({:?})", e).into())));
			return res;
		}
	};
	name.set_fqdn(true);
	// split domain to get a list of subdomains, for e.g.:
	// my.cool.subdomain.from.de
	// => from.de
	// => subdomain.from.de
	// => cool.subdomain.from.de
	(2..name.num_labels()).map(|i| {
		let name = name.trim_to(i as usize);
		//if let Ok(res) = resolve_srv(resolver.clone(), &format!("{}{}.", DNS_PREFIX_TCP, name)) {
			// Got tsdns server
			return resolve_tsdns(&addr).map(|mut addr| {
				if let Some(port) = port {
					// Overwrite port if it was specified
					addr.set_port(port);
				}
				addr
			});
		//}
	});
	unreachable!()

	/*// Try connecting to the tsdns service directly
	for i in 2..name.num_labels() {
		let name = name.trim_to(i as usize);
		return Box::new(resolve_tsdns(&addr).map(|mut addr| {
			if let Some(port) = port {
				addr.set_port(port);
			}
			addr
		}));
	}

	// Interpret as normal address and resolve with system resolver
	Ok((addr.as_str(), port.unwrap_or(DEFAULT_PORT)).to_socket_addrs()?
	   .next()
	   .ok_or_else(|| format_err!("Unable to resolve address"))?)*/
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

pub fn resolve_tsdns(addr: &str) -> impl Stream<Item = SocketAddr, Error = Error> {
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
