# hmod port: invsee
Parity: 82% now / 82% with our PRs (37/45 items; no open PR is counted yet, the open items are API-blocked, see Round 4)
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

Round 4 (100 % round, 20.11.2026, blind)
- Typed UUIDs: `names::parse_uuid` reads the word as Java's `UUID.fromString` does (five dash-separated hex groups,
  short ones masked as the JDK masks them, 36 characters at most). An undashed 32-hex word is a name, as in InvSee++
  (the exempt lists, the usercache and Mojang's answers keep the lenient reader `parse_uuid_lenient`).
- Tab completion lists every match: the 100-name cap is gone.
- Read failures use InvSee++'s wording: `An internal fault occurred when trying to load X's inventory.` (any unreadable
  inventory), `Could not open X's inventory for an unknown reason.` (busy), enderchest in the same way. The reason goes
  to stderr (InvSee++ logged a SEVERE line there).
- Resolver defaults are InvSee++'s shipped order (its commented-out defaults in `config.yml`): UUID `online`,
  `logged out`, `paper offline player cache`, `permission plugin`, `proxy`, `mojang`, `player data`; name `online`,
  `logged out`, `permission plugin`, `mojang`, `player data`. `player data` answers from `usercache.json` (the last known
  names of players who were on) until the game exposes save files. Existing configs keep their own lists.
- Gaps still open (all API-blocked, see GAPS_UPSTREAM.md): offline inventories (read and write), the target's workstation
  inputs and cursor/crafting edits, live shared menu over the target, permission/proxy/save-file lookups (save files only
  through usercache), server log lines (stderr only), `invseeplusplus.*` wildcard (no typed name), default "nobody" of the
  exempt and bypass-exempt nodes (level 4 stays).
- Checked, no change: body and saddle. The glass palette shows the gray `Body` and orange `Saddle` panes as InvSee++'s
  `GLASS_BODY`/`GLASS_SADDLE` do (the audit's "null palette" claim is for the icons palette only, which matches `ICON_BODY`/`ICON_SADDLE`).
- Not applicable: the PWI/Multiverse command arguments and the addon plugins (`/invgive`, `/invclear`, `/invclone`).
- Unsure (blind): `Busy` now answers "Could not open ... for an unknown reason" instead of retrying on the next tick.

Checklist round 100 (x done, ~ partly / API-blocked, - not applicable, [ ] open)
Commands
- [x] /invsee: usage, aliases inventorysee, isee; level; only the first argument counts; console answer
- [x] /endersee: usage, aliases enderchestsee, esee
- [x] /invseeplusplusreload: aliases isppreload, invsee++reload, is++reload (player and console)
- [x] Usage line without a target (player and console); "This command can only be used by players!" with a target
- [x] Tab completion of names: first argument, sorted, no cap, needs `invseeplusplus.tabcomplete` only
- [~] Offline names in tab completion are offered, opening them is refused (offline reads are API-blocked)
- [x] Async tab completer: answered synchronously with the same names
- [-] PWI/Multiverse extra arguments and their completion (no such plugin on Horizon)
Permission nodes
- [x] invseeplusplus.invsee.view|edit, endersee.view|edit, reload, tabcomplete (defaults from `[access]`)
- [x] invseeplusplus.view|edit|exempt|bypass-exempt (aggregates grant their children in the mod)
- [x] invseeplusplus.exempt.invsee|endersee, bypass-exempt.invsee|endersee (declared at level 4)
- [ ] invseeplusplus.* (typed nodes cannot be named with a wildcard)
- [-] invseeplusplus.give.*, clear.*, clone.* (addon plugins)
Config keys
- [x] enable-offline-player-support, enable-unknown-player-support, tabcomplete-offline-players (as `players.*`)
- [x] titles.inventory, titles.enderchest, templates.inventory, templates.enderchest (same defaults)
- [x] placeholder-palette: glass panes, icons, empty
- [x] logging.granularity (LOG_NEVER, LOG_ON_CLOSE, LOG_EVERY_CHANGE), output PLUGIN_LOG_FILE, SPECTATOR_LOG_FILE
- [~] output SERVER_LOG_FILE and CONSOLE: written to stderr (no server log API)
- [x] format-plugin-log-file, format-spectator-log-file, format-server-log-file, format-console
- [x] uuid-resolve-strategies, username-resolve-strategies: names and defaults
- [~] permission plugin and proxy strategies answer nothing; player data answers from usercache.json
- [-] config.yml file format (Horizon mods use TOML: `invsee-server.toml`)
Messages (InvSee++ wording)
- [x] This command can only be used by players!
- [x] Player X does not exist. / has not logged onto the server yet. / is exempted from being spectated.
- [x] Spectating offline players' inventories is disabled. / enderchests
- [x] Another plugin prevented you from spectating X's inventory / X's ender chest.
- [x] An internal fault occurred when trying to load X's inventory / enderchest.
- [x] Could not open X's inventory for an unknown reason. / enderchest
- [-] Could not create ... for an unknown reason, An error occurred while trying to open ... (no path produces them: the mod has no error channel of that kind)
- [x] InvSee++ configuration was reloaded.
- [x] Permission denied text (Bukkit's default, as shipped)
- [~] "never played" text when clicking in an unknown player's view: invented, kept because nothing can be stored yet
Listeners and behaviour
- [x] Edit clicks (SpectatorInventoryEditListener): click rules, cursor, drag, numbers, double click (round 3)
- [x] Placeholders per container, body and saddle (glass palette matches InvSee++)
- [x] Log on close (granularity), spectator and plugin log files
- [x] Live/file switch on join and leave (player join hooks)
- [~] Viewers without edit rights see a copy (the original shows the live inventory; the target's live menu is API-blocked)
- [x] Exemption by config list or by the target's `invseeplusplus.exempt.*` node

Round 2 (combined SDK, 2026-11-25): feature `next-engine` removed. `Host::inventory` is `&mut self` now so an offline target is read with `server_player_inventory`; moves with an offline side use `server_move_inventory_stack`; single-slot edits of a saved player rewrite `Inventory`/`EnderItems` through `offline_player_data`/`set_offline_player_data`. Log lines use `console_log`. Permission nodes declared with `NodeDefault::Nobody` (exempt, bypass-exempt, `invseeplusplus.*`) and `add_with_children` (plugin.yml children). The `player data` resolver reads `bukkit.lastKnownName`.

Round 8 (2026-11-26): live inventory view with the target's cursor and crafting grid (`open_player_view_extras`); `/invclear` block tags (`Tags::blocks`); ENGINE_NEEDS `block_tag_items` closed, `menu_view_player_container` narrowed to the workstation inputs. Item list 34/38.
