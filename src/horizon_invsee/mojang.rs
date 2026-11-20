//! Mojang's profile API, asked on a thread of its own so the server thread never waits:
//! a name to its UUID, a UUID to its name.
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::time::Duration;

/// What a lookup found: the profile's UUID and name as Mojang spells it, or nothing.
pub type Found = Option<(u128, String)>;

/// A lookup under way.
pub struct Lookup {
    rx: Receiver<Found>,
}

pub enum Poll {
    Pending,
    Done(Found),
}

impl Lookup {
    pub fn poll(&self) -> Poll {
        match self.rx.try_recv() {
            Ok(found) => Poll::Done(found),
            Err(TryRecvError::Empty) => Poll::Pending,
            Err(TryRecvError::Disconnected) => Poll::Done(None),
        }
    }

    /// Asks for `name`'s profile.
    pub fn by_name(name: &str, timeout: Duration) -> Lookup {
        let url = format!("https://api.mojang.com/users/profiles/minecraft/{name}");
        Self::spawn(url, timeout)
    }

    /// Asks for the profile of `uuid`.
    pub fn by_uuid(uuid: u128, timeout: Duration) -> Lookup {
        let url = format!("https://sessionserver.mojang.com/session/minecraft/profile/{uuid:032x}");
        Self::spawn(url, timeout)
    }

    /// Asks the Electroid mirror (`ashcon.app`) for `name`'s profile.
    pub fn by_name_electroid(name: &str, timeout: Duration) -> Lookup {
        Self::spawn_with(format!("https://api.ashcon.app/mojang/v2/user/{name}"), timeout, parse_electroid)
    }

    /// Asks the Electroid mirror for the profile of `uuid`.
    pub fn by_uuid_electroid(uuid: u128, timeout: Duration) -> Lookup {
        let dashed = format!(
            "{:08x}-{:04x}-{:04x}-{:04x}-{:012x}",
            (uuid >> 96) as u32,
            ((uuid >> 80) & 0xffff) as u16,
            ((uuid >> 64) & 0xffff) as u16,
            ((uuid >> 48) & 0xffff) as u16,
            (uuid & 0xffff_ffff_ffff) as u64
        );
        Self::spawn_with(format!("https://api.ashcon.app/mojang/v2/user/{dashed}"), timeout, parse_electroid)
    }

    /// A lookup that already has its answer (tests, a resolver that is switched off).
    pub fn ready(found: Found) -> Lookup {
        let (tx, rx) = channel();
        let _ = tx.send(found);
        Lookup { rx }
    }

    fn spawn(url: String, timeout: Duration) -> Lookup {
        Self::spawn_with(url, timeout, parse_profile)
    }

    fn spawn_with(url: String, timeout: Duration, parse: fn(&str) -> Found) -> Lookup {
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("invsee-mojang".into())
            .spawn(move || {
                let _ = tx.send(fetch(&url, timeout, parse));
            });
        if spawned.is_err() {
            return Lookup::ready(None);
        }
        Lookup { rx }
    }
}

fn fetch(url: &str, timeout: Duration, parse: fn(&str) -> Found) -> Found {
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    // 204 or 404: no such profile; any error is "not found" too.
    let resp = agent.get(url).call().ok()?;
    if resp.status() != 200 {
        return None;
    }
    parse(&resp.into_string().ok()?)
}

/// `{"id": "<32 hex>", "name": "..."}`, as both endpoints answer.
pub fn parse_profile(body: &str) -> Found {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let id = crate::horizon_invsee::names::parse_uuid_lenient(v.get("id")?.as_str()?)?;
    let name = v.get("name")?.as_str()?;
    crate::horizon_invsee::names::valid_name(name).then(|| (id, name.to_owned()))
}

/// `{"uuid": "<dashed>", "username": "..."}`, as the Electroid mirror answers.
pub fn parse_electroid(body: &str) -> Found {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let id = crate::horizon_invsee::names::parse_uuid_lenient(v.get("uuid")?.as_str()?)?;
    let name = v.get("username")?.as_str()?;
    crate::horizon_invsee::names::valid_name(name).then(|| (id, name.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_parse() {
        let body = r#"{"id":"0000000000000000000000000000002a","name":"Some_One"}"#;
        assert_eq!(parse_profile(body), Some((42, "Some_One".into())));
        assert_eq!(parse_profile(r#"{"errorMessage":"nope"}"#), None);
        assert_eq!(parse_profile(""), None);
    }

    #[test]
    fn electroid_profiles_parse() {
        let body = r#"{"uuid":"00000000-0000-0000-0000-00000000002a","username":"Some_One"}"#;
        assert_eq!(parse_electroid(body), Some((42, "Some_One".into())));
        assert_eq!(parse_electroid(r#"{"code":404}"#), None);
    }

    #[test]
    fn a_ready_lookup_answers_at_once() {
        match Lookup::ready(Some((1, "A".into()))).poll() {
            Poll::Done(Some((1, n))) => assert_eq!(n, "A"),
            _ => panic!(),
        }
    }
}
