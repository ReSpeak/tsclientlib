//! Resolve TeamSpeak server addresses of any kind.
// Changes with TeamSpeak client 3.1:
// https://support.teamspeakusa.com/index.php?/Knowledgebase/Article/View/332

use std::net::{SocketAddr, ToSocketAddrs};
use std::str::{self, FromStr};
use std::time::Duration;

use failure::format_err;
use futures::{future, stream, Async, Future, Poll, Stream};
use futures::sync::oneshot;
use itertools::Itertools;
use rand::{thread_rng, Rng};
use slog::{debug, o, warn, Logger};
use tokio::io;
use tokio::net::TcpStream;
use tokio::util::StreamExt;
use trust_dns_resolver::config::ResolverConfig;
use trust_dns_resolver::{AsyncResolver, Name};

use crate::{Error, Result};

const DEFAULT_PORT: u16 = 9987;
const DNS_PREFIX_TCP: &str = "_tsdns._tcp.";
const DNS_PREFIX_UDP: &str = "_ts3._udp.";
const NICKNAME_LOOKUP_ADDRESS: &str = "https://named.myteamspeak.com/lookup";
// TODO Dynamic timeout with exponential backoff starting at 1s
/// Wait this amount of seconds before giving up.
const TIMEOUT_SECONDS: u64 = 10;

#[derive(Debug, PartialEq, Eq)]
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
	fn new(streams: Vec<S>) -> Self { Self { streams } }
}

impl<S: Stream> Stream for StreamCombiner<S> {
	type Item = S::Item;
	type Error = S::Error;

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		if self.streams.is_empty() {
			return Ok(Async::Ready(None));
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
/// 1. Server nicknames are resolved by a http request to TeamSpeak
/// 1. The SRV record at `_ts3._udp.<address>`
/// 1. The SRV record at `_tsdns._tcp.address.tld` to get the address of a tsdns
///    server, e.g. when the address is `ts3.subdomain.from.com`, the SRV record
///    at `_tsdns._tcp.from.com` is requested
/// 1. Directly resolve the address to an ip address
///
/// If a port is given with `:port`, it overwrites the automatically determined
/// port. IPv6 addresses are put in square brackets when a port is present:
/// `[::1]:9987`
pub fn resolve(
	logger: &Logger,
	address: &str,
) -> Box<Stream<Item = SocketAddr, Error = Error> + Send>
{
	let logger = logger.new(o!("module" => "resolver"));
	debug!(logger, "Starting resolve"; "address" => address);
	let addr;
	let port;
	match parse_ip(address) {
		Ok(ParseIpResult::Addr(res)) => {
			let res: Box<Stream<Item = _, Error = _> + Send> =
				Box::new(stream::once(Ok(res)));
			return res;
		}
		Ok(ParseIpResult::Other(a, p)) => {
			addr = a.to_string();
			port = p;
			if let Some(port) = port {
				debug!(logger, "Found port"; "port" => port);
			}
		}
		Err(res) => return Box::new(stream::once(Err(res))),
	}

	let p = port.clone();
	let address = addr.clone();
	let logger2 = logger.clone();
	let nickname_res = (move || -> Box<Stream<Item = _, Error = _> + Send> {
		let addr = address;
		if !addr.contains('.') && addr != "localhost" {
			let addr2 = addr.clone();
			debug!(logger2, "Resolving nickname"; "address" => addr2);
			// Could be a server nickname
			Box::new(resolve_nickname(addr).map(move |mut addr| {
				if let Some(port) = p {
					addr.set_port(port);
				}
				addr
			}))
		} else {
			Box::new(stream::once(Err(
				format_err!("Not a valid nickname").into()
			)))
		}
	})();

	// The system config does not yet work on android:
	// https://github.com/bluejekyll/trust-dns/issues/652
	let resolver = match AsyncResolver::from_system_conf() {
		Ok((resolver, background)) => {
			tokio::spawn(background);
			resolver
		}
		Err(e) => {
			warn!(logger, "Failed to use system dns resolver config";
				"error" => ?e);
			// Fallback
			let (resolver, background) = AsyncResolver::new(
				ResolverConfig::cloudflare(),
				Default::default(),
			);
			tokio::spawn(background);
			resolver
		}
	};

	let resolve = resolver.clone();
	let address = addr.clone();
	let srv_res =
		stream::futures_ordered(Some(future::lazy(move || -> Result<_> {
			let resolver = resolve.clone();
			let addr = address;
			// Try to get the address by an SRV record
			let prefix = Name::from_str(DNS_PREFIX_UDP).map_err(|e| {
				format_err!("Canot parse udp domain prefix ({:?})", e)
			})?;
			let mut name = Name::from_str(&addr)
				.map_err(|e| format_err!("Cannot parse domain ({:?})", e))?;
			name.set_fqdn(true);

			Ok(resolve_srv(resolver.clone(), prefix.append_name(&name)))
		})))
		.flatten();

	let address = addr.clone();
	let tsdns_srv_res = stream::futures_ordered(Some(future::lazy(
		move || -> Box<Future<Item = _, Error = Error> + Send> {
			let addr = address;
			// Try to get the address of a tsdns server by an SRV record
			let prefix = match Name::from_str(DNS_PREFIX_TCP) {
				Ok(r) => r,
				Err(e) => {
					return Box::new(future::err(
						format_err!("Cannot parse tcp domain prefix ({:?})", e)
							.into(),
					));
				}
			};
			let mut name = match Name::from_str(&addr) {
				Ok(r) => r,
				Err(e) => {
					return Box::new(future::err(
						format_err!("Cannot parse domain ({:?})", e).into(),
					));
				}
			};
			name.set_fqdn(true);

			let name = name.trim_to(2);
			// Pick the first srv record of the first server that answers
			Box::new(
				resolve_srv_raw(resolver.clone(), prefix.append_name(&name))
					.map(move |srvs| {
						StreamCombiner::new(
							srvs.into_iter()
								.map(|addrs| {
									// Got tsdns server
									let addr = addr.clone();
									let port = port.clone();
									addrs
										.map(move |srv| {
											let port = port.clone();
											resolve_tsdns(srv, addr.clone())
												.map(move |mut addr| {
													if let Some(port) = port {
														// Overwrite port if it was specified
														addr.set_port(port);
													}
													addr
												})
										})
										.flatten()
								})
								.collect(),
						)
					}),
			)
		},
	)))
	.flatten();

	let last_res = stream::futures_ordered(Some(
		// Interpret as normal address and resolve with system resolver
		Ok(resolve_hostname(addr, port.unwrap_or(DEFAULT_PORT))) as Result<_>,
	))
	.flatten();

	let streams = vec![
		nickname_res,
		Box::new(srv_res),
		Box::new(tsdns_srv_res),
		Box::new(last_res),
	];

	Box::new(
		StreamCombiner::new(streams)
			.timeout(Duration::from_secs(TIMEOUT_SECONDS))
			.map_err(|e| {
				e.into_inner()
					.unwrap_or_else(|| format_err!("Resolve timed out").into())
			}),
	)
}

fn parse_ip(address: &str) -> Result<ParseIpResult> {
	let mut addr = address;
	let mut port = None;
	if let Some(pos) = address.rfind(':') {
		// Either with port or IPv6 address
		if address.find(':').unwrap() == pos {
			// Port is appended
			addr = &address[..pos];
			port = Some(&address[pos + 1..]);
			if addr.chars().all(|c| c.is_digit(10) || c == '.') {
				// IPv4 address
				return Ok(ParseIpResult::Addr(
					address.to_socket_addrs()?.next().ok_or_else(|| {
						format_err!("Cannot parse IPv4 address")
					})?,
				));
			}
		} else if let Some(pos_bracket) = address.rfind(']') {
			if pos_bracket < pos {
				// IPv6 address and port
				return Ok(ParseIpResult::Addr(
					address.to_socket_addrs()?.next().ok_or_else(|| {
						format_err!("Cannot parse IPv6 address")
					})?,
				));
			} else if pos_bracket == address.len() - 1
				&& address.chars().next() == Some('[')
			{
				// IPv6 address
				return Ok(ParseIpResult::Addr(
					(&address[1..pos_bracket], DEFAULT_PORT)
						.to_socket_addrs()?
						.next()
						.ok_or_else(|| {
							format_err!("Cannot parse IPv6 address")
						})?,
				));
			} else {
				return Err(format_err!("Invalid ip address").into());
			}
		} else {
			// IPv6 address
			return Ok(ParseIpResult::Addr(
				(address, DEFAULT_PORT)
					.to_socket_addrs()?
					.next()
					.ok_or_else(|| format_err!("Cannot parse IPv6 address"))?,
			));
		}
	} else if address.chars().all(|c| c.is_digit(10) || c == '.') {
		// IPv4 address
		return Ok(ParseIpResult::Addr(
			(address, DEFAULT_PORT)
				.to_socket_addrs()?
				.next()
				.ok_or_else(|| format_err!("Cannot parse IPv4 address"))?,
		));
	}
	let port = if let Some(port) = port.map(|p| {
		p.parse()
			.map_err(|e| format_err!("Cannot parse port ({:?})", e))
	}) {
		Some(port?)
	} else {
		None
	};
	Ok(ParseIpResult::Other(addr, port))
}

pub fn resolve_nickname(
	nickname: String,
) -> impl Stream<Item = SocketAddr, Error = Error> {
	let url = reqwest::Url::parse_with_params(
		NICKNAME_LOOKUP_ADDRESS,
		Some(("name", &nickname)),
	)
	.expect("Cannot parse nickname lookup address");
	let addrs = reqwest::r#async::Client::new()
		.get(url)
		.send()
		.and_then(|r| r.error_for_status())
		.and_then(|r| r.into_body().concat2())
		.from_err::<Error>()
		.and_then(|body| {
			Ok(str::from_utf8(body.as_ref())?
				.split(&['\r', '\n'][..])
				.filter(|s| !s.is_empty())
				.map(|s| s.to_string())
				.collect::<Vec<_>>())
		});

	stream::futures_ordered(Some(
		addrs
		.map(|addrs| {
			stream::futures_ordered(addrs.iter().map(
				|addr| -> Result<Box<Stream<Item = _, Error = _> + Send>> {
					match parse_ip(addr) {
						Err(e) => Ok(Box::new(stream::once(Err(e)))),
						Ok(ParseIpResult::Addr(a)) => {
							Ok(Box::new(stream::once(Ok(a))))
						}
						Ok(ParseIpResult::Other(a, p)) => {
							Ok(Box::new(resolve_hostname(
								a.to_string(),
								p.unwrap_or(DEFAULT_PORT),
							)))
						}
					}
				},
			))
			.flatten()
		}),
	))
	.flatten()
}

pub fn resolve_tsdns(
	server: SocketAddr,
	addr: String,
) -> impl Stream<Item = SocketAddr, Error = Error>
{
	stream::futures_ordered(Some(
		TcpStream::connect(&server)
			.and_then(move |tcp| io::write_all(tcp, addr))
			.and_then(|(tcp, _)| io::read_to_end(tcp, Vec::new()))
			.from_err::<Error>()
			.and_then(|(_, data)| {
				let addr = str::from_utf8(&data)?;
				if addr.starts_with("404") {
					return Err(format_err!(
						"tsdns server does not know the address"
					)
					.into());
				}
				match parse_ip(addr)? {
					ParseIpResult::Addr(a) => Ok(a),
					_ => Err(format_err!("tsdns did not return an ip address")
						.into()),
				}
			}),
	))
}

fn resolve_srv_raw(
	resolver: AsyncResolver,
	addr: Name,
) -> impl Future<
	Item = Vec<impl Stream<Item = SocketAddr, Error = Error>>,
	Error = Error,
>
{
	resolver.lookup_srv(addr).from_err::<Error>().and_then(
		|lookup| -> Result<_> {
			let mut entries = Vec::new();
			let mut max_prio = if let Some(e) = lookup.iter().next() {
				e.priority()
			} else {
				return Err(format_err!("Found no SRV entry").into());
			};

			// Move all SRV records into entries and only retain the ones with
			// the lowest priority.
			for srv in lookup.iter() {
				if srv.priority() < max_prio {
					max_prio = srv.priority();
					entries.clear();
					entries.push(srv);
				} else if srv.priority() == max_prio {
					entries.push(srv);
				}
			}

			let prios = lookup.iter().group_by(|e| e.priority());
			let entries = prios.into_iter().sorted_by_key(|(p, _)| *p);

			// Select by weight
			let mut sorted_entries = Vec::new();
			for (_, es) in entries {
				let mut zero_entries = Vec::new();

				// All non-zero entries
				let mut entries = es
					.filter_map(|e| {
						if e.weight() == 0 {
							zero_entries.push(e);
							None
						} else {
							Some(e)
						}
					})
					.collect::<Vec<_>>();

				while !entries.is_empty() {
					let weight: u32 =
						entries.iter().map(|e| e.weight() as u32).sum();
					let mut rng = thread_rng();
					let mut w = rng.gen_range(0, weight + 1);
					if w == 0 {
						// Pick the first entry with weight 0
						if let Some(i) =
							entries.iter().position(|e| e.weight() == 0)
						{
							sorted_entries.push(entries.remove(i));
						}
					}
					for i in 0..entries.len() {
						let weight = entries[i].weight() as u32;
						if w <= weight {
							sorted_entries.push(entries.remove(i));
							break;
						}
						w -= weight;
					}
				}
			}

			let entries: Vec<_> = sorted_entries
				.into_iter()
				.map(|e| {
					let addr = e.target().to_string();
					let port = e.port();
					resolve_hostname(addr, port)
				})
				.collect();
			Ok(entries)
		},
	)
}

fn resolve_srv(
	resolver: AsyncResolver,
	addr: Name,
) -> impl Stream<Item = SocketAddr, Error = Error>
{
	stream::futures_ordered(Some(
		resolve_srv_raw(resolver, addr).map(StreamCombiner::new),
	))
	.flatten()
}

fn resolve_hostname(
	name: String,
	port: u16,
) -> impl Stream<Item = SocketAddr, Error = Error>
{
	let (send, recv) = oneshot::channel();
	std::thread::spawn(move || {
		// Ignore error if the future was canceled
		let _ = send.send((name.as_str(), port).to_socket_addrs()
			.map(|i| i.collect::<Vec<_>>()));
	});

	stream::futures_ordered(Some(recv))
		.from_err()
		.and_then(|r| r.map_err(Error::from))
		.map(|addrs| stream::futures_ordered(addrs.into_iter().map(Ok)))
		.flatten()
}

#[cfg(test)]
mod test {
	use slog::Drain;
	use tokio::runtime::Runtime;

	use super::*;

	fn setup() -> (Logger, Runtime) {
		let rt = Runtime::new().unwrap();
		let logger = {
			let decorator =
				slog_term::PlainSyncDecorator::new(std::io::stdout());
			let drain = slog_term::CompactFormat::new(decorator).build().fuse();
			let drain = slog_async::Async::new(drain).build().fuse();

			slog::Logger::root(drain, o!())
		};
		(logger, rt)
	}

	#[test]
	fn parse_ip_without_port() {
		let res = parse_ip("127.0.0.1");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr(
				format!("127.0.0.1:{}", DEFAULT_PORT).parse().unwrap()
			)
		);
	}

	#[test]
	fn parse_ip_with_port() {
		let res = parse_ip("127.0.0.1:1");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr("127.0.0.1:1".parse().unwrap())
		);
	}

	#[test]
	fn parse_ip6_without_port() {
		let res = parse_ip("::");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr(
				format!("[::]:{}", DEFAULT_PORT).parse().unwrap()
			)
		);
	}

	#[test]
	fn parse_ip6_without_port2() {
		let res = parse_ip("[::]");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr(
				format!("[::]:{}", DEFAULT_PORT).parse().unwrap()
			)
		);
	}

	#[test]
	fn parse_ip6_with_port() {
		let res = parse_ip("[::]:1");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr("[::]:1".parse().unwrap())
		);
	}

	#[test]
	fn parse_ip_address_without_port() {
		let res = parse_ip("localhost");
		assert_eq!(res.unwrap(), ParseIpResult::Other("localhost", None));
	}

	#[test]
	fn parse_ip_address_with_port() {
		let res = parse_ip("localhost:1");
		assert_eq!(res.unwrap(), ParseIpResult::Other("localhost", Some(1)));
	}

	#[test]
	fn parse_ip_with_large_port() {
		let res = parse_ip("127.0.0.1:65536");
		assert!(res.is_err());
	}

	#[test]
	fn parse_ip_bad() {
		assert!(parse_ip("").is_err());
		assert!(parse_ip(":1").is_err());
		assert!(parse_ip(":").is_err());
		assert!(parse_ip("127.0.0.1:").is_err());
	}

	#[test]
	fn resolve_localhost() {
		let (logger, mut rt) = setup();
		let res = rt.block_on(future::lazy(move || {
			resolve(&logger, "127.0.0.1").collect()
		}));
		assert_eq!(
			res.unwrap().as_slice(),
			&[format!("127.0.0.1:{}", DEFAULT_PORT).parse().unwrap()]
		);
	}

	#[test]
	fn resolve_localhost2() {
		let (logger, mut rt) = setup();
		let res = rt.block_on(future::lazy(move || {
			resolve(&logger, "localhost").collect()
		}));
		assert!(res
			.unwrap()
			.contains(&format!("127.0.0.1:{}", DEFAULT_PORT).parse().unwrap()));
	}

	#[test]
	fn resolve_example() {
		let (logger, mut rt) = setup();
		let res = rt.block_on(future::lazy(move || {
			resolve(&logger, "example.com").collect()
		}));
		assert!(res.unwrap().contains(
			&format!("93.184.216.34:{}", DEFAULT_PORT).parse().unwrap()
		));
	}

	#[test]
	fn resolve_loc() {
		let (logger, mut rt) = setup();
		let res = rt
			.block_on(future::lazy(move || resolve(&logger, "loc").collect()));
		assert!(res
			.unwrap()
			.contains(&format!("127.0.0.1:{}", DEFAULT_PORT).parse().unwrap()));
	}
}
