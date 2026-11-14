//! invsee for Horizon: `/invsee` and `/endersee` open a player's inventory or ender chest, online
//! or offline, as a chest view the spectator can take items from and put items into.
//!
//! This crate holds the whole behaviour and no game type: the game side (a support module of
//! the bundle) implements [`Host`] over Horizon's server menus (API 10) and player inventories
//! (API 132) and forwards the hooks here. Everything runs on the server thread; a Mojang lookup
//! runs on a thread of its own and is polled from the tick.
//!
//! A port of InvSee++ (LGPL-2.1, Jannyboy11): its commands, messages, defaults and log formats
//! follow the original's source (see README.md, NOTICE and STATUS.md).
pub mod audit;
pub mod config;
pub mod mojang;
pub mod names;
pub mod template;

use config::{Config, Granularity, Palette, Resolver};
use mojang::{Lookup, Poll};
use names::{parse_uuid, valid_name, Directory, Usercache};
use std::path::PathBuf;
use std::time::Duration;
use template::{Cell, Layout, Unavailable};

/// Bukkit's default permission message (`Command.testPermission`), sent when a sender may not run a command.
const NO_PERMISSION: &str = "§cI'm sorry, but you do not have permission to perform this command. Please contact the server administrators if you believe that this is in error.";
/// The name shown for a UUID nobody can name (InvSee++ uses the same placeholder).
const UNKNOWN_NAME: &str = "InvSee++ Player";
/// The folder under the mod's data folder (InvSee++ names its own `InvSee++ logs`).
const LOG_FOLDER: &str = "InvSee++ logs";

/// The usage line of a view command (plugin.yml's `usage`).
fn usage(kind: Kind) -> String {
    match kind {
        Kind::Inventory => "/invsee <username>|<uuid>".into(),
        Kind::Ender => "/endersee <username>|<uuid>".into(),
    }
}

/// The permission node of `action` (`view`, `edit`, `bypass-exempt`) on a kind of view.
fn node(kind: Kind, action: &str) -> String {
    let word = match kind {
        Kind::Inventory => "invsee",
        Kind::Ender => "endersee",
    };
    format!("invseeplusplus.{word}.{action}")
}

/// The target-inventory slot an item is worn in, by its id: armour, and the off hand for a
/// shield or a totem.
fn worn_slot(item: &str) -> Option<usize> {
    let id = item.strip_prefix("minecraft:").unwrap_or(item);
    if id.ends_with("_helmet") {
        Some(template::HEAD)
    } else if id.ends_with("_chestplate") || id == "elytra" {
        Some(template::CHEST)
    } else if id.ends_with("_leggings") {
        Some(template::LEGS)
    } else if id.ends_with("_boots") {
        Some(template::FEET)
    } else if id == "shield" || id == "totem_of_undying" {
        Some(template::OFF_HAND)
    } else {
        None
    }
}

/// Which of the target's inventories a view shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    /// 41 slots: 0..9 the hotbar, 9..36 the rest, 36..40 feet, legs, chest, head, 40 the off hand.
    Inventory,
    /// 27 slots.
    Ender,
}

impl Kind {
    pub fn size(self) -> usize {
        match self {
            Kind::Inventory => 41,
            Kind::Ender => 27,
        }
    }
}

/// A player on the server.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Online {
    pub uuid: u128,
    pub name: String,
    /// Operator level, 0 to 4.
    pub level: u8,
}

/// A player's inventory as the game read it: live, or from their file.
#[derive(Clone, Debug, PartialEq)]
pub struct Inv<S> {
    pub slots: Vec<Option<S>>,
    pub online: bool,
}

/// Why an inventory can't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvError {
    /// For now: logging in, dead, or read from inside a player's turn.
    Busy(String),
    /// No player file: they never played here.
    NeverPlayed,
    Other(String),
}

impl InvError {
    /// Horizon's `native_inventory` error text (API 132) sorted; the strings are its own.
    pub fn from_horizon(e: &str) -> InvError {
        match e {
            "no saved player" => InvError::NeverPlayed,
            "the player is logging in" | "the player is dead" | "inside a player's turn" => {
                InvError::Busy(e.trim_start_matches("the player is ").to_owned())
            }
            _ => InvError::Other(e.to_owned()),
        }
    }
}

/// What the mod needs from the game. The game side implements it over `Game`.
pub trait Host {
    type Stack: Clone + PartialEq;
    /// The players on the server.
    fn players(&self) -> Vec<Online>;
    /// `uuid`'s inventory `kind` (`native_inventory`).
    fn inventory(&self, uuid: u128, kind: Kind) -> Result<Inv<Self::Stack>, InvError>;
    /// One whole stack moved, checked against `expected` (`native_move_stack`).
    fn move_stack(
        &mut self,
        from: (u128, Kind, usize),
        expected: &Self::Stack,
        to: (u128, Kind),
    ) -> Result<(), String>;
    /// Opens a chest view of `rows` rows; its window id (`native_open_menu`).
    fn open_menu(
        &mut self,
        viewer: u128,
        rows: u8,
        title: &str,
        items: Vec<Option<Self::Stack>>,
    ) -> Option<i32>;
    /// Opens a chest view whose shown slots ARE the target's (`native_open_inventory_menu`):
    /// the game moves items by vanilla's click rules, live or on the target's file. `layout[i]`
    /// is the target slot menu slot `i` shows. None when the game has no such menu (or refuses
    /// it); the mod then falls back to `open_menu` and its own whole-stack transfers.
    fn open_inventory_menu(
        &mut self,
        _viewer: u128,
        _rows: u8,
        _title: &str,
        _target: (u128, Kind),
        _layout: &[Option<usize>],
    ) -> Option<i32> {
        None
    }
    /// Puts `new` in slot `slot` of `uuid`'s inventory `kind` (armour and the off hand too),
    /// which must still hold exactly `expected`; returns what was there
    /// (`native_set_inventory_slot`). The default refuses, and the mod falls back to
    /// `move_stack`.
    fn set_slot(
        &mut self,
        _uuid: u128,
        _kind: Kind,
        _slot: usize,
        _expected: Option<&Self::Stack>,
        _new: Option<Self::Stack>,
    ) -> Result<Option<Self::Stack>, String> {
        Err("the game cannot set a single slot".into())
    }
    /// The same stack with another count (to split one); None when the game cannot.
    fn with_count(&self, _stack: &Self::Stack, _count: u32) -> Option<Self::Stack> {
        None
    }
    /// A player the server knows by name (on, or in its profile cache): id and name as written
    /// (`native_profile_by_name`).
    fn profile_by_name(&self, _name: &str) -> Option<(u128, String)> {
        None
    }
    /// The name of a player the server knows by id (`native_profile_by_uuid`).
    fn profile_by_uuid(&self, _uuid: u128) -> Option<String> {
        None
    }
    /// Whether `uuid` has the permission `node`: a permissions mod's answer, else whether their
    /// operator level is `default_level` or more (`native_has_permission`).
    fn has_permission(&self, uuid: u128, _node: &str, default_level: u8) -> bool {
        self.players()
            .iter()
            .find(|o| o.uuid == uuid)
            .map_or(0, |o| o.level)
            >= default_level
    }
    /// Sends every player their command tree again after the mod's usages changed
    /// (`native_refresh_commands`).
    fn refresh_commands(&mut self) {}
    fn set_menu_items(&mut self, viewer: u128, items: Vec<Option<Self::Stack>>);
    fn close_menu(&mut self, viewer: u128);
    /// A chat line (`§` colour codes allowed).
    fn tell(&mut self, uuid: u128, line: &str);
    /// A line in the server's log.
    fn log(&mut self, line: &str);
    /// One `item` named `name`, to stand in an empty or unavailable slot.
    fn placeholder(&self, item: &str, name: &str) -> Self::Stack;
    /// A stack's item id and count, for the log.
    fn describe(&self, stack: &Self::Stack) -> (String, u32);
    /// The server's folder (where usercache.json is).
    fn server_dir(&self) -> Option<PathBuf>;
    /// The mod's folder for its logs (`<world>/invsee`).
    fn data_dir(&self) -> Option<PathBuf>;
}

/// What was typed after the command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Query {
    Name(String),
    Uuid(u128),
}

/// Starts a Mojang lookup; replaced in tests.
pub type LookupFn = fn(&Query, Duration) -> Lookup;

fn mojang_lookup(q: &Query, timeout: Duration) -> Lookup {
    match q {
        Query::Name(n) => Lookup::by_name(n, timeout),
        Query::Uuid(u) => Lookup::by_uuid(*u, timeout),
    }
}

/// A command waiting for its target to be found (the next tick, or a Mojang answer).
struct Pending {
    viewer: u128,
    viewer_name: String,
    kind: Kind,
    /// The target as typed (messages use it, as InvSee++ does).
    typed: String,
    query: Query,
    /// The resolver tried next.
    step: usize,
    lookup: Option<Lookup>,
}

/// An open view.
struct Session<S> {
    viewer: u128,
    viewer_name: String,
    target: u128,
    target_name: String,
    kind: Kind,
    window: i32,
    layout: Layout,
    shown: Vec<Option<S>>,
    /// The target has no file yet: nothing can be put in.
    unknown: bool,
    last_read: u64,
    taken: Vec<(String, u32)>,
    given: Vec<(String, u32)>,
    /// The game's own inventory menu (`open_inventory_menu`): it follows and edits the target
    /// itself, so the mod neither refreshes nor handles its clicks (nor logs single items).
    live: bool,
}

/// What a click on a view asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The target's slot to the spectator's inventory.
    Take(usize),
    /// The spectator's inventory slot to the target.
    Give(usize),
    /// A number key on a target slot: take it when it holds something, else give the
    /// spectator's hotbar slot.
    TakeOrGive(usize, usize),
    /// A right click on a target slot: half of the stack (the larger half) to the spectator.
    Split(usize),
    Nothing,
}

/// The inventory slot (0..9 hotbar, 9..36 the rest) of a click below a view of `size` slots,
/// as Horizon's `native_menu_inventory_slot` counts it.
pub fn inventory_slot_below(size: usize, slot: i32) -> Option<usize> {
    let n = usize::try_from(slot).ok()?.checked_sub(size)?;
    match n {
        0..27 => Some(n + 9),
        27..36 => Some(n - 27),
        _ => None,
    }
}

/// The click `mode` (vanilla's `ClickType` id) and `button` on menu slot `slot` of `layout`.
/// Pickup (0), shift-click (1) and a number key (2) move whole stacks; middle click (3),
/// throw (4), drag (5) and double click (6) do nothing: without a cursor they can't be
/// carried out without making or losing items.
pub fn decide(layout: &Layout, slot: i32, button: i32, mode: i32) -> Action {
    if !matches!(mode, 0..=2) {
        return Action::Nothing;
    }
    let size = layout.cells.len();
    if let Some(i) = usize::try_from(slot).ok().filter(|&s| s < size) {
        return match layout.cells[i] {
            Cell::Slot(t) if mode == 2 && (0..9).contains(&button) => {
                Action::TakeOrGive(t, button as usize)
            }
            Cell::Slot(t) if mode == 0 && button == 1 => Action::Split(t),
            Cell::Slot(t) => Action::Take(t),
            _ => Action::Nothing,
        };
    }
    match inventory_slot_below(size, slot) {
        Some(v) => Action::Give(v),
        None => Action::Nothing,
    }
}

/// The item and name standing in for an empty armour or off-hand slot.
/// The names are InvSee++'s (`internal/placeholder/Placeholders`), in its aqua colour.
fn armour_placeholder(palette: Palette, slot: usize) -> Option<(&'static str, &'static str)> {
    let (glass, icon, name) = match slot {
        template::FEET => ("minecraft:light_blue_stained_glass_pane", "minecraft:chainmail_boots", "§bBoots"),
        template::LEGS => ("minecraft:light_blue_stained_glass_pane", "minecraft:chainmail_leggings", "§bLeggings"),
        template::CHEST => ("minecraft:light_blue_stained_glass_pane", "minecraft:chainmail_chestplate", "§bChestplate"),
        template::HEAD => ("minecraft:light_blue_stained_glass_pane", "minecraft:chainmail_helmet", "§bHelmet"),
        template::OFF_HAND => ("minecraft:yellow_stained_glass_pane", "minecraft:shield", "§bOff-hand"),
        _ => return None,
    };
    match palette {
        Palette::GlassPanes => Some((glass, name)),
        Palette::Icons => Some((icon, name)),
        Palette::Empty => None,
    }
}

/// The item and name standing in a slot this engine can't reach (body, saddle, cursor and the
/// crafting grid: InvSee++ shows them too, with the same names and colours).
fn unavailable_placeholder(palette: Palette, u: Unavailable) -> Option<(&'static str, &'static str)> {
    let (glass, icon, name) = match u {
        Unavailable::Body => ("minecraft:gray_stained_glass_pane", "minecraft:diamond_horse_armor", "§bBody"),
        Unavailable::Saddle => ("minecraft:orange_stained_glass_pane", "minecraft:saddle", "§bSaddle"),
        Unavailable::Cursor => ("minecraft:white_stained_glass_pane", "minecraft:structure_void", "§bCursor"),
        Unavailable::Crafting(_) => ("minecraft:orange_stained_glass_pane", "minecraft:crafting_table", "§bCrafting ingredient"),
    };
    match palette {
        Palette::GlassPanes => Some((glass, name)),
        Palette::Icons => Some((icon, name)),
        Palette::Empty => None,
    }
}

/// The view's items: the target's slots by the layout, placeholders where the palette says.
pub fn render<H: Host>(
    host: &H,
    layout: &Layout,
    kind: Kind,
    palette: Palette,
    slots: &[Option<H::Stack>],
) -> Vec<Option<H::Stack>> {
    let ph = |p: Option<(&str, &str)>| p.map(|(item, name)| host.placeholder(item, name));
    layout
        .cells
        .iter()
        .map(|c| match *c {
            Cell::Slot(i) => slots.get(i).cloned().flatten().or_else(|| {
                if kind == Kind::Inventory {
                    ph(armour_placeholder(palette, i))
                } else {
                    None
                }
            }),
            Cell::Unavailable(u) => ph(unavailable_placeholder(palette, u)),
            Cell::Unused => None,
        })
        .collect()
}

fn command_kind(word: &str) -> Option<Option<Kind>> {
    Some(match word {
        "invsee" | "inventorysee" | "isee" => Some(Kind::Inventory),
        "endersee" | "enderchestsee" | "esee" => Some(Kind::Ender),
        "invseeplusplusreload" | "isppreload" | "invsee++reload" | "is++reload" => None,
        _ => return None,
    })
}

/// The mod's state.
pub struct InvSee<S> {
    cfg: Config,
    cfg_path: Option<PathBuf>,
    inventory_layout: Layout,
    ender_layout: Layout,
    tick: u64,
    pending: Vec<Pending>,
    sessions: Vec<Session<S>>,
    /// The players on, as of the last tick.
    online: Vec<Online>,
    /// Players who were on since the start ("logged out players cache").
    seen: Directory,
    usercache: Usercache,
    server_dir: Option<PathBuf>,
    lookup: LookupFn,
    /// A config was read before (a later read that changes the usages must resend the trees).
    configured: bool,
    /// The usages changed since the trees were last sent.
    usages_changed: bool,
}

impl<S: Clone + PartialEq> Default for InvSee<S> {
    fn default() -> Self {
        Self::new()
    }
}

impl<S: Clone + PartialEq> InvSee<S> {
    pub fn new() -> Self {
        let cfg = Config::default();
        let (inventory_layout, _) = Layout::parse(&cfg.templates.inventory, Kind::Inventory);
        let (ender_layout, _) = Layout::parse(&cfg.templates.enderchest, Kind::Ender);
        InvSee {
            cfg,
            cfg_path: None,
            inventory_layout,
            ender_layout,
            tick: 0,
            pending: Vec::new(),
            sessions: Vec::new(),
            online: Vec::new(),
            seen: Directory::default(),
            usercache: Usercache::default(),
            server_dir: None,
            lookup: mojang_lookup,
            configured: false,
            usages_changed: false,
        }
    }

    /// Tests replace the Mojang lookup.
    pub fn with_lookup(mut self, lookup: LookupFn) -> Self {
        self.lookup = lookup;
        self
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// The config file's text (loaded or changed); `path` is kept for the reload command.
    /// Returns what to log: warnings, or why the text was refused (the old settings stay).
    pub fn load_config(&mut self, text: &str, path: Option<PathBuf>) -> Vec<String> {
        if path.is_some() {
            self.cfg_path = path;
        }
        self.apply(text).unwrap_or_else(|e| vec![e])
    }

    fn apply(&mut self, text: &str) -> Result<Vec<String>, String> {
        let cfg = Config::parse(text)
            .map_err(|e| format!("invsee: the config is not valid, the old one stays: {e}"))?;
        let (inv, mut w) = Layout::parse(&cfg.templates.inventory, Kind::Inventory);
        let (ender, w2) = Layout::parse(&cfg.templates.enderchest, Kind::Ender);
        w.extend(w2);
        self.inventory_layout = inv;
        self.ender_layout = ender;
        let before = self.usages();
        self.cfg = cfg;
        if self.configured && before != self.usages() {
            self.usages_changed = true;
        }
        self.configured = true;
        Ok(w.into_iter().map(|l| format!("invsee: {l}")).collect())
    }

    /// The command usages and the operator level each needs, as they are registered when the
    /// world opens (a level changed later applies to running, not to seeing, the command).
    pub fn usages(&self) -> Vec<(String, u8)> {
        let a = &self.cfg.access;
        let mut out = Vec::new();
        // `word?` (not `word`): the client asks the server for completions, so `tab_complete` can offer names.
        for w in ["invsee", "inventorysee", "isee"] {
            out.push((format!("{w} [target:word?]"), a.invsee_view));
        }
        for w in ["endersee", "enderchestsee", "esee"] {
            out.push((format!("{w} [target:word?]"), a.endersee_view));
        }
        // The `++` aliases of plugin.yml (`invsee++reload`, `is++reload`) are no command words for the engine; the
        // mod's `player_command` hook answers them (`command_kind` still knows them).
        for w in ["invseeplusplusreload", "isppreload"] {
            out.push((w.to_string(), a.reload));
        }
        out
    }

    fn view_level(&self, kind: Kind) -> u8 {
        match kind {
            Kind::Inventory => self.cfg.access.invsee_view,
            Kind::Ender => self.cfg.access.endersee_view,
        }
    }

    fn edit_level(&self, kind: Kind) -> u8 {
        match kind {
            Kind::Inventory => self.cfg.access.invsee_edit,
            Kind::Ender => self.cfg.access.endersee_edit,
        }
    }

    /// `/invseeplusplusreload`: the file is read again (InvSee++ reloads its config the same way).
    fn reload(&mut self) -> Vec<String> {
        let Some(path) = self.cfg_path.clone() else {
            return vec!["§cInvSee++ has no configuration file to read.".into()];
        };
        match std::fs::read_to_string(&path) {
            Ok(text) => match self.apply(&text) {
                Ok(w) => {
                    let mut out = vec!["§aInvSee++ configuration was reloaded.".to_string()];
                    out.extend(w.into_iter().map(|l| format!("§e{l}")));
                    out
                }
                Err(e) => vec![format!("§c{e}")],
            },
            Err(e) => vec![format!("§cInvSee++ can't read {}: {e}", path.display())],
        }
    }

    /// A player's command (no slash). None when it isn't one of ours; else the lines to answer.
    /// The view opens on the next tick: inventories can't be read inside a player's turn.
    /// As in InvSee++, only the first argument counts; anything after it is ignored.
    pub fn perform<H: Host<Stack = S>>(&mut self, host: &H, viewer: u128, name: &str, line: &str) -> Option<Vec<String>> {
        let mut words = line.split_whitespace();
        let word = words.next()?.to_lowercase();
        let Some(kind) = command_kind(&word)? else {
            if !host.has_permission(viewer, "invseeplusplus.reload", self.cfg.access.reload) {
                return Some(vec![NO_PERMISSION.into()]);
            }
            return Some(self.reload());
        };
        if !host.has_permission(viewer, &node(kind, "view"), self.view_level(kind)) {
            return Some(vec![NO_PERMISSION.into()]);
        }
        let Some(target) = words.next() else {
            return Some(vec![usage(kind)]);
        };
        let query = match parse_uuid(target) {
            Some(u) => Query::Uuid(u),
            // A name that can't be one is simply not found (InvSee++ asks no one about it).
            None if valid_name(target) => Query::Name(target.to_owned()),
            None => return Some(vec![format!("§cPlayer {target} does not exist.")]),
        };
        // One request at a time per spectator: the newest wins.
        self.pending.retain(|p| p.viewer != viewer);
        self.pending.push(Pending {
            viewer,
            viewer_name: name.to_owned(),
            kind,
            typed: target.to_owned(),
            query,
            step: 0,
            lookup: None,
        });
        Some(Vec::new())
    }

    /// A console command. None when it isn't one of ours.
    pub fn console(&mut self, line: &str) -> Option<Vec<String>> {
        let mut words = line.split_whitespace();
        let word = words.next()?.to_lowercase();
        match command_kind(&word)? {
            None => Some(self.reload()),
            Some(kind) => Some(match words.next() {
                None => vec![usage(kind)],
                Some(_) => vec!["§cThis command can only be used by players!".into()],
            }),
        }
    }

    pub fn logged_in(&mut self, uuid: u128, name: &str) {
        self.seen.add(uuid, name);
    }

    pub fn logged_out(&mut self, uuid: u128, name: &str) {
        self.seen.add(uuid, name);
        self.pending.retain(|p| p.viewer != uuid);
    }

    fn usercache(&mut self) -> Option<&Directory> {
        let path = self.server_dir.as_ref()?.join("usercache.json");
        Some(self.usercache.get(&path))
    }

    /// Completions for the argument of `command` (as the client sent it, slash or not): the
    /// byte where the argument starts, and the names. None when it isn't ours.
    pub fn tab_complete<H: Host<Stack = S>>(&mut self, host: &H, player: u128, command: &str) -> Option<(usize, Vec<String>)> {
        let cmd = command.trim_start_matches('/');
        let offset = command.len() - cmd.len();
        let (word, arg) = cmd.split_once(' ')?;
        let kind = command_kind(&word.to_lowercase())??;
        let level = self.view_level(kind);
        if arg.contains(' ')
            || !host.has_permission(player, &node(kind, "view"), level)
            || !host.has_permission(player, "invseeplusplus.tabcomplete", level)
        {
            return None;
        }
        let prefix = arg.to_lowercase();
        let mut names: Vec<String> = self.online.iter().map(|p| p.name.clone()).collect();
        if self.cfg.players.offline && self.cfg.players.tabcomplete_offline {
            names.extend(self.seen.names().map(str::to_owned));
            if let Some(d) = self.usercache() {
                names.extend(d.names().map(str::to_owned));
            }
        }
        names.retain(|n| n.to_lowercase().starts_with(&prefix));
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        names.truncate(100);
        Some((offset + word.len() + 1, names))
    }

    /// Once a tick (its end): finds the targets of commands, opens their views, follows the
    /// open views' inventories.
    pub fn tick<H: Host<Stack = S>>(&mut self, host: &mut H) {
        self.tick += 1;
        self.online = host.players();
        self.server_dir = host.server_dir();
        if std::mem::take(&mut self.usages_changed) {
            host.refresh_commands();
        }
        for p in &self.online {
            self.seen.add(p.uuid, &p.name);
        }
        let pending = std::mem::take(&mut self.pending);
        for mut p in pending {
            if !self.online.iter().any(|o| o.uuid == p.viewer) {
                continue;
            }
            match self.resolve(host, &mut p) {
                Resolve::Wait => self.pending.push(p),
                Resolve::Found(uuid, name) => self.open(host, &p, uuid, name),
                Resolve::NotFound => {
                    host.tell(p.viewer, &format!("§cPlayer {} does not exist.", p.typed));
                }
            }
        }
        // Views whose spectator is gone (the close hook normally ends them first).
        let gone: Vec<usize> = (0..self.sessions.len())
            .filter(|&i| !self.online.iter().any(|o| o.uuid == self.sessions[i].viewer))
            .collect();
        for i in gone.into_iter().rev() {
            let s = self.sessions.remove(i);
            self.finish(host, s);
        }
        let every = self.cfg.players.offline_refresh_ticks;
        for i in 0..self.sessions.len() {
            let s = &self.sessions[i];
            let online = self.online.iter().any(|o| o.uuid == s.target);
            if online || self.tick.saturating_sub(s.last_read) >= every {
                self.refresh(host, i);
            }
        }
    }

    fn resolve<H: Host<Stack = S>>(&mut self, host: &H, p: &mut Pending) -> Resolve {
        let list = match p.query {
            Query::Name(_) => self.cfg.players.uuid_resolve.clone(),
            Query::Uuid(_) => self.cfg.players.name_resolve.clone(),
        };
        while let Some(r) = list.get(p.step).copied() {
            let found = match (&p.query, r) {
                (Query::Name(n), Resolver::Online) => self
                    .online
                    .iter()
                    .find(|o| o.name.eq_ignore_ascii_case(n))
                    .map(|o| (o.uuid, o.name.clone())),
                (Query::Uuid(u), Resolver::Online) => self
                    .online
                    .iter()
                    .find(|o| o.uuid == *u)
                    .map(|o| (o.uuid, o.name.clone())),
                (Query::Name(n), Resolver::LoggedOut) => self.seen.uuid(n),
                (Query::Uuid(u), Resolver::LoggedOut) => self.seen.name(*u).map(|n| (*u, n)),
                (Query::Name(n), Resolver::Usercache) => {
                    // The server's profile cache first; the mod's own read of usercache.json
                    // only for a game that has no such helper.
                    host.profile_by_name(n).or_else(|| {
                        let n = n.clone();
                        self.usercache().and_then(|d| d.uuid(&n))
                    })
                }
                (Query::Uuid(u), Resolver::Usercache) => {
                    let u = *u;
                    host.profile_by_uuid(u)
                        .or_else(|| self.usercache().and_then(|d| d.name(u)))
                        .map(|n| (u, n))
                }
                (q, Resolver::Mojang) => {
                    let timeout = Duration::from_millis(self.cfg.players.mojang_timeout_ms);
                    let lookup = p.lookup.get_or_insert_with(|| (self.lookup)(q, timeout));
                    match lookup.poll() {
                        Poll::Pending => return Resolve::Wait,
                        Poll::Done(found) => {
                            p.lookup = None;
                            // A name's answer must be the same player as asked about.
                            match (q, found) {
                                (Query::Uuid(u), Some((id, name))) if id == *u => Some((id, name)),
                                (Query::Name(n), Some((id, name))) if name.eq_ignore_ascii_case(n) => {
                                    Some((id, name))
                                }
                                _ => None,
                            }
                        }
                    }
                }
                (Query::Name(n), Resolver::Offline) => Some((names::offline_uuid(n), n.clone())),
                (Query::Uuid(_), Resolver::Offline) => None,
            };
            if let Some((uuid, name)) = found {
                return Resolve::Found(uuid, name);
            }
            p.step += 1;
        }
        match p.query {
            // A UUID nobody can name opens under InvSee++'s placeholder name.
            Query::Uuid(u) => Resolve::Found(u, UNKNOWN_NAME.to_owned()),
            Query::Name(_) => Resolve::NotFound,
        }
    }

    fn exempt(&self, kind: Kind, uuid: u128, name: &str) -> bool {
        let list = match kind {
            Kind::Inventory => &self.cfg.access.exempt_invsee,
            Kind::Ender => &self.cfg.access.exempt_endersee,
        };
        list.iter().any(|e| {
            parse_uuid(e).map_or_else(|| e.eq_ignore_ascii_case(name), |u| u == uuid)
        })
    }

    fn layout(&self, kind: Kind) -> &Layout {
        match kind {
            Kind::Inventory => &self.inventory_layout,
            Kind::Ender => &self.ender_layout,
        }
    }

    fn open<H: Host<Stack = S>>(&mut self, host: &mut H, p: &Pending, target: u128, name: String) {
        // The original's order: offline support first (for players not on), then the exemption.
        let is_online = self.online.iter().any(|o| o.uuid == target);
        if !is_online && !self.cfg.players.offline {
            let what = match p.kind {
                Kind::Inventory => "inventories",
                Kind::Ender => "enderchests",
            };
            host.tell(p.viewer, &format!("§cSpectating offline players' {what} is disabled."));
            return;
        }
        let bypass = host.has_permission(p.viewer, &node(p.kind, "bypass-exempt"), self.cfg.access.bypass_exempt);
        if self.exempt(p.kind, target, &name) && !bypass {
            host.tell(p.viewer, &format!("§cPlayer {} is exempted from being spectated.", p.typed));
            return;
        }
        let (slots, unknown) = match host.inventory(target, p.kind) {
            Ok(inv) => (inv.slots, false),
            Err(InvError::NeverPlayed) if self.cfg.players.unknown => {
                (vec![None; p.kind.size()], true)
            }
            Err(InvError::NeverPlayed) => {
                host.tell(p.viewer, &format!("§cPlayer {} has not logged onto the server yet.", p.typed));
                return;
            }
            Err(InvError::Busy(why)) => {
                host.tell(p.viewer, &format!("§cCan't open {}'s inventory now: {why}.", p.typed));
                return;
            }
            Err(InvError::Other(why)) => {
                host.tell(p.viewer, &format!("§cCan't open {}'s inventory: {why}.", p.typed));
                return;
            }
        };
        // A spectator has one view: the old one ends (and is logged) before the new opens.
        if let Some(i) = self.sessions.iter().position(|s| s.viewer == p.viewer) {
            let s = self.sessions.remove(i);
            self.finish(host, s);
        }
        let layout = self.layout(p.kind).clone();
        let title = match p.kind {
            Kind::Inventory => &self.cfg.titles.inventory,
            Kind::Ender => &self.cfg.titles.enderchest,
        }
        .replace("<player>", &name);
        let items = render(host, &layout, p.kind, self.cfg.menu.placeholder_palette, &slots);
        // A viewer who may edit a known player gets the game's live inventory menu when there is
        // one (drag, split, armour slots as in InvSee++); placeholders are not shown there.
        let may_edit = host.has_permission(p.viewer, &node(p.kind, "edit"), self.edit_level(p.kind));
        let live_window = if !unknown && may_edit {
            let cells: Vec<Option<usize>> = layout
                .cells
                .iter()
                .map(|c| match c {
                    Cell::Slot(i) => Some(*i),
                    _ => None,
                })
                .collect();
            host.open_inventory_menu(p.viewer, layout.rows(), &title, (target, p.kind), &cells)
        } else {
            None
        };
        let live = live_window.is_some();
        let window = match live_window {
            Some(w) => w,
            None => match host.open_menu(p.viewer, layout.rows(), &title, items.clone()) {
                Some(w) => w,
                None => {
                    // The game refused the view: InvSee++'s own message for a cancelled open.
                    // (InvSee++ ends the ender chest message with a full stop, the inventory one without.)
                    let text = match p.kind {
                        Kind::Inventory => format!("§cAnother plugin prevented you from spectating {}'s inventory", p.typed),
                        Kind::Ender => format!("§cAnother plugin prevented you from spectating {}'s ender chest.", p.typed),
                    };
                    host.tell(p.viewer, &text);
                    return;
                }
            },
        };
        if unknown {
            host.tell(
                p.viewer,
                &format!("§e{name} has never played here: the view stays empty until they join."),
            );
        }
        self.sessions.push(Session {
            viewer: p.viewer,
            viewer_name: p.viewer_name.clone(),
            target,
            target_name: name,
            kind: p.kind,
            window,
            layout,
            shown: items,
            unknown,
            last_read: self.tick,
            taken: Vec::new(),
            given: Vec::new(),
            live,
        });
    }

    fn refresh<H: Host<Stack = S>>(&mut self, host: &mut H, i: usize) {
        let palette = self.cfg.menu.placeholder_palette;
        let s = &mut self.sessions[i];
        s.last_read = self.tick;
        if s.live {
            return;
        }
        let Ok(inv) = host.inventory(s.target, s.kind) else {
            return;
        };
        s.unknown = false;
        let items = render(host, &s.layout, s.kind, palette, &inv.slots);
        if items != s.shown {
            host.set_menu_items(s.viewer, items.clone());
            s.shown = items;
        }
    }

    /// A click on a view (`native_menu_clicked`).
    pub fn clicked<H: Host<Stack = S>>(
        &mut self,
        host: &mut H,
        viewer: u128,
        window: i32,
        slot: i32,
        button: i32,
        mode: i32,
    ) {
        let Some(i) = self
            .sessions
            .iter()
            .position(|s| s.viewer == viewer && s.window == window)
        else {
            return;
        };
        if self.sessions[i].live {
            return;
        }
        let action = decide(&self.sessions[i].layout, slot, button, mode);
        if action == Action::Nothing {
            return;
        }
        let kind = self.sessions[i].kind;
        // Without the edit permission the click is cancelled and nothing is said (InvSee++'s listener).
        if !host.has_permission(viewer, &node(kind, "edit"), self.edit_level(kind)) {
            return;
        }
        if self.sessions[i].unknown {
            let name = self.sessions[i].target_name.clone();
            host.tell(viewer, &format!("§c{name} never played here: nothing can be stored yet."));
            return;
        }
        let target = self.sessions[i].target;
        // Each helper answers None when there is nothing to do, else what moved: (taken from
        // the target, item and count).
        type Moved = Option<Result<Vec<(bool, (String, u32))>, String>>;
        let take = |host: &mut H, t: usize| -> Moved {
            let inv = host.inventory(target, kind).ok()?;
            let stack = inv.slots.get(t).cloned().flatten()?;
            let d = host.describe(&stack);
            Some(
                host.move_stack((target, kind, t), &stack, (viewer, Kind::Inventory))
                    .map(|_| vec![(true, d)]),
            )
        };
        // Right click: the larger half to a free slot of the spectator, the rest stays.
        let split = |host: &mut H, t: usize| -> Moved {
            let inv = host.inventory(target, kind).ok()?;
            let stack = inv.slots.get(t).cloned().flatten()?;
            let (item, count) = host.describe(&stack);
            if count < 2 {
                return take(host, t);
            }
            let (Some(half), Some(rest)) = (
                host.with_count(&stack, count.div_ceil(2)),
                host.with_count(&stack, count / 2),
            ) else {
                return take(host, t);
            };
            let mine = host.inventory(viewer, Kind::Inventory).ok()?;
            let Some(free) = mine.slots.iter().take(36).position(Option::is_none) else {
                return Some(Err("no room".into()));
            };
            if let Err(e) = host.set_slot(viewer, Kind::Inventory, free, None, Some(half)) {
                return Some(Err(e));
            }
            if let Err(e) = host.set_slot(target, kind, t, Some(&stack), Some(rest)) {
                // Undo the first write so nothing is made.
                if let Some(half) = host.with_count(&stack, count.div_ceil(2)) {
                    let _ = host.set_slot(viewer, Kind::Inventory, free, Some(&half), None);
                }
                return Some(Err(e));
            }
            Some(Ok(vec![(true, (item, count.div_ceil(2)))]))
        };
        let give = |host: &mut H, v: usize| -> Moved {
            let inv = host.inventory(viewer, Kind::Inventory).ok()?;
            let stack = inv.slots.get(v).cloned().flatten()?;
            let d = host.describe(&stack);
            // Armour and shields go to the slot they are worn in; a worn piece swaps places.
            if kind == Kind::Inventory {
                if let Some(slot) = worn_slot(&d.0) {
                    let worn = host
                        .inventory(target, kind)
                        .ok()
                        .and_then(|t| t.slots.get(slot).cloned());
                    let placed = worn.as_ref().map(|w| {
                        host.set_slot(target, kind, slot, w.as_ref(), Some(stack.clone())).is_ok()
                    });
                    if let (Some(worn), Some(true)) = (worn, placed) {
                        if let Err(e) = host.set_slot(viewer, Kind::Inventory, v, Some(&stack), worn.clone()) {
                            let _ = host.set_slot(target, kind, slot, Some(&stack), worn);
                            return Some(Err(e));
                        }
                        let mut moved = vec![(false, d)];
                        if let Some(old) = worn {
                            moved.push((true, host.describe(&old)));
                        }
                        return Some(Ok(moved));
                    }
                }
            }
            Some(
                host.move_stack((viewer, Kind::Inventory, v), &stack, (target, kind))
                    .map(|_| vec![(false, d)]),
            )
        };
        let result = match action {
            Action::Take(t) => take(host, t),
            Action::Split(t) => split(host, t),
            Action::Give(v) => give(host, v),
            Action::TakeOrGive(t, h) => take(host, t).or_else(|| give(host, h)),
            Action::Nothing => None,
        };
        match result {
            None => {}
            Some(Err(e)) => host.tell(viewer, &format!("§cNothing moved: {e}.")),
            Some(Ok(moved)) => {
                let (mut t, mut g) = (Vec::new(), Vec::new());
                for (taken, item) in moved {
                    if taken {
                        t.push(item);
                    } else {
                        g.push(item);
                    }
                }
                let s = &mut self.sessions[i];
                s.taken.extend(t.iter().cloned());
                s.given.extend(g.iter().cloned());
                if self.cfg.logging.granularity == Granularity::EveryChange {
                    let (viewer_name, target_name) = (s.viewer_name.clone(), s.target_name.clone());
                    self.write_log(host, viewer, &viewer_name, &target_name, &t, &g);
                }
                self.refresh(host, i);
            }
        }
    }

    /// A view closed (`native_menu_closed`): the spectator closed it, opened another or left.
    pub fn closed<H: Host<Stack = S>>(&mut self, host: &mut H, viewer: u128, window: i32) {
        if let Some(i) = self
            .sessions
            .iter()
            .position(|s| s.viewer == viewer && s.window == window)
        {
            let s = self.sessions.remove(i);
            self.finish(host, s);
        }
    }

    /// The server stops: every open view is logged and closed.
    pub fn stopping<H: Host<Stack = S>>(&mut self, host: &mut H) {
        for s in std::mem::take(&mut self.sessions) {
            host.close_menu(s.viewer);
            self.finish(host, s);
        }
        self.pending.clear();
    }

    fn finish<H: Host<Stack = S>>(&mut self, host: &mut H, s: Session<S>) {
        if self.cfg.logging.granularity == Granularity::OnClose
            && (!s.taken.is_empty() || !s.given.is_empty())
        {
            self.write_log(host, s.viewer, &s.viewer_name, &s.target_name, &s.taken, &s.given);
        }
    }

    fn write_log<H: Host<Stack = S>>(
        &self,
        host: &mut H,
        viewer: u128,
        viewer_name: &str,
        target: &str,
        taken: &[(String, u32)],
        given: &[(String, u32)],
    ) {
        let dir = host
            .data_dir()
            .unwrap_or_else(|| PathBuf::from("invsee"))
            .join(LOG_FOLDER);
        let entry = audit::Entry {
            spectator: viewer,
            spectator_name: viewer_name,
            target,
            taken,
            given,
        };
        for line in audit::write(&self.cfg.logging, &dir, &entry) {
            host.log(&line);
        }
    }

    /// How many views are open (tests, status).
    pub fn open_views(&self) -> usize {
        self.sessions.len()
    }
}

enum Resolve {
    Wait,
    Found(u128, String),
    NotFound,
}

#[cfg(test)]
mod tests;
