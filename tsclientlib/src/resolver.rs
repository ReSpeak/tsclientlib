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

use futures::{Future, future};
use rand::{thread_rng, Rng};
use trust_dns_proto::rr::RecordType;
use trust_dns_proto::rr::record_data::RData;
use trust_dns_resolver::{AsyncResolver, Name};
//use trust_dns::client::{Client, ClientFuture, ClientHandle};
//use trust_dns::rr::{DNSClass, Name, RData, Record, RecordType};
//use trust_dns::rr::rdata::key::KEY;

use {Error, Result};

const RESOLVER_ADDR: ([u8; 4], u16) = ([8,8,8,8], 53);
const DEFAULT_PORT: u16 = 9987;
const DNS_PREFIX_TCP: &str = "_tsdns._tcp.";
const DNS_PREFIX_UDP: &str = "_ts3._udp.";
const NICKNAME_LOOKUP_ADDRESS: &str = "https://named.myteamspeak.com/lookup?name=";
/// After this timeout, the next resolving method is tried.
const FIRST_TIMEOUT_SECONDS: u64 = 1;
/// Wait this amount of seconds in the end, before giving up.
const TIMEOUT_SECONDS: u64 = 5;

// Return future
/// Beware that this may be slow because it tries all available methods.
pub fn resolve(address: &str) -> Result<SocketAddr> {
	// TODO Return better error types
	let mut addr = address;
	let mut port = None;
	if let Some(pos) = address.rfind(':') {
		// Either with port or IPv6 address
		if address.find(':').unwrap() == pos {
			// Port is appended
			addr = &address[..pos];
			port = Some(&address[pos..]);
		} else if let Some(pos_bracket) = address.rfind(']') {
			if pos_bracket < pos {
				// IPv6 address and port
				return Ok(address.to_socket_addrs()?.next()
					.ok_or_else(|| format_err!("Cannot parse IPv6 address"))?);
			} else {
				// IPv6 address
				return Ok((address, DEFAULT_PORT).to_socket_addrs()?.next()
					.ok_or_else(|| format_err!("Cannot parse IPv6 address"))?);
			}
		} else {
			// IPv6 address
			return Ok((address, DEFAULT_PORT).to_socket_addrs()?.next()
				.ok_or_else(|| format_err!("Cannot parse IPv6 address"))?);
		}
	}

	if !addr.contains('.') && addr != "localhost" {
		// Could be a server nickname
		match resolve_nickname(addr) {
			Ok(addr) => return Ok(SocketAddr::new(addr, port.map(|p| p.parse())
				.unwrap_or(Ok(DEFAULT_PORT))
				.map_err(|e| format_err!("Cannot parse port ({:?})", e))?)),
			Err(_) => {}
		}
	}

	let orig_port = port;
	let port = orig_port.map(|p| p.parse()).unwrap_or(Ok(DEFAULT_PORT))
		.map_err(|e| format_err!("Cannot parse port ({:?})", e))?;

	let (resolver, background) = AsyncResolver::from_system_conf()?;
	//handle.spawn(background);

	// Try to get the address by an SRV record
	//if let Ok(res) = resolve_srv(resolver.clone(), &format!("{}{}.", DNS_PREFIX_UDP, addr)) {
	//}

	// Try to get the address of a tsdns server by an SRV record
	let name = Name::parse(&format!("{}.", addr), None)?;
	// split domain to get a list of subdomains, for e.g.:
	// my.cool.subdomain.from.de
	// => from.de
	// => subdomain.from.de
	// => cool.subdomain.from.de
	for i in 2..name.num_labels() {
		let name = name.trim_to(i as usize);
		//if let Ok(res) = resolve_srv(resolver.clone(), &format!("{}{}.", DNS_PREFIX_TCP, name)) {
			// Got tsdns server
			return resolve_tsdns(addr).map(|mut addr| {
				if orig_port.is_some() {
					// Overwrite port if it was specified
					addr.set_port(port);
				}
				addr
			});
		//}
	}

	// Try resolve to the tsdns service directly
	for i in 2..name.num_labels() {
		let name = name.trim_to(i as usize);
		return resolve_tsdns(addr).map(|mut addr| {
			if orig_port.is_some() {
				addr.set_port(port);
			}
			addr
		});
	}

	// Interpret as normal address and resolve with system resolver
	Ok((addr, port).to_socket_addrs()?
	   .next()
	   .ok_or_else(|| format_err!("Unable to resolve address"))?)
}

// TODO Return future
pub fn resolve_nickname(nickname: &str) -> Result<IpAddr> {
	unimplemented!()
}

pub fn resolve_tsdns(addr: &str) -> Result<SocketAddr> {
	unimplemented!()
}

fn resolve_srv(resolver: AsyncResolver, addr: &str)
	-> impl Future<Item = SocketAddr, Error = Error> {
	resolver.lookup_srv(addr).from_err::<Error>().and_then(|lookup| {
		let mut entries = Vec::new();
		let mut max_prio = lookup.iter().next()
			.ok_or_else(|| format_err!("Found no SRV entry"))?.priority();

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

		if entries.len() == 1 {
			return Ok((entries[0].target().to_string(), entries[0].port()));
		}

		// Select by weight
		let weight: u32 = entries.iter().map(|e| e.weight() as u32).sum();
		let mut rng = thread_rng();
		let mut w = rng.gen_range(0, weight + 1);
		if w == 0 {
			// Pick the first entry with weight 0
			if let Some(e) = entries.iter().filter(|e| e.weight() == 0).next() {
				return Ok((e.target().to_string(), e.port()));
			}
		}
		for e in &entries {
			if w <= e.weight() as u32 {
				return Ok((e.target().to_string(), e.port()));
			}
			w -= e.weight() as u32;
		}
		unreachable!("Choosing the SRV record failed");
	}).and_then(move |(addr, port)| {
		// Get the ip
		resolver.lookup_ip(addr.as_str()).map(move |r| (r, port)).from_err()
	}).and_then(|(lookup, port)| {
		// TODO take them all, we also need the ipv4 address if we cannot connect using ipv6
		Ok((lookup.iter().next().ok_or_else(|| format_err!("Found no IP address"))?, port).into())
	})
}
