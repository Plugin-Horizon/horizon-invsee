# Status (2026-11-08, ported from the source; not built)

## Switched to Horizon helpers (2026-11-08, core tests only: 40 pass)

Exact slot writes (`native_set_inventory_slot`): armour/off-hand placement, swap of a worn piece, right-click split.
Profile cache (`native_profile_by_name/uuid`) for name lookup. Permission nodes (`native_has_permission`) for view, edit,
reload, tab completion and exempt bypass. `native_refresh_commands` after a level change. The glue is not built;
the four helpers are pending API fragments, so `"hooks"` in the bundle needs their stamped number.

## Ported from the source (2026-11-08)

The original source (github.com/Jannyboy11/InvSee-plus-plus, `722def9`, LGPL-2.1) was read in its `InvSee++_Plugin`,
`InvSee++_Common` and the Paper 26.3 platform code (for the default offline creation path). Changed to follow it:

- Commands: the first argument only (extra arguments are ignored); a bare `/invsee` shows plugin.yml's usage; a console
  call with a target says "This command can only be used by players!"; a name that cannot be one says "does not exist".
- Messages, as the original words them: "Player X does not exist.", "Player X has not logged onto the server yet.",
  "Player X is exempted from being spectated.", "Spectating offline players' inventories/enderchests is disabled.",
  "Another plugin prevented you from spectating X's inventory", the reload text "InvSee++ configuration was reloaded."
  (green), Bukkit's permission message. The original's messages use the name as typed; so does the port.
- A UUID nobody can name opens as "InvSee++ Player" (the original's placeholder), not as the UUID.
- Permissions: `default: op` is level 1 (any operator), not 2; bypass-exempt is `default: false`, so level 5 ("nobody").
- A click below the edit level is cancelled silently (the original's listener says nothing).
- Tab completion of offline names needs offline support on, as in the original.
- Placeholders: the original's names (Helmet, Chestplate, Leggings, Boots, Off-hand, Body, Saddle, Cursor, Crafting
  ingredient) in aqua, with the original's glass-pane and icon materials per slot.
- Logs: folder `InvSee++ logs`, plugin log `_global.log`, items as `Material` names (`[DIAMOND x 5]`), as the original writes them.
- License: LGPL-2.1 text in LICENSE (from the original), NOTICE added, Cargo `license = "LGPL-2.1-only"`.

Counted against the source: 15 permission nodes in plugin.yml (addons excluded), all mapped to levels or config
lists; 10 command words, 8 registered (`invsee++reload` and `is++reload` stay out, see GAPS.md); 16 config keys (14
active, 2 commented-out resolve lists), all read (the resolve-strategy lists have fewer options: no permission plugin,
proxy, player-data or spoof strategy yet). Of the 18 distinct message lines in the three executors, 9 now match word
for word; the other 9 are the failure texts (an exception, an internal fault, "unknown reason") that this flow does not
produce, and the port's own "Can't open ..." texts stand in for them.

Not done, and why: the 'unknown player' view is still read-only (GAPS: no writable file for a never-played UUID);
no drag, cursor, split or armour placement (GAPS: cursor and crafting-grid helpers); placeholder lore
("InvSee++ placeholder") is not set; the LuckPerms workaround of the original (offline exemption skipped when LuckPerms
is on) has no counterpart; the default resolve order uses Mojang, while the original uses offline UUIDs on offline-mode
servers (on an offline-mode server `spoof` would be its default). Nothing has been built: cargo
was not run (one shared build later).

## Before the port (2026-11-06, blind: no engine build yet)

The core crate builds and its 33 tests pass on their own (`cargo test` in `mixins/invsee/core`, 2026-11-06, no warnings); the bundle itself
(`horizon-mixins apply` + engine build) has **not** been built or run. Feature list from InvSee++ 0.31.x core
(no addons), with what is written here.

| Feature of the original | State | Notes |
|---|---|---|
| `/invsee <name>\|<uuid>`, aliases `inventorysee`, `isee` | written | arg optional so a bare `/invsee` prints the usage |
| `/endersee <name>\|<uuid>`, aliases `enderchestsee`, `esee` | written | 3 rows (vanilla's 27 slots) |
| `/invseeplusplusreload`, alias `isppreload` | written | also the console; the game reloads the file by itself when it changes |
| Aliases `invsee++reload`, `is++reload` | not started | `+` is refused in usages (GAPS) |
| Online targets, view follows live | written | polled every tick |
| Offline targets (player file) | written | Horizon API 132 reads/writes `players/data/<uuid>.dat` |
| Target logs in / out while viewed | written | `native_inventory` switches between live and file by itself |
| Players who never played (`enable-unknown-player-support`) | partly | empty view, read-only (GAPS: no file to write) |
| Editing (`*.edit`) | partly | click = whole-stack transfer: pickup / shift-click / number key take from the target or give from below; no split, no armour placement, no drag, no cursor (GAPS) |
| Middle click (creative clone), Q (drop), drag, double click | not started | ignored on purpose: without a cursor they would make or lose items |
| Permissions view/edit per kind, reload | partly | operator levels in `[access]` instead of nodes (GAPS) |
| `exempt.*` / `bypass-exempt.*` | partly | `exempt_invsee` / `exempt_endersee` lists (names or UUIDs) + `bypass_exempt` level |
| `tabcomplete` + `tabcomplete-offline-players` | partly | `tab_complete` hook written (online, seen, usercache names); the target argument is `word?`, so the client asks (not yet built) |
| `enable-offline-player-support` | written | `[players] offline` |
| `titles` with `<player>` | written | `§` codes allowed |
| `templates` (`i_xx`, `a_b/l/c/h`, `oh`, `_`, `e_xx`) | written | duplicates, unknown words and missing slots warn and become unused |
| Template slots `b`, `s`, `c`, `p_xx` | partly | shown as "unavailable" placeholders (GAPS: cursor, crafting grid) |
| `placeholder-palette` (glass panes / icons / empty) | written | item choice is ours (clean room) |
| Logging `granularity` (never / on close / every change) | written | taken/given are exact: every move goes through the mod |
| Logging outputs (plugin file, spectator file, server log, console) and their formats + macros | written | files under `<world>/invsee/logs/`; server log and console via stderr (GAPS) |
| `uuid-resolve-strategies` | partly | online, logged-out cache, usercache.json (≈ "paper offline player cache"), Mojang API, offline UUID ("spoof"); no permission-plugin / proxy / electroid / player-data-file strategies |
| `username-resolve-strategies` | partly | online, logged-out cache, usercache.json, Mojang API |
| PerWorldInventory / Multiverse-Inventories (`PWI{...}`) | not started | no such plugins on Horizon |
| Addons InvSee++_Give / _Clear / _Clone | not started | not part of the reference setup |
| bStats / FastStats metrics | not started | left out on purpose |

Counts: written 12, partly 8, not started 5.

## Test first, once a build is possible

1. `horizon-mixins apply` on `a589a3dd9`: the paths the glue names (`super::native_inventories::NativeInventoryKind`
   from a child module of `game`, `horizon_client::item_data::StackData: Default`,
   `horizon_client::lan::data::component_nbt`, `Text::legacy`, `crate::game::native_configs::*`), and that the
   `STATE` static is `Sync` (`ItemStack: Send`).
2. `/invsee <online player>`: the view opens on the next tick, shows hotbar row first, armour placeholders, follows
   the target's changes; `/endersee` shows 3 rows.
3. Clicks: left/shift click on a target slot moves the stack to the spectator; click on the spectator's own
   inventory below moves it to the target; a full inventory says "no room"; nothing duplicates (count items before
   and after). Check `native_menu_clicked`'s `slot` numbering below a 6-row and a 3-row view.
4. Offline: `/invsee <offline name>` from usercache.json, take an item, the player joins and has it gone; with a
   sync mod (HuskSync port) the database copy may win on join.
5. `native_menu_closed` when the spectator closes, opens another view, leaves; the log entry in
   `<world>/invsee/InvSee++ logs/_global.log`.
6. Whether the client sends a completion request for the `word?` target argument and the tab_complete hook answers it (`/invsee <Tab>` lists online and offline names).
7. Re-entrancy: no deadlock when `native_open_menu` closes an old view inside the tick (the close is deferred).

## Checked against public docs (2026-11-08, no code change)

Sources: the Modrinth and Hangar project pages, the GitHub README and the public plugin.yml and config.yml of
InvSee++ (documentation only, no source). Commands and aliases match (`invsee`/`inventorysee`/`isee`,
`endersee`/`enderchestsee`/`esee`, `invseeplusplusreload`/`isppreload`). Titles, placeholder palette, log
granularity and outputs and the template slot codes match. Open points, all already in GAPS.md or guesses:

- Plugin.yml also lists the aliases `invsee++reload` and `is++reload` (GAPS: `+` in usages).
- Permission defaults: `.view`/`.edit` are "op" in the plugin, mapped to operator levels 2 (and 3 for reload) here.
  "op" may mean any operator level (1 to 4); level 2 is a guess.
- `bypass-exempt` defaults to false in the plugin (nobody by default); here `bypass_exempt = 4` is a guess.
- Enderchest: the default template lists 6 rows, the view shows 3 (vanilla's 27 slots); the plugin's own rendering of
  slots past 27 is not documented.
