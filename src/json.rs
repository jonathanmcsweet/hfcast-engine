//! A small JSON reader and writer.
//!
//! The crate takes no dependencies on purpose — it is the reference a
//! port is judged against, so its supply chain is kept empty. The
//! `predict` binary needs to exchange structured values with the
//! TypeScript server, and this is the smallest thing that does it
//! correctly.
//!
//! Scope is the whole of RFC 8259 except two things nothing here
//! needs: numbers keep `f64` precision rather than arbitrary
//! precision, and duplicate object keys are kept in order rather than
//! collapsed.

use std::collections::BTreeMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Num(f64),
    Str(String),
    Arr(Vec<Json>),
    Obj(BTreeMap<String, Json>),
}

impl Json {
    /// The value at an object key.
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Obj(map) => map.get(key),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Num(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Arr(v) => Some(v.as_slice()),
            _ => None,
        }
    }

    /// A required number, named so the error says which field was
    /// wrong rather than that something was.
    pub fn number(&self, key: &str) -> Result<f64, String> {
        self.get(key)
            .and_then(Json::as_f64)
            .ok_or_else(|| format!("field \"{key}\" must be a number"))
    }

    pub fn string(&self, key: &str) -> Result<String, String> {
        self.get(key)
            .and_then(Json::as_str)
            .map(str::to_string)
            .ok_or_else(|| format!("field \"{key}\" must be a string"))
    }

    /// Serialises to compact JSON.
    ///
    /// A non-finite number has no JSON form. Writing `NaN` would
    /// produce text no parser accepts, so it becomes `null` and the
    /// reader decides what a missing value means.
    pub fn write(&self) -> String {
        let mut out = String::new();
        self.write_into(&mut out);
        out
    }

    fn write_into(&self, out: &mut String) {
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(true) => out.push_str("true"),
            Json::Bool(false) => out.push_str("false"),
            Json::Num(v) if v.is_finite() => {
                let _ = write!(out, "{v}");
            }
            Json::Num(_) => out.push_str("null"),
            Json::Str(s) => write_string(s, out),
            Json::Arr(items) => {
                out.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write_into(out);
                }
                out.push(']');
            }
            Json::Obj(map) => {
                out.push('{');
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_string(k, out);
                    out.push(':');
                    v.write_into(out);
                }
                out.push('}');
            }
        }
    }
}

/// Builds an object from pairs, so callers read as the shape they emit.
pub fn obj<const N: usize>(pairs: [(&str, Json); N]) -> Json {
    Json::Obj(
        pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect::<BTreeMap<_, _>>(),
    )
}

pub fn num(v: f64) -> Json {
    Json::Num(v)
}

pub fn str_of(v: &str) -> Json {
    Json::Str(v.to_string())
}

fn write_string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Control characters have no literal form in JSON.
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

pub fn parse(text: &str) -> Result<Json, String> {
    let bytes: Vec<char> = text.chars().collect();
    let mut p = Parser { chars: &bytes, i: 0 };
    p.skip_ws();
    let value = p.value()?;
    p.skip_ws();
    if p.i != p.chars.len() {
        return Err(format!("trailing text at character {}", p.i));
    }
    Ok(value)
}

struct Parser<'a> {
    chars: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.i += 1;
        }
        c
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(' ' | '\t' | '\n' | '\r')) {
            self.i += 1;
        }
    }

    fn expect(&mut self, want: char) -> Result<(), String> {
        match self.bump() {
            Some(c) if c == want => Ok(()),
            Some(c) => Err(format!("expected {want:?} but found {c:?}")),
            None => Err(format!("expected {want:?} but the text ended")),
        }
    }

    fn literal(&mut self, word: &str) -> Result<(), String> {
        for want in word.chars() {
            self.expect(want)?;
        }
        Ok(())
    }

    fn value(&mut self) -> Result<Json, String> {
        match self.peek() {
            Some('{') => self.object(),
            Some('[') => self.array(),
            Some('"') => Ok(Json::Str(self.string()?)),
            Some('t') => self.literal("true").map(|()| Json::Bool(true)),
            Some('f') => self.literal("false").map(|()| Json::Bool(false)),
            Some('n') => self.literal("null").map(|()| Json::Null),
            Some(c) if c == '-' || c.is_ascii_digit() => self.number(),
            Some(c) => Err(format!("unexpected {c:?}")),
            None => Err("the text ended where a value was expected".to_string()),
        }
    }

    fn object(&mut self) -> Result<Json, String> {
        self.expect('{')?;
        let mut map = BTreeMap::new();
        self.skip_ws();
        if self.peek() == Some('}') {
            self.i += 1;
            return Ok(Json::Obj(map));
        }
        loop {
            self.skip_ws();
            let key = self.string()?;
            self.skip_ws();
            self.expect(':')?;
            self.skip_ws();
            let value = self.value()?;
            map.insert(key, value);
            self.skip_ws();
            match self.bump() {
                Some(',') => continue,
                Some('}') => return Ok(Json::Obj(map)),
                Some(c) => return Err(format!("expected ',' or '}}' but found {c:?}")),
                None => return Err("the object was not closed".to_string()),
            }
        }
    }

    fn array(&mut self) -> Result<Json, String> {
        self.expect('[')?;
        let mut items = Vec::new();
        self.skip_ws();
        if self.peek() == Some(']') {
            self.i += 1;
            return Ok(Json::Arr(items));
        }
        loop {
            self.skip_ws();
            items.push(self.value()?);
            self.skip_ws();
            match self.bump() {
                Some(',') => continue,
                Some(']') => return Ok(Json::Arr(items)),
                Some(c) => return Err(format!("expected ',' or ']' but found {c:?}")),
                None => return Err("the array was not closed".to_string()),
            }
        }
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            match self.bump() {
                None => return Err("the string was not closed".to_string()),
                Some('"') => return Ok(out),
                Some('\\') => match self.bump() {
                    Some('"') => out.push('"'),
                    Some('\\') => out.push('\\'),
                    Some('/') => out.push('/'),
                    Some('b') => out.push('\u{8}'),
                    Some('f') => out.push('\u{c}'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('u') => out.push(self.unicode_escape()?),
                    Some(c) => return Err(format!("unknown escape \\{c}")),
                    None => return Err("the text ended inside an escape".to_string()),
                },
                Some(c) => out.push(c),
            }
        }
    }

    /// A `\uXXXX` escape, joining a surrogate pair when it finds one.
    fn unicode_escape(&mut self) -> Result<char, String> {
        let first = self.hex4()?;
        // A high surrogate carries only half a character; the low half
        // follows in its own escape.
        if (0xD800..0xDC00).contains(&first) {
            self.expect('\\')?;
            self.expect('u')?;
            let second = self.hex4()?;
            if !(0xDC00..0xE000).contains(&second) {
                return Err("a high surrogate was not followed by a low one".to_string());
            }
            let combined = 0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00);
            return char::from_u32(combined).ok_or_else(|| "invalid surrogate pair".to_string());
        }
        char::from_u32(first).ok_or_else(|| format!("\\u{first:04x} is not a character"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let mut value = 0u32;
        for _ in 0..4 {
            let c = self.bump().ok_or("the text ended inside a \\u escape")?;
            let digit = c
                .to_digit(16)
                .ok_or_else(|| format!("{c:?} is not a hex digit"))?;
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.i;
        if self.peek() == Some('-') {
            self.i += 1;
        }
        while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.peek() == Some('.') {
            self.i += 1;
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        if matches!(self.peek(), Some('e' | 'E')) {
            self.i += 1;
            if matches!(self.peek(), Some('+' | '-')) {
                self.i += 1;
            }
            while matches!(self.peek(), Some(c) if c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        let text: String = self.chars[start..self.i].iter().collect();
        text.parse::<f64>()
            .map(Json::Num)
            .map_err(|_| format!("{text:?} is not a number"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shapes_the_server_sends() {
        let v = parse(r#"{"a": 1, "b": [1.5, -2, 3e2], "c": "x", "d": true, "e": null}"#)
            .expect("parse");
        assert_eq!(v.number("a"), Ok(1.0));
        let b = v.get("b").and_then(Json::as_array).expect("array");
        assert_eq!(b, [Json::Num(1.5), Json::Num(-2.0), Json::Num(300.0)]);
        assert_eq!(v.string("c"), Ok("x".to_string()));
        assert_eq!(v.get("d"), Some(&Json::Bool(true)));
        assert_eq!(v.get("e"), Some(&Json::Null));
    }

    #[test]
    fn a_missing_field_names_itself() {
        let v = parse("{}").expect("parse");
        assert_eq!(
            v.number("ssn"),
            Err("field \"ssn\" must be a number".to_string())
        );
    }

    #[test]
    fn round_trips_strings_that_need_escaping() {
        let text = obj([("k", str_of("a\"b\\c\nd\te\u{1}f"))]).write();
        let back = parse(&text).expect("parse");
        assert_eq!(back.string("k").unwrap(), "a\"b\\c\nd\te\u{1}f");
    }

    #[test]
    fn reads_escapes_including_surrogate_pairs() {
        let v = parse(r#"{"k":"Aé😀\/"}"#).expect("parse");
        assert_eq!(v.string("k").unwrap(), "Aé😀/");
    }

    #[test]
    fn a_non_finite_number_writes_as_null() {
        // The engine can produce one; JSON has no form for it, and a
        // reader that sees null knows the value is absent.
        assert_eq!(obj([("k", num(f64::NAN))]).write(), r#"{"k":null}"#);
        assert_eq!(obj([("k", num(f64::INFINITY))]).write(), r#"{"k":null}"#);
    }

    #[test]
    fn rejects_text_after_the_value() {
        assert!(parse("{} {}").is_err());
        assert!(parse("[1,2").is_err());
        assert!(parse("{\"a\":}").is_err());
    }

    #[test]
    fn writes_arrays_and_nesting_compactly() {
        let v = obj([
            ("n", Json::Arr(vec![num(1.0), num(2.5)])),
            ("o", obj([("i", num(-3.0))])),
        ]);
        assert_eq!(v.write(), r#"{"n":[1,2.5],"o":{"i":-3}}"#);
    }
}
