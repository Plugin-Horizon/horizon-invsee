// invsee for Horizon: the game side. A support module in `game.rs`, so it can name the game's
// types and call the helpers of `native_menus.rs` (API 10) and `native_inventories.rs` (API 132);
// the hooks' helpers only call into it. All of it runs on the server thread.
use super::Game;
use super::native_inventories::NativeInventoryKind;
use horizon_client::items::ItemStack;
use horizon_client::text::Text;
use horizon_invsee as inv;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The mod's state, one per process (a dedicated server runs one world).
static STATE: Mutex<Option<inv::InvSee<ItemStack>>> = Mutex::new(None);

/// Menu events that arrived while the state was in use further up the same thread (a helper
/// of ours led the game into one of our hooks): handled at the next tick.
static DEFERRED: Mutex<Vec<Deferred>> = Mutex::new(Vec::new());

enum Deferred {
    Clicked(u128, i32, i32, i32, i32),
    Closed(u128, i32),
}

/// Runs `f` on the state; None when the state is already in use on this thread (a hook
/// re-entered from inside one of ours: the std mutex is not re-entrant).
fn with<R>(f: impl FnOnce(&mut inv::InvSee<ItemStack>) -> R) -> Option<R> {
    let mut guard = match STATE.try_lock() {
        Ok(g) => g,
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return None,
    };
    Some(f(guard.get_or_insert_with(inv::InvSee::new)))
}

fn log(line: &str) {
    // No helper writes into the server's log yet (GAPS.md): stderr reaches the console.
    for l in line.lines() {
        eprintln!("[invsee] {l}");
    }
}

fn kind(k: inv::Kind) -> NativeInventoryKind {
    match k {
        inv::Kind::Inventory => NativeInventoryKind::Player,
        inv::Kind::Ender => NativeInventoryKind::Ender,
    }
}

/// The game, as the mod's crate sees it.
struct GameHost<'a, 'g> {
    game: &'a mut Game<'g>,
}

impl inv::Host for GameHost<'_, '_> {
    type Stack = ItemStack;

    fn players(&self) -> Vec<inv::Online> {
        self.game
            .native_server_players()
            .into_iter()
            .map(|p| inv::Online {
                uuid: p.uuid,
                name: p.name,
                level: p.permission,
            })
            .collect()
    }

    fn inventory(&self, uuid: u128, k: inv::Kind) -> Result<inv::Inv<ItemStack>, inv::InvError> {
        self.game
            .native_inventory(uuid, kind(k))
            .map(|i| inv::Inv {
                slots: i.slots,
                online: i.online,
            })
            .map_err(|e| inv::InvError::from_horizon(&e))
    }

    fn move_stack(
        &mut self,
        from: (u128, inv::Kind, usize),
        expected: &ItemStack,
        to: (u128, inv::Kind),
    ) -> Result<(), String> {
        self.game
            .native_move_stack((from.0, kind(from.1), from.2), expected, (to.0, kind(to.1)))
    }

    fn open_menu(
        &mut self,
        viewer: u128,
        rows: u8,
        title: &str,
        items: Vec<Option<ItemStack>>,
    ) -> Option<i32> {
        self.game
            .native_open_menu(viewer, rows, Text::legacy(title), items)
    }

    fn open_inventory_menu(
        &mut self,
        viewer: u128,
        rows: u8,
        title: &str,
        target: (u128, inv::Kind),
        layout: &[Option<usize>],
    ) -> Option<i32> {
        // hooks-b-all `native_open_inventory_menu`: the game's own click engine over the
        // target's slots, live or on their file.
        self.game
            .native_open_inventory_menu(viewer, rows, Text::legacy(title), (target.0, kind(target.1)), layout)
    }

    fn set_slot(
        &mut self,
        uuid: u128,
        k: inv::Kind,
        slot: usize,
        expected: Option<&ItemStack>,
        new: Option<ItemStack>,
    ) -> Result<Option<ItemStack>, String> {
        // Armour, the off hand, an exact slot: checked against what the spectator saw.
        self.game.native_set_inventory_slot(uuid, kind(k), slot, expected, new)
    }

    fn with_count(&self, stack: &ItemStack, count: u32) -> Option<ItemStack> {
        Some(ItemStack {
            count: count.try_into().ok()?,
            ..stack.clone()
        })
    }

    fn profile_by_name(&self, name: &str) -> Option<(u128, String)> {
        self.game.native_profile_by_name(name)
    }

    fn profile_by_uuid(&self, uuid: u128) -> Option<String> {
        self.game.native_profile_by_uuid(uuid)
    }

    fn has_permission(&self, uuid: u128, node: &str, default_level: u8) -> bool {
        self.game.native_has_permission(uuid, node, default_level)
    }

    fn refresh_commands(&mut self) {
        self.game.native_refresh_commands();
    }

    fn set_menu_items(&mut self, viewer: u128, items: Vec<Option<ItemStack>>) {
        self.game.native_set_menu_items(viewer, items);
    }

    fn close_menu(&mut self, viewer: u128) {
        self.game.native_close_menu(viewer);
    }

    fn tell(&mut self, uuid: u128, line: &str) {
        self.game.native_tell(uuid, Text::legacy(line));
    }

    fn log(&mut self, line: &str) {
        log(line);
    }

    fn placeholder(&self, item: &str, name: &str) -> ItemStack {
        // The name carries its own colour code (§b), as InvSee++ names its placeholders.
        let title = Text::legacy(name);
        ItemStack {
            item: item.into(),
            count: 1,
            damage: 0,
            extra: Some(Box::new(horizon_client::item_data::StackData {
                custom_name: Some(horizon_client::lan::data::component_nbt(&title.component())),
                ..Default::default()
            })),
        }
    }

    fn describe(&self, stack: &ItemStack) -> (String, u32) {
        (stack.item.to_string(), stack.count)
    }

    fn server_dir(&self) -> Option<PathBuf> {
        // usercache.json lies in the folder that holds the world.
        self.game
            .native_world_dir()
            .and_then(|w| w.parent().map(Path::to_path_buf))
            .or_else(|| std::env::current_dir().ok())
    }

    fn data_dir(&self) -> Option<PathBuf> {
        self.game.native_world_dir().map(|w| w.join("invsee"))
    }
}

/// `config/invsee-server.toml` was read (`native_config_loading`/`_reloading`).
pub(crate) fn config(text: &str, path: &Path) {
    let lines = with(|m| m.load_config(text, Some(path.to_path_buf()))).unwrap_or_default();
    for l in lines {
        log(&l);
    }
}

/// The commands, with the levels of the config read before the world opened.
pub(crate) fn usages() -> Vec<(String, u8)> {
    with(|m| m.usages()).unwrap_or_default()
}

/// A player's command; true when it was ours.
pub(super) fn perform(game: &mut Game<'_>, uuid: u128, name: &str, line: &str) -> bool {
    let host = GameHost { game };
    let answer = with(|m| m.perform(&host, uuid, name, line));
    let Some(Some(lines)) = answer else {
        return false;
    };
    for l in lines {
        game.native_feedback(Text::legacy(&l));
    }
    true
}

/// A console command; true when it was ours.
pub(super) fn console(game: &mut Game<'_>, line: &str) -> bool {
    let Some(Some(lines)) = with(|m| m.console(line)) else {
        return false;
    };
    for l in lines {
        game.native_feedback(Text::legacy(&l));
    }
    true
}

/// The end of a server tick.
pub(super) fn tick(game: &mut Game<'_>) {
    let deferred = std::mem::take(&mut *DEFERRED.lock().unwrap_or_else(|e| e.into_inner()));
    let mut host = GameHost { game };
    with(|m| {
        for d in deferred {
            match d {
                Deferred::Clicked(u, w, s, b, mode) => m.clicked(&mut host, u, w, s, b, mode),
                Deferred::Closed(u, w) => m.closed(&mut host, u, w),
            }
        }
        m.tick(&mut host);
    });
}

pub(super) fn clicked(game: &mut Game<'_>, uuid: u128, window: i32, slot: i32, button: i32, mode: i32) {
    let mut host = GameHost { game };
    if with(|m| m.clicked(&mut host, uuid, window, slot, button, mode)).is_none() {
        DEFERRED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Deferred::Clicked(uuid, window, slot, button, mode));
    }
}

pub(super) fn closed(game: &mut Game<'_>, uuid: u128, window: i32) {
    let mut host = GameHost { game };
    if with(|m| m.closed(&mut host, uuid, window)).is_none() {
        DEFERRED
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .push(Deferred::Closed(uuid, window));
    }
}

pub(crate) fn logged_in(uuid: u128, name: &str) {
    with(|m| m.logged_in(uuid, name));
}

pub(crate) fn logged_out(uuid: u128, name: &str) {
    with(|m| m.logged_out(uuid, name));
}

/// Completes the name argument of /invsee and /endersee.
pub(crate) fn tab_complete(
    game: &mut Game<'_>,
    player: u128,
    command: &str,
    suggestions: &mut Vec<horizon_client::mod_api::Suggestion>,
) {
    let host = GameHost { game };
    let Some(Some((start, names))) = with(|m| m.tab_complete(&host, player, command)) else {
        return;
    };
    let end = command.len();
    for text in names {
        if !suggestions.iter().any(|s| s.text.eq_ignore_ascii_case(&text)) {
            suggestions.push(horizon_client::mod_api::Suggestion {
                range: (start, end),
                text,
                tooltip: None,
                int_value: None,
            });
        }
    }
}

/// The server stops: open views are logged and closed.
pub(super) fn stopping(game: &mut Game<'_>) {
    let mut host = GameHost { game };
    with(|m| m.stopping(&mut host));
}
