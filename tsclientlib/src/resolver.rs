//! Resolve TeamSpeak server addresses of any kind.
// Changes with TeamSpeak client 3.1:
// https://support.teamspeakusa.com/index.php?/Knowledgebase/Article/View/332

use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::str::{self, FromStr};

use futures::prelude::*;
use hickory_resolver::config::{ResolverConfig, ResolverOpts};
use hickory_resolver::{Name, TokioAsyncResolver};
use itertools::Itertools;
use rand::Rng;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{self, TcpStream};
use tokio::time::Duration;
use tracing::{debug, instrument, warn};

const DEFAULT_PORT: u16 = 9987;
const DNS_PREFIX_TCP: &str = "_tsdns._tcp.";
const DNS_PREFIX_UDP: &str = "_ts3._udp.";
const NICKNAME_LOOKUP_ADDRESS: &str = "https://named.myteamspeak.com/lookup";
/// Wait this amount of seconds before giving up.
const TIMEOUT_SECONDS: u64 = 10;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
	#[error("Failed to create resolver ({system})")]
	CreateResolver {
		#[source]
		system: hickory_resolver::error::ResolveError,
	},
	#[error("Failed to composed domain from {0:?} and {1:?}: {2}")]
	InvalidComposedDomain(String, String, #[source] hickory_proto::error::ProtoError),
	#[error("Failed to parse domain {0:?}: {1}")]
	InvalidDomain(String, #[source] hickory_proto::error::ProtoError),
	#[error("Invalid IPv4 address")]
	InvalidIp4Address,
	#[error("Invalid IPv6 address")]
	InvalidIp6Address,
	#[error("Invalid IP address")]
	InvalidIpAddress,
	#[error("Not a valid nickname")]
	InvalidNickname,
	#[error("Failed to parse port: {0}")]
	InvalidPort(#[source] std::num::ParseIntError),
	#[error("Failed to contact {0} server: {1}")]
	Io(&'static str, #[source] std::io::Error),
	#[error("Failed to parse url: {0}")]
	NicknameParseUrl(#[source] url::ParseError),
	#[error("Failed to resolve nickname: {0}")]
	NicknameResolve(#[source] reqwest::Error),
	#[error("Found no SRV entry")]
	NoSrvEntry,
	#[error("Failed to resolve hostname: {0}")]
	ResolveHost(#[source] tokio::io::Error),
	#[error("Failed to get SRV record")]
	SrvLookup(#[source] hickory_resolver::error::ResolveError),
	#[error("tsdns did not return an ip address but {0:?}")]
	TsdnsAddressInvalidResponse(String),
	#[error("tsdns server does not know the address")]
	TsdnsAddressNotFound,
	#[error("Failed to parse tsdns response: {0}")]
	TsdnsParseResponse(#[source] std::str::Utf8Error),
}

#[derive(Debug, PartialEq, Eq)]
enum ParseIpResult<'a> {
	Addr(SocketAddr),
	Other(&'a str, Option<u16>),
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
#[instrument]
pub fn resolve(address: String) -> impl Stream<Item = Result<SocketAddr>> {
	debug!("Starting resolve");
	let addr;
	let port;
	match parse_ip(&address) {
		Ok(ParseIpResult::Addr(res)) => {
			return stream::once(future::ok(res)).left_stream();
		}
		Ok(ParseIpResult::Other(a, p)) => {
			addr = a.to_string();
			port = p;
			if let Some(port) = port {
				debug!(port, "Found port");
			}
		}
		Err(res) => return stream::once(future::err(res)).left_stream(),
	}

	// Resolve as nickname
	let res = if !address.contains('.') && addr != "localhost" {
		debug!("Resolving nickname");
		// Could be a server nickname
		resolve_nickname(address.clone())
			.map_ok(move |mut addr| {
				if let Some(port) = port {
					addr.set_port(port);
				}
				addr
			})
			.left_stream()
	} else {
		stream::once(future::err(Error::InvalidNickname)).right_stream()
	};

	// The system config does not yet work on android:
	// https://github.com/bluejekyll/trust-dns/issues/652
	let addr2 = addr.clone();
	// TODO Move current span into stream
	let res = res.chain(
		stream::once(async move {
			let resolver = create_resolver();

			// Try to get the address by an SRV record
			let prefix = Name::from_str(DNS_PREFIX_UDP).expect("Cannot parse udp domain prefix");
			let mut name =
				Name::from_str(&addr2).map_err(|e| Error::InvalidDomain(addr2.clone(), e))?;
			name.set_fqdn(true);

			Result::<_>::Ok(resolve_srv(
				resolver,
				prefix.append_name(&name).map_err(|e| {
					Error::InvalidComposedDomain(DNS_PREFIX_UDP.to_string(), addr2.clone(), e)
				})?,
			))
		})
		.try_flatten(),
	);

	// Try to get the address of a tsdns server by an SRV record
	let addr2 = addr.clone();
	// TODO Move current span into stream
	let res = res.chain(
		stream::once(async move {
			let resolver = create_resolver();
			let prefix = Name::from_str(DNS_PREFIX_TCP).expect("Cannot parse udp domain prefix");
			let mut name =
				Name::from_str(&addr2).map_err(|e| Error::InvalidDomain(addr2.clone(), e))?;
			name.set_fqdn(true);

			let name = name.trim_to(2);
			// Pick the first srv record of the first server that answers
			Result::<_>::Ok(
				resolve_srv(
					resolver,
					prefix.append_name(&name).map_err(|e| {
						Error::InvalidComposedDomain(DNS_PREFIX_UDP.to_string(), addr2.clone(), e)
					})?,
				)
				.and_then(move |srv| {
					let address = address.clone();
					async move {
						// Got tsdns server
						let mut addr = resolve_tsdns(srv, &address).await?;
						if let Some(port) = port {
							// Overwrite port if it was specified
							addr.set_port(port);
						}
						Ok(addr)
					}
				}),
			)
		})
		.try_flatten(),
	);

	// Interpret as normal address and resolve with system resolver
	let res = res.chain(
		stream::once(async move {
			let res = net::lookup_host((addr.as_str(), port.unwrap_or(DEFAULT_PORT)))
				.await
				.map_err(Error::ResolveHost)?
				.map(Ok)
				.collect::<Vec<_>>();
			Result::<_>::Ok(stream::iter(res))
		})
		.try_flatten(),
	);

	// TODO Move current span into stream
	tokio_stream::StreamExt::timeout(res, Duration::from_secs(TIMEOUT_SECONDS))
		.filter_map(move |r: std::result::Result<Result<SocketAddr>, _>| {
			future::ready(match r {
				// Timeout
				Err(_) => None,
				// Error
				Ok(Err(error)) => {
					debug!(%error, "Resolver failed in one step");
					None
				}
				// Success
				Ok(Ok(r)) => Some(Ok(r)),
			})
		})
		.right_stream()
}

// Windows for some reason automatically adds a link-local address to the dns
// resolver. These addresses are usually not reachable and should be filtered out.
// See: https://superuser.com/questions/638566/strange-value-in-dns-shown-in-ipconfig
const FILTERED_IPS: &[IpAddr] = &[
	IpAddr::V6(Ipv6Addr::new(0xfec0, 0, 0, 0xffff, 0, 0, 0, 1)),
	IpAddr::V6(Ipv6Addr::new(0xfec0, 0, 0, 0xffff, 0, 0, 0, 2)),
	IpAddr::V6(Ipv6Addr::new(0xfec0, 0, 0, 0xffff, 0, 0, 0, 3)),
];

fn create_resolver() -> TokioAsyncResolver {
	let (config, options) = create_resolver_config();
	TokioAsyncResolver::tokio(config, options)
}

fn create_resolver_config() -> (ResolverConfig, ResolverOpts) {
	match hickory_resolver::system_conf::read_system_conf() {
		Ok(r) => {
			let mut rc = ResolverConfig::from_parts(
				None,
				vec![],
				hickory_resolver::config::NameServerConfigGroup::new(),
			);
			for ns in
				r.0.name_servers().iter().filter(|ns| !FILTERED_IPS.contains(&ns.socket_addr.ip()))
			{
				rc.add_name_server(ns.clone());
			}
			(rc, r.1)
		}
		Err(error) => {
			warn!(%error, "Failed to use system dns resolver config");
			// Fallback
			(ResolverConfig::cloudflare(), ResolverOpts::default())
		}
	}
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
			if addr.chars().all(|c| c.is_ascii_digit() || c == '.') {
				// IPv4 address
				return Ok(ParseIpResult::Addr(
					std::net::ToSocketAddrs::to_socket_addrs(address)
						.map_err(|_| Error::InvalidIp4Address)?
						.next()
						.ok_or(Error::InvalidIp4Address)?,
				));
			}
		} else if let Some(pos_bracket) = address.rfind(']') {
			if pos_bracket < pos {
				// IPv6 address and port
				return Ok(ParseIpResult::Addr(
					std::net::ToSocketAddrs::to_socket_addrs(address)
						.map_err(|_| Error::InvalidIp6Address)?
						.next()
						.ok_or(Error::InvalidIp6Address)?,
				));
			} else if pos_bracket == address.len() - 1 && address.starts_with('[') {
				// IPv6 address
				return Ok(ParseIpResult::Addr(
					std::net::ToSocketAddrs::to_socket_addrs(&(
						&address[1..pos_bracket],
						DEFAULT_PORT,
					))
					.map_err(|_| Error::InvalidIp6Address)?
					.next()
					.ok_or(Error::InvalidIp6Address)?,
				));
			} else {
				return Err(Error::InvalidIpAddress);
			}
		} else {
			// IPv6 address
			return Ok(ParseIpResult::Addr(
				std::net::ToSocketAddrs::to_socket_addrs(&(address, DEFAULT_PORT))
					.map_err(|_| Error::InvalidIp6Address)?
					.next()
					.ok_or(Error::InvalidIp6Address)?,
			));
		}
	} else if address.chars().all(|c| c.is_ascii_digit() || c == '.') {
		// IPv4 address
		return Ok(ParseIpResult::Addr(
			std::net::ToSocketAddrs::to_socket_addrs(&(address, DEFAULT_PORT))
				.map_err(|_| Error::InvalidIp4Address)?
				.next()
				.ok_or(Error::InvalidIp4Address)?,
		));
	}
	let port = if let Some(port) = port.map(|p| p.parse().map_err(Error::InvalidPort)) {
		Some(port?)
	} else {
		None
	};
	Ok(ParseIpResult::Other(addr, port))
}

pub fn resolve_nickname(nickname: String) -> impl Stream<Item = Result<SocketAddr>> {
	stream::once(async {
		let nickname = nickname;
		let url =
			reqwest::Url::parse_with_params(NICKNAME_LOOKUP_ADDRESS, Some(("name", &nickname)))
				.map_err(Error::NicknameParseUrl)?;
		let body = reqwest::get(url)
			.await
			.map_err(Error::NicknameResolve)?
			.error_for_status()
			.map_err(Error::NicknameResolve)?
			.text()
			.await
			.map_err(Error::NicknameResolve)?;
		let addrs = body
			.split(&['\r', '\n'][..])
			.filter(|s| !s.is_empty())
			.map(|s| Result::<_>::Ok(s.to_string()))
			.collect::<Vec<_>>();

		Result::<_>::Ok(
			stream::iter(addrs)
				.and_then(|addr| async move {
					match parse_ip(&addr)? {
						ParseIpResult::Addr(a) => Ok(stream::once(future::ok(a)).left_stream()),
						ParseIpResult::Other(a, p) => {
							let addrs = net::lookup_host((a, p.unwrap_or(DEFAULT_PORT)))
								.await
								.map_err(Error::ResolveHost)?
								.collect::<Vec<_>>();
							Ok(stream::iter(addrs).map(Result::<_>::Ok).right_stream())
						}
					}
				})
				.try_flatten(),
		)
	})
	.try_flatten()
}

pub async fn resolve_tsdns<A: net::ToSocketAddrs>(server: A, addr: &str) -> Result<SocketAddr> {
	let mut stream = TcpStream::connect(server).await.map_err(|e| Error::Io("tsdns", e))?;
	stream.write_all(addr.as_bytes()).await.map_err(|e| Error::Io("tsdns", e))?;
	let mut data = Vec::new();
	stream.read_to_end(&mut data).await.map_err(|e| Error::Io("tsdns", e))?;

	let addr = str::from_utf8(&data).map_err(Error::TsdnsParseResponse)?;
	if addr.starts_with("404") {
		return Err(Error::TsdnsAddressNotFound);
	}
	match parse_ip(addr)? {
		ParseIpResult::Addr(a) => Ok(a),
		_ => Err(Error::TsdnsAddressInvalidResponse(addr.to_string())),
	}
}

fn resolve_srv(resolver: TokioAsyncResolver, addr: Name) -> impl Stream<Item = Result<SocketAddr>> {
	stream::once(async {
		let lookup = resolver.srv_lookup(addr).await.map_err(Error::SrvLookup)?;
		let mut entries = Vec::new();
		let mut max_prio = if let Some(e) = lookup.iter().next() {
			e.priority()
		} else {
			return Err(Error::NoSrvEntry);
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

		let prios = lookup.iter().chunk_by(|e| e.priority());
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
				let weight: u32 = entries.iter().map(|e| e.weight() as u32).sum();
				let mut w = rand::thread_rng().gen_range(0..=weight);
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
						break;
					}
					w -= weight;
				}
			}
		}

		let res = sorted_entries
			.into_iter()
			.map(|e| Ok((e.target().to_ascii(), e.port())))
			.collect::<Vec<Result<(String, u16)>>>();
		drop(resolver);
		Ok(stream::iter(res)
			.and_then(|(e, port)| async move {
				let res = net::lookup_host((e.as_str(), port))
					.await
					.map_err(Error::ResolveHost)?
					.map(Ok)
					.collect::<Vec<_>>();
				Ok(stream::iter(res))
			})
			.try_flatten())
	})
	.try_flatten()
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::tests::create_logger;

	#[test]
	fn parse_ip_without_port() {
		let res = parse_ip("127.0.0.1");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr(format!("127.0.0.1:{}", DEFAULT_PORT).parse().unwrap())
		);
	}

	#[test]
	fn parse_ip_with_port() {
		let res = parse_ip("127.0.0.1:1");
		assert_eq!(res.unwrap(), ParseIpResult::Addr("127.0.0.1:1".parse().unwrap()));
	}

	#[test]
	fn parse_ip6_without_port() {
		let res = parse_ip("::");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr(format!("[::]:{}", DEFAULT_PORT).parse().unwrap())
		);
	}

	#[test]
	fn parse_ip6_without_port2() {
		let res = parse_ip("[::]");
		assert_eq!(
			res.unwrap(),
			ParseIpResult::Addr(format!("[::]:{}", DEFAULT_PORT).parse().unwrap())
		);
	}

	#[test]
	fn parse_ip6_with_port() {
		let res = parse_ip("[::]:1");
		assert_eq!(res.unwrap(), ParseIpResult::Addr("[::]:1".parse().unwrap()));
	}

	#[test]
	fn parse_ip_address_without_port() {
		assert_eq!(parse_ip("localhost").unwrap(), ParseIpResult::Other("localhost", None));
	}

	#[test]
	fn parse_ip_address_with_port() {
		assert_eq!(parse_ip("localhost:1").unwrap(), ParseIpResult::Other("localhost", Some(1)));
	}

	#[test]
	fn parse_ip_with_large_port() {
		assert!(parse_ip("127.0.0.1:65536").is_err());
	}

	#[tokio::test]
	async fn resolve_localhost() {
		create_logger();
		let res: Vec<_> = resolve("127.0.0.1".into()).map(|r| r.unwrap()).collect().await;
		let addr = format!("127.0.0.1:{}", DEFAULT_PORT).parse::<SocketAddr>().unwrap();
		assert_eq!(res.as_slice(), &[addr]);
	}

	#[tokio::test]
	async fn resolve_localhost2() {
		create_logger();
		let res: Vec<_> = resolve("localhost".into()).map(|r| r.unwrap()).collect().await;
		assert!(res.contains(&format!("127.0.0.1:{}", DEFAULT_PORT).parse().unwrap()));
	}

	#[tokio::test]
	async fn resolve_example() {
		create_logger();
		let res: Vec<_> = resolve("example.com".into()).map(|r| r.unwrap()).collect().await;
		assert!(!res.is_empty());
	}

	#[tokio::test]
	async fn resolve_splamy_de() {
		create_logger();

		let res: Vec<_> = tokio::time::timeout(
			Duration::from_secs(5),
			resolve("splamy.de".into()).map(|r| r.unwrap()).collect(),
		)
		.await
		.expect("Resolve takes unacceptable long");
		assert!(res.contains(&format!("37.120.179.68:{}", DEFAULT_PORT).parse().unwrap()));
	}

	#[tokio::test]
	async fn resolve_loc() {
		create_logger();
		let res: Vec<_> = resolve("loc".into()).map(|r| r.unwrap()).collect().await;
		assert!(res.contains(&format!("127.0.0.1:{}", DEFAULT_PORT).parse().unwrap()));
	}
}
