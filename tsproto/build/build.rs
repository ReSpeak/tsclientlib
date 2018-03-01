use std::{env, mem, str};
use std::fs::File;
use std::io::{self, Cursor, Result};
use std::io::prelude::*;
use std::path::Path;

#[derive(Debug)]
struct Struct {
    fields: Vec<Field>,
}

#[derive(Debug)]
struct Enum {
    possibilities: Vec<Field>,
}

#[derive(Debug)]
struct Field {
    doc: String,
    name: String,
    value: Option<String>,
    /// (var, value)
    pre_conditions: Vec<(String, String)>,
    /// Code that returns a bool
    pre_code: Vec<String>,
    post_conditions: Vec<(String, String)>,
    post_code: Vec<String>,
    optional: bool,
    content: FieldType,
}

#[derive(Debug)]
enum FieldType {
    Struct(Struct),
    Enum(Enum),
    Integer(String),
    Array(String),
    Custom(String),
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

    fn write_decl(&self, f: &Field, w: &mut Write) -> Result<()> {
        writeln!(w, "#[derive(Clone)]\npub struct {} {{", f.name)?;
        for sf in &self.fields {
            let t = sf.get_type();
            if sf.name != "-" && !t.is_empty() {
                if !sf.doc.is_empty() {
                    write!(w, "{}", indent(&sf.doc, 1))?;
                }
                writeln!(w, "\tpub {}: {},", to_snake_case(&sf.name), t)?;
            }
        }
        writeln!(w, "}}\n")?;
        for sf in &self.fields {
            sf.write_decl(w)?;
        }
        Ok(())
    }

    fn write_impl(
        &self,
        w: &mut Write,
        f: &Field,
        before: Option<&Field>,
    ) -> Result<()> {
        write!(w, "impl {} {{\n\tpub fn read(", f.name)?;
        if let Some(before) = before {
            write!(
                w,
                "{}: &{}, ",
                to_snake_case(&before.name),
                before.get_type()
            )?;
        }
        writeln!(w, "r: &mut Cursor<&[u8]>) -> Result<Self> {{")?;
        if let Some(before) = before {
            writeln!(w, "\t\tlet _ = {};", to_snake_case(&before.name))?;
        }
        let mut buf = Vec::new();
        self.write_raw_read_impl(&mut buf, f, before)?;
        writeln!(w, "{}\t}}\n", indent(str::from_utf8(&buf).unwrap(), 2))?;

        writeln!(w, "\tpub fn write(&self, w: &mut Write) -> ::Result<()> {{")?;
        let mut buf = Vec::new();
        self.write_raw_write_impl(&mut buf, f)?;
        writeln!(w, "{}\t}}", indent(str::from_utf8(&buf).unwrap(), 2))?;
        writeln!(w, "}}\n")?;

        // Implement Debug
        writeln!(w, "impl fmt::Debug for {} {{", f.name)?;
        writeln!(
            w,
            "\tfn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{"
        )?;
        writeln!(w, "\t\twrite!(f, \"{} {{{{ \")?;", f.name)?;
        let mut buf = Vec::new();
        self.write_raw_debug_impl(&mut buf, f)?;
        writeln!(
            w,
            "{}\t\twrite!(f, \"}}}}\")\n\t}}",
            indent(str::from_utf8(&buf).unwrap(), 2)
        )?;
        writeln!(w, "}}\n")?;

        self.write_other_impls(w, f, before)
    }

    fn write_other_impls(
        &self,
        w: &mut Write,
        _: &Field,
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

    fn write_raw_read_impl<'a>(
        &'a self,
        w: &mut Write,
        _: &Field,
        mut before: Option<&'a Field>,
    ) -> Result<()> {
        for fs in &self.fields {
            if !fs.get_type().is_empty() {
                if fs.name != "-" {
                    write!(w, "let {} = ", to_snake_case(&fs.name))?;
                }
                fs.call_read(w, before)?;
                writeln!(w, ";")?;
                before = Some(fs);
            }
        }

        writeln!(w, "\nOk(Self {{")?;
        for fs in &self.fields {
            if fs.name != "-" && !fs.get_type().is_empty() {
                writeln!(w, "\t{},", to_snake_case(&fs.name))?;
            }
        }
        writeln!(w, "}})")?;
        Ok(())
    }

    fn write_raw_write_impl<'a>(
        &'a self,
        w: &mut Write,
        _: &Field,
    ) -> Result<()> {
        for fs in &self.fields {
            if !fs.get_type().is_empty() {
                fs.call_write(w, &format!("self.{}", to_snake_case(&fs.name)))?;
                writeln!(w, ";")?;
            }
        }
        writeln!(w, "Ok(())")?;
        Ok(())
    }

    fn write_raw_debug_impl<'a>(
        &'a self,
        w: &mut Write,
        _: &Field,
    ) -> Result<()> {
        for fs in &self.fields {
            if !fs.get_type().is_empty() {
                writeln!(w, "write!(f, \"{}: \")?;", fs.name)?;
                fs.call_debug(w, &format!("self.{}", to_snake_case(&fs.name)))?;
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

    fn write_decl(&self, f: &Field, w: &mut Write) -> Result<()> {
        writeln!(w, "#[derive(Clone)]\npub enum {} {{", f.name)?;
        let mut next_decls = String::new();
        for pos in &self.possibilities {
            match pos.content {
                FieldType::Struct(ref s) => {
                    // Write struct inline
                    let mut buf = Vec::new();
                    s.write_decl(pos, &mut buf)?;
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
                FieldType::Enum(_) => {
                    writeln!(w, "\t{}({}),", pos.name, pos.get_type())?;
                }
                FieldType::Integer(ref s) |
                FieldType::Array(ref s) |
                FieldType::Custom(ref s) => {
                    writeln!(w, "\t{}({}),", pos.name, s)?;
                }
            }
        }
        writeln!(w, "}}\n{}", next_decls)?;
        for pos in &self.possibilities {
            match pos.content {
                FieldType::Enum(ref s) => {
                    s.write_decl(pos, w)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn write_impl(
        &self,
        w: &mut Write,
        f: &Field,
        before: Option<&Field>,
    ) -> Result<()> {
        write!(w, "impl {} {{\n\tpub fn read(", f.name)?;
        if let Some(before) = before {
            write!(
                w,
                "{}: &{}, ",
                to_snake_case(&before.name),
                before.get_type()
            )?;
        }
        writeln!(w, "r: &mut Cursor<&[u8]>) -> Result<Self> {{")?;
        if let Some(before) = before {
            writeln!(w, "\t\tlet _ = {};", to_snake_case(&before.name))?;
        }
        let mut buf = Vec::new();
        self.write_raw_read_impl(&mut buf, f, before)?;
        writeln!(w, "{}\t}}\n", indent(str::from_utf8(&buf).unwrap(), 2))?;

        writeln!(w, "\tpub fn write(&self, w: &mut Write) -> ::Result<()> {{")?;
        let mut buf = Vec::new();
        self.write_raw_write_impl(&mut buf, f)?;
        writeln!(w, "{}\t}}", indent(str::from_utf8(&buf).unwrap(), 2))?;
        writeln!(w, "}}\n")?;

        // Implement Debug
        writeln!(w, "impl fmt::Debug for {} {{", f.name)?;
        writeln!(
            w,
            "\tfn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {{"
        )?;
        writeln!(w, "\t\twrite!(f, \"{}::\")?;", f.name)?;
        let mut buf = Vec::new();
        self.write_raw_debug_impl(&mut buf, f)?;
        writeln!(w, "{}\t}}", indent(str::from_utf8(&buf).unwrap(), 2))?;
        writeln!(w, "}}\n")?;

        for p in &self.possibilities {
            match p.content {
                // Inlined
                FieldType::Struct(ref s) => s.write_other_impls(w, p, before)?,
                _ => p.write_impl(w, before)?,
            }
        }
        Ok(())
    }

    fn write_raw_read_impl(
        &self,
        w: &mut Write,
        f: &Field,
        before: Option<&Field>,
    ) -> Result<()> {
        // Create a buffer so we can read multiple times
        writeln!(
            w,
            "let reset_cursor = r.clone();\nlet mut err_buf = Vec::new();"
        )?;
        // Try each possibility and pick the first that is `Ok`
        for p in &self.possibilities {
            let (read, is_struct) = match p.content {
                FieldType::Struct(_) => {
                    let read = {
                        let mut buf = Vec::new();
                        p.write_raw_read_impl(&mut buf, before)?;
                        let mut s = String::from_utf8(buf).unwrap();
                        // Change struct creation
                        let pos = s.rfind("Self {").unwrap();
                        let tmp = s.split_off(pos);
                        s.push_str(&format!("{}::{}", f.name, p.name));
                        s.push_str(&tmp[4..]);
                        s
                    };

                    let mut buf = Vec::new();
                    p.call_read(&mut buf, before)?;
                    let s = String::from_utf8(buf).unwrap();
                    let before = if let Some(before) = before {
                        format!("&{}, ", to_snake_case(&before.name))
                    } else {
                        String::new()
                    };
                    let s = s.replace(
                        &format!("{}::read({}r)", p.name, before),
                        &read,
                    );
                    (indent(&s[..(s.len() - 1)], 2), true)
                }
                _ => {
                    let mut buf = Vec::new();
                    p.call_read(&mut buf, before)?;
                    (
                        indent(
                            &format!("Ok({})", String::from_utf8(buf).unwrap()),
                            2,
                        ),
                        false,
                    )
                }
            };

            write!(
                w,
                "match (|| -> Result<_> {{\n\tlet mut r = reset_cursor.clone();\
                 \n\tlet r = &mut r;\n{}}})() \
                 {{\n\tOk(res) => {{\n\t\treturn Ok(",
                read
            )?;
            if !is_struct {
                // Convert
                write!(w, "{}::{}(res)", f.name, p.name)?;
            } else {
                write!(w, "res")?;
            }
            writeln!(w, ");\n\t}}")?;

            // Build a list of errors and return it if nothing matched
            writeln!(
                w,
                "\tErr(error) => {{\n\t\terr_buf.push(format!(\"{} did not \
                 match: {{}}\", error));\n\t}}\n}}\n",
                p.name
            )?;
        }
        writeln!(
            w,
            "Err(Error::ParsePacket(format!(\"No matching possibility for \
            enum {} ({{:?}})\", err_buf)))",
            f.name
        )?;
        Ok(())
    }

    fn write_raw_write_impl(&self, w: &mut Write, f: &Field) -> Result<()> {
        writeln!(w, "match *self {{")?;
        for p in &self.possibilities {
            let ref_s = if p.is_ref_type() { "ref " } else { "" };
            match p.content {
                FieldType::Struct(ref s) => {
                    // Struct declaration
                    write!(w, "\t{}::{} {{ ", f.name, p.name)?;
                    for sf in &s.fields {
                        if !sf.get_type().is_empty() && sf.name != "-" {
                            write!(w, "{}{}, ", ref_s, sf.name)?;
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
                        f.name,
                        p.name,
                        ref_s,
                        to_snake_case(&p.name)
                    )?;
                    let mut buf = Vec::new();
                    p.call_write(&mut buf, &to_snake_case(&p.name))?;
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

    fn write_raw_debug_impl(&self, w: &mut Write, f: &Field) -> Result<()> {
        writeln!(w, "match *self {{")?;
        for p in &self.possibilities {
            let ref_s = if p.is_ref_type() { "ref " } else { "" };
            match p.content {
                FieldType::Struct(ref s) => {
                    // Struct declaration
                    write!(w, "\t{}::{} {{ ", f.name, p.name)?;
                    for sf in &s.fields {
                        if !sf.get_type().is_empty() && sf.name != "-" {
                            write!(w, "{}{}, ", ref_s, sf.name)?;
                        }
                    }
                    writeln!(w, "}} => {{")?;
                    write!(w, "\t\twrite!(f, \"{} {{{{\")?;", p.name)?;
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
                        f.name,
                        p.name,
                        ref_s,
                        to_snake_case(&p.name)
                    )?;
                    write!(w, "\t\twrite!(f, \"{}(\")?;", p.name)?;
                    let mut buf = Vec::new();
                    p.call_debug(&mut buf, &to_snake_case(&p.name))?;
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
            name: String::new(),
            value: None,
            pre_conditions: Vec::new(),
            pre_code: Vec::new(),
            post_conditions: Vec::new(),
            post_code: Vec::new(),
            optional: false,
            content: FieldType::Struct(Struct { fields: Vec::new() }),
        }
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

            res.name = if let Some(i) = line.find(' ') {
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

            if prefix == Some('#') {
                // Enum
                res.content = FieldType::Enum(
                    Enum::read(&mut Cursor::new(content.as_bytes()))?,
                );
            } else if line.is_empty() {
                // Struct
                res.content = FieldType::Struct(
                    Struct::read(&mut Cursor::new(content.as_bytes()))?,
                );
            } else {
                // Normal field
                let first_c = line.chars().next().unwrap();
                if first_c == '[' {
                    if let Some(i) = line.find(']') {
                        res.content =
                            FieldType::Array(split_rev(&mut line, i + 1));
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Expected ] after array",
                        ));
                    }
                } else if line.starts_with("Vec<") {
                    let pos = line.find(' ').unwrap_or(line.len());
                    res.content = FieldType::Array(split_rev(&mut line, pos));
                } else if ['i', 'u'].contains(&first_c) {
                    let pos = line.find(' ').unwrap_or(line.len());
                    res.content = FieldType::Integer(split_rev(&mut line, pos));
                } else {
                    let pos = line.find(' ').unwrap_or(line.len());
                    res.content = FieldType::Custom(split_rev(&mut line, pos));
                }
                if !line.is_empty() {
                    res.value = Some(line);
                }
            }

            // Read conditions and code
            let mut pre_done = false;
            let mut input = Cursor::new(content.as_bytes());
            let mut l = String::new();
            while input.read_line(&mut l)? != 0 {
                if l.starts_with("++") {
                    let s = l[2..].trim().to_string();
                    if pre_done {
                        res.post_code.push(s);
                    } else {
                        res.pre_code.push(s);
                    }
                } else if l.starts_with("+") {
                    let l = l[1..].trim();
                    if let Some(i) = l.find(' ') {
                        let (key, val) = l.split_at(i);
                        if pre_done {
                            res.post_conditions
                                .push((key.to_string(), val[1..].to_string()));
                        } else {
                            res.pre_conditions
                                .push((key.to_string(), val[1..].to_string()));
                        }
                    } else {
                        return Err(io::Error::new(
                            io::ErrorKind::Other,
                            "Condition must have a value",
                        ));
                    }
                } else if !l.starts_with("//") {
                    pre_done = true;
                }
                l.clear();
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
        match self.content {
            FieldType::Struct(_) | FieldType::Enum(_) => &self.name,
            FieldType::Integer(ref s) |
            FieldType::Array(ref s) |
            FieldType::Custom(ref s) => if s == "-" {
                ""
            } else {
                s
            },
        }.to_string()
    }

    fn is_ref_type(&self) -> bool {
        if let FieldType::Integer(_) = self.content {
            false
        } else {
            true
        }
    }

    /// The size of this field in bytes, this will be a method call or something
    /// similar.
    fn get_size(&self) -> String {
        match self.content {
            FieldType::Struct(ref s) => s.get_size(),
            FieldType::Integer(ref s) => {
                let size: usize = s[1..].parse().unwrap();
                format!("{}", size / 8)
            }
            FieldType::Array(ref s) => {
                let pos = s.find(';').unwrap();
                let int_size: usize = s[2..pos].parse().unwrap();
                let pos2 = s.find(']').unwrap();
                let arr_size: usize = s[(pos + 2)..pos2].parse().unwrap();
                format!("{}", int_size / 8 * arr_size)
            }
            FieldType::Custom(ref s) => format!("{}.get_size()", s),
            _ => panic!("Called get_size on unsupported type"),
        }
    }

    fn write_decl(&self, w: &mut Write) -> Result<()> {
        match self.content {
            FieldType::Struct(ref s) => {
                if !self.doc.is_empty() {
                    writeln!(w, "{}", self.doc)?;
                }
                s.write_decl(&self, w)?;
            }
            FieldType::Enum(ref s) => {
                if !self.doc.is_empty() {
                    writeln!(w, "{}", self.doc)?;
                }
                s.write_decl(&self, w)?;
            }
            _ => {}
        }
        Ok(())
    }

    fn write_impl(&self, w: &mut Write, before: Option<&Field>) -> Result<()> {
        match self.content {
            FieldType::Struct(ref s) => {
                s.write_impl(w, &self, before)?;
            }
            FieldType::Enum(ref s) => {
                s.write_impl(w, &self, before)?;
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
        match self.content {
            FieldType::Struct(ref s) => {
                s.write_raw_read_impl(w, &self, before)?;
            }
            FieldType::Enum(ref s) => {
                s.write_raw_read_impl(w, &self, before)?;
            }
            FieldType::Integer(_) |
            FieldType::Array(_) |
            FieldType::Custom(_) => {
                self.call_read(w, before)?;
            }
        }
        Ok(())
    }

    fn write_raw_write_impl(&self, w: &mut Write) -> Result<()> {
        match self.content {
            FieldType::Struct(ref s) => {
                s.write_raw_write_impl(w, &self)?;
            }
            FieldType::Enum(ref s) => {
                s.write_raw_write_impl(w, &self)?;
            }
            FieldType::Integer(_) |
            FieldType::Array(_) |
            FieldType::Custom(_) => {
                self.call_write(w, &self.name)?;
            }
        }
        Ok(())
    }

    fn write_raw_debug_impl(&self, w: &mut Write) -> Result<()> {
        match self.content {
            FieldType::Struct(ref s) => {
                s.write_raw_debug_impl(w, &self)?;
            }
            FieldType::Enum(ref s) => {
                s.write_raw_debug_impl(w, &self)?;
            }
            FieldType::Integer(_) |
            FieldType::Array(_) |
            FieldType::Custom(_) => {
                self.call_debug(w, &self.name)?;
            }
        }
        Ok(())
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

    fn call_read(&self, w: &mut Write, before: Option<&Field>) -> Result<()> {
        write!(w, "{{ ")?;

        // Pre conditions
        let cond = self.get_pre_conditions();
        let cond2 = self.get_pre_code();
        if !cond.is_empty() || !cond2.is_empty() {
            if !cond.is_empty() && !cond2.is_empty() {
                writeln!(w, "if !(({}) && ({})) {{", cond, cond2)?;
            } else {
                writeln!(w, "if !({}{}) {{", cond, cond2)?;
            }
            writeln!(w, "\tErr(Error::ParsePacket(String::from(\
                \"Pre condition failed for {}\")))", self.name)?;
            write!(w, "}} else {{\n\t")?;
        }

        let before = if let Some(before) = before {
            format!("&{}, ", to_snake_case(&before.name))
        } else {
            String::new()
        };
        match self.content {
            FieldType::Struct(_) => {
                write!(w, "{}::read({}r)", self.name, before)?;
            }
            FieldType::Enum(_) => {
                write!(w, "{}::read({}r)", self.name, before)?;
            }
            FieldType::Integer(_) => if self.get_size() == "1" {
                write!(
                    w,
                    "r.read_{}().map_err(|e| Error::from(e))",
                    self.get_raw_type()
                )?;
            } else {
                write!(
                    w,
                    "r.read_{}::<NetworkEndian>().map_err(|e| Error::from(e))",
                    self.get_raw_type()
                )?;
            },
            FieldType::Array(ref s) => if s.starts_with("Vec<") {
                write!(
                    w,
                    "{{\n\tlet mut res = Vec::new();\n\tif let Err(error) = \
                     r.read_to_end(&mut res) \
                     {{\n\t\tErr(Error::from(error))\n\t}} else \
                     {{\n\t\tOk(res)\n\t}}\n}}"
                )?;
            } else {
                write!(
                    w,
                    "{{\n\tlet mut res = [0; {}];\n\tif let Err(error) = \
                     r.read_exact(&mut res) \
                     {{\n\t\tErr(Error::from(error))\n\t}} else \
                     {{\n\t\tOk(res)\n\t}}\n}}",
                    self.get_size()
                )?;
            },
            FieldType::Custom(ref s) => if s == "-" {
                write!(w, "Ok(())")?;
            } else {
                write!(w, "{}::read({}r)", s, before)?;
            },
        }
        if !cond.is_empty() || !cond2.is_empty() {
            write!(w, "\n}}")?;
        }

        // Value
        if let Some(ref val) = self.value {
            write!(w, ".and_then(|res| ")?;
            writeln!(w, "if res != {} {{", val)?;
            writeln!(w, "\tErr(Error::ParsePacket(String::from(\
                \"Wrong value, expected {}\")))", val)?;
            write!(w, "}} else {{\n\tOk(res)\n}})")?;
        }

        // Post conditions
        let cond = self.get_post_conditions();
        let cond2 = self.get_post_code();
        if !cond.is_empty() || !cond2.is_empty() {
            write!(w, ".and_then(|res| ")?;
            if !cond.is_empty() && !cond2.is_empty() {
                writeln!(w, "if !(({}) && ({})) {{", cond, cond2)?;
            } else {
                writeln!(w, "if !({}{}) {{", cond, cond2)?;
            }
            writeln!(w, "\tErr(Error::ParsePacket(String::from(\
                \"Post condition failed for {}\")))", self.name)?;
            write!(w, "}} else {{\n\tOk(res)\n}})")?;
        }

        write!(w, " }}")?;
        if self.optional {
            write!(w, ".ok()")?;
        } else {
            write!(w, "?")?;
        }
        Ok(())
    }

    fn call_write<'a>(
        &'a self,
        w: &mut Write,
        mut name: &'a str,
    ) -> Result<()> {
        if self.optional {
            writeln!(w, "if let Some({}) = {} {{", self.name, name)?;
            name = &self.name
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
        match self.content {
            FieldType::Struct(_) => {
                write!(w, "{}.write(w)", data)?;
            }
            FieldType::Enum(_) => {
                write!(w, "{}.write(w)", data)?;
            }
            FieldType::Integer(_) => if self.get_size() == "1" {
                write!(w, "w.write_{}({})", self.get_raw_type(), data_val)?;
            } else {
                write!(
                    w,
                    "w.write_{}::<NetworkEndian>({})",
                    self.get_raw_type(),
                    data_val
                )?;
            },
            FieldType::Array(_) => {
                write!(w, "w.write_all(&{})", data_val)?;
            }
            FieldType::Custom(ref s) => if s == "-" {
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
                self.name,
                name
            )?;
            name = &self.name
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
        match self.content {
            FieldType::Struct(_) => {
                write!(w, "write!(f, \"{{:?}}\", {})", data)?;
            }
            FieldType::Enum(_) => {
                write!(w, "write!(f, \"{{:?}}\", {})", data)?;
            }
            FieldType::Integer(_) => {
                writeln!(
                    w,
                    "if {} == 0 {{\n\twrite!(f, \"0\")\n}} else {{",
                    data_val
                )?;
                writeln!(w, "\twrite!(f, \"{{:#x}}\", {})", data_val)?;
                writeln!(w, "}}")?;
            }
            FieldType::Array(_) => {
                write!(w, "write!(f, \"{{:?}}\", HexSlice(&{}))", data_val)?;
            }
            FieldType::Custom(ref s) => if s == "-" {
                write!(w, "Ok(())")?;
            } else {
                write!(w, "write!(f, \"{{:?}}\", {})", data)?;
            },
        }

        write!(w, "?")?;
        if self.optional {
            writeln!(
                w,
                ";\twrite!(f, \")\")?;\n}} else {{\n\twrite!(f, \
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

/// Read into `String` until we find a not indented line.
fn read_until_unindent(r: &mut BufRead, s: &mut String) -> Result<()> {
    while r.fill_buf()?.first() == Some(&b'\t') {
        r.read_line(s)?;
    }
    Ok(())
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

/// Indent a string by a given count using tabs.
fn indent(s: &str, count: usize) -> String {
    let line_count = s.lines().count();
    let mut result = String::with_capacity(s.len() + line_count * count * 4);
    for l in s.lines() {
        if !l.is_empty() {
            result.push_str(
                std::iter::repeat("\t")
                    .take(count)
                    .collect::<String>()
                    .as_str(),
            );
        }
        result.push_str(l);
        result.push('\n');
    }
    result
}

#[allow(dead_code)]
fn to_pascal_case(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    let mut uppercase = true;
    for c in text.chars() {
        if c == '_' {
            uppercase = true;
        } else {
            if uppercase {
                s.push(c.to_uppercase().next().unwrap());
                uppercase = false;
            } else {
                s.push(c);
            }
        }
    }
    s
}

fn to_snake_case(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    let mut first = true;
    for c in text.chars() {
        if !first && c.is_uppercase() {
            s.push('_');
            s.push(c.to_lowercase().next().unwrap());
        } else {
            s.push(c.to_lowercase().next().unwrap());
        }
        first = false;
    }
    s
}

/// Unindent a string by a given count of tabs.
fn unindent(mut s: &mut String) {
    mem::swap(&mut s.replace("\n\t", "\n"), &mut s);
    if s.get(0..1) == Some("\t") {
        s.remove(0);
    }
}

fn main() {
    println!("cargo:rerun-if-changed=build/build.rs");
    println!("cargo:rerun-if-changed=build/Packets.txt");

    let out_dir = env::var("OUT_DIR").unwrap();

    let mut input =
        io::BufReader::new(File::open("build/Packets.txt").unwrap());
    let packet = Field::read(&mut input).unwrap().unwrap();

    let path = Path::new(&out_dir);
    let mut f = File::create(&path.join("packets.rs")).unwrap();
    packet.write_decl(&mut f).unwrap();

    let mut from_client = Field::new();
    from_client.name = String::from("from_client");
    from_client.content = FieldType::Custom(String::from("bool"));
    packet.write_impl(&mut f, Some(&from_client)).unwrap();
}
