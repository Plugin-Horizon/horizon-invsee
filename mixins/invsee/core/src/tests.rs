//! The whole flow against a fake game: commands, opening, clicks, live refresh, closing, logs.
use super::*;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq)]
struct St {
    item: String,
    count: u32,
    /// A placeholder's name.
    name: Option<String>,
}

fn st(item: &str, count: u32) -> St {
    St {
        item: format!("minecraft:{item}"),
        count,
        name: None,
    }
}

#[derive(Default)]
struct Fake {
    players: Vec<Online>,
    /// Inventories by player and kind; a player not in `players` is "in their file".
    invs: HashMap<(u128, Kind), Vec<Option<St>>>,
    /// Players with a file or online.
    known: HashSet<u128>,
    menus: HashMap<u128, (i32, u8, String, Vec<Option<St>>)>,
    next_window: i32,
    told: Vec<(u128, String)>,
    logged: Vec<String>,
    dir: PathBuf,
    /// The game has `native_open_inventory_menu`.
    live: bool,
    live_menus: HashMap<u128, (i32, (u128, Kind), Vec<Option<usize>>)>,
    /// Permission nodes a permissions mod answers, by player.
    perms: HashMap<(u128, String), bool>,
    /// Profiles the server's cache holds.
    profiles: Vec<(u128, String)>,
    refreshed: u32,
}

const ADMIN: u128 = 1;
const BOB: u128 = 2;
const HELPER: u128 = 3;
const CAROL: u128 = 4; // offline, has a file

impl Fake {
    fn new() -> Fake {
        let dir = std::env::temp_dir().join(format!(
            "invsee-flow-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut f = Fake {
            dir,
            next_window: 10,
            ..Fake::default()
        };
        f.join(ADMIN, "Admin", 4);
        f.join(BOB, "Bob", 0);
        f.join(HELPER, "Helper", 1);
        f.known.insert(CAROL);
        f.invs.insert((CAROL, Kind::Inventory), vec![None; 41]);
        f.invs.insert((CAROL, Kind::Ender), vec![None; 27]);
        f
    }
    fn join(&mut self, uuid: u128, name: &str, level: u8) {
        self.players.push(Online {
            uuid,
            name: name.into(),
            level,
        });
        self.known.insert(uuid);
        self.invs.entry((uuid, Kind::Inventory)).or_insert_with(|| vec![None; 41]);
        self.invs.entry((uuid, Kind::Ender)).or_insert_with(|| vec![None; 27]);
    }
    fn set(&mut self, uuid: u128, kind: Kind, slot: usize, s: Option<St>) {
        self.invs.get_mut(&(uuid, kind)).unwrap()[slot] = s;
    }
    fn get(&self, uuid: u128, kind: Kind, slot: usize) -> Option<St> {
        self.invs[&(uuid, kind)][slot].clone()
    }
    fn menu(&self, uuid: u128) -> &(i32, u8, String, Vec<Option<St>>) {
        &self.menus[&uuid]
    }
    fn told(&self, uuid: u128) -> Vec<&str> {
        self.told
            .iter()
            .filter(|(u, _)| *u == uuid)
            .map(|(_, l)| l.as_str())
            .collect()
    }
}

impl Host for Fake {
    type Stack = St;
    fn players(&self) -> Vec<Online> {
        self.players.clone()
    }
    fn inventory(&self, uuid: u128, kind: Kind) -> Result<Inv<St>, InvError> {
        if !self.known.contains(&uuid) {
            return Err(InvError::from_horizon("no saved player"));
        }
        Ok(Inv {
            slots: self.invs[&(uuid, kind)].clone(),
            online: self.players.iter().any(|p| p.uuid == uuid),
        })
    }
    fn move_stack(
        &mut self,
        from: (u128, Kind, usize),
        expected: &St,
        to: (u128, Kind),
    ) -> Result<(), String> {
        if (from.0, from.1) == to {
            return Err("the same inventory".into());
        }
        let mut src = self.invs[&(from.0, from.1)].clone();
        let mut dst = self.invs[&to].clone();
        if src[from.2].as_ref() != Some(expected) {
            return Err("that stack changed".into());
        }
        src[from.2] = None;
        let room = if to.1 == Kind::Inventory { 36 } else { 27 };
        let mut left = expected.count;
        for s in dst[..room].iter_mut().flatten() {
            if s.item == expected.item && s.count < 64 && left > 0 {
                let n = (64 - s.count).min(left);
                s.count += n;
                left -= n;
            }
        }
        for s in dst[..room].iter_mut() {
            if s.is_none() && left > 0 {
                *s = Some(St {
                    count: left.min(64),
                    ..expected.clone()
                });
                left -= left.min(64);
            }
        }
        if left > 0 {
            return Err("no room".into());
        }
        self.invs.insert((from.0, from.1), src);
        self.invs.insert(to, dst);
        Ok(())
    }
    fn open_inventory_menu(
        &mut self,
        viewer: u128,
        _rows: u8,
        _title: &str,
        target: (u128, Kind),
        layout: &[Option<usize>],
    ) -> Option<i32> {
        if !self.live || !self.players.iter().any(|p| p.uuid == viewer) {
            return None;
        }
        self.next_window += 1;
        self.live_menus.insert(viewer, (self.next_window, target, layout.to_vec()));
        Some(self.next_window)
    }
    fn set_slot(
        &mut self,
        uuid: u128,
        kind: Kind,
        slot: usize,
        expected: Option<&St>,
        new: Option<St>,
    ) -> Result<Option<St>, String> {
        let at = self
            .invs
            .get_mut(&(uuid, kind))
            .ok_or("no saved player")?
            .get_mut(slot)
            .ok_or("no such slot")?;
        if at.as_ref() != expected {
            return Err("that stack changed".into());
        }
        Ok(std::mem::replace(at, new))
    }
    fn with_count(&self, s: &St, count: u32) -> Option<St> {
        Some(St { count, ..s.clone() })
    }
    fn profile_by_name(&self, name: &str) -> Option<(u128, String)> {
        self.profiles.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)).cloned()
    }
    fn profile_by_uuid(&self, uuid: u128) -> Option<String> {
        self.profiles.iter().find(|(u, _)| *u == uuid).map(|(_, n)| n.clone())
    }
    fn has_permission(&self, uuid: u128, node: &str, default_level: u8) -> bool {
        self.perms.get(&(uuid, node.to_owned())).copied().unwrap_or_else(|| {
            self.players.iter().find(|o| o.uuid == uuid).map_or(0, |o| o.level) >= default_level
        })
    }
    fn refresh_commands(&mut self) {
        self.refreshed += 1;
    }
    fn open_menu(&mut self, viewer: u128, rows: u8, title: &str, items: Vec<Option<St>>) -> Option<i32> {
        if !self.players.iter().any(|p| p.uuid == viewer) {
            return None;
        }
        self.next_window += 1;
        self.menus
            .insert(viewer, (self.next_window, rows, title.into(), items));
        Some(self.next_window)
    }
    fn set_menu_items(&mut self, viewer: u128, items: Vec<Option<St>>) {
        if let Some(m) = self.menus.get_mut(&viewer) {
            m.3 = items;
        }
    }
    fn close_menu(&mut self, viewer: u128) {
        self.menus.remove(&viewer);
    }
    fn tell(&mut self, uuid: u128, line: &str) {
        self.told.push((uuid, line.into()));
    }
    fn log(&mut self, line: &str) {
        self.logged.push(line.into());
    }
    fn placeholder(&self, item: &str, name: &str) -> St {
        St {
            item: item.into(),
            count: 1,
            name: Some(name.into()),
        }
    }
    fn describe(&self, s: &St) -> (String, u32) {
        (s.item.clone(), s.count)
    }
    fn server_dir(&self) -> Option<PathBuf> {
        Some(self.dir.clone())
    }
    fn data_dir(&self) -> Option<PathBuf> {
        Some(self.dir.join("world").join("invsee"))
    }
}

fn mojang_says_nothing(_: &Query, _: Duration) -> Lookup {
    Lookup::ready(None)
}

fn mojang_knows_dave(q: &Query, _: Duration) -> Lookup {
    Lookup::ready(match q {
        Query::Name(n) if n.eq_ignore_ascii_case("dave") => Some((99, "Dave".into())),
        _ => None,
    })
}

fn setup() -> (InvSee<St>, Fake) {
    let mut m = InvSee::new().with_lookup(mojang_says_nothing);
    let mut f = Fake::new();
    m.tick(&mut f);
    (m, f)
}

/// The inventory menu slot below a six-row view showing inventory slot `inv`.
fn below(inv: usize) -> i32 {
    (54 + if inv < 9 { 27 + inv } else { inv - 9 }) as i32
}

#[test]
fn invsee_opens_a_live_view_of_an_online_player() {
    let (mut m, mut f) = setup();
    f.set(BOB, Kind::Inventory, 0, Some(st("diamond_sword", 1)));
    f.set(BOB, Kind::Inventory, 39, Some(st("iron_helmet", 1)));
    assert_eq!(m.perform(&f, ADMIN, "Admin", "invsee bob"), Some(vec![]));
    assert!(f.menus.is_empty(), "opens on the tick, not inside the command");
    m.tick(&mut f);
    let (_, rows, title, items) = f.menu(ADMIN).clone();
    assert_eq!(rows, 6);
    assert_eq!(title, "Bob's inventory");
    assert_eq!(items[0], Some(st("diamond_sword", 1)));
    assert_eq!(items[39], Some(st("iron_helmet", 1)));
    // Empty boots: a named glass pane; the cursor slot: unavailable.
    assert_eq!(items[36].as_ref().unwrap().name.as_deref(), Some("§bBoots"));
    assert_eq!(items[36].as_ref().unwrap().item, "minecraft:light_blue_stained_glass_pane");
    assert_eq!(items[43].as_ref().unwrap().name.as_deref(), Some("§bCursor"));
    assert_eq!(items[44], None);
    // Bob picks something up: the view follows on the next tick.
    f.set(BOB, Kind::Inventory, 5, Some(st("bread", 3)));
    m.tick(&mut f);
    assert_eq!(f.menu(ADMIN).3[5], Some(st("bread", 3)));
}

#[test]
fn a_click_takes_and_a_click_below_gives_and_the_close_is_logged() {
    let (mut m, mut f) = setup();
    f.set(BOB, Kind::Inventory, 3, Some(st("diamond", 5)));
    f.set(ADMIN, Kind::Inventory, 0, Some(st("stone", 64)));
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    let window = f.menu(ADMIN).0;
    // Left click on Bob's slot 3: the diamonds go to the admin.
    m.clicked(&mut f, ADMIN, window, 3, 0, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 3), None);
    assert_eq!(f.get(ADMIN, Kind::Inventory, 1), Some(st("diamond", 5)));
    assert_eq!(f.menu(ADMIN).3[3], None, "the view shows it at once");
    // Shift click on the admin's hotbar slot 0 below the view: the stone goes to Bob.
    m.clicked(&mut f, ADMIN, window, below(0), 0, 1);
    assert_eq!(f.get(ADMIN, Kind::Inventory, 0), None);
    assert_eq!(f.get(BOB, Kind::Inventory, 0), Some(st("stone", 64)));
    // Nothing made, nothing lost.
    m.closed(&mut f, ADMIN, window);
    assert_eq!(m.open_views(), 0);
    let log = std::fs::read_to_string(f.dir.join("world/invsee/InvSee++ logs/_global.log")).unwrap();
    assert!(log.contains("Taken         : [DIAMOND x 5]"), "{log}");
    assert!(log.contains("Given         : [STONE x 64]"), "{log}");
    assert!(log.contains("Target        : Bob"), "{log}");
    assert!(log.contains("Spectator Name: Admin"), "{log}");
}

#[test]
fn a_number_key_takes_or_gives_the_hotbar_slot() {
    let (mut m, mut f) = setup();
    f.set(ADMIN, Kind::Inventory, 2, Some(st("apple", 7)));
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    let w = f.menu(ADMIN).0;
    // Key 3 over Bob's empty slot 10: the admin's hotbar slot 2 goes to Bob.
    m.clicked(&mut f, ADMIN, w, 10, 2, 2);
    assert_eq!(f.get(ADMIN, Kind::Inventory, 2), None);
    assert_eq!(f.get(BOB, Kind::Inventory, 0), Some(st("apple", 7)));
    // Key 1 over Bob's slot 0: it comes back.
    m.clicked(&mut f, ADMIN, w, 0, 0, 2);
    assert_eq!(f.get(BOB, Kind::Inventory, 0), None);
    // Drag, throw, middle click: nothing.
    f.set(BOB, Kind::Inventory, 1, Some(st("dirt", 1)));
    for mode in [3, 4, 5, 6] {
        m.clicked(&mut f, ADMIN, w, 1, 0, mode);
    }
    assert_eq!(f.get(BOB, Kind::Inventory, 1), Some(st("dirt", 1)));
}

#[test]
fn a_viewer_below_the_edit_level_only_looks() {
    let (mut m, mut f) = setup();
    m.load_config("[access]\ninvsee_view = 1\ninvsee_edit = 3\n", None);
    f.set(BOB, Kind::Inventory, 0, Some(st("diamond", 1)));
    m.perform(&f, HELPER, "Helper", "isee Bob");
    m.tick(&mut f);
    let w = f.menu(HELPER).0;
    m.clicked(&mut f, HELPER, w, 0, 0, 0);
    m.clicked(&mut f, HELPER, w, 0, 0, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 0), Some(st("diamond", 1)));
    assert!(f.told(HELPER).is_empty(), "silent, as InvSee++: {:?}", f.told(HELPER));
    // Below the view level: refused at the command, with Bukkit's permission message.
    let r = m.perform(&f, BOB, "Bob", "invsee Helper").unwrap();
    assert!(r[0].contains("do not have permission"), "{r:?}");
}

#[test]
fn endersee_shows_three_rows_and_edits_the_ender_chest() {
    let (mut m, mut f) = setup();
    f.set(BOB, Kind::Ender, 26, Some(st("elytra", 1)));
    m.perform(&f, ADMIN, "Admin", "esee Bob");
    m.tick(&mut f);
    let (w, rows, title, items) = f.menu(ADMIN).clone();
    assert_eq!((rows, title.as_str()), (3, "Bob's enderchest"));
    assert_eq!(items.len(), 27);
    assert_eq!(items[26], Some(st("elytra", 1)));
    m.clicked(&mut f, ADMIN, w, 26, 0, 0);
    assert_eq!(f.get(BOB, Kind::Ender, 26), None);
    assert_eq!(f.get(ADMIN, Kind::Inventory, 0), Some(st("elytra", 1)));
    // Below a three-row view, the admin's slot 0 is at 27 + 27.
    m.clicked(&mut f, ADMIN, w, 54, 0, 0);
    assert_eq!(f.get(BOB, Kind::Ender, 0), Some(st("elytra", 1)));
}

#[test]
fn offline_players_are_found_in_the_usercache_and_read_from_their_file() {
    let (mut m, mut f) = setup();
    std::fs::write(
        f.dir.join("usercache.json"),
        r#"[{"name":"Carol","uuid":"00000000-0000-0000-0000-000000000004","expiresOn":"2026-12-01 10:00:00 +0000"}]"#,
    )
    .unwrap();
    f.set(CAROL, Kind::Inventory, 8, Some(st("torch", 32)));
    m.perform(&f, ADMIN, "Admin", "invsee carol");
    m.tick(&mut f);
    assert_eq!(f.menu(ADMIN).2, "Carol's inventory");
    assert_eq!(f.menu(ADMIN).3[8], Some(st("torch", 32)));
    // Offline views are read again every 20 ticks, not every tick.
    f.set(CAROL, Kind::Inventory, 7, Some(st("stick", 1)));
    m.tick(&mut f);
    assert_eq!(f.menu(ADMIN).3[7], None);
    for _ in 0..20 {
        m.tick(&mut f);
    }
    assert_eq!(f.menu(ADMIN).3[7], Some(st("stick", 1)));
    // With offline support off, the same command is refused.
    m.load_config("[players]\noffline = false\n", None);
    m.perform(&f, ADMIN, "Admin", "invsee Carol");
    m.tick(&mut f);
    assert!(f.told(ADMIN).iter().any(|l| l.contains("is disabled")), "{:?}", f.told);
}

#[test]
fn a_uuid_works_and_a_player_who_left_keeps_their_name() {
    let (mut m, mut f) = setup();
    m.logged_out(CAROL, "Carol");
    m.perform(&f, ADMIN, "Admin", "invsee 00000000-0000-0000-0000-000000000004");
    m.tick(&mut f);
    assert_eq!(f.menu(ADMIN).2, "Carol's inventory");
    m.perform(&f, ADMIN, "Admin", "invsee CAROL");
    m.tick(&mut f);
    assert_eq!(f.menu(ADMIN).2, "Carol's inventory");
}

#[test]
fn unknown_names_are_asked_of_mojang_and_never_played_players_get_an_empty_view() {
    let mut m = InvSee::<St>::new().with_lookup(mojang_knows_dave);
    let mut f = Fake::new();
    m.tick(&mut f);
    m.perform(&f, ADMIN, "Admin", "invsee nobody_here");
    m.tick(&mut f);
    assert!(f.told(ADMIN).iter().any(|l| l.contains("does not exist")), "{:?}", f.told);
    m.perform(&f, ADMIN, "Admin", "invsee dave");
    m.tick(&mut f);
    let (w, _, title, items) = f.menu(ADMIN).clone();
    assert_eq!(title, "Dave's inventory");
    assert!(items[..36].iter().all(Option::is_none));
    f.set(ADMIN, Kind::Inventory, 0, Some(st("stone", 1)));
    m.clicked(&mut f, ADMIN, w, below(0), 0, 0);
    assert_eq!(f.get(ADMIN, Kind::Inventory, 0), Some(st("stone", 1)), "nothing stored");
    // With unknown players off, the view doesn't open.
    m.load_config("[players]\nunknown = false\n", None);
    m.perform(&f, ADMIN, "Admin", "invsee dave");
    m.tick(&mut f);
    assert!(f.told(ADMIN).iter().any(|l| l.contains("has not logged onto the server yet")), "{:?}", f.told);
}

#[test]
fn offline_uuids_are_the_last_resort_when_listed() {
    let mut m = InvSee::<St>::new().with_lookup(mojang_says_nothing);
    let mut f = Fake::new();
    let ghost = names::offline_uuid("Ghost");
    f.known.insert(ghost);
    f.invs.insert((ghost, Kind::Inventory), vec![Some(st("bone", 2)); 41]);
    m.load_config("[players]\nuuid_resolve = [\"online\", \"mojang\", \"offline\"]\n", None);
    m.tick(&mut f);
    m.perform(&f, ADMIN, "Admin", "invsee Ghost");
    m.tick(&mut f);
    assert_eq!(f.menu(ADMIN).3[0], Some(st("bone", 2)));
}

#[test]
fn exempt_players_need_the_bypass_level() {
    let (mut m, mut f) = setup();
    m.load_config(
        "[access]\ninvsee_view = 1\nbypass_exempt = 4\nexempt_invsee = [\"bob\"]\nexempt_endersee = [\"00000000-0000-0000-0000-000000000002\"]\n",
        None,
    );
    m.perform(&f, HELPER, "Helper", "invsee Bob");
    m.tick(&mut f);
    assert!(!f.menus.contains_key(&HELPER));
    assert!(f.told(HELPER)[0].contains("exempted"));
    m.perform(&f, ADMIN, "Admin", "endersee Bob");
    m.tick(&mut f);
    assert!(f.menus.contains_key(&ADMIN), "the bypass level opens it");
}

#[test]
fn a_new_view_ends_the_old_and_a_leaving_spectator_ends_theirs() {
    let (mut m, mut f) = setup();
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    m.perform(&f, ADMIN, "Admin", "endersee Bob");
    m.tick(&mut f);
    assert_eq!(m.open_views(), 1);
    f.players.retain(|p| p.uuid != ADMIN);
    m.tick(&mut f);
    assert_eq!(m.open_views(), 0);
}

#[test]
fn every_change_logging_writes_each_move_to_the_console() {
    let (mut m, mut f) = setup();
    m.load_config(
        "[logging]\ngranularity = \"LOG_EVERY_CHANGE\"\noutput = [\"CONSOLE\"]\nformat_console = \"<spectator_name> took <taken> gave <given> from <target>\"\n",
        None,
    );
    f.set(BOB, Kind::Inventory, 0, Some(st("coal", 9)));
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    let w = f.menu(ADMIN).0;
    m.clicked(&mut f, ADMIN, w, 0, 0, 0);
    assert_eq!(f.logged, vec!["Admin took [COAL x 9] gave [] from Bob".to_string()]);
    m.closed(&mut f, ADMIN, w);
    assert_eq!(f.logged.len(), 1, "nothing more on close");
}

#[test]
fn a_full_inventory_moves_nothing_and_says_so() {
    let (mut m, mut f) = setup();
    for i in 0..36 {
        f.set(ADMIN, Kind::Inventory, i, Some(st("dirt", 64)));
    }
    f.set(BOB, Kind::Inventory, 0, Some(st("diamond", 1)));
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    let w = f.menu(ADMIN).0;
    m.clicked(&mut f, ADMIN, w, 0, 0, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 0), Some(st("diamond", 1)));
    assert!(f.told(ADMIN).iter().any(|l| l.contains("no room")));
}

#[test]
fn commands_usages_console_and_completion() {
    let (mut m, mut f) = setup();
    assert_eq!(m.perform(&f, ADMIN, "Admin", "gamemode creative"), None);
    let r = m.perform(&f, ADMIN, "Admin", "invsee").unwrap();
    assert_eq!(r, vec!["/invsee <username>|<uuid>".to_string()]);
    // Only the first argument counts; the rest is ignored, as InvSee++ does.
    assert_eq!(m.perform(&f, ADMIN, "Admin", "invsee a b"), Some(vec![]));
    let r = m.perform(&f, ADMIN, "Admin", "invsee no-such!name").unwrap();
    assert!(r[0].contains("does not exist"), "{r:?}");
    assert_eq!(m.console("invsee"), Some(vec!["/invsee <username>|<uuid>".into()]));
    assert_eq!(
        m.console("invsee Bob"),
        Some(vec!["§cThis command can only be used by players!".into()])
    );
    let usages = m.usages();
    assert!(usages.contains(&("invsee [target:word?]".into(), 1)));
    assert!(usages.contains(&("esee [target:word?]".into(), 1)));
    assert!(usages.contains(&("isppreload".into(), 1)));
    m.logged_out(CAROL, "Carol");
    m.tick(&mut f);
    assert_eq!(
        m.tab_complete(&f, ADMIN, "/invsee "),
        Some((8, vec!["Admin".into(), "Bob".into(), "Carol".into(), "Helper".into()]))
    );
    assert_eq!(m.tab_complete(&f, ADMIN, "esee c"), Some((5, vec!["Carol".into()])));
    assert_eq!(m.tab_complete(&f, BOB, "invsee "), None, "below the view level");
    assert_eq!(m.tab_complete(&f, ADMIN, "tp "), None);
}

#[test]
fn reload_reads_the_file_again() {
    let (mut m, f) = setup();
    let path = f.dir.join("invsee-server.toml");
    std::fs::write(&path, config::DEFAULT).unwrap();
    m.load_config(config::DEFAULT, Some(path.clone()));
    std::fs::write(&path, "[titles]\ninventory = \"Inv of <player>\"\n").unwrap();
    let r = m.perform(&f, ADMIN, "Admin", "invseeplusplusreload").unwrap();
    assert_eq!(r[0], "§aInvSee++ configuration was reloaded.", "{r:?}");
    assert_eq!(m.config().titles.inventory, "Inv of <player>");
    let r = m.perform(&f, BOB, "Bob", "isppreload").unwrap();
    assert!(r[0].contains("do not have permission"));
    std::fs::write(&path, "[titles]\nnope = 1\n").unwrap();
    let r = m.console("isppreload").unwrap();
    assert!(r.iter().any(|l| l.contains("not valid")), "{r:?}");
    assert_eq!(m.config().titles.inventory, "Inv of <player>", "the old one stays");
}

#[test]
fn horizon_errors_are_sorted() {
    assert_eq!(InvError::from_horizon("no saved player"), InvError::NeverPlayed);
    assert_eq!(
        InvError::from_horizon("the player is dead"),
        InvError::Busy("dead".into())
    );
    assert_eq!(
        InvError::from_horizon("no world"),
        InvError::Other("no world".into())
    );
}

#[test]
fn clicks_map_as_horizon_counts_them() {
    let (l, _) = Layout::parse(template::DEFAULT_INVENTORY, Kind::Inventory);
    assert_eq!(decide(&l, 0, 0, 0), Action::Take(0));
    assert_eq!(decide(&l, 40, 0, 1), Action::Take(template::OFF_HAND));
    assert_eq!(decide(&l, 43, 0, 0), Action::Nothing, "the cursor placeholder");
    assert_eq!(decide(&l, 54, 0, 0), Action::Give(9));
    assert_eq!(decide(&l, 81, 0, 0), Action::Give(0));
    assert_eq!(decide(&l, 89, 0, 0), Action::Give(8));
    assert_eq!(decide(&l, 90, 0, 0), Action::Nothing);
    assert_eq!(decide(&l, -999, 0, 0), Action::Nothing);
    assert_eq!(decide(&l, 5, 40, 2), Action::Take(5), "the off-hand key takes");
    assert_eq!(inventory_slot_below(27, 27), Some(9));
}

#[test]
fn an_editor_gets_the_games_live_inventory_menu_and_a_viewer_the_copy() {
    let (mut m, mut f) = setup();
    m.load_config("[access]\ninvsee_view = 1\ninvsee_edit = 3\n", None);
    f.live = true;
    f.set(BOB, Kind::Inventory, 0, Some(st("diamond_sword", 1)));
    assert_eq!(m.perform(&f, ADMIN, "Admin", "invsee bob"), Some(vec![]));
    m.tick(&mut f);
    let (window, target, layout) = f.live_menus.get(&ADMIN).cloned().expect("live menu");
    assert_eq!(target, (BOB, Kind::Inventory));
    assert_eq!(layout[0], Some(0));
    assert_eq!(layout[39], Some(39));
    assert_eq!(layout[43], None, "the cursor cell is not a target slot");
    assert!(!f.menus.contains_key(&ADMIN), "no copy menu beside the live one");
    // Clicks on the live menu are the game's: the mod moves nothing.
    m.clicked(&mut f, ADMIN, window, 0, 0, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 0), Some(st("diamond_sword", 1)));
    // A viewer below the edit level still gets the mod's read-only copy.
    m.perform(&f, HELPER, "Helper", "invsee bob");
    m.tick(&mut f);
    assert!(!f.live_menus.contains_key(&HELPER));
    assert!(f.menus.contains_key(&HELPER));
}

#[test]
fn a_right_click_takes_the_larger_half() {
    let (mut m, mut f) = setup();
    f.set(BOB, Kind::Inventory, 3, Some(st("diamond", 5)));
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    let window = f.menu(ADMIN).0;
    m.clicked(&mut f, ADMIN, window, 3, 1, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 3), Some(st("diamond", 2)));
    assert_eq!(f.get(ADMIN, Kind::Inventory, 0), Some(st("diamond", 3)));
}

#[test]
fn a_right_click_with_no_free_slot_changes_nothing() {
    let (mut m, mut f) = setup();
    f.set(BOB, Kind::Inventory, 3, Some(st("diamond", 4)));
    for i in 0..36 {
        f.set(ADMIN, Kind::Inventory, i, Some(st("dirt", 1)));
    }
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    let window = f.menu(ADMIN).0;
    m.clicked(&mut f, ADMIN, window, 3, 1, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 3), Some(st("diamond", 4)));
    assert!(f.told(ADMIN).iter().any(|l| l.contains("no room")));
}

#[test]
fn armour_goes_to_its_slot_and_a_worn_piece_swaps() {
    let (mut m, mut f) = setup();
    f.set(ADMIN, Kind::Inventory, 0, Some(st("iron_helmet", 1)));
    f.set(ADMIN, Kind::Inventory, 1, Some(st("shield", 1)));
    f.set(BOB, Kind::Inventory, 39, Some(st("leather_helmet", 1)));
    m.perform(&f, ADMIN, "Admin", "invsee Bob");
    m.tick(&mut f);
    let window = f.menu(ADMIN).0;
    m.clicked(&mut f, ADMIN, window, below(0), 0, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 39), Some(st("iron_helmet", 1)));
    assert_eq!(f.get(ADMIN, Kind::Inventory, 0), Some(st("leather_helmet", 1)), "swapped");
    m.clicked(&mut f, ADMIN, window, below(1), 0, 0);
    assert_eq!(f.get(BOB, Kind::Inventory, 40), Some(st("shield", 1)));
    assert_eq!(f.get(ADMIN, Kind::Inventory, 1), None);
}

#[test]
fn permission_nodes_decide_before_the_operator_level() {
    let (mut m, mut f) = setup();
    // Bob is no operator but a permissions mod gives him the node; Admin is refused his.
    f.perms.insert((BOB, "invseeplusplus.invsee.view".into()), true);
    f.perms.insert((ADMIN, "invseeplusplus.endersee.view".into()), false);
    assert_eq!(m.perform(&f, BOB, "Bob", "invsee Admin"), Some(vec![]));
    let r = m.perform(&f, ADMIN, "Admin", "endersee Bob").unwrap();
    assert!(r[0].contains("do not have permission"), "{r:?}");
    f.perms.insert((BOB, "invseeplusplus.tabcomplete".into()), false);
    assert_eq!(m.tab_complete(&f, BOB, "invsee "), None);
}

#[test]
fn names_come_from_the_servers_profile_cache() {
    let (mut m, mut f) = setup();
    f.profiles.push((77, "Erin".into()));
    f.known.insert(77);
    f.invs.insert((77, Kind::Inventory), vec![None; 41]);
    m.perform(&f, ADMIN, "Admin", "invsee erin");
    m.tick(&mut f);
    assert_eq!(f.menu(ADMIN).2, "Erin's inventory");
}

#[test]
fn a_reload_that_changes_the_levels_resends_the_command_trees() {
    let (mut m, mut f) = setup();
    m.load_config(config::DEFAULT, None);
    m.tick(&mut f);
    assert_eq!(f.refreshed, 0, "the first read happens before the trees exist");
    m.load_config("[access]\ninvsee_view = 2\n", None);
    m.tick(&mut f);
    assert_eq!(f.refreshed, 1);
    m.tick(&mut f);
    assert_eq!(f.refreshed, 1, "once");
}
