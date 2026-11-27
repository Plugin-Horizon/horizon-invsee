//! The `proxy` resolve strategy (`UUIDBungeeCordStrategy`): BungeeCord's plugin messaging channel answers a name with
//! the UUID the proxy knows (`UUIDOther`). The message is `writeUTF("UUIDOther")`, `writeUTF(name)`; the reply is
//! `writeUTF("UUIDOther")`, `writeUTF(name)`, `writeUTF(uuid without dashes)`.

use super::names::parse_uuid_lenient;

/// The channel Velocity and BungeeCord read plugin messages of a backend on (Paper's legacy `BungeeCord`, namespaced).
pub const CHANNEL: &str = "bungeecord:main";
/// Ticks the original waits for the answer (`20 * 5`).
pub const TIMEOUT_TICKS: u64 = 100;

fn write_utf(out: &mut Vec<u8>, s: &str) {
    out.extend_from_slice(&(s.len() as u16).to_be_bytes());
    out.extend_from_slice(s.as_bytes());
}

fn read_utf<'a>(data: &mut &'a [u8]) -> Option<&'a str> {
    let len = u16::from_be_bytes([*data.first()?, *data.get(1)?]) as usize;
    let body = data.get(2..2 + len)?;
    *data = &data[2 + len..];
    std::str::from_utf8(body).ok()
}

/// The request for the UUID of `name`.
pub fn request(name: &str) -> Vec<u8> {
    let mut out = Vec::new();
    write_utf(&mut out, "UUIDOther");
    write_utf(&mut out, name);
    out
}

/// The name and UUID of a `UUIDOther` reply; None for any other message.
pub fn parse_reply(mut data: &[u8]) -> Option<(String, u128)> {
    if read_utf(&mut data)? != "UUIDOther" {
        return None;
    }
    let name = read_utf(&mut data)?.to_owned();
    let uuid = parse_uuid_lenient(read_utf(&mut data)?)?;
    Some((name, uuid))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_request_is_two_utf_strings() {
        let mut want = vec![0, 9];
        want.extend_from_slice(b"UUIDOther");
        want.extend_from_slice(&[0, 3]);
        want.extend_from_slice(b"Bob");
        assert_eq!(request("Bob"), want);
    }

    #[test]
    fn the_reply_carries_the_name_and_an_undashed_uuid() {
        let mut data = request("Bob");
        data.extend_from_slice(&[0, 32]);
        data.extend_from_slice(b"069a79f444e94726a5befca90e38aaf5");
        assert_eq!(parse_reply(&data), Some(("Bob".to_owned(), 0x069a79f4_44e9_4726_a5be_fca90e38aaf5)));
    }

    #[test]
    fn other_messages_and_short_data_are_not_replies() {
        let mut other = vec![0, 8];
        other.extend_from_slice(b"PlayerIP");
        other.extend_from_slice(&[0, 1, b'x']);
        assert_eq!(parse_reply(&other), None);
        assert_eq!(parse_reply(&[0, 9, b'U']), None);
        assert_eq!(parse_reply(&request("Bob")), None);
        assert_eq!(parse_reply(&[]), None);
    }
}
