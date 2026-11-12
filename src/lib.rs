//! invsee as an arismoko native trait mod (`mod_api`). The logic is the unchanged `horizon-invsee`
//! crate (`core/`); this file wires its hooks and its `Host` to arismoko's API. Where arismoko has
//! no API yet the mapping falls back and says so with a `TODO(arismoko-api: ...)`; PORT_NOTES.md
//! lists them all.
//!
//! Config: `config/invsee-server.toml` (`ConfigKind::Server`, written from the default when missing,
//! read again when it changes).
use horizon_invsee::{self as inv, Kind};
use mod_api::{
    Command, CommandArgumentValue, CommandInvocation, CommandSource, Config, ConfigKind,
    ConfigLoadingEvent, ConfigReloadingEvent, Flow, HostSide, Hooks, Identifier, ItemStack,
    MenuClickedEvent, MenuClosedEvent, PlayerLoggedInEvent,
    PlayerLoggedOutEvent, RegisterCommandsEvent, RegisterConfigsEvent, RunModCommandEvent,
    ServerHost, ServerStoppingEvent, ServerTickEvent, ServerTickPhase, Suggestion,
    TabCompleteEvent, Text,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

const ID: &str = "invsee";

/// The command words the core answers to (its `usages()`, with their aliases).
const OWN_WORDS: [&str; 8] = [
    "invsee",
    "inventorysee",
    "isee",
    "endersee",
    "enderchestsee",
    "esee",
    "invseeplusplusreload",
    "isppreload",
];

/// The mod's state, one per process (a dedicated server runs one world). Hooks take it with
/// `try_lock`: a call that re-enters a hook while it is in use is deferred to the next tick.
static STATE: Mutex<Option<inv::InvSee<ItemStack>>> = Mutex::new(None);

/// Menu events that arrived while `STATE` was in use further up the same thread.
static DEFERRED: Mutex<Vec<Deferred>> = Mutex::new(Vec::new());

/// The window id of each viewer's open view (from `open_menu`), so `set_menu_items` can refresh it.
static WINDOWS: Mutex<BTreeMap<u128, i32>> = Mutex::new(BTreeMap::new());

enum Deferred {
    Clicked(u128, i32, i32, i32, i32),
    Closed(u128, i32),
}

/// Runs `f` on the state; None when it is already in use on this thread.
fn with<R>(f: impl FnOnce(&mut inv::InvSee<ItemStack>) -> R) -> Option<R> {
    let mut guard = match STATE.try_lock() {
        Ok(g) => g,
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    Some(f(guard.get_or_insert_with(inv::InvSee::new)))
}

fn defer(d: Deferred) {
    DEFERRED.lock().unwrap_or_else(|e| e.into_inner()).push(d);
}

fn eprint_line(line: &str) {
    for l in line.lines() {
        eprintln!("[invsee] {l}");
    }
}

/// A line for the chat or a title. The core writes `§` colour codes, read as the font draws them
/// (`text_from_legacy`); if the engine refuses the text, the codes are dropped.
fn text(host: &dyn ServerHost, line: &str) -> Text {
    host.text_from_legacy(line).unwrap_or_else(|_| {
        let mut out = String::with_capacity(line.len());
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            if c == '§' {
                chars.next();
            } else {
                out.push(c);
            }
        }
        Text::literal(out)
    })
}

/// The world folder from `server.properties` (`level-name`), relative to the server folder.
fn level_dir() -> PathBuf {
    let level = std::fs::read_to_string("server.properties")
        .ok()
        .and_then(|p| {
            p.lines()
                .find_map(|l| l.strip_prefix("level-name=").map(|v| v.trim().to_owned()))
        })
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "world".into());
    PathBuf::from(level)
}

/// The game, as the core sees it: `host` is the server of the hook that asked.
struct GameHost<'a> {
    host: &'a mut dyn ServerHost,
}

impl inv::Host for GameHost<'_> {
    type Stack = ItemStack;

    fn players(&self) -> Vec<inv::Online> {
        self.host
            .players()
            .server_players()
            .into_iter()
            .map(|p| inv::Online {
                uuid: p.uuid(),
                name: p.name(&*self.host).unwrap_or_default(),
                level: p.permission_level(&*self.host).unwrap_or(0),
            })
            .collect()
    }

    fn inventory(&self, uuid: u128, k: Kind) -> Result<inv::Inv<ItemStack>, inv::InvError> {
        let Some(player) = self.host.player(uuid) else {
            // TODO(arismoko-api: player_offline_inventory): a player's file cannot be read from a mod.
            return Err(inv::InvError::Other(
                "offline inventories cannot be read on this server yet".into(),
            ));
        };
        // Vanilla's slot numbers (0-35 main, 36-39 armour, 40 off hand; ender chest 0-26), as the core counts.
        let read = match k {
            Kind::Inventory => player.inventory_slots(&*self.host),
            Kind::Ender => player.ender_chest(&*self.host),
        };
        let Some(read) = read else {
            return Err(inv::InvError::Busy("unavailable".into()));
        };
        let mut slots = vec![None; k.size()];
        for (slot, stack) in read {
            if let Some(s) = slots.get_mut(slot as usize) {
                *s = Some(stack);
            }
        }
        Ok(inv::Inv { slots, online: true })
    }

    /// One slot written (`player_set_inventory_slot` / `player_set_ender_chest_slot`) when it still
    /// holds `expected`; what was there.
    fn set_slot(
        &mut self,
        uuid: u128,
        kind: Kind,
        slot: usize,
        expected: Option<&ItemStack>,
        new: Option<ItemStack>,
    ) -> Result<Option<ItemStack>, String> {
        let now = self.inventory(uuid, kind).map_err(|e| format!("{e:?}"))?;
        let Some(current) = now.slots.get(slot).cloned() else {
            return Err(format!("no slot {slot}"));
        };
        if current.as_ref() != expected {
            return Err("the slot changed".into());
        }
        let player = self.host.player(uuid).ok_or("the player left")?;
        let written = match kind {
            Kind::Inventory => player.set_inventory_slot(&mut *self.host, slot as u32, new),
            Kind::Ender => player.set_ender_chest_slot(&mut *self.host, slot as u32, new),
        };
        match written {
            Ok(true) => Ok(current),
            Ok(false) => Err("the game refused the slot".into()),
            Err(e) => Err(format!("{e:?}")),
        }
    }

    /// The whole stack into the first empty slot of `to` (main inventory or ender chest), then the
    /// source slot emptied; the source is put back if the second write fails.
    fn move_stack(
        &mut self,
        from: (u128, Kind, usize),
        expected: &ItemStack,
        to: (u128, Kind),
    ) -> Result<(), String> {
        let target = self.inventory(to.0, to.1).map_err(|e| format!("{e:?}"))?;
        let room = match to.1 {
            Kind::Inventory => 36,
            Kind::Ender => 27,
        };
        let free = (0..room).find(|&i| target.slots[i].is_none()).ok_or("no free slot")?;
        self.set_slot(from.0, from.1, from.2, Some(expected), None)?;
        if let Err(e) = self.set_slot(to.0, to.1, free, None, Some(expected.clone())) {
            let _ = self.set_slot(from.0, from.1, from.2, None, Some(expected.clone()));
            return Err(e);
        }
        Ok(())
    }

    fn open_menu(
        &mut self,
        viewer: u128,
        rows: u8,
        title: &str,
        items: Vec<Option<ItemStack>>,
    ) -> Option<i32> {
        let title = text(&*self.host, title);
        let window = self.host.open_menu(viewer, rows, title, items).ok().flatten();
        if let Some(window) = window {
            WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).insert(viewer, window);
        }
        window
    }

    // `open_inventory_menu` is not overridden: the core's default (None) makes it use `open_menu`.
    // TODO(arismoko-api: menu_open_inventory): a live menu over the target's slots (drag, split,
    // armour) is missing; the view is a snapshot taken when it opens.

    fn with_count(&self, stack: &ItemStack, count: u32) -> Option<ItemStack> {
        let mut split = stack.clone();
        split.set_count(count).ok()?;
        Some(split)
    }

    // The core asks the server's own profile cache first (`profile_by_name`, `profile_by_uuid`) and
    // reads usercache.json or Mojang only when that gives nothing.
    fn profile_by_name(&self, name: &str) -> Option<(u128, String)> {
        self.host.profile_by_name(name).map(|p| (p.uuid, p.name))
    }

    fn profile_by_uuid(&self, uuid: u128) -> Option<String> {
        self.host.profile_by_uuid(uuid).map(|p| p.name)
    }

    /// A permissions mod's answer for `node` (`player_permission_value`); without one, the
    /// operator level against `default_level`, as the core's default does.
    fn has_permission(&self, uuid: u128, node: &str, default_level: u8) -> bool {
        let Some(player) = self.host.player(uuid) else {
            return false;
        };
        match player.permission_value(&*self.host, node) {
            Some(answer) => answer,
            None => player.permission_level(&*self.host).unwrap_or(0) >= default_level,
        }
    }

    fn refresh_commands(&mut self) {
        self.host.commands().refresh_commands();
    }

    fn set_menu_items(&mut self, viewer: u128, items: Vec<Option<ItemStack>>) {
        let window = WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).get(&viewer).copied();
        if let Some(window) = window {
            let _ = self.host.menu_set_items(viewer, window, items);
        }
    }

    fn close_menu(&mut self, viewer: u128) {
        WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).remove(&viewer);
        self.host.close_menu(viewer);
    }

    fn tell(&mut self, uuid: u128, line: &str) {
        let line = text(&*self.host, line);
        if let Some(player) = self.host.player(uuid) {
            let _ = player.tell(&mut *self.host, line);
        }
    }

    fn log(&mut self, line: &str) {
        // TODO(arismoko-api: server_log): stderr only, not the server's log file.
        eprint_line(line);
    }

    fn placeholder(&self, item: &str, name: &str) -> ItemStack {
        let key = Identifier::parse(item)
            .ok()
            .and_then(|id| self.host.item_key(&id))
            .or_else(|| {
                Identifier::parse("minecraft:barrier")
                    .ok()
                    .and_then(|id| self.host.item_key(&id))
            })
            .expect("minecraft:barrier is a vanilla item");
        let mut stack = ItemStack::of(key);
        // The name as the item's custom name; a refused name leaves the plain item.
        if let Ok(name) = self.host.text_from_legacy(name) {
            let _ = stack.set_display(&*self.host, Some(&name), &[]);
        }
        stack
    }

    fn describe(&self, stack: &ItemStack) -> (String, u32) {
        (stack.item().id(&*self.host), stack.count())
    }

    fn server_dir(&self) -> Option<PathBuf> {
        // The server runs in its own folder (where usercache.json lies), as stage.py starts it.
        std::env::current_dir().ok()
    }

    fn data_dir(&self) -> Option<PathBuf> {
        Some(level_dir().join("invsee"))
    }
}

/// The command usages the core has for the current config.
fn usages() -> Vec<(String, u8)> {
    with(|m| m.usages()).unwrap_or_default()
}

/// The config text (`config/invsee-server.toml`) was read.
fn config(text: &str, path: &Path) {
    let lines = with(|m| m.load_config(text, Some(path.to_path_buf()))).unwrap_or_default();
    for l in lines {
        eprint_line(&l);
    }
}

/// A player's command line (no slash); the core's lines for the player, or None when it is not ours.
fn perform(host: &mut dyn ServerHost, uuid: u128, name: &str, line: &str) -> Option<Vec<String>> {
    let game = GameHost { host };
    with(|m| m.perform(&game, uuid, name, line)).flatten()
}

/// The console's command line; the core's lines, or None when it is not ours.
fn console(line: &str) -> Option<Vec<String>> {
    with(|m| m.console(line)).flatten()
}

/// The command as the core reads a line: its literals, then its text and word arguments.
fn command_line(command: &CommandInvocation) -> String {
    let mut words = command.literals.clone();
    for argument in &command.arguments {
        if let CommandArgumentValue::String(word) = &argument.value {
            words.push(word.clone());
        }
    }
    words.join(" ")
}

/// Once a tick, at its end: opens pending views, follows open views, polls Mojang lookups.
fn tick(game: &mut GameHost<'_>) {
    let deferred = std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|e| e.into_inner()));
    with(|m| {
        for d in deferred {
            match d {
                Deferred::Clicked(u, w, s, b, mode) => m.clicked(game, u, w, s, b, mode),
                Deferred::Closed(u, w) => m.closed(game, u, w),
            }
        }
        m.tick(game);
    });
}

#[derive(Default)]
struct Mod;

#[mod_api::horizon_mod]
impl Hooks for Mod {
    fn register_configs(&self, _: HostSide, event: &mut RegisterConfigsEvent<'_>) {
        event
            .configs
            .push(Config::new(ID, ConfigKind::Server, inv::config::DEFAULT));
    }

    fn config_loading(&self, _: HostSide, event: &mut ConfigLoadingEvent<'_>) {
        if event.config.mod_id == ID {
            config(&event.config.text, &Path::new("config").join(&event.config.file));
        }
    }

    fn config_reloading(&self, _: HostSide, event: &mut ConfigReloadingEvent<'_>) {
        if event.config.mod_id == ID {
            config(&event.config.text, &Path::new("config").join(&event.config.file));
        }
    }

    fn register_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        for (usage, level) in usages() {
            // The core writes `word?` so the client asks for names; arismoko's word is plain `word`.
            event
                .commands
                .push(Command::op(&usage.replace("word?", "word"), level));
        }
    }

    fn run_mod_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        let ours = event
            .command
            .literals
            .first()
            .is_some_and(|w| OWN_WORDS.contains(&w.as_str()));
        if !ours {
            return Flow::Continue;
        }
        let line = command_line(&event.command);
        let answer = match &event.source {
            CommandSource::Player { player, .. } => {
                let uuid = player.uuid();
                let name = player.name(&*host).unwrap_or_default();
                perform(host, uuid, &name, &line)
            }
            CommandSource::Console { .. } => console(&line),
            _ => None,
        };
        let Some(lines) = answer else {
            return Flow::Continue;
        };
        for l in lines {
            let line = text(&*host, &l);
            let _ = host.commands().feedback(line);
        }
        Flow::Cancel
    }

    fn tab_complete(&self, host: &mut dyn ServerHost, event: &mut TabCompleteEvent<'_>) {
        let game = GameHost { host };
        let Some(Some((start, names))) =
            with(|m| m.tab_complete(&game, event.player, event.command))
        else {
            return;
        };
        let end = event.command.len();
        for name in names {
            if !event.suggestions.iter().any(|s| s.text.eq_ignore_ascii_case(&name)) {
                event.suggestions.push(Suggestion::new((start, end), name));
            }
        }
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            let mut game = GameHost { host };
            tick(&mut game);
        }
    }

    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent) {
        let (uuid, window, slot, button, mode) =
            (event.uuid, event.window, event.slot, event.button, event.mode);
        let mut game = GameHost { host };
        if with(|m| m.clicked(&mut game, uuid, window, slot, button, mode)).is_none() {
            defer(Deferred::Clicked(uuid, window, slot, button, mode));
        }
    }

    fn menu_closed(&self, host: &mut dyn ServerHost, event: &mut MenuClosedEvent) {
        let (uuid, window) = (event.uuid, event.window);
        {
            let mut windows = WINDOWS.lock().unwrap_or_else(|e| e.into_inner());
            if windows.get(&uuid) == Some(&window) {
                windows.remove(&uuid);
            }
        }
        let mut game = GameHost { host };
        if with(|m| m.closed(&mut game, uuid, window)).is_none() {
            defer(Deferred::Closed(uuid, window));
        }
    }

    fn player_logged_in(&self, _: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        with(|m| m.logged_in(event.player, &event.name));
    }

    fn player_logged_out(&self, _: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        with(|m| m.logged_out(event.player, &event.name));
    }

    fn server_stopping(&self, host: &mut dyn ServerHost, _: &mut ServerStoppingEvent) {
        let mut game = GameHost { host };
        with(|m| m.stopping(&mut game));
    }
}
