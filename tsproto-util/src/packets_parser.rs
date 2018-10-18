// TODO The parsing is outdated, it should be migrated to tsproto_structs.
use std::io;
use std::mem;
use std::str;
use tsproto_structs::unindent;
use *;

type Result<T> = ::std::result::Result<T, io::Error>;

#[derive(Debug)]
struct ElementWrapper {
	elem: Element,
	element_type: ElementType,
}

/// Similar properties of structs and enums, etc. (could be the parent class).
#[derive(Debug)]
struct Element {
	name: String,

	/// (var, value)
	pre_conditions: Vec<(String, String)>,
	/// Code that returns a bool
	pre_code: Vec<String>,
	post_conditions: Vec<(String, String)>,
	post_code: Vec<String>,
}

#[derive(Debug)]
enum ElementType {
	Struct(Struct),
	Enum(Enum),
	Integer(String),
	Array(String),
	Custom(String),
}

#[derive(Debug)]
struct Struct {
	fields: Vec<Field>,
}

#[derive(Debug)]
struct Enum {
	possibilities: Vec<Field>,
}

/// Member of something greater
#[derive(Debug)]
struct Field {
	doc: String,
	value: Option<String>,
	optional: bool,
	content: ElementWrapper,
}

#[derive(Template)]
#[TemplatePath = "src/PacketDeclarations.tt"]
#[derive(Debug)]
pub struct Packets {
	root: Field,
}

impl Declaration for Packets {
	type Dep = ();

	fn get_filename() -> &'static str {
		"Packets.txt"
	}

	fn parse_from_read(read: &mut Read, (): Self::Dep) -> Packets {
		let f = Field::read(&mut io::BufReader::new(read)).unwrap().unwrap();
		Packets { root: f }
	}
}

impl Element {
	fn new(name: String) -> Self {
		Self {
			name,

			pre_conditions: Vec::new(),
			pre_code: Vec::new(),
			post_conditions: Vec::new(),
			post_code: Vec::new(),
		}
	}

	fn get_pre_conditions(&self) -> String {
		let mut res = String::new();
		for &(ref k, ref v) in &self.pre_conditions {
			res.push_str(&format!("{} == {} && ", k, v));
		}
		res.pop();
		res.pop();
		res.pop();
		res
	}

	fn get_pre_code(&self) -> String {
		let mut res = String::new();
		for c in &self.pre_code {
			res.push_str(&format!("{} && ", c));
		}
		res.pop();
		res.pop();
		res.pop();
		res
	}

	fn get_post_conditions(&self) -> String {
		let mut res = String::new();
		for &(ref k, ref v) in &self.post_conditions {
			res.push_str(&format!("{} == {} && ", k, v));
		}
		res.pop();
		res.pop();
		res.pop();
		res
	}

	fn get_post_code(&self) -> String {
		let mut res = String::new();
		for c in &self.post_code {
			res.push_str(&format!("{} && ", c));
		}
		res.pop();
		res.pop();
		res.pop();
		res
	}

	fn get_pre(&self) -> String {
		let mut res = Vec::new();
		let cond = self.get_pre_conditions();
		let cond2 = self.get_pre_code();
		if !cond.is_empty() || !cond2.is_empty() {
			if !cond.is_empty() && !cond2.is_empty() {
				writeln!(&mut res, "if !(({}) && ({})) {{", cond, cond2)
					.unwrap();
			} else {
				writeln!(&mut res, "if !({}{}) {{", cond, cond2).unwrap();
			}
			writeln!(
				&mut res,
				"\tErr(Error::ParsePacket(String::from(\"Pre condition failed \
				 for {}\")))",
				self.name
			).unwrap();
			write!(&mut res, "}} else {{\n").unwrap();
		}
		String::from_utf8(res).unwrap()
	}

	fn get_post(&self) -> String {
		let mut res = Vec::new();
		let cond = self.get_post_conditions();
		let cond2 = self.get_post_code();
		if !cond.is_empty() || !cond2.is_empty() {
			write!(&mut res, "match res {{\n\tOk(res) => ").unwrap();
			if !cond.is_empty() && !cond2.is_empty() {
				writeln!(&mut res, "if !(({}) && ({})) {{", cond, cond2)
					.unwrap();
			} else {
				writeln!(&mut res, "if !({}{}) {{", cond, cond2).unwrap();
			}
			writeln!(
				&mut res,
				"\t\tErr(Error::ParsePacket(String::from(\"Post condition \
				 failed for {}\")))",
				self.name
			).unwrap();
			write!(
				&mut res,
				"\t}} else {{\n\t\tOk(res)\n\t}}\n\terror => error,\n}}"
			).unwrap();
		}
		String::from_utf8(res).unwrap()
	}

	/// Check pre-conditions, read (inner), check post-conditions.
	fn wrap_read(&self, inner: &str) -> Result<String> {
		let mut res = Vec::new();
		let pre = self.get_pre();
		let post = self.get_post();

		if pre.is_empty() && post.is_empty() {
			return Ok(inner.into());
		}

		// Pre conditions
		write!(&mut res, "{{\n\tlet res = ")?;
		if !pre.is_empty() {
			write!(
				&mut res,
				"{}{}\t}}",
				&indent(pre, 1)[1..],
				indent(inner, 2)
			)?;
		} else {
			write!(&mut res, "{}", &indent(inner, 1)[1..])?;
		}
		while res.last() == Some(&b'\n') {
			res.pop();
		}
		writeln!(&mut res, ";")?;

		// Post conditions
		if post.is_empty() {
			write!(&mut res, "\tres\n}}")?;
		} else {
			write!(&mut res, "{}}}", indent(&post, 1))?;
		}

		Ok(String::from_utf8(res).unwrap())
	}

	/// Read conditions and code
	fn read_conditions(&mut self, content: &str) {
		let mut pre_done = false;
		let mut input = Cursor::new(content.as_bytes());
		let mut l = String::new();
		while input.read_line(&mut l).unwrap() != 0 {
			if l.starts_with("++") {
				let s = l[2..].trim().to_string();
				if pre_done {
					self.post_code.push(s);
				} else {
					self.pre_code.push(s);
				}
			} else if !l.starts_with("//") {
				pre_done = true;
			}
			l.clear();
		}
	}
}

impl Struct {
	fn read(r: &mut BufRead) -> Result<Self> {
		// Read all fields
		let mut fields = Vec::new();
		while let Some(mut line) = read_line(r)? {
			let mut doc = String::new();
			while line.starts_with("///") {
				doc.push_str(&line);
				line = match read_line(r)? {
					Some(line) => line,
					None => break,
				};
				doc.push('\n');
			}
			line.push('\n');
			read_until_unindent(r, &mut line)?;
			if let Some(mut f) = Field::read(&mut Cursor::new(line.as_bytes()))?
			{
				f.doc = doc;
				fields.push(f);
			}
		}
		Ok(Self { fields })
	}

	fn get_size(&self) -> String {
		let mut res = String::new();
		for f in &self.fields {
			res.push_str(&format!("{} + ", f.get_size()));
		}
		res.pop();
		res.pop();
		res.pop();
		res
	}

	fn write_decl(&self, elem: &Element, w: &mut Write) -> Result<()> {
		writeln!(w, "#[derive(Clone)]\npub struct {} {{", elem.name)?;
		for sf in &self.fields {
			let t = sf.get_type();
			if sf.get_name() != "-" && !t.is_empty() {
				if !sf.doc.is_empty() {
					write!(w, "{}", indent(&sf.doc, 1))?;
				}
				writeln!(w, "\tpub {}: {},", to_snake_case(sf.get_name()), t)?;
			}
		}
		writeln!(w, "}}\n")?;
		for sf in &self.fields {
			sf.write_decl(w)?;
		}
		Ok(())
	}

	/// `before` is an additional argument to the read functions.
	///
	/// It contains the field which was read just before this one.
	fn write_impl(
		&self,
		w: &mut Write,
		elem: &Element,
		before: Option<&Field>,
	) -> Result<()> {
		write!(w, "impl {} {{\n\tpub fn read(", elem.name)?;
		if let Some(before) = before {
			write!(
				w,
				"{}: &{}, ",
				to_snake_case(before.get_name()),
				before.get_type()
			)?;
		}
		writeln!(w, "r: &mut Cursor<&[u8]>) -> Result<Self> {{")?;
		if let Some(before) = before {
			// Ignore if not used
			writeln!(w, "\t\tlet _ = {};", to_snake_case(before.get_name()))?;
		}
		let mut buf = Vec::new();
		self.write_raw_read_impl(&mut buf, elem, before)?;
		writeln!(w, "{}\t}}\n", indent(str::from_utf8(&buf).unwrap(), 2))?;

		writeln!(w, "\tpub fn write(&self, w: &mut Write) -> ::Result<()> {{")?;
		let mut buf = Vec::new();
		self.write_raw_write_impl(&mut buf)?;
		writeln!(w, "{}\t}}", indent(str::from_utf8(&buf).unwrap(), 2))?;
		writeln!(w, "}}\n")?;

		// Implement Debug
		writeln!(w, "impl fmt::Debug for {} {{", elem.name)?;
		writeln!(
			w,
			"\tfn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{"
		)?;
		writeln!(w, "\t\twrite!(f, \"{} {{{{ \")?;", elem.name)?;
		let mut buf = Vec::new();
		self.write_raw_debug_impl(&mut buf)?;
		writeln!(
			w,
			"{}\t\twrite!(f, \"}}}}\")\n\t}}",
			indent(str::from_utf8(&buf).unwrap(), 2)
		)?;
		writeln!(w, "}}\n")?;

		self.write_other_impls(w, before)
	}

	fn write_other_impls(
		&self,
		w: &mut Write,
		before: Option<&Field>,
	) -> Result<()> {
		if let Some(f2) = self.fields.first() {
			f2.write_impl(w, before)?;
		}
		for fs in self.fields.windows(2) {
			let f1 = &fs[0];
			let f2 = &fs[1];
			f2.write_impl(w, Some(f1))?;
		}
		Ok(())
	}

	fn write_raw_read_impl(
		&self,
		real_w: &mut Write,
		elem: &Element,
		before: Option<&Field>,
	) -> Result<()> {
		let mut res = Vec::new();
		{
			let w = &mut res;

			writeln!(w, "{{")?;
			let mut before = before;
			for fs in &self.fields {
				if !fs.get_type().is_empty() {
					if fs.get_name() != "-" {
						write!(w, "\tlet {} = ", to_snake_case(fs.get_name()))?;
					}
					let mut buf = Vec::new();
					fs.call_read(&mut buf, before)?;
					let s = str::from_utf8(&buf).unwrap();
					let s = indent(s, 1);
					let mut s = s.as_str();
					if s.starts_with("\t") {
						s = &s[1..];
					}
					if s.ends_with("\n") {
						s = &s[..s.len() - 1];
					}
					write!(w, "{}", s)?;

					if fs.optional {
						writeln!(w, ".ok();")?;
					} else {
						writeln!(w, "?;")?;
					}
					before = Some(fs);
				}
			}

			writeln!(w, "\n\tOk(Self {{")?;
			for fs in &self.fields {
				if fs.get_name() != "-" && !fs.get_type().is_empty() {
					writeln!(w, "\t\t{},", to_snake_case(fs.get_name()))?;
				}
			}
			write!(w, "\t}})\n}}")?;
		}

		write!(real_w, "{}", elem.wrap_read(str::from_utf8(&res).unwrap())?)?;

		Ok(())
	}

	fn write_raw_write_impl(&self, w: &mut Write) -> Result<()> {
		for fs in &self.fields {
			if !fs.get_type().is_empty() {
				fs.call_write(
					w,
					&format!("self.{}", to_snake_case(fs.get_name())),
				)?;
				writeln!(w, ";")?;
			}
		}
		writeln!(w, "Ok(())")?;
		Ok(())
	}

	fn write_raw_debug_impl(&self, w: &mut Write) -> Result<()> {
		for fs in &self.fields {
			if !fs.get_type().is_empty() {
				writeln!(w, "write!(f, \"{}: \")?;", fs.get_name())?;
				fs.call_debug(
					w,
					&format!("self.{}", to_snake_case(fs.get_name())),
				)?;
				writeln!(w, ";")?;
				writeln!(w, "write!(f, \", \")?;")?;
			}
		}
		Ok(())
	}
}

impl Enum {
	fn read(r: &mut BufRead) -> Result<Self> {
		// Read all fields
		let mut possibilities = Vec::new();
		while let Some(mut line) = read_line(r)? {
			let mut doc = String::new();
			while line.starts_with("///") {
				doc.push_str(&line);
				line = match read_line(r)? {
					Some(line) => line,
					None => break,
				};
			}
			line.push('\n');
			read_until_unindent(r, &mut line)?;
			if let Some(mut f) = Field::read(&mut Cursor::new(line.as_bytes()))?
			{
				f.doc = doc;
				possibilities.push(f);
			}
		}
		Ok(Self { possibilities })
	}

	fn write_decl(&self, elem: &Element, w: &mut Write) -> Result<()> {
		writeln!(w, "#[derive(Clone)]\npub enum {} {{", elem.name)?;
		let mut next_decls = String::new();
		for pos in &self.possibilities {
			match pos.content.element_type {
				ElementType::Struct(ref s) => {
					// Write struct inline
					let mut buf = Vec::new();
					s.write_decl(&pos.content.elem, &mut buf)?;
					let mut buf_s = String::from_utf8(buf).unwrap();
					buf_s = buf_s.replacen(
						"#[derive(Clone)]\npub struct ",
						"\n",
						1,
					);
					buf_s = buf_s.replace("pub ", "");
					let end = buf_s.find("\n}\n\n").unwrap();
					next_decls += &buf_s.split_off(end + 4);
					buf_s.pop();

					buf_s = indent(&buf_s, 1);
					buf_s.pop();
					writeln!(w, "{},", buf_s)?;
				}
				ElementType::Enum(_) => {
					writeln!(w, "\t{}({}),", pos.get_name(), pos.get_type())?
				}
				ElementType::Integer(ref s)
				| ElementType::Array(ref s)
				| ElementType::Custom(ref s) => {
					writeln!(w, "\t{}({}),", pos.get_name(), s)?
				}
			}
		}
		writeln!(w, "}}\n{}", next_decls)?;
		for pos in &self.possibilities {
			match pos.content.element_type {
				ElementType::Enum(ref s) => {
					s.write_decl(&pos.content.elem, w)?;
				}
				_ => {}
			}
		}
		Ok(())
	}

	fn write_impl(
		&self,
		w: &mut Write,
		elem: &Element,
		before: Option<&Field>,
	) -> Result<()> {
		write!(w, "impl {} {{\n\tpub fn read(", elem.name)?;
		if let Some(before) = before {
			write!(
				w,
				"{}: &{}, ",
				to_snake_case(before.get_name()),
				before.get_type()
			)?;
		}
		writeln!(w, "r: &mut Cursor<&[u8]>) -> Result<Self> {{")?;
		if let Some(before) = before {
			writeln!(w, "\t\tlet _ = {};", to_snake_case(before.get_name()))?;
		}
		let mut buf = Vec::new();
		self.write_raw_read_impl(&mut buf, elem, before)?;
		writeln!(w, "{}\t}}\n", indent(str::from_utf8(&buf).unwrap(), 2))?;

		writeln!(w, "\tpub fn write(&self, w: &mut Write) -> ::Result<()> {{")?;
		let mut buf = Vec::new();
		self.write_raw_write_impl(&mut buf, elem)?;
		writeln!(w, "{}\t}}", indent(str::from_utf8(&buf).unwrap(), 2))?;
		writeln!(w, "}}\n")?;

		// Implement Debug
		writeln!(w, "impl fmt::Debug for {} {{", elem.name)?;
		writeln!(
			w,
			"\tfn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{"
		)?;
		writeln!(w, "\t\twrite!(f, \"{}::\")?;", elem.name)?;
		let mut buf = Vec::new();
		self.write_raw_debug_impl(&mut buf, elem)?;
		writeln!(w, "{}\t}}", indent(str::from_utf8(&buf).unwrap(), 2))?;
		writeln!(w, "}}\n")?;

		for p in &self.possibilities {
			match p.content.element_type {
				// Inlined
				ElementType::Struct(ref s) => s.write_other_impls(w, before)?,
				_ => p.write_impl(w, before)?,
			}
		}
		Ok(())
	}

	fn write_raw_read_impl(
		&self,
		real_w: &mut Write,
		elem: &Element,
		before: Option<&Field>,
	) -> Result<()> {
		let mut res = Vec::new();
		{
			let w = &mut res;
			// Create a buffer so we can read multiple times
			writeln!(
				w,
				"let reset_cursor = r.clone();\nlet mut err_buf = \
				 Vec::new();\nlet mut enum_res = None;"
			)?;

			// Try each possibility and pick the first that is `Ok`
			for p in &self.possibilities {
				let (read, is_struct) = match p.content.element_type {
					ElementType::Struct(_) => {
						let read = {
							let mut buf = Vec::new();
							p.write_raw_read_impl(&mut buf, before)?;
							let mut s = String::from_utf8(buf).unwrap();
							// Change struct creation
							let pos = s.rfind("Self {").unwrap();
							let tmp = s.split_off(pos);
							s.push_str(&format!(
								"{}::{}",
								elem.name,
								p.get_name()
							));
							s.push_str(&tmp[4..]);
							s
						};

						let mut buf = Vec::new();
						p.call_read(&mut buf, before)?;
						let s = String::from_utf8(buf).unwrap();
						let before = if let Some(before) = before {
							format!("&{}, ", to_snake_case(before.get_name()))
						} else {
							String::new()
						};
						let s = s.replace(
							&format!("{}::read({}r)", p.get_name(), before),
							&read,
						);
						(indent(&s, 2), true)
					}
					_ => {
						let mut buf = Vec::new();
						p.call_read(&mut buf, before)?;
						(indent(str::from_utf8(&buf).unwrap(), 1), false)
					}
				};

				write!(
					w,
					"match (|| -> Result<_> {{\n\tlet mut r = \
					 reset_cursor.clone();\n\tlet r = &mut r;\n{}}})() \
					 {{\n\tOk(res) => {{\n\t\tenum_res = Some(",
					read
				)?;
				if !is_struct {
					// Convert
					write!(w, "{}::{}(res)", elem.name, p.get_name())?;
				} else {
					write!(w, "res")?;
				}
				writeln!(w, ");\n\t}}")?;

				// Build a list of errors and return it if nothing matched
				writeln!(
					w,
					"\tErr(error) => {{\n\t\terr_buf.push(format!(\"{} did \
					 not match: {{}}\", error));\n\t}}\n}}\n",
					p.get_name()
				)?;
			}

			writeln!(
				w,
				"match enum_res {{\n\tSome(res) => Ok(res),\n\tNone => \
				 Err(Error::ParsePacket(format!(\"No matching possibility for \
				 enum {} ({{:?}})\", err_buf))),\n}}",
				elem.name
			)?;
		}

		write!(real_w, "{}", elem.wrap_read(str::from_utf8(&res).unwrap())?)?;

		Ok(())
	}

	fn write_raw_write_impl(
		&self,
		w: &mut Write,
		elem: &Element,
	) -> Result<()> {
		writeln!(w, "match *self {{")?;
		for p in &self.possibilities {
			let ref_s = if p.is_ref_type() { "ref " } else { "" };
			match p.content.element_type {
				ElementType::Struct(ref s) => {
					// Struct declaration
					write!(w, "\t{}::{} {{ ", elem.name, p.get_name())?;
					for sf in &s.fields {
						if !sf.get_type().is_empty() && sf.get_name() != "-" {
							write!(w, "{}{}, ", ref_s, sf.get_name())?;
						}
					}
					writeln!(w, "}} => {{")?;
					let write = {
						let mut buf = Vec::new();
						p.write_raw_write_impl(&mut buf)?;
						let mut s = String::from_utf8(buf).unwrap();
						// Without final `Ok`
						s = s[..(s.len() - 7)].replace("(self.", "(*");
						s = s.replace("(&self.", "(");
						s = s.replace("self.", "");
						s
					};

					writeln!(w, "{}\t}}", indent(&write, 2))?;
				}
				_ => {
					writeln!(
						w,
						"\t{}::{}({}{}) => {{",
						elem.name,
						p.get_name(),
						ref_s,
						to_snake_case(p.get_name())
					)?;
					let mut buf = Vec::new();
					p.call_write(&mut buf, &to_snake_case(p.get_name()))?;
					writeln!(
						w,
						"{}\t}}",
						indent(&String::from_utf8(buf).unwrap(), 2)
					)?;
				}
			}
		}
		writeln!(w, "}}\nOk(())")?;
		Ok(())
	}

	fn write_raw_debug_impl(
		&self,
		w: &mut Write,
		elem: &Element,
	) -> Result<()> {
		writeln!(w, "match *self {{")?;
		for p in &self.possibilities {
			let ref_s = if p.is_ref_type() { "ref " } else { "" };
			match p.content.element_type {
				ElementType::Struct(ref s) => {
					// Struct declaration
					write!(w, "\t{}::{} {{ ", elem.name, p.get_name())?;
					for sf in &s.fields {
						if !sf.get_type().is_empty() && sf.get_name() != "-" {
							write!(w, "{}{}, ", ref_s, sf.get_name())?;
						}
					}
					writeln!(w, "}} => {{")?;
					write!(w, "\t\twrite!(f, \"{} {{{{\")?;", p.get_name())?;
					let write = {
						let mut buf = Vec::new();
						p.write_raw_debug_impl(&mut buf)?;
						let mut s = String::from_utf8(buf).unwrap();
						s = s.replace(" self.", " *");
						s = s.replace("(&self.", "(");
						s = s.replace("self.", "");
						s
					};

					write!(w, "{}", indent(&write, 2))?;
					writeln!(w, "\t\twrite!(f, \"}}}}\")?;\t}}")?;
				}
				_ => {
					writeln!(
						w,
						"\t{}::{}({}{}) => {{",
						elem.name,
						p.get_name(),
						ref_s,
						to_snake_case(p.get_name())
					)?;
					write!(w, "\t\twrite!(f, \"{}(\")?;", p.get_name())?;
					let mut buf = Vec::new();
					p.call_debug(&mut buf, &to_snake_case(p.get_name()))?;
					writeln!(
						w,
						"{}",
						indent(&String::from_utf8(buf).unwrap(), 2)
					)?;
					writeln!(w, ";\t\twrite!(f, \")\")?;\t}}")?;
				}
			}
		}
		writeln!(w, "}}\nOk(())")?;
		Ok(())
	}
}

impl Field {
	fn new() -> Self {
		Self {
			doc: String::new(),
			value: None,
			optional: false,
			content: ElementWrapper {
				elem: Element::new(String::new()),
				element_type: ElementType::Custom(String::new()),
			},
		}
	}

	fn get_name(&self) -> &str {
		&self.content.elem.name
	}

	fn read(r: &mut BufRead) -> Result<Option<Self>> {
		if let Some(mut line) = read_line(r)? {
			let mut res = Self::new();
			let prefix =
				if ['#', '?', '+'].contains(&line.chars().next().unwrap()) {
					let r = Some(line.remove(0));
					r
				} else {
					None
				};
			if prefix == Some('+') {
				// Code or condition
				return Ok(None);
			}

			res.content.elem.name = if let Some(i) = line.find(' ') {
				split_rev(&mut line, i)
			} else {
				line.split_off(0)
			};
			if prefix == Some('?') {
				res.optional = true;
			}
			line = line.trim().to_string();

			let mut content = String::new();
			r.read_to_string(&mut content)?;
			unindent(&mut content);

			res.content.elem.read_conditions(&content);

			if prefix == Some('#') {
				// Enum
				res.content.element_type = ElementType::Enum(Enum::read(
					&mut Cursor::new(content.as_bytes()),
				)?);
			} else if line.is_empty() {
				// Struct
				res.content.element_type = ElementType::Struct(Struct::read(
					&mut Cursor::new(content.as_bytes()),
				)?);
			} else {
				// Normal field
				let first_c = line.chars().next().unwrap();
				if first_c == '[' {
					if let Some(i) = line.find(']') {
						res.content.element_type =
							ElementType::Array(split_rev(&mut line, i + 1));
					} else {
						return Err(io::Error::new(
							io::ErrorKind::Other,
							"Expected ] after array",
						));
					}
				} else if line.starts_with("Vec<") {
					let pos = line.find(' ').unwrap_or(line.len());
					res.content.element_type =
						ElementType::Array(split_rev(&mut line, pos));
				} else if ['i', 'u'].contains(&first_c) {
					let pos = line.find(' ').unwrap_or(line.len());
					res.content.element_type =
						ElementType::Integer(split_rev(&mut line, pos));
				} else {
					let pos = line.find(' ').unwrap_or(line.len());
					res.content.element_type =
						ElementType::Custom(split_rev(&mut line, pos));
				}
				if !line.is_empty() {
					res.value = Some(line);
				}
			}

			Ok(Some(res))
		} else {
			Err(io::Error::new(
				io::ErrorKind::Other,
				"Unexpected end of file",
			))
		}
	}

	fn get_type(&self) -> String {
		let name = self.get_raw_type();
		if self.optional {
			format!("Option<{}>", name)
		} else {
			name
		}
	}

	fn get_raw_type(&self) -> String {
		match self.content.element_type {
			ElementType::Struct(_) | ElementType::Enum(_) => {
				&self.content.elem.name
			}
			ElementType::Integer(ref s)
			| ElementType::Array(ref s)
			| ElementType::Custom(ref s) => if s == "-" {
				""
			} else {
				s
			},
		}.to_string()
	}

	fn is_ref_type(&self) -> bool {
		if let ElementType::Integer(_) = self.content.element_type {
			false
		} else {
			true
		}
	}

	/// The size of this field in bytes, this will be a method call or something
	/// similar.
	fn get_size(&self) -> String {
		match self.content.element_type {
			ElementType::Struct(ref s) => s.get_size(),
			ElementType::Integer(ref s) => {
				let size: usize = s[1..].parse().unwrap();
				format!("{}", size / 8)
			}
			ElementType::Array(ref s) => {
				let pos = s.find(';').unwrap();
				let int_size: usize = s[2..pos].parse().unwrap();
				let pos2 = s.find(']').unwrap();
				let arr_size: usize = s[(pos + 2)..pos2].parse().unwrap();
				format!("{}", int_size / 8 * arr_size)
			}
			ElementType::Custom(ref s) => format!("{}.get_size()", s),
			_ => panic!("Called get_size on unsupported type"),
		}
	}

	fn write_decl(&self, w: &mut Write) -> Result<()> {
		match self.content.element_type {
			ElementType::Struct(ref s) => {
				if !self.doc.is_empty() {
					writeln!(w, "{}", self.doc)?;
				}
				s.write_decl(&self.content.elem, w)?;
			}
			ElementType::Enum(ref s) => {
				if !self.doc.is_empty() {
					writeln!(w, "{}", self.doc)?;
				}
				s.write_decl(&self.content.elem, w)?;
			}
			_ => {}
		}
		Ok(())
	}

	fn write_impl(&self, w: &mut Write, before: Option<&Field>) -> Result<()> {
		match self.content.element_type {
			ElementType::Struct(ref s) => {
				s.write_impl(w, &self.content.elem, before)?;
			}
			ElementType::Enum(ref s) => {
				s.write_impl(w, &self.content.elem, before)?;
			}
			_ => {}
		}
		Ok(())
	}

	fn write_raw_read_impl(
		&self,
		w: &mut Write,
		before: Option<&Field>,
	) -> Result<()> {
		match self.content.element_type {
			ElementType::Struct(ref s) => {
				s.write_raw_read_impl(w, &self.content.elem, before)?;
			}
			ElementType::Enum(ref s) => {
				s.write_raw_read_impl(w, &self.content.elem, before)?;
			}
			ElementType::Integer(_)
			| ElementType::Array(_)
			| ElementType::Custom(_) => {
				self.call_read(w, before)?;
			}
		}
		Ok(())
	}

	fn write_raw_write_impl(&self, w: &mut Write) -> Result<()> {
		match self.content.element_type {
			ElementType::Struct(ref s) => s.write_raw_write_impl(w)?,
			ElementType::Enum(ref s) => {
				s.write_raw_write_impl(w, &self.content.elem)?
			}
			ElementType::Integer(_)
			| ElementType::Array(_)
			| ElementType::Custom(_) => self.call_write(w, self.get_name())?,
		}
		Ok(())
	}

	fn write_raw_debug_impl(&self, w: &mut Write) -> Result<()> {
		match self.content.element_type {
			ElementType::Struct(ref s) => s.write_raw_debug_impl(w)?,
			ElementType::Enum(ref s) => {
				s.write_raw_debug_impl(w, &self.content.elem)?
			}
			ElementType::Integer(_)
			| ElementType::Array(_)
			| ElementType::Custom(_) => self.call_debug(w, self.get_name())?,
		}
		Ok(())
	}

	fn call_read(&self, w: &mut Write, before: Option<&Field>) -> Result<()> {
		let before = if let Some(before) = before {
			format!("&{}, ", to_snake_case(before.get_name()))
		} else {
			String::new()
		};
		match self.content.element_type {
			ElementType::Struct(_) => {
				write!(w, "{}::read({}r)", self.get_name(), before)?;
			}
			ElementType::Enum(_) => {
				write!(w, "{}::read({}r)", self.get_name(), before)?;
			}
			ElementType::Integer(_) => if self.get_size() == "1" {
				write!(
					w,
					"{}",
					self.content.elem.wrap_read(&format!(
						"r.read_{}().map_err(Error::from)",
						self.get_raw_type()
					))?
				)?;
			} else {
				write!(
					w,
					"{}",
					self.content.elem.wrap_read(&format!(
						"r.read_{}::<NetworkEndian>().map_err(Error::from)",
						self.get_raw_type()
					))?
				)?;
			},
			ElementType::Array(ref s) => if s.starts_with("Vec<") {
				write!(
					w,
					"{}",
					self.content.elem.wrap_read(&format!(
						"{{\n\tlet mut res = Vec::new();\n\tif let Err(error) \
						 = r.read_to_end(&mut res) \
						 {{\n\t\tErr(Error::from(error))\n\t}} else \
						 {{\n\t\tOk(res)\n\t}}\n}}"
					))?
				)?;
			} else {
				write!(
					w,
					"{}",
					self.content.elem.wrap_read(&format!(
						"{{\n\tlet mut res = [0; {}];\n\tif let Err(error) = \
						 r.read_exact(&mut res) \
						 {{\n\t\tErr(Error::from(error))\n\t}} else \
						 {{\n\t\tOk(res)\n\t}}\n}}",
						self.get_size()
					))?
				)?;
			},
			ElementType::Custom(ref s) => if s == "-" {
				write!(
					w,
					"{}",
					self.content.elem.wrap_read(&format!("Ok(())"))?
				)?;
			} else {
				write!(
					w,
					"{}",
					self.content
						.elem
						.wrap_read(&format!("{}::read({}r)", s, before))?
				)?;
			},
		}

		// Value
		if let Some(ref val) = self.value {
			write!(w, ".and_then(|res| ")?;
			writeln!(w, "if res != {} {{", val)?;
			writeln!(
				w,
				"\tErr(Error::ParsePacket(String::from(\"Wrong value, \
				 expected {}\")))",
				val
			)?;
			write!(w, "}} else {{\n\tOk(res)\n}})")?;
		}

		Ok(())
	}

	fn call_write<'a>(
		&'a self,
		w: &mut Write,
		mut name: &'a str,
	) -> Result<()> {
		if self.optional {
			writeln!(w, "if let Some({}) = {} {{", self.get_name(), name)?;
			name = self.get_name()
		}
		let data = if let Some(ref value) = self.value {
			value
		} else {
			name
		};
		let data_val = if data.contains('.') {
			data.to_string()
		} else {
			data.to_string()
		};
		match self.content.element_type {
			ElementType::Struct(_) => {
				write!(w, "{}.write(w)", data)?;
			}
			ElementType::Enum(_) => {
				write!(w, "{}.write(w)", data)?;
			}
			ElementType::Integer(_) => if self.get_size() == "1" {
				write!(w, "w.write_{}({})", self.get_raw_type(), data_val)?;
			} else {
				write!(
					w,
					"w.write_{}::<NetworkEndian>({})",
					self.get_raw_type(),
					data_val
				)?;
			},
			ElementType::Array(_) => {
				write!(w, "w.write_all(&{})", data_val)?;
			}
			ElementType::Custom(ref s) => if s == "-" {
				write!(w, "Ok(())")?;
			} else {
				write!(w, "{}.write(w)", data)?;
			},
		}

		write!(w, "?")?;
		if self.optional {
			writeln!(w, "}}")?;
		}
		Ok(())
	}

	fn call_debug<'a>(
		&'a self,
		w: &mut Write,
		mut name: &'a str,
	) -> Result<()> {
		if self.optional {
			writeln!(
				w,
				"if let Some({}) = {} {{\n\twrite!(f, \"Some(\")?;",
				self.get_name(),
				name
			)?;
			name = self.get_name()
		}
		let data = if let Some(ref value) = self.value {
			value
		} else {
			name
		};
		let data_val = if data.contains('.') {
			data.to_string()
		} else {
			data.to_string()
		};
		match self.content.element_type {
			ElementType::Struct(_) => {
				write!(w, "write!(f, \"{{:?}}\", {})", data)?;
			}
			ElementType::Enum(_) => {
				write!(w, "write!(f, \"{{:?}}\", {})", data)?;
			}
			ElementType::Integer(_) => {
				if data_val == "0" {
					write!(w, "write!(f, \"0\")")?;
				} else {
					writeln!(
						w,
						"if {} == 0 {{\n\twrite!(f, \"0\")\n}} else {{",
						data_val
					)?;
					writeln!(w, "\twrite!(f, \"{{:#x}}\", {})", data_val)?;
					write!(w, "}}")?;
				}
			}
			ElementType::Array(_) => {
				write!(w, "write!(f, \"{{:?}}\", HexSlice(&{}))", data_val)?;
			}
			ElementType::Custom(ref s) => if s == "-" {
				write!(w, "Ok(())")?;
			} else {
				write!(w, "write!(f, \"{{:?}}\", {})", data)?;
			},
		}

		write!(w, "?")?;
		if self.optional {
			writeln!(
				w,
				";\n\twrite!(f, \")\")?;\n}} else {{\n\twrite!(f, \
				 \"None\")?;\n}}"
			)?;
		}
		Ok(())
	}
}

/// Like `String::split_off`, but reverse: Returns the first half, s contains
/// the second half. Also removes the `i`th character if it is a space.
fn split_rev(s: &mut String, i: usize) -> String {
	if s.get(i..(i + 1)) == Some(" ") {
		let mut s2 = s.split_off(i + 1);
		s.pop();
		mem::swap(&mut s2, s);
		s2
	} else {
		let mut s2 = s.split_off(i);
		mem::swap(&mut s2, s);
		s2
	}
}

fn read_line(r: &mut BufRead) -> Result<Option<String>> {
	let mut s = String::new();
	if r.read_line(&mut s)? == 0 {
		Ok(None)
	} else {
		let t = s.trim();
		if (t.starts_with("//") && !t.starts_with("///")) || t.is_empty() {
			read_line(r)
		} else {
			Ok(Some(t.to_string()))
		}
	}
}

/// Read into `String` until we find a not indented line.
fn read_until_unindent(r: &mut BufRead, s: &mut String) -> Result<()> {
	while r.fill_buf()?.first() == Some(&b'\t') {
		r.read_line(s)?;
	}
	Ok(())
}
