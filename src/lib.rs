//! invsee as an arismoko native trait mod (`mod_api`). The logic is the unchanged `horizon-invsee`
//! crate (`core/`); this file wires its hooks and its `Host` to the mod API.
//!
//! Config: `config/invsee-server.toml` (`ConfigKind::Server`, written from the default when missing,
//! read again when it changes).

pub mod horizon_invsee;
pub mod snbt;
use crate::horizon_invsee::{self as inv, Kind};
use mod_api::{
    Command, CommandArgumentValue, CommandInvocation, CommandSource, Compound, Config, ConfigKind,
    ConfigLoadingEvent, ConfigReloadingEvent, Flow, HostSide, HostTransaction, Hooks, Identifier, ItemPlace,
    ItemResource, ItemStack, PlayerContainerCloseEvent, PlayerContainerOpenEvent, TransactionSeal,
    MenuClickedEvent, MenuClosedEvent, MenuReplace, PlayerViewKind, NodeDefault, PermissionGatherNodesEvent,
    PermissionLevel, PermissionNode, PermissionValueType, PlayerCommandEvent, PlayerLoggedInEvent,
    PlayerLoggedOutEvent, RegisterNodeCommandsEvent, RegisterConfigsEvent, RunModCommandEvent,
    ModServiceCallEvent, ModServiceError, PayloadBytes,
    ServerHost, ServerStoppingEvent, ServerTickEvent, ServerTickPhase, Suggestion,
    TabCompleteEvent, Tag, Text, PlayerInventoryError, PlayerInventoryKind, PlayerInventoryMove,
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

/// Whether `word` is a command of the mod: the view and reload words, the addon commands.
fn is_own(word: &str) -> bool {
    OWN_WORDS.contains(&word) || inv::addons::addon_word(word).is_some()
}

/// InvSee++'s `++` reload aliases: the engine refuses them as command words, so they are never registered;
/// `player_command` answers them (the client sends the typed line even for an unknown command).
const PLUS_WORDS: [&str; 2] = ["invsee++reload", "is++reload"];

/// The mod's state, one per process (a dedicated server runs one world). Hooks take it with
/// `try_lock`: a call that re-enters a hook while it is in use is deferred to the next tick.
static STATE: Mutex<Option<inv::InvSee<ItemStack>>> = Mutex::new(None);

/// The menu type each player has open (from `player_container_open`), so the target's personal slots show the
/// placeholder of their container.
static OPEN: Mutex<BTreeMap<u128, String>> = Mutex::new(BTreeMap::new());

/// Menu events that arrived while `STATE` was in use further up the same thread.
static DEFERRED: Mutex<Vec<Deferred>> = Mutex::new(Vec::new());

/// The window id of each viewer's open view (from `open_menu`), so `set_menu_items` can refresh it.
/// The mods that called `invsee:api` `subscribe`: their `<id>:invsee_event` service gets the spectator inventory events.
static SUBSCRIBERS: Mutex<Vec<String>> = Mutex::new(Vec::new());

static WINDOWS: Mutex<BTreeMap<u128, i32>> = Mutex::new(BTreeMap::new());

enum Deferred {
    Clicked(u128, i32, i32, i32, i32),
    Closed(u128, i32),
    /// A command that arrived while the state was in use: the player (None: the console) and the line.
    Command(Option<(u128, String)>, String),
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
    /// The groups of writes `begin` opened and `end` has not closed.
    seals: Vec<TransactionSeal>,
}

impl<'a> GameHost<'a> {
    fn new(host: &'a mut dyn ServerHost) -> Self {
        GameHost { host, seals: Vec::new() }
    }
}

/// The permission nodes that grant `node` too (plugin.yml `children`: `edit` gives `view`, `view` gives
/// `tabcomplete`, the aggregate nodes give their kind). `invseeplusplus.*` cannot be named as a typed node.
fn granting(node: &str) -> Vec<&str> {
    match node {
        "invseeplusplus.invsee.view" => vec![node, "invseeplusplus.invsee.edit", "invseeplusplus.view", "invseeplusplus.edit"],
        "invseeplusplus.endersee.view" => vec![node, "invseeplusplus.endersee.edit", "invseeplusplus.view", "invseeplusplus.edit"],
        "invseeplusplus.invsee.edit" | "invseeplusplus.endersee.edit" => vec![node, "invseeplusplus.edit"],
        "invseeplusplus.tabcomplete" => vec![
            node,
            "invseeplusplus.invsee.view",
            "invseeplusplus.endersee.view",
            "invseeplusplus.invsee.edit",
            "invseeplusplus.endersee.edit",
            "invseeplusplus.view",
            "invseeplusplus.edit",
        ],
        "invseeplusplus.exempt.invsee" | "invseeplusplus.exempt.endersee" => vec![node, "invseeplusplus.exempt"],
        "invseeplusplus.bypass-exempt.invsee" | "invseeplusplus.bypass-exempt.endersee" => {
            vec![node, "invseeplusplus.bypass-exempt"]
        }
        "invseeplusplus.give.inventory" | "invseeplusplus.give.enderchest" => vec![node, "invseeplusplus.give.*"],
        "invseeplusplus.clear.inventory" | "invseeplusplus.clear.enderchest" => vec![node, "invseeplusplus.clear.*"],
        "invseeplusplus.clone.inventory" | "invseeplusplus.clone.enderchest" => vec![node, "invseeplusplus.clone.*"],
        _ => vec![node],
    }
}

/// Whether `uuid` holds `node` or a node that grants it; any error is a denial.
fn has(host: &mut dyn ServerHost, uuid: u128, node: &str) -> bool {
    let Some(player) = host.player(uuid) else {
        return false;
    };
    granting(node)
        .into_iter()
        .any(|n| player.permission_bool(&mut *host, &bool_node(n)).unwrap_or(false))
}

/// The colour a `§` code names.
fn legacy_colour(code: char) -> Option<&'static str> {
    Some(match code {
        '0' => "black",
        '1' => "dark_blue",
        '2' => "dark_green",
        '3' => "dark_aqua",
        '4' => "dark_red",
        '5' => "dark_purple",
        '6' => "gold",
        '7' => "gray",
        '8' => "dark_gray",
        '9' => "blue",
        'a' => "green",
        'b' => "aqua",
        'c' => "red",
        'd' => "light_purple",
        'e' => "yellow",
        'f' => "white",
        _ => return None,
    })
}

/// The words the core sorts a failed offline read by (`InvError::from_horizon`).
fn offline_error(e: &PlayerInventoryError) -> String {
    match e {
        PlayerInventoryError::NoSavedPlayer => "no saved player".into(),
        PlayerInventoryError::LoggingIn => "the player is logging in".into(),
        PlayerInventoryError::Dead => "the player is dead".into(),
        PlayerInventoryError::SwappedOut => "inside a player's turn".into(),
        PlayerInventoryError::StackChanged => "the slot changed".into(),
        PlayerInventoryError::NoRoom => "no free slot".into(),
        other => format!("{other:?}"),
    }
}

/// The name Paper keeps in a saved player file (`bukkit.lastKnownName`).
fn last_known_name(data: &Compound) -> Option<String> {
    for (k, v) in data {
        if k.to_string_lossy() != "bukkit" {
            continue;
        }
        let Tag::Compound(bukkit) = v else { continue };
        for (bk, bv) in bukkit {
            if let (true, Tag::String(name)) = (bk.to_string_lossy() == "lastKnownName", bv) {
                return Some(name.to_string_lossy());
            }
        }
    }
    None
}

/// `Slot` of a saved item compound.
fn saved_slot(tag: &Tag) -> Option<i8> {
    let Tag::Compound(map) = tag else { return None };
    map.iter().find_map(|(k, v)| {
        let name = k.to_string_lossy();
        match (&*name, v) {
            ("Slot", Tag::Byte(b)) => Some(*b),
            _ => None,
        }
    })
}

impl GameHost<'_> {
    /// One slot of a saved player's file written (`set_offline_player_data`): the item compound with its `Slot`
    /// (0-35 main, 100-103 armour feet to head, -106 off hand; the ender chest 0-26) replaces the one there.
    fn write_offline_slot(&mut self, uuid: u128, kind: Kind, slot: usize, new: Option<ItemStack>) -> Result<(), String> {
        let (key, saved): (&str, i8) = match kind {
            Kind::Inventory => (
                "Inventory",
                match slot {
                    0..=35 => slot as i8,
                    36..=39 => (100 + slot - 36) as i8,
                    40 => -106,
                    _ => return Err(format!("no slot {slot}")),
                },
            ),
            Kind::Ender if slot < 27 => ("EnderItems", slot as i8),
            Kind::Ender => return Err(format!("no slot {slot}")),
        };
        let data = self.host.offline_player_data(uuid).ok_or("no saved player")?;
        let entry = match &new {
            Some(stack) => {
                let mut c = stack.to_nbt(&*self.host).ok_or("the stack cannot be saved")?;
                c.insert("Slot".into(), Tag::Byte(saved));
                Some(Tag::Compound(c))
            }
            None => None,
        };
        let mut out = Compound::new();
        let mut written = false;
        for (k, v) in &data {
            if k.to_string_lossy() == key {
                if let Tag::List(items) = v {
                    let mut list: Vec<Tag> = items.iter().filter(|t| saved_slot(t) != Some(saved)).cloned().collect();
                    list.extend(entry.clone());
                    out.insert(k.clone(), Tag::List(list));
                    written = true;
                    continue;
                }
            }
            out.insert(k.clone(), v.clone());
        }
        if !written {
            if let Some(e) = entry {
                out.insert(key.into(), Tag::List(vec![e]));
            }
        }
        self.host.set_offline_player_data(uuid, out).map_err(|e| format!("{e:?}"))
    }
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

    fn inventory(&mut self, uuid: u128, k: Kind) -> Result<inv::Inv<ItemStack>, inv::InvError> {
        let Some(player) = self.host.player(uuid) else {
            // A player who is not online: the game reads their save (`server_player_inventory`).
            let kind = match k {
                Kind::Inventory => PlayerInventoryKind::Player,
                Kind::Ender => PlayerInventoryKind::Ender,
            };
            let snapshot = self
                .host
                .server_player_inventory(uuid, kind)
                .map_err(|e| inv::InvError::from_horizon(&offline_error(&e)))?;
            let mut slots = vec![None; k.size()];
            for (i, stack) in snapshot.slots.into_iter().enumerate() {
                if let Some(s) = slots.get_mut(i) {
                    *s = stack;
                }
            }
            return Ok(inv::Inv { slots, online: false });
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
        let Some(player) = self.host.player(uuid) else {
            return self.write_offline_slot(uuid, kind, slot, new).map(|()| current);
        };
        let written = match kind {
            Kind::Inventory => player.set_inventory_slot(&mut *self.host, slot as u32, new),
            Kind::Ender => player.set_ender_chest_slot(&mut *self.host, slot as u32, new),
        };
        match written {
            Ok(()) => Ok(current),
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
        if self.host.player(from.0).is_none() || self.host.player(to.0).is_none() {
            // A saved player's file is changed by the game, whole stack or nothing.
            let kind = |k: Kind| match k {
                Kind::Inventory => PlayerInventoryKind::Player,
                Kind::Ender => PlayerInventoryKind::Ender,
            };
            let request = PlayerInventoryMove::new((from.0, kind(from.1), from.2 as u32), expected.clone(), (to.0, kind(to.1)));
            return self.host.server_move_inventory_stack(&request).map_err(|e| offline_error(&e));
        }
        let target = self.inventory(to.0, to.1).map_err(|e| format!("{e:?}"))?;
        let room = match to.1 {
            Kind::Inventory => 36,
            Kind::Ender => 27,
        };
        let free = (0..room).find(|&i| target.slots[i].is_none()).ok_or("no free slot")?;
        self.set_slot(from.0, from.1, from.2, Some(expected), None)?;
        if let Err(e) = self.set_slot(to.0, to.1, free, None, Some(expected.clone())) {
            // The source slot is put back only if it is still empty; a failed rollback is said, not hidden.
            if self.set_slot(from.0, from.1, from.2, None, Some(expected.clone())).is_err() {
                return Err(format!("{e}; the item could not be put back, check the slot"));
            }
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
        let window = self.host.open_menu(viewer, rows, title, items, MenuReplace::CloseFirst).ok().flatten();
        if let Some(window) = window {
            WINDOWS.lock().unwrap_or_else(|e| e.into_inner()).insert(viewer, window);
        }
        window
    }

    /// The game's live view of another online player's inventory or ender chest (`open_player_view`):
    /// its slots are the target's own, clicks run by vanilla's rules and the target's changes show on
    /// the next tick. The game's layout is fixed (the placeholders of the template are not shown),
    /// so `_layout` is not read. None (the mod then opens its copy) for one's own inventory, a target
    /// who is not online and any refusal.
    fn open_inventory_menu(
        &mut self,
        viewer: u128,
        _rows: u8,
        title: &str,
        target: (u128, Kind),
        _layout: &[Option<usize>],
    ) -> Option<i32> {
        if viewer == target.0 {
            return None;
        }
        let kind = match target.1 {
            Kind::Inventory => PlayerViewKind::Inventory,
            Kind::Ender => PlayerViewKind::EnderChest,
        };
        let title = text(&*self.host, title);
        self.host
            .open_player_view(viewer, target.0, kind, title, MenuReplace::CloseFirst)
            .ok()
            .flatten()
    }

    fn with_count(&self, stack: &ItemStack, count: u32) -> Option<ItemStack> {
        let mut split = stack.clone();
        split.set_count(count).ok()?;
        Some(split)
    }

    // The core asks the server's own profile cache first (`profile_by_name`, `profile_by_uuid`) and
    // reads usercache.json or Mojang only when that gives nothing.
    fn profile_by_name(&self, name: &str) -> Option<(u128, String)> {
        let player = self.host.players().player_by_name(name)?;
        Some((player.uuid(), player.name(&*self.host)?))
    }

    /// The `player data save files` strategy: the last known name in the player's saved file.
    fn save_file_lookup(&mut self, q: &inv::Query) -> Option<(u128, String)> {
        let inv::Query::Uuid(uuid) = q else { return None };
        let data = self.host.offline_player_data(*uuid)?;
        last_known_name(&data).map(|name| (*uuid, name))
    }

    fn profile_by_uuid(&self, uuid: u128) -> Option<String> {
        self.host.player(uuid)?.name(&*self.host)
    }

    /// The permission node's answer from the server's handler. The node's default is its shipped level
    /// (`permission_gather_nodes`), so the config's `default_level` is not read here; a refused query is a denial.
    fn has_permission(&mut self, uuid: u128, node: &str, _default_level: u8) -> bool {
        has(&mut *self.host, uuid, node)
    }

    fn same_stack(&self, a: &ItemStack, b: &ItemStack) -> bool {
        a.same_item(b) && a.same_components(b)
    }

    fn max_stack(&self, stack: &ItemStack) -> u32 {
        self.host.max_stack_size(stack.item()).map(|n| n as u32).unwrap_or(64)
    }

    fn cursor(&mut self, viewer: u128, window: i32) -> Option<Option<ItemStack>> {
        let menu = self.host.menu(viewer, window)?;
        Some(menu.cursor().stack(&mut *self.host))
    }

    fn set_cursor(
        &mut self,
        viewer: u128,
        window: i32,
        expected: Option<&ItemStack>,
        new: Option<ItemStack>,
    ) -> Result<(), String> {
        let menu = self.host.menu(viewer, window).ok_or("the view is gone")?;
        let place = menu.cursor();
        let host: &mut dyn ServerHost = &mut *self.host;
        host.transaction(|host, tx| -> Result<(), String> {
            if let Some(old) = expected {
                let n = old.count();
                if place.extract(host, tx, &ItemResource::of(old), n) != n {
                    return Err("the cursor changed".into());
                }
            }
            if let Some(stack) = &new {
                let n = stack.count();
                if place.insert(host, tx, &ItemResource::of(stack), n) != n {
                    return Err("the cursor takes no more".into());
                }
            }
            Ok(())
        })
    }

    fn begin(&mut self) -> bool {
        let seal = self.host.transaction_open();
        self.seals.push(seal);
        true
    }

    fn end(&mut self, commit: bool) {
        if let Some(seal) = self.seals.pop() {
            self.host.transaction_close(&seal, commit);
        }
    }

    fn open_container(&self, uuid: u128) -> Option<String> {
        OPEN.lock().unwrap_or_else(|e| e.into_inner()).get(&uuid).cloned()
    }

    /// The target's crafting grid and cursor, read from their inventory screen (the only menu the game lends).
    fn target_extras(&mut self, uuid: u128) -> inv::Extras<ItemStack> {
        if OPEN.lock().unwrap_or_else(|e| e.into_inner()).contains_key(&uuid) {
            return inv::Extras::default();
        }
        let Some(player) = self.host.player(uuid) else {
            return inv::Extras::default();
        };
        let Some(menu) = player.container_menu(&*self.host) else {
            return inv::Extras::default();
        };
        let grid = (1..=4)
            .map(|slot| ItemPlace::MenuSlot { menu, slot }.stack(&mut *self.host))
            .collect();
        inv::Extras { grid: Some(grid), cursor: menu.cursor().stack(&mut *self.host) }
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
        for l in line.lines() {
            let _ = self.host.messages().console_log(ID, mod_api::ConsoleLevel::Info, l);
        }
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
        // The name is a text component (`minecraft:custom_name`): InvSee++'s names are `§<colour><name>`, not italic.
        let mut chars = name.chars();
        let (color, plain) = match (chars.next(), chars.next()) {
            (Some('\u{a7}'), Some(code)) if legacy_colour(code).is_some() => (legacy_colour(code), &name[3..]),
            _ => (None, name),
        };
        let mut text: Compound = [
            ("text".into(), Tag::String(plain.into())),
            ("italic".into(), Tag::Byte(0)),
        ]
        .into();
        if let Some(color) = color {
            text.insert("color".into(), Tag::String(color.into()));
        }
        // Every placeholder carries InvSee++'s lore line.
        let lore: Compound = [("text".into(), Tag::String("InvSee++ placeholder".into()))].into();
        let mut patch: Compound = [
            ("minecraft:custom_name".into(), Tag::Compound(text)),
            ("minecraft:lore".into(), Tag::List(vec![Tag::Compound(lore)])),
        ]
        .into();
        // The icon palette hides the armour's attributes; the off-hand icon is a banner-patterned shield.
        let base = item.strip_prefix("minecraft:").unwrap_or(item);
        let hidden: Option<&[&str]> = match base {
            "chainmail_helmet" | "chainmail_chestplate" | "chainmail_leggings" | "chainmail_boots" => {
                Some(&["minecraft:attribute_modifiers"])
            }
            "shield" => Some(&["minecraft:attribute_modifiers", "minecraft:dyed_color"]),
            _ => None,
        };
        if let Some(hidden) = hidden {
            let tooltip: Compound = [(
                "hidden_components".into(),
                Tag::List(hidden.iter().map(|h| Tag::String((*h).into())).collect()),
            )]
            .into();
            patch.insert("minecraft:tooltip_display".into(), Tag::Compound(tooltip));
        }
        if base == "shield" {
            let stripe = |pattern: &str, color: &str| {
                let c: Compound = [
                    ("pattern".into(), Tag::String(pattern.into())),
                    ("color".into(), Tag::String(color.into())),
                ]
                .into();
                Tag::Compound(c)
            };
            patch.insert("minecraft:base_color".into(), Tag::String("white".into()));
            patch.insert(
                "minecraft:banner_patterns".into(),
                Tag::List(vec![stripe("minecraft:stripe_top", "red"), stripe("minecraft:stripe_bottom", "blue")]),
            );
        }
        let _ = stack.apply_components(&patch);
        stack
    }

    fn describe(&self, stack: &ItemStack) -> (String, u32) {
        (stack.item().id(&*self.host), stack.count())
    }

    fn item_ids(&self) -> Vec<String> {
        self.host.item_ids().iter().map(|i| i.to_string()).collect()
    }

    /// Vanilla's item argument: the id, then `[component=value,!component]`.
    fn make_stack(&self, item: &str, components: Option<&str>) -> Result<ItemStack, String> {
        let unknown = || format!("Unknown item '{item}'");
        let id = Identifier::parse(item).map_err(|_| unknown())?;
        let key = self.host.item_key(&id).ok_or_else(unknown)?;
        let mut stack = ItemStack::of(key);
        if let Some(inner) = components {
            let patch = snbt::parse_components(inner)?;
            stack.apply_components(&patch).map_err(|e| format!("Invalid components: {e:?}"))?;
        }
        Ok(stack)
    }

    fn tag_items(&self, tag: &str) -> Vec<String> {
        let Ok(id) = Identifier::parse(tag) else { return Vec::new() };
        let tags = self.host.tags();
        let Some(key) = tags.key(&id) else { return Vec::new() };
        let Some(items) = tags.items(key) else { return Vec::new() };
        items.into_iter().map(|k| k.id(&*self.host)).collect()
    }

    /// The spectator inventory events, as service calls to the mods that subscribed. A reply of `cancel` cancels the
    /// save event.
    fn event(&mut self, line: &str) -> bool {
        let subscribers = SUBSCRIBERS.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let mut cancelled = false;
        for id in subscribers {
            match self.host.commands().call_mod_service(&format!("{id}:invsee_event"), line.as_bytes()) {
                Ok(reply) => cancelled |= reply == b"cancel",
                Err(ModServiceError::ProviderNotLoaded) => {
                    SUBSCRIBERS.lock().unwrap_or_else(|e| e.into_inner()).retain(|s| *s != id);
                }
                Err(_) => {}
            }
        }
        cancelled
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

/// The plugin.yml permission node of a command word (`invseeplusplus.invsee.view` for `invsee` and its
/// aliases, `invseeplusplus.endersee.view`, `invseeplusplus.reload` for the reload words).
fn node_of_word(word: &str) -> String {
    if let Some((addon, kind)) = inv::addons::addon_word(word) {
        return inv::addons::addon_node(addon, kind);
    }
    match word {
        "endersee" | "enderchestsee" | "esee" => "invseeplusplus.endersee.view",
        "invseeplusplusreload" | "isppreload" | "invsee++reload" | "is++reload" => "invseeplusplus.reload",
        _ => "invseeplusplus.invsee.view",
    }
    .to_owned()
}

/// The typed Boolean node of a plugin.yml permission name (`invseeplusplus.reload`).
fn bool_node(name: &str) -> PermissionNode {
    PermissionNode::new(name, PermissionValueType::Boolean)
}

/// One declared node: its default (`None` is plugin.yml's `default: false`, nobody, operators included) and the
/// nodes holding it grants (plugin.yml `children`).
struct Declared {
    name: &'static str,
    level: Option<i32>,
    children: &'static [&'static str],
}

/// The permission nodes of plugin.yml. The view, edit, reload and tab-complete levels are the `[access]` levels of
/// `config/invsee-server.toml` at start (the shipped defaults without the file; changing them needs a restart).
fn permission_nodes() -> Vec<Declared> {
    let access = std::fs::read_to_string(Path::new("config").join("invsee-server.toml"))
        .ok()
        .and_then(|t| inv::config::Config::parse(&t).ok())
        .map(|c| c.access)
        .unwrap_or_default();
    let node = |name, level: Option<i32>, children| Declared { name, level, children };
    vec![
        node(
            "invseeplusplus.*",
            None,
            &[
                "invseeplusplus.invsee.edit",
                "invseeplusplus.endersee.edit",
                "invseeplusplus.exempt.invsee",
                "invseeplusplus.exempt.endersee",
                "invseeplusplus.bypass-exempt.invsee",
                "invseeplusplus.bypass-exempt.endersee",
                "invseeplusplus.tabcomplete",
                "invseeplusplus.reload",
                "invseeplusplus.give.*",
                "invseeplusplus.clear.*",
                "invseeplusplus.clone.*",
            ],
        ),
        node(
            "invseeplusplus.view",
            Some(access.invsee_view.min(access.endersee_view) as i32),
            &["invseeplusplus.invsee.view", "invseeplusplus.endersee.view"],
        ),
        node(
            "invseeplusplus.edit",
            Some(access.invsee_edit.min(access.endersee_edit) as i32),
            &["invseeplusplus.invsee.edit", "invseeplusplus.endersee.edit"],
        ),
        node("invseeplusplus.exempt", None, &["invseeplusplus.exempt.invsee", "invseeplusplus.exempt.endersee"]),
        node(
            "invseeplusplus.bypass-exempt",
            None,
            &["invseeplusplus.bypass-exempt.invsee", "invseeplusplus.bypass-exempt.endersee"],
        ),
        node("invseeplusplus.invsee.view", Some(access.invsee_view as i32), &["invseeplusplus.tabcomplete"]),
        node("invseeplusplus.invsee.edit", Some(access.invsee_edit as i32), &["invseeplusplus.invsee.view"]),
        node("invseeplusplus.endersee.view", Some(access.endersee_view as i32), &["invseeplusplus.tabcomplete"]),
        node("invseeplusplus.endersee.edit", Some(access.endersee_edit as i32), &["invseeplusplus.endersee.view"]),
        node("invseeplusplus.tabcomplete", Some(access.invsee_view.min(access.endersee_view) as i32), &[]),
        node("invseeplusplus.reload", Some(access.reload as i32), &[]),
        node("invseeplusplus.give.*", None, &["invseeplusplus.give.inventory", "invseeplusplus.give.enderchest"]),
        node("invseeplusplus.give.inventory", Some(access.give_inventory as i32), &[]),
        node("invseeplusplus.give.enderchest", Some(access.give_enderchest as i32), &[]),
        node("invseeplusplus.clear.*", None, &["invseeplusplus.clear.inventory", "invseeplusplus.clear.enderchest"]),
        node("invseeplusplus.clear.inventory", Some(access.clear_inventory as i32), &[]),
        node("invseeplusplus.clear.enderchest", Some(access.clear_enderchest as i32), &[]),
        node("invseeplusplus.clone.*", None, &["invseeplusplus.clone.inventory", "invseeplusplus.clone.enderchest"]),
        node("invseeplusplus.clone.inventory", Some(access.clone_inventory as i32), &[]),
        node("invseeplusplus.clone.enderchest", Some(access.clone_enderchest as i32), &[]),
        node("invseeplusplus.exempt.invsee", None, &[]),
        node("invseeplusplus.exempt.endersee", None, &[]),
        node("invseeplusplus.bypass-exempt.invsee", None, &[]),
        node("invseeplusplus.bypass-exempt.endersee", None, &[]),
    ]
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
    perform_try(host, uuid, name, line).flatten()
}

/// As `perform`, but the outer None says the state was in use (the caller defers the command).
fn perform_try(host: &mut dyn ServerHost, uuid: u128, name: &str, line: &str) -> Option<Option<Vec<String>>> {
    let mut game = GameHost::new(host);
    with(|m| m.perform(&mut game, uuid, name, line))
}

/// The console's command line; the core's lines, or None when it is not ours or the state is in use.
fn console_try(host: &mut dyn ServerHost, line: &str) -> Option<Option<Vec<String>>> {
    let mut game = GameHost::new(host);
    with(|m| m.console_with(&mut game, line))
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
    // Commands first: they take the state themselves.
    for d in &deferred {
        if let Deferred::Command(sender, line) = d {
            match sender {
                Some((uuid, name)) => {
                    for l in perform(&mut *game.host, *uuid, name, line).unwrap_or_default() {
                        inv::Host::tell(&mut *game, *uuid, &l);
                    }
                }
                None => {
                    for l in console_try(&mut *game.host, line).flatten().unwrap_or_default() {
                        eprint_line(&l);
                    }
                }
            }
        }
    }
    with(|m| {
        for d in deferred {
            match d {
                Deferred::Clicked(u, w, s, b, mode) => m.clicked(game, u, w, s, b, mode),
                Deferred::Closed(u, w) => m.closed(game, u, w),
                Deferred::Command(..) => {}
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

    fn register_node_commands(&self, _: &mut dyn ServerHost, event: &mut RegisterNodeCommandsEvent<'_>) {
        for (usage, level) in usages() {
            // The core writes `word?` so the client asks for names; arismoko's word is plain `word`.
            let usage = usage.replace("word?", "word");
            // The plugin.yml node of the word: a permissions mod's answer decides who sees the usage, the level
            // only when none answers (as Bukkit's `permission:`). Others get Bukkit's text from `player_command`.
            let word = usage.split_whitespace().next().unwrap_or_default();
            event
                .commands
                .push(Command::op(&usage, level).with_node(&bool_node(&node_of_word(word))));
        }
    }

    fn permission_gather_nodes(&self, event: &mut PermissionGatherNodesEvent<'_>) {
        for d in permission_nodes() {
            let default = match d.level {
                Some(level) => NodeDefault::Level(PermissionLevel::from_level(level)),
                None => NodeDefault::Nobody,
            };
            let children: Vec<PermissionNode> = d.children.iter().map(|c| bool_node(c)).collect();
            let _ = event.nodes.add_with_children(bool_node(d.name), default, &children);
        }
    }

    /// Bukkit's text for a player without the command's permission (plugin.yml `permission:`, red).
    /// The usage is hidden from such a player (`register_commands`), so the typed command would read
    /// "unknown command" without this.
    fn player_command(&self, host: &mut dyn ServerHost, event: &mut PlayerCommandEvent<'_>) -> Flow {
        let root = event
            .command
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if PLUS_WORDS.contains(&root.as_str()) {
            // Same node as `isppreload`; permitted players get the reload, others Bukkit's text.
            let uuid = event.player;
            let name = host.player(uuid).and_then(|p| p.name(&*host)).unwrap_or_default();
            let allowed = has(host, uuid, "invseeplusplus.reload");
            event.cancelled = true;
            if allowed {
                for l in perform(host, uuid, &name, "invseeplusplusreload").unwrap_or_default() {
                    let line = text(&*host, &l);
                    event.replies.push(line);
                }
            } else {
                event.replies.push(Text::literal(
                    "\u{a7}cI'm sorry, but you do not have permission to perform this command. Please contact the server administrators if you believe that this is in error.",
                ));
            }
            return Flow::Cancel;
        }
        if !is_own(&root) {
            return Flow::Continue;
        }
        let uuid = event.player;
        let allowed = has(host, uuid, &node_of_word(&root));
        if allowed {
            return Flow::Continue;
        }
        event.cancelled = true;
        event.replies.push(Text::literal(
            "\u{a7}cI'm sorry, but you do not have permission to perform this command. Please contact the server administrators if you believe that this is in error.",
        ));
        Flow::Cancel
    }

    /// The service `invsee:api`: `subscribe <mod id>` / `unsubscribe <mod id>` for the spectator inventory events (the
    /// mod's own service `<id>:invsee_event` is then called with `<event>\t<spectator>\t<target>\t<name>\t<view>`;
    /// the save event is cancelled by the reply `cancel`), `version`.
    fn mod_service_call(&self, _: &mut dyn ServerHost, event: &mut ModServiceCallEvent<'_>) -> Option<PayloadBytes> {
        if event.service != "invsee:api" {
            return None;
        }
        let text = std::str::from_utf8(&event.payload.0).ok()?;
        let mut parts = text.split('\t');
        let reply = match parts.next()? {
            "version" => "0".to_owned(),
            "subscribe" => {
                let id = parts.next()?;
                let mut subs = SUBSCRIBERS.lock().unwrap_or_else(|e| e.into_inner());
                if !subs.iter().any(|s| s == id) {
                    subs.push(id.to_owned());
                }
                "true".to_owned()
            }
            "unsubscribe" => {
                let id = parts.next()?;
                SUBSCRIBERS.lock().unwrap_or_else(|e| e.into_inner()).retain(|s| s != id);
                "true".to_owned()
            }
            _ => return None,
        };
        Some(PayloadBytes(reply.into_bytes()))
    }

    fn run_mod_command(&self, host: &mut dyn ServerHost, event: &mut RunModCommandEvent<'_>) -> Flow {
        let ours = event
            .command
            .literals
            .first()
            .is_some_and(|w| is_own(w.as_str()));
        if !ours {
            return Flow::Continue;
        }
        let line = command_line(&event.command);
        // Outer None: the state is in use; the command runs at the end of the tick and answers the player (or the log).
        let answer = match &event.source {
            CommandSource::Player { player, .. } => {
                let uuid = player.uuid();
                let name = player.name(&*host).unwrap_or_default();
                let answer = perform_try(host, uuid, &name, &line);
                if answer.is_none() {
                    defer(Deferred::Command(Some((uuid, name)), line));
                    return Flow::Cancel;
                }
                answer
            }
            CommandSource::Console { .. } => {
                let answer = console_try(host, &line);
                if answer.is_none() {
                    defer(Deferred::Command(None, line));
                    return Flow::Cancel;
                }
                answer
            }
            _ => None,
        };
        let Some(lines) = answer.flatten() else {
            return Flow::Continue;
        };
        for l in lines {
            let line = text(&*host, &l);
            let _ = host.commands().feedback(line);
        }
        Flow::Cancel
    }

    fn tab_complete(&self, host: &mut dyn ServerHost, event: &mut TabCompleteEvent<'_>) {
        let mut game = GameHost::new(host);
        let Some(Some((start, names))) =
            with(|m| m.tab_complete(&mut game, event.player, event.command))
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
            let mut game = GameHost::new(host);
            tick(&mut game);
        }
    }

    fn menu_clicked(&self, host: &mut dyn ServerHost, event: &mut MenuClickedEvent) {
        let (uuid, window, slot, button, mode) =
            (event.uuid, event.window, event.slot, event.button, event.mode);
        let mut game = GameHost::new(host);
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
        let mut game = GameHost::new(host);
        if with(|m| m.closed(&mut game, uuid, window)).is_none() {
            defer(Deferred::Closed(uuid, window));
        }
    }

    fn player_container_open(&self, _: &mut dyn ServerHost, event: &mut PlayerContainerOpenEvent<'_>) {
        OPEN.lock().unwrap_or_else(|e| e.into_inner()).insert(event.player, event.menu.to_owned());
    }

    fn player_container_close(&self, _: &mut dyn ServerHost, event: &mut PlayerContainerCloseEvent<'_>) {
        OPEN.lock().unwrap_or_else(|e| e.into_inner()).remove(&event.player);
    }

    fn player_logged_in(&self, _: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        with(|m| m.logged_in(event.player, &event.name));
    }

    fn player_logged_out(&self, _: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        with(|m| m.logged_out(event.player, &event.name));
    }

    fn server_stopping(&self, host: &mut dyn ServerHost, _: &mut ServerStoppingEvent) {
        let mut game = GameHost::new(host);
        with(|m| m.stopping(&mut game));
    }
}
