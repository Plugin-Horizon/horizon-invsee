//! A small SNBT reader for the component text of `/invgive minecraft:sword[enchantments={sharpness:5}]`
//! (vanilla's item argument): `key=value` pairs and `!key` removals into the patch `apply_components` takes.
use mod_api::{Compound, Tag};

struct Reader<'a> {
    s: &'a [u8],
    at: usize,
}

type R<T> = Result<T, String>;

impl<'a> Reader<'a> {
    fn ws(&mut self) {
        while self.at < self.s.len() && self.s[self.at].is_ascii_whitespace() {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.s.get(self.at).copied()
    }

    fn eat(&mut self, c: u8) -> bool {
        self.ws();
        if self.peek() == Some(c) {
            self.at += 1;
            true
        } else {
            false
        }
    }

    fn expect(&mut self, c: u8) -> R<()> {
        if self.eat(c) {
            Ok(())
        } else {
            Err(format!("Expected '{}' at position {}", c as char, self.at))
        }
    }

    fn quoted(&mut self) -> R<String> {
        let q = self.s[self.at];
        self.at += 1;
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            self.at += 1;
            if c == b'\\' {
                match self.peek() {
                    Some(n) => {
                        self.at += 1;
                        out.push(match n {
                            b'n' => b'\n',
                            b't' => b'\t',
                            other => other,
                        });
                    }
                    None => break,
                }
            } else if c == q {
                return String::from_utf8(out).map_err(|e| e.to_string());
            } else {
                out.push(c);
            }
        }
        Err("Unclosed quoted string".into())
    }

    fn bare(&mut self) -> String {
        let start = self.at;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.' | b'+' | b':' | b'/') {
                self.at += 1;
            } else {
                break;
            }
        }
        String::from_utf8_lossy(&self.s[start..self.at]).into_owned()
    }

    fn key(&mut self) -> R<String> {
        self.ws();
        match self.peek() {
            Some(b'"') | Some(b'\'') => self.quoted(),
            _ => {
                let k = self.bare();
                if k.is_empty() {
                    Err(format!("Expected a key at position {}", self.at))
                } else {
                    Ok(k)
                }
            }
        }
    }

    fn value(&mut self) -> R<Tag> {
        self.ws();
        match self.peek() {
            Some(b'{') => {
                self.at += 1;
                let mut c = Compound::new();
                if self.eat(b'}') {
                    return Ok(Tag::Compound(c));
                }
                loop {
                    let k = self.key()?;
                    self.expect(b':')?;
                    let v = self.value()?;
                    c.insert(k.as_str().into(), v);
                    if self.eat(b',') {
                        continue;
                    }
                    self.expect(b'}')?;
                    return Ok(Tag::Compound(c));
                }
            }
            Some(b'[') => {
                self.at += 1;
                self.ws();
                // `[B;1b,2b]`, `[I;1,2]`, `[L;1l]` typed arrays.
                if matches!(self.s.get(self.at), Some(b'B' | b'I' | b'L')) && self.s.get(self.at + 1) == Some(&b';') {
                    let kind = self.s[self.at];
                    self.at += 2;
                    let mut nums: Vec<i64> = Vec::new();
                    if !self.eat(b']') {
                        loop {
                            self.ws();
                            let w = self.bare();
                            let t = w.trim_end_matches(|c: char| matches!(c, 'b' | 'B' | 'l' | 'L'));
                            nums.push(t.parse::<i64>().map_err(|_| format!("Not a number: {w}"))?);
                            if self.eat(b',') {
                                continue;
                            }
                            self.expect(b']')?;
                            break;
                        }
                    }
                    return Ok(match kind {
                        b'B' => Tag::ByteArray(nums.iter().map(|n| *n as u8).collect()),
                        b'I' => Tag::IntArray(nums.iter().map(|n| *n as i32).collect()),
                        _ => Tag::LongArray(nums),
                    });
                }
                let mut items = Vec::new();
                if self.eat(b']') {
                    return Ok(Tag::List(items));
                }
                loop {
                    items.push(self.value()?);
                    if self.eat(b',') {
                        continue;
                    }
                    self.expect(b']')?;
                    return Ok(Tag::List(items));
                }
            }
            Some(b'"') | Some(b'\'') => Ok(Tag::String(self.quoted()?.as_str().into())),
            Some(_) => {
                let w = self.bare();
                if w.is_empty() {
                    return Err(format!("Unexpected character at position {}", self.at));
                }
                Ok(scalar(&w))
            }
            None => Err("Unexpected end of text".into()),
        }
    }
}

/// An unquoted word: a number with its suffix, true/false, else a string.
fn scalar(w: &str) -> Tag {
    match w {
        "true" => return Tag::Byte(1),
        "false" => return Tag::Byte(0),
        _ => {}
    }
    let (body, suffix) = match w.chars().last() {
        Some(c) if c.is_ascii_alphabetic() && w.len() > 1 => (&w[..w.len() - 1], c.to_ascii_lowercase()),
        _ => (w, ' '),
    };
    let int = |t: &str| t.parse::<i64>().ok();
    match suffix {
        'b' => int(body).map(|n| Tag::Byte(n as i8)),
        's' => int(body).map(|n| Tag::Short(n as i16)),
        'l' => int(body).map(Tag::Long),
        'f' => body.parse::<f32>().ok().map(|f| Tag::Float(f.to_bits())),
        'd' => body.parse::<f64>().ok().map(|f| Tag::Double(f.to_bits())),
        _ => int(w).map(|n| Tag::Int(n as i32)).or_else(|| {
            if w.contains('.') {
                w.parse::<f64>().ok().map(|f| Tag::Double(f.to_bits()))
            } else {
                None
            }
        }),
    }
    .unwrap_or_else(|| Tag::String(w.into()))
}

/// One SNBT value.
pub fn parse_value(text: &str) -> R<Tag> {
    let mut r = Reader { s: text.as_bytes(), at: 0 };
    let v = r.value()?;
    r.ws();
    if r.at < r.s.len() {
        return Err(format!("Trailing data at position {}", r.at));
    }
    Ok(v)
}

/// `a=1,b={x:2},!c` (the inside of an item's `[...]`) as the component patch: keys get `minecraft:` when they have
/// no namespace; a removal is the key behind `!` with an empty compound.
pub fn parse_components(inner: &str) -> R<Compound> {
    let mut r = Reader { s: inner.as_bytes(), at: 0 };
    let mut patch = Compound::new();
    r.ws();
    if r.at >= r.s.len() {
        return Ok(patch);
    }
    loop {
        r.ws();
        let removal = r.peek() == Some(b'!');
        if removal {
            r.at += 1;
        }
        let key = r.key()?;
        let key = if key.contains(':') { key } else { format!("minecraft:{key}") };
        if removal {
            patch.insert(format!("!{key}").as_str().into(), Tag::Compound(Compound::new()));
        } else {
            r.expect(b'=')?;
            let v = r.value()?;
            patch.insert(key.as_str().into(), v);
        }
        if r.eat(b',') {
            continue;
        }
        r.ws();
        if r.at < r.s.len() {
            return Err(format!("Expected ',' or ']' at position {}", r.at));
        }
        return Ok(patch);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalars_lists_and_compounds() {
        assert_eq!(parse_value("5").unwrap(), Tag::Int(5));
        assert_eq!(parse_value("5b").unwrap(), Tag::Byte(5));
        assert_eq!(parse_value("true").unwrap(), Tag::Byte(1));
        assert_eq!(parse_value("2.5f").unwrap(), Tag::Float(2.5f32.to_bits()));
        assert_eq!(parse_value("'it\\'s'").unwrap(), Tag::String("it's".into()));
        assert_eq!(parse_value("[1,2]").unwrap(), Tag::List(vec![Tag::Int(1), Tag::Int(2)]));
        assert_eq!(parse_value("[I;1,2]").unwrap(), Tag::IntArray(vec![1, 2]));
        let c = parse_value("{a:1, \"b c\":[x]}").unwrap();
        let Tag::Compound(c) = c else { panic!() };
        assert_eq!(c.len(), 2);
        assert!(parse_value("{a:1").is_err());
        assert!(parse_value("1 2").is_err());
    }

    #[test]
    fn components_take_a_namespace_and_removals() {
        let p = parse_components("custom_name='\"Hi\"',!minecraft:lore,minecraft:damage=3").unwrap();
        let keys: Vec<String> = p.keys().map(|k| k.to_string_lossy()).collect();
        assert!(keys.contains(&"minecraft:custom_name".to_string()));
        assert!(keys.contains(&"!minecraft:lore".to_string()));
        assert!(keys.contains(&"minecraft:damage".to_string()));
        assert!(parse_components("").unwrap().is_empty());
        assert!(parse_components("a").is_err());
    }
}
