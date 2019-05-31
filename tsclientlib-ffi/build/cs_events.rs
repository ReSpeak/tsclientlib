use std::collections::HashMap;
use std::default::Default;
use std::ops::Deref;

use t4rust_derive::Template;
use tsproto_structs::book::*;

#[derive(Template)]
#[TemplatePath = "build/CsEvents.tt"]
#[derive(Debug)]
pub struct CsEventDeclarations<'a>(&'a BookDeclarations, HashMap<String, IdPath>);

/// Id values of a struct
#[derive(Clone, Debug)]
struct IdPath {
	/// Amount of values to use for the struct id
	id_count: usize,
	ids: Vec<String>,
}

impl<'a> Deref for CsEventDeclarations<'a> {
	type Target = BookDeclarations;
	fn deref(&self) -> &Self::Target { &self.0 }
}

impl Default for CsEventDeclarations<'static> {
	fn default() -> Self {
		let mut paths = HashMap::new();
		paths.insert("Connection".into(), IdPath {
			id_count: 0,
			ids: vec![],
		});

		paths.insert("Channel".into(), IdPath {
			id_count: 1,
			ids: vec![
				"(ulong) ConnectionPropertyId.Server".into(),
				"(ulong) ServerPropertyId.Channels".into(),
				"i0".into(),
			],
		});

		paths.insert("OptionalChannelData".into(), IdPath {
			id_count: 1,
			ids: vec![
				"(ulong) ConnectionPropertyId.Server".into(),
				"(ulong) ServerPropertyId.Channels".into(),
				"i0".into(),
				"(ulong) ChannelPropertyId.OptionalData".into(),
			],
		});

		paths.insert("Client".into(), IdPath {
			id_count: 1,
			ids: vec![
				"(ulong) ConnectionPropertyId.Server".into(),
				"(ulong) ServerPropertyId.Clients".into(),
				"i0".into(),
			],
		});

		for &(s, id) in &[("OptionalClientData", "OptionalData"),
			("ConnectionClientData", "ConnectionData")] {
			paths.insert(s.into(), IdPath {
				id_count: 1,
				ids: vec![
					"(ulong) ConnectionPropertyId.Server".into(),
					"(ulong) ServerPropertyId.Clients".into(),
					"i0".into(),
					format!("(ulong) ClientPropertyId.{}", id),
				],
			});
		}

		paths.insert("ChatEntry".into(), IdPath {
			id_count: 0,
			ids: vec![],
		});

		paths.insert("File".into(), IdPath {
			id_count: 0,
			ids: vec![],
		});

		paths.insert("Server".into(), IdPath {
			id_count: 0,
			ids: vec!["(ulong) ConnectionPropertyId.Server".into()],
		});

		for &(s, id) in &[("OptionalServerData", "OptionalData"),
			("ConnectionServerData", "ConnectionData")] {
			paths.insert(s.into(), IdPath {
				id_count: 1,
				ids: vec![
					"(ulong) ConnectionPropertyId.Server".into(),
					format!("(ulong) ServerPropertyId.{}", id),
				],
			});
		}

		paths.insert("ServerGroup".into(), IdPath {
			id_count: 1,
			ids: vec![
				"(ulong) ConnectionPropertyId.Server".into(),
				"(ulong) ServerPropertyId.Groups".into(),
				"i0".into(),
			],
		});

		CsEventDeclarations(&DATA, paths)
	}
}

pub fn get_property_name(p: &Property) -> &str {
	if p.modifier.is_some() && p.name.ends_with('s') {
		&p.name[..p.name.len() - 1]
	} else {
		&p.name
	}
}

pub fn get_properties<'a>(
	structs: &'a [Struct],
	s: &'a Struct,
) -> Vec<&'a Property>
{
	s.properties
		.iter()
		.filter(|p| !structs.iter().any(|s| s.name == p.type_s))
		.collect()
}
