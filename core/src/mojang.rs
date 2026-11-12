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

    /// A lookup that already has its answer (tests, a resolver that is switched off).
    pub fn ready(found: Found) -> Lookup {
        let (tx, rx) = channel();
        let _ = tx.send(found);
        Lookup { rx }
    }

    fn spawn(url: String, timeout: Duration) -> Lookup {
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new()
            .name("invsee-mojang".into())
            .spawn(move || {
                let _ = tx.send(fetch(&url, timeout));
            });
        if spawned.is_err() {
            return Lookup::ready(None);
        }
        Lookup { rx }
    }
}

fn fetch(url: &str, timeout: Duration) -> Found {
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    // 204 or 404: no such profile; any error is "not found" too.
    let resp = agent.get(url).call().ok()?;
    if resp.status() != 200 {
        return None;
    }
    parse_profile(&resp.into_string().ok()?)
}

/// `{"id": "<32 hex>", "name": "..."}`, as both endpoints answer.
pub fn parse_profile(body: &str) -> Found {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let id = crate::names::parse_uuid(v.get("id")?.as_str()?)?;
    let name = v.get("name")?.as_str()?;
    crate::names::valid_name(name).then(|| (id, name.to_owned()))
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
    fn a_ready_lookup_answers_at_once() {
        match Lookup::ready(Some((1, "A".into()))).poll() {
            Poll::Done(Some((1, n))) => assert_eq!(n, "A"),
            _ => panic!(),
        }
    }
}
