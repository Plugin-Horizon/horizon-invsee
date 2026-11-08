//! The mod's settings: `config/invsee-server.toml` (the game's mod config system writes the
//! default file when it is missing and reloads it when it changes).
use serde::Deserialize;

/// The default file, as written next to the server when missing.
pub const DEFAULT: &str = include_str!("../invsee-server.toml");

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub access: Access,
    pub players: Players,
    pub titles: Titles,
    pub templates: Templates,
    pub menu: Menu,
    pub logging: Logging,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Access {
    pub invsee_view: u8,
    pub invsee_edit: u8,
    pub endersee_view: u8,
    pub endersee_edit: u8,
    pub reload: u8,
    pub bypass_exempt: u8,
    pub exempt_invsee: Vec<String>,
    pub exempt_endersee: Vec<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Resolver {
    Online,
    LoggedOut,
    Usercache,
    Mojang,
    Offline,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Players {
    pub offline: bool,
    pub unknown: bool,
    pub tabcomplete_offline: bool,
    pub uuid_resolve: Vec<Resolver>,
    pub name_resolve: Vec<Resolver>,
    pub mojang_timeout_ms: u64,
    pub offline_refresh_ticks: u64,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Titles {
    pub inventory: String,
    pub enderchest: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Templates {
    pub inventory: String,
    pub enderchest: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Palette {
    #[serde(rename = "glass panes")]
    GlassPanes,
    #[serde(rename = "icons")]
    Icons,
    #[serde(rename = "empty")]
    Empty,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Menu {
    pub placeholder_palette: Palette,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Granularity {
    #[serde(rename = "LOG_NEVER")]
    Never,
    #[serde(rename = "LOG_ON_CLOSE")]
    OnClose,
    #[serde(rename = "LOG_EVERY_CHANGE")]
    EveryChange,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Output {
    #[serde(rename = "SERVER_LOG_FILE")]
    ServerLogFile,
    #[serde(rename = "PLUGIN_LOG_FILE")]
    PluginLogFile,
    #[serde(rename = "SPECTATOR_LOG_FILE")]
    SpectatorLogFile,
    #[serde(rename = "CONSOLE")]
    Console,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Logging {
    pub granularity: Granularity,
    pub output: Vec<Output>,
    pub format_server_log_file: String,
    pub format_plugin_log_file: String,
    pub format_spectator_log_file: String,
    pub format_console: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            access: Access::default(),
            players: Players::default(),
            titles: Titles::default(),
            templates: Templates::default(),
            menu: Menu::default(),
            logging: Logging::default(),
        }
    }
}

// The values of the default file (a test keeps them equal).
impl Default for Access {
    fn default() -> Self {
        // plugin.yml: view, edit and reload are `default: op` (any operator, level 1);
        // bypass-exempt is `default: false` (nobody), which is level 5 here.
        Access {
            invsee_view: 1,
            invsee_edit: 1,
            endersee_view: 1,
            endersee_edit: 1,
            reload: 1,
            bypass_exempt: 5,
            exempt_invsee: Vec::new(),
            exempt_endersee: Vec::new(),
        }
    }
}

impl Default for Players {
    fn default() -> Self {
        use Resolver::*;
        Players {
            offline: true,
            unknown: true,
            tabcomplete_offline: true,
            uuid_resolve: vec![Online, LoggedOut, Usercache, Mojang],
            name_resolve: vec![Online, LoggedOut, Usercache, Mojang],
            mojang_timeout_ms: 5000,
            offline_refresh_ticks: 20,
        }
    }
}

impl Default for Titles {
    fn default() -> Self {
        Titles {
            inventory: "<player>'s inventory".into(),
            enderchest: "<player>'s enderchest".into(),
        }
    }
}

impl Default for Templates {
    fn default() -> Self {
        Templates {
            inventory: crate::template::DEFAULT_INVENTORY.into(),
            enderchest: crate::template::DEFAULT_ENDERCHEST.into(),
        }
    }
}

impl Default for Menu {
    fn default() -> Self {
        Menu {
            placeholder_palette: Palette::GlassPanes,
        }
    }
}

const SERVER_LOG: &str = "\nSpectator UUID: <spectator_uuid>\nSpectator Name: <spectator_name>\nTaken         : <taken>\nGiven         : <given>\nTarget        : <target>";
const PLUGIN_LOG: &str = "[<date> <time>] [<level>]:\nSpectator UUID: <spectator_uuid>\nSpectator Name: <spectator_name>\nTaken         : <taken>\nGiven         : <given>\nTarget        : <target>";
const SPECTATOR_LOG: &str = "[<date> <time>] [<level>]:\nTaken         : <taken>\nGiven         : <given>\nTarget        : <target>";

impl Default for Logging {
    fn default() -> Self {
        Logging {
            granularity: Granularity::OnClose,
            output: vec![Output::PluginLogFile],
            format_server_log_file: SERVER_LOG.into(),
            format_plugin_log_file: PLUGIN_LOG.into(),
            format_spectator_log_file: SPECTATOR_LOG.into(),
            format_console: PLUGIN_LOG.into(),
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Palette::GlassPanes
    }
}

impl Config {
    /// The file's settings, or why it can't be read (the caller keeps what it had).
    pub fn parse(text: &str) -> Result<Config, String> {
        let mut c: Config = toml::from_str(text).map_err(|e| e.to_string())?;
        for l in [
            &mut c.access.invsee_view,
            &mut c.access.invsee_edit,
            &mut c.access.endersee_view,
            &mut c.access.endersee_edit,
            &mut c.access.reload,
        ] {
            *l = (*l).min(4);
        }
        // 5 = nobody (no operator has it).
        c.access.bypass_exempt = c.access.bypass_exempt.min(5);
        // Edit implies view.
        c.access.invsee_edit = c.access.invsee_edit.max(c.access.invsee_view);
        c.access.endersee_edit = c.access.endersee_edit.max(c.access.endersee_view);
        c.players.offline_refresh_ticks = c.players.offline_refresh_ticks.max(1);
        c.players.mojang_timeout_ms = c.players.mojang_timeout_ms.clamp(100, 60_000);
        Ok(c)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_file_is_the_default() {
        let c = Config::parse(DEFAULT).unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.access.invsee_view, 1);
        assert_eq!(c.access.bypass_exempt, 5);
        assert!(c.players.offline && c.players.unknown);
        assert_eq!(c.logging.granularity, Granularity::OnClose);
        assert_eq!(c.logging.output, vec![Output::PluginLogFile]);
        assert_eq!(c.menu.placeholder_palette, Palette::GlassPanes);
        assert_eq!(c.titles.inventory, "<player>'s inventory");
    }

    #[test]
    fn missing_keys_take_the_default_and_levels_are_kept_sane() {
        let c = Config::parse("[access]\ninvsee_view = 3\ninvsee_edit = 1\nreload = 9\n").unwrap();
        assert_eq!(c.access.invsee_view, 3);
        assert_eq!(c.access.invsee_edit, 3, "edit implies view");
        assert_eq!(c.access.reload, 4);
        assert_eq!(c.access.endersee_view, 1);
        assert_eq!(c.players.uuid_resolve.len(), 4);
    }

    #[test]
    fn a_wrong_file_is_refused() {
        assert!(Config::parse("[access]\nnope = 1\n").is_err());
        assert!(Config::parse("[menu]\nplaceholder_palette = \"neon\"\n").is_err());
        assert!(Config::parse("[logging]\ngranularity = \"SOMETIMES\"\n").is_err());
    }
}
