# What Horizon lacks for a full InvSee++ rebuild

Base: Horizon `a589a3dd9` (API 411). Each line: the helper or hook wanted, the feature it serves, and what this
mod does without it. Most important first.

- [x] `Game::native_set_inventory_slot(uuid: u128, kind: NativeInventoryKind, slot: usize, expected: Option<&ItemStack>, new: Option<ItemStack>) -> Result<Option<ItemStack>, String>`
  (checked like `native_move_stack`: `expected` must still be in the slot, live or the player's file, returns what was
  there) — needed for **putting an item into a chosen slot** (armour, off hand, an exact hotbar slot), **splitting a
  stack** (right click takes half) and **swapping** two stacks — today: a click moves a whole stack, and what goes to the
  target lands where `native_move_stack` fits it (main 36 slots first-fit), never on armour or the off hand.
  (done 2026-11-08: `Host::set_slot` over `native_set_inventory_slot` places armour and the off hand by item id, swaps a worn piece and splits a stack on a right click. `native_take_stacks` has no use here. Not verified in a build.)
- [x] (hooks-b-all `native_open_inventory_menu`, used for viewers at the edit level) A server menu that is a **live container over another player's inventory**:
  `Game::native_open_inventory_menu(viewer: u128, rows: u8, title: Text, target: (u128, NativeInventoryKind), layout: &[Option<usize>]) -> Option<i32>`,
  whose slots ARE the target's slots (vanilla click engine, cursor, drag, shift-click, number keys, double click),
  file-backed when the target is offline (written on each change or on close) — needed for **drag-and-drop editing as
  in InvSee++** — today: API 10 menus move nothing; the mod turns clicks into whole-stack transfers (pickup, shift
  click, number key); middle click, Q, drag and double click do nothing.
- [ ] `NativeInventoryKind::{Cursor, Crafting}` in `native_inventory` (the target's carried stack and its 2x2 crafting
  grid + result) — needed for the template slots `c` and `p_00..p_08` — today: shown as "unavailable" placeholders.
  (Body `b` and saddle `s` are mob slots; a player has none, so they stay placeholders by design.)
- [ ] `native_inventory` errors as an enum (`NativeInventoryError::{NeverPlayed, LoggingIn, Dead, Acting, NoWorld, NoServer}`)
  instead of `String` — needed to tell "never played" from transient states — today: `InvError::from_horizon` matches
  the exact English strings of `native_inventories.rs` ("no saved player", "the player is logging in", ...).
- [ ] A writable empty player file for a UUID that never played:
  `Game::native_create_player_file(uuid: u128, name: &str) -> Result<(), String>` (or `native_inventory` answering
  `online: false` with empty slots and `native_move_stack` creating the file) — needed for
  `enable-unknown-player-support` **editing** (InvSee++ lets you prepare the inventory of someone who never joined) —
  today: an empty read-only view with a message.
- [x] `Game::native_profile_by_name(name: &str) -> Option<(u128, String)>` and
  `Game::native_profile_by_uuid(uuid: u128) -> Option<String>` over `dedicated.rs`'s `USER_CACHE` (and the profile
  service when online-mode is on) — needed for **offline name lookup** — today: the mod parses `usercache.json`
  itself (re-read when its mtime changes) and asks Mojang's API on a thread of its own (ureq).
  (done 2026-11-08: `Host::profile_by_name/uuid` over `native_profile_by_name/uuid`; the mod's own `usercache.json` read stays only for tab completion of offline names, and as a fallback.)
- [x] An argument kind that asks the server for completions (done 2026-11-08: the usages use `[target:word?]`, which
  Horizon sends with `minecraft:ask_server`, so the `tab_complete` hook is asked; `player_name` was not used because
  it offers online names only, and the hook also offers offline names). Not verified in a build yet.
- [x] `Game::native_has_permission(uuid: u128, node: &str) -> Option<bool>` (answered by a permissions mod such as a
  LuckPerms port; None = no permissions mod) — needed for InvSee++'s **permission nodes** (`invseeplusplus.invsee.view`,
  `.edit`, `.exempt.*`, `.bypass-exempt.*`, `.tabcomplete`, `.reload`) — today: operator levels per action in the
  config, and exempt players as a config list of names/UUIDs.
  (done 2026-11-08: `Host::has_permission` over `native_has_permission(uuid, node, default_level)` with the nodes `invseeplusplus.<invsee|endersee>.<view|edit|bypass-exempt>`, `invseeplusplus.reload`, `invseeplusplus.tabcomplete`, falling back to the configured level. The node names are a guess from the original's plugin.yml. The `exempt.*` nodes are not used: they need the permissions of a player who may be offline, so the config lists stay.)
- [ ] `Game::native_log(level: &str, line: &str)` into the dedicated server's log (`dedicated::log`) — needed for the
  `SERVER_LOG_FILE` and `CONSOLE` log outputs — today: `eprintln!`, which reaches the console but maybe not
  `logs/latest.log`.
- [x] `Game::native_refresh_commands()` (re-sends every player's command tree after the mod's usages changed) — needed
  for **reloaded command levels** — today: levels apply to running at once, to seeing (the tree) only after a restart.
  (done 2026-11-08: `Host::refresh_commands()` after a config read that changes the usages. Not verified in a build.)
- [ ] Command words with `+` in `NativeCommand` usages — needed for the aliases `invsee++reload` and `is++reload` —
  today: left out (only `invseeplusplusreload` and `isppreload`); vanilla's literal nodes would accept them.
- [ ] `native_inventory_changed(uuid: u128, kind: NativeInventoryKind)` hook (any change to a live player's
  inventory or ender chest) — needed to **follow a view without polling** — today: each open view of an online
  player reads `native_inventory` every tick (41 clones), an offline one every `offline_refresh_ticks`.
- [ ] A typed or bundle-level way to name a mod's Server config's world copy (`serverconfig/invsee-server.toml`) in
  messages — cosmetic; today the reload command reads the path the last load reported.
