//! What spectators changed, written where `[logging] output` says.
use crate::config::{Logging, Output};
use std::io::Write;
use std::path::Path;

/// A Bukkit `Material` name for an item id: `minecraft:diamond` is `DIAMOND`, as InvSee++ logs it.
pub fn material(id: &str) -> String {
    id.rsplit(':').next().unwrap_or(id).to_uppercase()
}

/// Items as the log lists them: `[DIAMOND x 5, STONE x 64]`, equal items summed, in the order
/// they first moved (InvSee++ sums per item type, its order is a hash order).
pub fn items(list: &[(String, u32)]) -> String {
    let mut summed: Vec<(String, u32)> = Vec::new();
    for (item, n) in list {
        match summed.iter_mut().find(|(i, _)| i == item) {
            Some((_, c)) => *c += n,
            None => summed.push((item.clone(), *n)),
        }
    }
    let parts: Vec<String> = summed
        .iter()
        .map(|(i, n)| format!("{} x {n}", material(i)))
        .collect();
    format!("[{}]", parts.join(", "))
}

/// One entry: who looked at whose inventory and what moved.
pub struct Entry<'a> {
    pub spectator: u128,
    pub spectator_name: &'a str,
    pub target: &'a str,
    pub taken: &'a [(String, u32)],
    pub given: &'a [(String, u32)],
}

/// `format` with its macros filled in, at the local time `now`.
pub fn fill(format: &str, e: &Entry<'_>, now: chrono::DateTime<chrono::Local>) -> String {
    format
        .replace("<date>", &now.format("%Y-%m-%d").to_string())
        .replace("<time>", &now.format("%H:%M:%S").to_string())
        .replace("<level>", "INFO")
        .replace("<spectator_uuid>", &crate::names::format_uuid(e.spectator))
        .replace("<spectator_name>", e.spectator_name)
        .replace("<taken>", &items(e.taken))
        .replace("<given>", &items(e.given))
        .replace("<target>", e.target)
}

fn append(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{text}")
}

/// Writes `e` to every output; lines for the server's log and console come back for the
/// caller to print. `dir` holds the log files.
pub fn write(cfg: &Logging, dir: &Path, e: &Entry<'_>) -> Vec<String> {
    let now = chrono::Local::now();
    let mut console = Vec::new();
    for out in &cfg.output {
        match out {
            Output::ServerLogFile => console.push(fill(&cfg.format_server_log_file, e, now)),
            Output::Console => console.push(fill(&cfg.format_console, e, now)),
            Output::PluginLogFile => {
                let text = fill(&cfg.format_plugin_log_file, e, now);
                if let Err(err) = append(&dir.join("_global.log"), &text) {
                    console.push(format!("invsee: can't write its log: {err}"));
                }
            }
            Output::SpectatorLogFile => {
                let text = fill(&cfg.format_spectator_log_file, e, now);
                let file = dir.join(format!("{}.log", crate::names::format_uuid(e.spectator)));
                if let Err(err) = append(&file, &text) {
                    console.push(format!("invsee: can't write its log: {err}"));
                }
            }
        }
    }
    console
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn entry<'a>(taken: &'a [(String, u32)], given: &'a [(String, u32)]) -> Entry<'a> {
        Entry {
            spectator: 1,
            spectator_name: "Admin",
            target: "Someone",
            taken,
            given,
        }
    }

    #[test]
    fn items_are_summed_in_order() {
        let l = vec![
            ("minecraft:stone".to_string(), 3),
            ("minecraft:dirt".to_string(), 1),
            ("minecraft:stone".to_string(), 2),
        ];
        assert_eq!(items(&l), "[STONE x 5, DIRT x 1]");
        assert_eq!(items(&[]), "[]");
    }

    #[test]
    fn macros_are_filled() {
        let taken = vec![("minecraft:diamond".to_string(), 2)];
        let now = chrono::Local.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let s = fill(
            "[<date> <time>] [<level>] <spectator_name> <spectator_uuid> took <taken> gave <given> from <target>",
            &entry(&taken, &[]),
            now,
        );
        assert_eq!(
            s,
            "[2026-01-02 03:04:05] [INFO] Admin 00000000-0000-0000-0000-000000000001 took [DIAMOND x 2] gave [] from Someone"
        );
    }

    #[test]
    fn files_are_appended() {
        let dir = std::env::temp_dir().join(format!("invsee-audit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut cfg = Logging::default();
        cfg.output = vec![Output::PluginLogFile, Output::SpectatorLogFile, Output::Console];
        let taken = vec![("minecraft:stone".to_string(), 1)];
        let console = write(&cfg, &dir, &entry(&taken, &[]));
        assert_eq!(console.len(), 1);
        write(&cfg, &dir, &entry(&taken, &[]));
        let plugin = std::fs::read_to_string(dir.join("_global.log")).unwrap();
        assert_eq!(plugin.matches("Spectator Name: Admin").count(), 2);
        let own = std::fs::read_to_string(dir.join("00000000-0000-0000-0000-000000000001.log")).unwrap();
        assert!(own.contains("Taken         : [STONE x 1]"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
