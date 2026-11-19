# hmod port: invsee
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
