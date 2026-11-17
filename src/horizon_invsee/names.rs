//! Players by name and by UUID: what was typed, the players who left, the server's
//! usercache.json and offline-mode UUIDs.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// A UUID as typed: with or without dashes, any case.
pub fn parse_uuid(s: &str) -> Option<u128> {
    let hex: String = s.chars().filter(|c| *c != '-').collect();
    if hex.len() != 32 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    // Dashes only where a UUID has them.
    if s.contains('-') {
        let parts: Vec<usize> = s.split('-').map(str::len).collect();
        if parts != [8, 4, 4, 4, 12] {
            return None;
        }
    }
    u128::from_str_radix(&hex, 16).ok()
}

/// `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
pub fn format_uuid(u: u128) -> String {
    let h = format!("{u:032x}");
    format!(
        "{}-{}-{}-{}-{}",
        &h[0..8],
        &h[8..12],
        &h[12..16],
        &h[16..20],
        &h[20..32]
    )
}

/// What a Minecraft name may be (3 to 16 of `A-Za-z0-9_`); older accounts and offline names
/// can be shorter, so 1 to 16 is accepted.
pub fn valid_name(s: &str) -> bool {
    (1..=16).contains(&s.len()) && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The UUID an offline-mode server gives `name`: version 3 (MD5) of `OfflinePlayer:<name>`,
/// as `UUIDUtil.createOfflinePlayerUUID` makes it.
pub fn offline_uuid(name: &str) -> u128 {
    let mut d = md5(format!("OfflinePlayer:{name}").as_bytes());
    d[6] = (d[6] & 0x0f) | 0x30;
    d[8] = (d[8] & 0x3f) | 0x80;
    u128::from_be_bytes(d)
}

/// MD5 (RFC 1321), for `offline_uuid` only: the lock file has no MD5 crate.
pub fn md5(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5,
        9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10,
        15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64)
        .map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32)
        .collect();
    let mut msg = input.to_vec();
    let bits = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    let (mut a0, mut b0, mut c0, mut d0) = (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk
            .chunks(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    for (i, v) in [a0, b0, c0, d0].iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&v.to_le_bytes());
    }
    out
}

/// Names and UUIDs both ways, names matched without case.
#[derive(Default, Debug, Clone)]
pub struct Directory {
    by_name: HashMap<String, (u128, String)>,
    by_uuid: HashMap<u128, String>,
}

impl Directory {
    pub fn add(&mut self, uuid: u128, name: &str) {
        if let Some(old) = self.by_uuid.insert(uuid, name.to_owned()) {
            if !old.eq_ignore_ascii_case(name) {
                self.by_name.remove(&old.to_lowercase());
            }
        }
        self.by_name
            .insert(name.to_lowercase(), (uuid, name.to_owned()));
    }
    pub fn uuid(&self, name: &str) -> Option<(u128, String)> {
        self.by_name.get(&name.to_lowercase()).cloned()
    }
    pub fn name(&self, uuid: u128) -> Option<String> {
        self.by_uuid.get(&uuid).cloned()
    }
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.by_uuid.values().map(String::as_str)
    }
    pub fn len(&self) -> usize {
        self.by_uuid.len()
    }
    pub fn is_empty(&self) -> bool {
        self.by_uuid.is_empty()
    }
}

/// usercache.json's entries (`[{"name", "uuid", "expiresOn"}]`); expired entries are still
/// a good guess for an offline lookup, so they are kept. Broken entries are skipped.
pub fn parse_usercache(text: &str) -> Directory {
    let mut d = Directory::default();
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str::<serde_json::Value>(text) else {
        return d;
    };
    // The file lists the most recently used first: add the oldest first, so a name that moved
    // to another account ends on the newest.
    for item in items.iter().rev() {
        let (Some(name), Some(uuid)) = (
            item.get("name").and_then(|v| v.as_str()),
            item.get("uuid").and_then(|v| v.as_str()).and_then(parse_uuid),
        ) else {
            continue;
        };
        if valid_name(name) {
            d.add(uuid, name);
        }
    }
    d
}

/// usercache.json, read again only when the file changed.
#[derive(Default)]
pub struct Usercache {
    path: Option<PathBuf>,
    stamp: Option<SystemTime>,
    dir: Directory,
}

impl Usercache {
    pub fn get(&mut self, path: &Path) -> &Directory {
        let stamp = std::fs::metadata(path).and_then(|m| m.modified()).ok();
        if self.path.as_deref() != Some(path) || stamp != self.stamp {
            self.dir = std::fs::read_to_string(path)
                .map(|t| parse_usercache(&t))
                .unwrap_or_default();
            self.path = Some(path.to_owned());
            self.stamp = stamp;
        }
        &self.dir
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }

    #[test]
    fn md5_matches_rfc_1321() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            hex(&md5(b"12345678901234567890123456789012345678901234567890123456789012345678901234567890")),
            "57edf4a22be3c955ac49da2e2107b67a"
        );
    }

    #[test]
    fn offline_uuids_are_version_3_of_the_name() {
        let u = offline_uuid("Steve");
        assert_eq!((u >> 76) & 0xf, 3, "version");
        assert_eq!((u >> 62) & 0b11, 0b10, "variant");
        assert_eq!(u, offline_uuid("Steve"));
        assert_ne!(u, offline_uuid("steve"), "case counts, as in vanilla");
        // Same bytes as md5 with the version and variant bits set.
        let d = md5(b"OfflinePlayer:Steve");
        assert_eq!(format_uuid(u)[..8], hex(&d[..4]));
    }

    #[test]
    fn uuids_parse_with_or_without_dashes() {
        let u = 0x0123456789abcdef0123456789abcdefu128;
        assert_eq!(parse_uuid("0123456789abcdef0123456789abcdef"), Some(u));
        assert_eq!(parse_uuid("01234567-89AB-cdef-0123-456789abcdef"), Some(u));
        assert_eq!(format_uuid(u), "01234567-89ab-cdef-0123-456789abcdef");
        assert_eq!(parse_uuid("0123-456789abcdef0123456789abcdef"), None);
        assert_eq!(parse_uuid("Steve"), None);
        assert_eq!(parse_uuid("0123456789abcdef0123456789abcdeg"), None);
    }

    #[test]
    fn the_usercache_resolves_both_ways_without_case() {
        // Synthetic entries only.
        let text = r#"[
            {"name":"Alice_1","uuid":"00000000-0000-0000-0000-000000000001","expiresOn":"2026-12-01 10:00:00 +0000"},
            {"name":"Bob","uuid":"00000000-0000-0000-0000-000000000002","expiresOn":"2026-12-01 10:00:00 +0000"},
            {"name":"broken"},
            {"name":"not a name","uuid":"00000000-0000-0000-0000-000000000003","expiresOn":"x"}
        ]"#;
        let d = parse_usercache(text);
        assert_eq!(d.len(), 2);
        assert_eq!(d.uuid("alice_1"), Some((1, "Alice_1".into())));
        assert_eq!(d.name(2).as_deref(), Some("Bob"));
        assert!(parse_usercache("not json").is_empty());
    }

    #[test]
    fn a_renamed_player_keeps_one_entry() {
        let mut d = Directory::default();
        d.add(7, "OldName");
        d.add(7, "NewName");
        assert_eq!(d.uuid("oldname"), None);
        assert_eq!(d.uuid("newname"), Some((7, "NewName".into())));
        assert_eq!(d.len(), 1);
    }
}
