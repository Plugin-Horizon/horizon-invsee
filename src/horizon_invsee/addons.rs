//! The InvSee++ addons: `/invgive` and `/endergive` (InvSee++_Give), `/invclear` and `/enderclear`
//! (InvSee++_Clear), `/invclone` and `/enderclone` (InvSee++_Clone), LGPL-2.1, Jannyboy11. They run on the same
//! target resolution as `/invsee`; the inventory is read and written through the [`Host`].
use super::config::Bypass;
use super::*;
use std::path::Path;

/// The pseudo-player of the console: its replies go to the log.
pub const CONSOLE: u128 = 0;

/// The three addons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Addon {
    Give,
    Clear,
    Clone,
}

/// What a pending command does once its target is found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Op {
    /// `/invsee`, `/endersee`.
    View,
    Give { item: String, components: Option<String>, amount: u32 },
    /// `ids` are the item ids matched (empty with `label` None: clear everything).
    Clear { ids: Vec<String>, label: Option<String>, max: Option<u32> },
    /// The source of a clone; `to` is the typed target (None: the sender).
    CloneFrom { to: Option<String> },
    /// The target of a clone, the source already found: (uuid, name, typed).
    CloneTo { source: (u128, String, String) },
}

/// Every command word the addons answer to, with its aliases (plugin.yml).
pub const WORDS: [&str; 18] = [
    "invgive", "inventorygive", "igive", "endergive", "enderchestgive", "egive", "invclear", "inventoryclear", "iclear",
    "enderclear", "enderchestclear", "eclear", "invclone", "inventoryclone", "iclone", "enderclone", "enderchestclone",
    "eclone",
];

pub fn addon_word(w: &str) -> Option<(Addon, Kind)> {
    Some(match w {
        "invgive" | "inventorygive" | "igive" => (Addon::Give, Kind::Inventory),
        "endergive" | "enderchestgive" | "egive" => (Addon::Give, Kind::Ender),
        "invclear" | "inventoryclear" | "iclear" => (Addon::Clear, Kind::Inventory),
        "enderclear" | "enderchestclear" | "eclear" => (Addon::Clear, Kind::Ender),
        "invclone" | "inventoryclone" | "iclone" => (Addon::Clone, Kind::Inventory),
        "enderclone" | "enderchestclone" | "eclone" => (Addon::Clone, Kind::Ender),
        _ => return None,
    })
}

/// The plugin.yml node (`invseeplusplus.give.inventory`).
pub fn addon_node(a: Addon, k: Kind) -> String {
    let what = match a {
        Addon::Give => "give",
        Addon::Clear => "clear",
        Addon::Clone => "clone",
    };
    let kind = match k {
        Kind::Inventory => "inventory",
        Kind::Ender => "enderchest",
    };
    format!("invseeplusplus.{what}.{kind}")
}

fn usage_of(a: Addon, word: &str) -> String {
    match a {
        Addon::Give => format!("/{word} <target player> <item type> [<amount>] [<nbt tag>]"),
        Addon::Clear => format!("/{word} <target player> <item type>? <max count>?"),
        Addon::Clone => format!("/{word} <source player> [<target player>]"),
    }
}

/// Bukkit's `ItemStack.toString()` for a plain stack.
pub fn stack_text(id: &str, count: u32) -> String {
    format!("ItemStack{{{} x {count}}}", id.strip_prefix("minecraft:").unwrap_or(id).to_uppercase())
}

/// `Material.matchMaterial`: the item id a typed name means, when the game has it.
fn match_material(ids: &[String], typed: &str) -> Option<String> {
    let lower = typed.to_lowercase().replace(' ', "_");
    let id = if lower.contains(':') { lower } else { format!("minecraft:{lower}") };
    ids.iter().any(|i| *i == id).then_some(id)
}

/// `Convert.convertAmount`.
fn convert_amount(input: &str) -> Result<u32, String> {
    match input.parse::<i32>() {
        Ok(v) if v > 0 => Ok(v as u32),
        _ => Err(format!("{input} is not a positive integer.")),
    }
}

/// The arguments of `/invgive`: item text (with its components), amount, the nbt tag.
#[derive(Debug, PartialEq, Eq)]
pub struct GiveArgs {
    pub target: String,
    pub item: String,
    pub amount: Option<String>,
    pub tag: Option<String>,
}

/// `ArgParser.determineFormat` and `splitArguments`: None when the usage should be shown.
pub fn split_give(args: &[&str]) -> Option<GiveArgs> {
    if args.len() < 2 {
        return None;
    }
    let last = args[args.len() - 1];
    let target = args[0].to_owned();
    Some(if last.parse::<i32>().is_ok() {
        GiveArgs { target, item: args[1..args.len() - 1].join(" "), amount: Some(last.to_owned()), tag: None }
    } else if last.ends_with('}') {
        GiveArgs {
            target,
            item: args[1].to_owned(),
            amount: args.get(2).map(|a| (*a).to_owned()),
            tag: Some(args.get(3..).unwrap_or(&[]).join(" ")),
        }
    } else {
        GiveArgs { target, item: args[1].to_owned(), amount: None, tag: None }
    })
}

/// An item text `minecraft:sword[components]` split into its id and the inside of the brackets.
pub fn split_item(text: &str) -> Result<(String, Option<String>), String> {
    let (id, comps) = match text.find('[') {
        Some(i) => {
            let rest = &text[i + 1..];
            let Some(inner) = rest.strip_suffix(']') else {
                return Err(format!("Expected ']' at position {}", text.len()));
            };
            (&text[..i], Some(inner.to_owned()))
        }
        None => (text, None),
    };
    Ok((id.to_owned(), comps))
}

// ---- the item queue (`ItemQueueManager`) --------------------------------------------------------------------------

/// One queued stack: the item as given, and how many.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Queued {
    pub item: String,
    pub components: Option<String>,
    pub count: u32,
}

/// A player's two queues (`<data>/item queues/<uuid>.yml`: JSON, which YAML reads).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Queues {
    #[serde(rename = "inventory-queue", default)]
    pub inventory: Vec<Queued>,
    #[serde(rename = "enderchest-queue", default)]
    pub enderchest: Vec<Queued>,
}

impl Queues {
    fn path(dir: &Path, uuid: u128) -> PathBuf {
        let h = format!("{uuid:032x}");
        let id = format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32]);
        dir.join("item queues").join(format!("{id}.yml"))
    }

    pub fn load(dir: &Path, uuid: u128) -> Queues {
        std::fs::read_to_string(Self::path(dir, uuid)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
    }

    /// Writes the queues; both empty removes the file (`ItemQueueManager.save`).
    pub fn save(&self, dir: &Path, uuid: u128) -> Result<(), String> {
        let path = Self::path(dir, uuid);
        if self.inventory.is_empty() && self.enderchest.is_empty() {
            let _ = std::fs::remove_file(&path);
            return Ok(());
        }
        std::fs::create_dir_all(path.parent().unwrap_or(dir)).map_err(|e| e.to_string())?;
        std::fs::write(&path, serde_json::to_string_pretty(self).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
    }

    fn of(&mut self, kind: Kind) -> &mut Vec<Queued> {
        match kind {
            Kind::Inventory => &mut self.inventory,
            Kind::Ender => &mut self.enderchest,
        }
    }
}

// ---- adding (`Inventory.addItem`) ---------------------------------------------------------------------------------

/// `Inventory.addItem` over the first `limit` slots: fill partial stacks of the same item, then empty slots.
/// Returns how many did not fit.
pub fn add_item<H: Host>(host: &H, slots: &mut [Option<H::Stack>], limit: usize, stack: &H::Stack, mut left: u32) -> u32 {
    let max = host.max_stack(stack).max(1);
    while left > 0 {
        let partial = (0..limit.min(slots.len())).find(|&i| match &slots[i] {
            Some(s) => host.same_stack(s, stack) && host.describe(s).1 < max,
            None => false,
        });
        if let Some(i) = partial {
            let have = host.describe(slots[i].as_ref().expect("partial slot")).1;
            let put = (max - have).min(left);
            match host.with_count(stack, have + put) {
                Some(s) => slots[i] = Some(s),
                None => return left,
            }
            left -= put;
            continue;
        }
        let Some(i) = (0..limit.min(slots.len())).find(|&i| slots[i].is_none()) else { return left };
        let put = max.min(left);
        match host.with_count(stack, put) {
            Some(s) => slots[i] = Some(s),
            None => return left,
        }
        left -= put;
    }
    0
}

/// How many slots a give/queue fills: the storage slots (hotbar and main), or the whole ender chest.
fn storage_limit(kind: Kind) -> usize {
    match kind {
        Kind::Inventory => 36,
        Kind::Ender => 27,
    }
}

/// An inventory as read for an addon.
struct Snap<S> {
    slots: Vec<Option<S>>,
    online: bool,
}

impl<S: Clone + PartialEq> InvSee<S> {
    fn say<H: Host<Stack = S>>(&self, host: &mut H, viewer: u128, line: &str) {
        if viewer == CONSOLE {
            host.log(&strip_colours(line));
        } else {
            host.tell(viewer, line);
        }
    }

    pub(super) fn addon_say<H: Host<Stack = S>>(&self, host: &mut H, viewer: u128, line: &str) {
        self.say(host, viewer, line);
    }

    pub(super) fn addon_level_pub(&self, a: Addon, k: Kind) -> u8 {
        self.addon_level(a, k)
    }

    fn addon_level(&self, a: Addon, k: Kind) -> u8 {
        let c = &self.cfg.access;
        match (a, k) {
            (Addon::Give, Kind::Inventory) => c.give_inventory,
            (Addon::Give, Kind::Ender) => c.give_enderchest,
            (Addon::Clear, Kind::Inventory) => c.clear_inventory,
            (Addon::Clear, Kind::Ender) => c.clear_enderchest,
            (Addon::Clone, Kind::Inventory) => c.clone_inventory,
            (Addon::Clone, Kind::Ender) => c.clone_enderchest,
        }
    }

    /// The usages of the addon commands and the level each needs (registered as the view commands are).
    pub fn addon_usages(&self) -> Vec<(String, u8)> {
        let mut out = Vec::new();
        for w in WORDS {
            let (a, k) = addon_word(w).expect("a listed word");
            let spec = match a {
                Addon::Give => format!("{w} <target:word?> <args:text>"),
                Addon::Clear => format!("{w} <target:word?> [args:text]"),
                Addon::Clone => format!("{w} <source:word?> [target:word?]"),
            };
            out.push((spec, self.addon_level(a, k)));
        }
        out
    }

    /// An addon command (`word` lower-cased, `args` after it). None when `word` is not one; else the lines to answer
    /// (the work itself happens at the next tick, when the target is found).
    pub fn addon_perform<H: Host<Stack = S>>(
        &mut self,
        host: &mut H,
        viewer: u128,
        viewer_name: &str,
        word: &str,
        args: &[&str],
    ) -> Option<Vec<String>> {
        let (a, kind) = addon_word(word)?;
        if viewer != CONSOLE && !host.has_permission(viewer, &addon_node(a, kind), self.addon_level(a, kind)) {
            return Some(vec![NO_PERMISSION.into()]);
        }
        let usage = vec![usage_of(a, word)];
        let query = |t: &str| match parse_uuid(t) {
            Some(u) => Query::Uuid(u),
            None => Query::Name(t.to_owned()),
        };
        let (typed, op) = match a {
            Addon::Give => {
                let Some(g) = split_give(args) else { return Some(usage) };
                let amount = match g.amount.as_deref() {
                    None => 1,
                    Some(t) => match convert_amount(t) {
                        Ok(n) => n,
                        Err(e) => return Some(vec![format!("§c{e}")]),
                    },
                };
                let (id, comps) = match split_item(&g.item) {
                    Ok(x) => x,
                    Err(e) => return Some(vec![format!("§c{e}")]),
                };
                let ids = host.item_ids();
                let Some(id) = match_material(&ids, &id) else {
                    return Some(vec![format!("§cUnknown item '{}'", g.item)]);
                };
                if let Err(e) = host.make_stack(&id, comps.as_deref()) {
                    return Some(vec![format!("§c{e}")]);
                }
                // The game's 1.20.5+ items have components, not nbt tags (InvSee++ says so for these versions).
                if g.tag.is_some() {
                    return Some(vec![
                        "§cInvSee++ for Minecraft 1.20.5 and up does not support NBT tags on item stacks.".into(),
                    ]);
                }
                (g.target, Op::Give { item: id, components: comps, amount })
            }
            Addon::Clear => {
                if args.is_empty() {
                    return Some(usage);
                }
                let mut ids = Vec::new();
                let mut label = None;
                let mut max = None;
                if let Some(input) = args.get(1) {
                    let known = host.item_ids();
                    if let Some(id) = match_material(&known, input) {
                        label = Some(id.strip_prefix("minecraft:").unwrap_or(&id).to_uppercase());
                        ids.push(id);
                    } else {
                        let key = if input.contains(':') { input.to_lowercase() } else { format!("minecraft:{}", input.to_lowercase()) };
                        let mut members = host.tag_items(&key);
                        if members.is_empty() {
                            members = host.block_tag_items(&key);
                        }
                        if members.is_empty() {
                            return Some(vec![format!("§cMaterial {input} does not exist.")]);
                        }
                        label = Some(key);
                        ids = members;
                    }
                }
                if let Some(input) = args.get(2) {
                    match convert_amount(input) {
                        Ok(n) => max = Some(n),
                        Err(e) => return Some(vec![format!("§c{e}")]),
                    }
                }
                (args[0].to_owned(), Op::Clear { ids, label, max })
            }
            Addon::Clone => {
                if args.is_empty() {
                    return Some(usage);
                }
                if args.len() < 2 && viewer == CONSOLE {
                    return Some(vec!["§cEither execute this command as a player, or provide a target player.".into()]);
                }
                (args[0].to_owned(), Op::CloneFrom { to: args.get(1).map(|t| (*t).to_owned()) })
            }
        };
        self.pending.push(Pending {
            viewer,
            viewer_name: viewer_name.to_owned(),
            kind,
            query: query(&typed),
            typed,
            step: 0,
            lookup: None,
            op,
        });
        Some(Vec::new())
    }

    /// Whether the sender bypasses exemptions under `setting`.
    fn addon_bypass<H: Host<Stack = S>>(&self, host: &mut H, viewer: u128, kind: Kind, setting: Bypass) -> bool {
        match setting {
            Bypass::Yes => true,
            Bypass::No => false,
            Bypass::Permission => {
                viewer != CONSOLE && host.has_permission(viewer, &node(kind, "bypass-exempt"), self.cfg.access.bypass_exempt)
            }
        }
    }

    /// The checks every addon makes before it touches a target, in the order of the API's creation: offline support,
    /// exemption, then the read. The text is what the original tells the sender.
    #[allow(clippy::too_many_arguments)]
    fn addon_read<H: Host<Stack = S>>(
        &mut self,
        host: &mut H,
        a: Addon,
        kind: Kind,
        uuid: u128,
        name: &str,
        shown: &str,
        offline_ok: bool,
        unknown_ok: bool,
        bypass: bool,
    ) -> Result<Snap<S>, String> {
        let is_online = self.online.iter().any(|o| o.uuid == uuid);
        let what = match kind {
            Kind::Inventory => "inventory",
            Kind::Ender => "enderchest",
        };
        if !is_online && !offline_ok {
            // The originals differ in the plural: Give's ender chest message is singular.
            let plural = match (a, kind) {
                (Addon::Give, Kind::Ender) => "enderchest",
                (_, Kind::Inventory) => "inventories",
                (_, Kind::Ender) => "enderchests",
            };
            return Err(format!("Spectating offline players' {plural} is disabled."));
        }
        let exempt = self.exempt(kind, uuid, name) || host.has_permission(uuid, &node(kind, "exempt"), 5);
        if exempt && !bypass {
            return Err(format!("Player {shown} is exempted from being spectated."));
        }
        match host.inventory(uuid, kind) {
            Ok(inv) => Ok(Snap { slots: inv.slots, online: inv.online }),
            Err(InvError::NeverPlayed) if unknown_ok => Ok(Snap { slots: vec![None; kind.size()], online: false }),
            Err(InvError::NeverPlayed) => Err(format!("Player {shown} has not logged onto the server yet.")),
            Err(InvError::Busy(why)) => {
                eprintln!("[invsee] could not open an inventory now: {why}");
                Err(format!("Could not create {shown}'s {what} for an unknown reason."))
            }
            Err(InvError::Other(why)) => {
                eprintln!("[invsee] could not read an inventory: {why}");
                Err(format!("An internal fault occurred when trying to load {shown}'s {what}."))
            }
        }
    }

    /// Writes the slots that differ (all or none when the game can group writes). An offline target's save is
    /// announced first (`SpectatorInventorySaveEvent`); a listener that cancels it stops the write.
    fn addon_write<H: Host<Stack = S>>(
        &self,
        host: &mut H,
        viewer: u128,
        kind: Kind,
        uuid: u128,
        name: &str,
        snap: &Snap<S>,
        after: &[Option<S>],
    ) -> Result<(), String> {
        if snap.slots.iter().zip(after.iter()).all(|(a, b)| a == b) {
            return Ok(());
        }
        if !snap.online && host.event(&event_line("spectator_inventory_save", viewer, uuid, name, kind)) {
            return Ok(());
        }
        let atomic = host.begin();
        let mut result = Ok(());
        for (i, (old, new)) in snap.slots.iter().zip(after.iter()).enumerate() {
            if old != new {
                if let Err(e) = host.set_slot(uuid, kind, i, old.as_ref(), new.clone()) {
                    result = Err(e);
                    break;
                }
            }
        }
        if atomic {
            host.end(result.is_ok());
        }
        result
    }

    /// A pending addon command found its target (`uuid`, `name`).
    pub(super) fn addon_found<H: Host<Stack = S>>(&mut self, host: &mut H, p: Pending, uuid: u128, name: String) {
        match p.op.clone() {
            Op::View => {}
            Op::Give { item, components, amount } => self.run_give(host, &p, uuid, &name, &item, components.as_deref(), amount),
            Op::Clear { ids, label, max } => self.run_clear(host, &p, uuid, &name, &ids, label.as_deref(), max),
            Op::CloneFrom { to } => {
                let source = (uuid, name, p.typed.clone());
                match to {
                    Some(t) => self.pending.push(Pending {
                        viewer: p.viewer,
                        viewer_name: p.viewer_name.clone(),
                        kind: p.kind,
                        query: match parse_uuid(&t) {
                            Some(u) => Query::Uuid(u),
                            None => Query::Name(t.clone()),
                        },
                        typed: t,
                        step: 0,
                        lookup: None,
                        op: Op::CloneTo { source },
                    }),
                    None => {
                        let target = (p.viewer, p.viewer_name.clone(), p.viewer_name.clone());
                        self.run_clone(host, &p, source, target);
                    }
                }
            }
            Op::CloneTo { source } => {
                let target = (uuid, name, p.typed.clone());
                self.run_clone(host, &p, source, target);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_give<H: Host<Stack = S>>(
        &mut self,
        host: &mut H,
        p: &Pending,
        uuid: u128,
        name: &str,
        item: &str,
        components: Option<&str>,
        amount: u32,
    ) {
        let kind = p.kind;
        let (offline_ok, unknown_ok) = (
            self.cfg.give.offline_player_support.resolve(self.cfg.players.offline),
            self.cfg.give.unknown_player_support.resolve(self.cfg.players.unknown),
        );
        let bypass = self.addon_bypass(host, p.viewer, kind, self.cfg.give.bypass_exempt);
        let snap = match self.addon_read(host, Addon::Give, kind, uuid, name, name, offline_ok, unknown_ok, bypass) {
            Ok(s) => s,
            Err(e) => return self.say(host, p.viewer, &format!("§c{e}")),
        };
        let stack = match host.make_stack(item, components) {
            Ok(s) => s,
            Err(e) => return self.say(host, p.viewer, &format!("§c{e}")),
        };
        let what = match kind {
            Kind::Inventory => "inventory",
            Kind::Ender => "enderchest",
        };
        let mut slots = snap.slots.clone();
        let left = add_item(host, &mut slots, storage_limit(kind), &stack, amount);
        let original = stack_text(item, amount);
        let dir = host.data_dir();
        if left == 0 {
            match self.addon_write(host, p.viewer, kind, uuid, name, &snap, &slots) {
                Ok(()) => self.say(host, p.viewer, &format!("§aAdded {original} to {name}'s {what}!")),
                Err(e) => {
                    host.log(&format!("Could not save {what}: {e}"));
                    self.say(host, p.viewer, &format!("§cCould not save {name}'s {what}."));
                }
            }
            return;
        }
        let remainder = stack_text(item, left);
        if self.cfg.give.queue_remaining_items {
            self.say(
                host,
                p.viewer,
                &format!("§eCould not add the following items to the player's {what}: {remainder}, enqueuing.."),
            );
            let entry = if self.cfg.give.save_partial_inventories {
                Queued { item: item.to_owned(), components: components.map(str::to_owned), count: left }
            } else {
                Queued { item: item.to_owned(), components: components.map(str::to_owned), count: amount }
            };
            if let Some(dir) = &dir {
                let mut q = Queues::load(dir, uuid);
                q.of(kind).push(entry);
                if let Err(e) = q.save(dir, uuid) {
                    host.log(&format!("Could not save player save file: {e}"));
                }
            }
        } else {
            self.say(host, p.viewer, &format!("§cCould not add the following items to the player's {what}: {remainder}"));
        }
        // The part that fitted stays for a player who is on (it is their live inventory); for one who is not, only
        // with `save-partial-inventories`.
        if snap.online || self.cfg.give.save_partial_inventories {
            if let Err(e) = self.addon_write(host, p.viewer, kind, uuid, name, &snap, &slots) {
                host.log(&format!("Could not save {what}: {e}"));
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn run_clear<H: Host<Stack = S>>(
        &mut self,
        host: &mut H,
        p: &Pending,
        uuid: u128,
        name: &str,
        ids: &[String],
        label: Option<&str>,
        max: Option<u32>,
    ) {
        let kind = p.kind;
        let offline_ok = self.cfg.clear.offline_player_support.resolve(self.cfg.players.offline);
        let unknown_ok = self.cfg.players.unknown;
        let bypass = self.addon_bypass(host, p.viewer, kind, self.cfg.clear.bypass_exempt);
        let snap = match self.addon_read(host, Addon::Clear, kind, uuid, name, name, offline_ok, unknown_ok, bypass) {
            Ok(s) => s,
            Err(e) => return self.say(host, p.viewer, &format!("§c{e}")),
        };
        let what = match kind {
            Kind::Inventory => "inventory",
            Kind::Ender => "enderchest",
        };
        let mut slots = snap.slots.clone();
        let message = match label {
            None => {
                slots.iter_mut().for_each(|s| *s = None);
                format!("§aCleared {name}'s {what}.")
            }
            Some(label) => {
                let matches = |h: &H, s: &S| ids.iter().any(|i| *i == h.describe(s).0);
                match max {
                    None => {
                        for s in slots.iter_mut() {
                            if s.as_ref().is_some_and(|st| matches(host, st)) {
                                *s = None;
                            }
                        }
                        format!("§aRemoved all {label} from {name}'s {what}.")
                    }
                    Some(mut at_most) => {
                        let mut removed = 0;
                        for s in slots.iter_mut() {
                            if at_most == 0 {
                                break;
                            }
                            let Some(st) = s.clone() else { continue };
                            if !matches(host, &st) {
                                continue;
                            }
                            let have = host.describe(&st).1;
                            let take = have.min(at_most);
                            *s = if take == have { None } else { host.with_count(&st, have - take) };
                            at_most -= take;
                            removed += take;
                        }
                        format!("§aRemoved {removed} {label} from {name}'s {what}.")
                    }
                }
            }
        };
        if let Err(e) = self.addon_write(host, p.viewer, kind, uuid, name, &snap, &slots) {
            host.log(&format!("Could not save {what}: {e}"));
        }
        self.say(host, p.viewer, &message);
    }

    fn run_clone<H: Host<Stack = S>>(
        &mut self,
        host: &mut H,
        p: &Pending,
        source: (u128, String, String),
        target: (u128, String, String),
    ) {
        let kind = p.kind;
        let (offline_ok, unknown_ok) = (self.cfg.players.offline, self.cfg.players.unknown);
        let bypass = self.addon_bypass(host, p.viewer, kind, Bypass::Permission);
        let what = match kind {
            Kind::Inventory => "inventory",
            Kind::Ender => "enderchest",
        };
        let src = match self.addon_read(host, Addon::Clone, kind, source.0, &source.1, &source.2, offline_ok, unknown_ok, bypass) {
            Ok(s) => s,
            Err(e) => return self.say(host, p.viewer, &format!("§c{e}")),
        };
        let dst = match self.addon_read(host, Addon::Clone, kind, target.0, &target.1, &target.2, offline_ok, unknown_ok, bypass) {
            Ok(s) => s,
            Err(e) => return self.say(host, p.viewer, &format!("§c{e}")),
        };
        if let Err(e) = self.addon_write(host, p.viewer, kind, target.0, &target.1, &dst, &src.slots) {
            host.log(&format!("Error while trying to clone {what}: {e}"));
            return self.say(host, p.viewer, &format!("§cCould not save contents of target {what}."));
        }
        self.say(host, p.viewer, &format!("§aCloned {}'s {what} to {}.", source.2, target.2));
    }

    /// A player joined: their queued items are added at the next tick that can read their inventory.
    pub(super) fn addon_joined(&mut self, uuid: u128, name: &str) {
        self.joined.retain(|(u, _, _)| *u != uuid);
        self.joined.push((uuid, name.to_owned(), self.tick));
    }

    /// `JoinListener`: queued items go into the inventory and the ender chest of a player who is on.
    pub(super) fn addon_queues<H: Host<Stack = S>>(&mut self, host: &mut H) {
        let Some(dir) = host.data_dir() else {
            self.joined.clear();
            return;
        };
        let joined = std::mem::take(&mut self.joined);
        for (uuid, name, since) in joined {
            if !self.online.iter().any(|o| o.uuid == uuid) {
                continue;
            }
            let mut queues = Queues::load(&dir, uuid);
            if queues.inventory.is_empty() && queues.enderchest.is_empty() {
                continue;
            }
            let mut again = false;
            for kind in [Kind::Inventory, Kind::Ender] {
                if queues.of(kind).is_empty() {
                    continue;
                }
                let snap = match host.inventory(uuid, kind) {
                    Ok(i) => Snap { slots: i.slots, online: i.online },
                    // Still logging in: the next tick tries again.
                    Err(InvError::Busy(_)) if self.tick.saturating_sub(since) < 200 => {
                        again = true;
                        continue;
                    }
                    Err(_) => continue,
                };
                let what = match kind {
                    Kind::Inventory => "inventory",
                    Kind::Ender => "enderchest",
                };
                let mut slots = snap.slots.clone();
                let mut queue = std::mem::take(queues.of(kind));
                let mut rest = Vec::new();
                let mut stopped = false;
                for q in queue.drain(..) {
                    if stopped {
                        rest.push(q);
                        continue;
                    }
                    let Ok(stack) = host.make_stack(&q.item, q.components.as_deref()) else { continue };
                    let left = add_item(host, &mut slots, storage_limit(kind), &stack, q.count);
                    if left == 0 {
                        host.log(&format!("[Queue] Added {} to {name}'s {what}!", stack_text(&q.item, q.count)));
                    } else {
                        host.log(&format!("[Queue] Could not add all of {} to {name}'s {what}.", stack_text(&q.item, q.count)));
                        host.log(&format!("[Queue] Remaining: {}.", stack_text(&q.item, left)));
                        rest.push(Queued { count: left, ..q });
                        stopped = true;
                    }
                }
                *queues.of(kind) = rest;
                if let Err(e) = self.addon_write(host, uuid, kind, uuid, &name, &snap, &slots) {
                    host.log(&format!("[Queue] Could not save {name}'s {what}: {e}"));
                    // Nothing was written: keep the whole queue.
                    queues = Queues::load(&dir, uuid);
                    break;
                }
            }
            if let Err(e) = queues.save(&dir, uuid) {
                host.log(&format!("Could not save player save file: {e}"));
            }
            if again {
                self.joined.push((uuid, name, since));
            }
        }
    }

    /// Completions of an addon command's argument: names, then item names, then amounts. None when `word` is not one.
    pub(super) fn addon_complete<H: Host<Stack = S>>(&mut self, host: &mut H, word: &str, args: &[&str]) -> Option<Vec<String>> {
        let (a, _) = addon_word(word)?;
        let current = args.last().copied().unwrap_or("");
        let prefix = current.to_lowercase();
        let index = args.len().saturating_sub(1);
        let mut out: Vec<String> = match (a, index) {
            (_, 0) => {
                let mut names: Vec<String> = self.online.iter().map(|p| p.name.clone()).collect();
                if self.cfg.players.offline && self.cfg.players.tabcomplete_offline {
                    names.extend(self.seen.names().map(str::to_owned));
                }
                names
            }
            (Addon::Clone, 1) => self.online.iter().map(|p| p.name.clone()).collect(),
            (Addon::Give | Addon::Clear, 1) => host
                .item_ids()
                .into_iter()
                .map(|i| i.strip_prefix("minecraft:").map(str::to_owned).unwrap_or(i))
                .collect(),
            (Addon::Give | Addon::Clear, 2) => {
                let top = match_material(&host.item_ids(), args[1])
                    .and_then(|id| host.make_stack(&id, None).ok())
                    .map_or(64, |s| host.max_stack(&s));
                (1..=top).map(|n| n.to_string()).collect()
            }
            _ => Vec::new(),
        };
        out.retain(|n| n.to_lowercase().starts_with(&prefix));
        if index == 0 {
            out.sort_by_key(|n| n.to_lowercase());
            out.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
        }
        Some(out)
    }
}

/// One event line: `<kind>\t<spectator>\t<target>\t<target name>\t<inventory|enderchest>`.
pub fn event_line(kind: &str, spectator: u128, target: u128, name: &str, view: Kind) -> String {
    let view = match view {
        Kind::Inventory => "inventory",
        Kind::Ender => "enderchest",
    };
    format!("{kind}\t{spectator:032x}\t{target:032x}\t{name}\t{view}")
}

/// A chat line without its `§` colour codes (for the log).
fn strip_colours(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c == '§' {
            chars.next();
        } else {
            out.push(c);
        }
    }
    out
}
