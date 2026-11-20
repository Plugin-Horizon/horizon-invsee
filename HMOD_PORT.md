# hmod port: invsee
Parity: before 55% -> after 73% (33/45 items; method: commands, nodes, config keys, messages and behaviours of the original counted, items needing a missing engine API count as not done)
Round 1
- Fixed:
  - `src/lib.rs` `set_slot`: `set_inventory_slot` / `set_ender_chest_slot` return `Result<(), _>`; match arms updated (`Ok(())`, `Err`).
  - `src/lib.rs` `open_menu`: passes `MenuReplace::CloseFirst` (the fifth argument).
  - `src/lib.rs` `profile_by_name` / `profile_by_uuid`: `host.players().player_by_name` and `host.player(uuid)`.
  - `src/lib.rs` `has_permission`: typed Boolean query `permission_bool` via `bool_node`; `GameHost` method is now `&mut self`.
  - `src/lib.rs` `placeholder`: `set_display` call removed (see gap); `name` is unused.
  - `src/lib.rs` `register_commands` and `player_command`: `with_node(&bool_node(..))`, typed `permission_bool` checks for the view/reload nodes; level fallbacks removed.
  - `src/horizon_invsee/mod.rs` (core, the one signature change): trait `Host::has_permission` takes `&mut self`; `perform`, `tab_complete`, `resolve` take `host: &mut H`.
  - `src/horizon_invsee/tests.rs`: `Fake::has_permission` takes `&mut self`; call sites use `&mut f`; one `let (mut m, mut f)`.
  - `src/lib.rs`: added `permission_gather_nodes` (`PERMISSION_NODES`).
- Permission nodes (default level, declared in `permission_gather_nodes`):
  - `invseeplusplus.invsee.view` -> 1
  - `invseeplusplus.invsee.edit` -> 1
  - `invseeplusplus.endersee.view` -> 1
  - `invseeplusplus.endersee.edit` -> 1
  - `invseeplusplus.reload` -> 1
  - `invseeplusplus.tabcomplete` -> 1
  - `invseeplusplus.invsee.bypass-exempt` / `endersee.bypass-exempt`: not declared (see gaps).
- Gaps:
  - HMOD-GAP(item_custom_name): no custom-name API on an item stack in Horizon main; the menu placeholder items are plain items without their name.
  - HMOD-GAP(bypass_exempt): default is level 5 (nobody), which a Boolean node cannot express (`PermissionLevel::from_level` caps at 4). The nodes are not declared, so they answer as a denial, which gives the same default; a permission handler cannot grant them.
- Unsure:
  - Node defaults are the shipped `Access` defaults (level 1). The config's `access` levels are not read at gathering, so a changed level in `invsee-server.toml` no longer moves the node check; the core's `default_level` argument is ignored by the mod's `has_permission`.
  - `invseeplusplus.tabcomplete` is one node for both kinds, its default is the shipped invsee view level (1).
  - `MenuReplace::CloseFirst` chosen for `open_menu` (the vanilla default; the old API had no choice).
  - The core signature change (`has_permission(&mut self)`, `host: &mut H`) is a deliberate edit inside the core module, needed because the typed query needs a mutable host.

Round 2 (parity fixes)
- Fixed: bypass-exempt node names are `invseeplusplus.bypass-exempt.invsee|endersee` (were reversed); added
  `invseeplusplus.exempt.invsee|endersee` (target holds it -> exempt, online targets); all four declared at level 4.
- Fixed: node defaults for view/edit/reload/tabcomplete now follow the `[access]` levels read from the config at gather time.
- Fixed: any typed word is looked up as a name (Bedrock names with a dot); `valid_name` only guards the Mojang request;
  usercache entries accept dotted names.
- Fixed: a command that arrives while the state is in use is deferred to the next tick instead of being dropped.
- Fixed: a failed rollback in `move_stack` is reported to the spectator.
- Closed: HMOD-GAP(item_custom_name): placeholders get `minecraft:custom_name` through `ItemStack::apply_components`.
- Gaps still open: offline inventories (no API; tab completion still offers offline names), live shared menu, log to stderr,
  `bypass_exempt` default (level 4, not nobody), proxy/permission uuid strategies, personal-slot placeholders per container.
- Dropped finding: undashed 32-hex UUIDs are still accepted (harmless superset; the config exempt lists rely on it).
- Unsure: `Tag::Byte(0)` for `italic` and a compound text component for `custom_name` are written blind.

Round 3 (100 % round)
- Live menu: with a readable cursor (`menu(..).cursor()`), clicks run vanilla's click rules on a board (`click.rs`: pick up, put down, merge, swap, shift click, number keys, double click, drag); every changed slot of the target and the spectator and the cursor are written in one transaction (`begin`/`end`), all or none; the view is sent whole afterwards. Without a cursor the old whole-stack clicks stay.
- Placeholders: unused cells show the inaccessible placeholder; the cursor cell shows the target's cursor; personal slots follow the target's open menu (`player_container_open/close` hooks) with InvSee++'s per-container names and colours and its lore line; icon palette hides armour attributes, off-hand icon is a banner shield; the inventory screen's crafting grid is shown with its real items.
- Resolve strategies: `paper_offline_player_cache`, `electroid`, `spoof` (alias of `offline`), original spellings accepted; `permission_plugin`, `proxy`, `player_data` are accepted and answer nothing (no API).
- Tab completion needs only `invseeplusplus.tabcomplete`; the aggregate nodes `invseeplusplus.view|edit|exempt|bypass-exempt` are declared and grant their children (a typed node has no children, so the mod checks them).
- Removed the invented "never played" message on open.
- Gaps: offline inventories (read needs `player_inventory_slots` for saved players); editing the target's cursor/crafting grid and other menus' inputs; permission/proxy/save-file lookups; the exempt/bypass "nobody" default.
- Unsure (blind): lore style, `tooltip_display`/`banner_patterns` component shapes, `TransactionSeal` import path, `transaction_open` through `dyn ServerHost`, an edit permission denied while the view node is granted by default of its parent.
