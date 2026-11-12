> **Frozen branch (`horizon-plain`).** This is the version written for horizon-rust/horizon's native hooks / horizon-mixins. We stopped working on it on 2026-11-12: horizon-rust moved all mod API work to the fork [arismoko/horizon](https://github.com/arismoko/horizon) (see "Mod-related pull requests" in its CONTRIBUTING.md) and closed mod pull requests there. This branch is experimental and unsupported. The maintained version, for arismoko/horizon only, is on the default branch.

# invsee for Horizon

> **Unofficial Rust port of InvSee++ for the Horizon server (https://github.com/horizon-rust/horizon). Not affiliated with or endorsed by the InvSee++ authors.**

A native mod for [Horizon](https://github.com/horizon-rust/horizon) servers: operators open another player's
inventory or ender chest as a chest view, whether that player is online or not, and take items out or put items in.
It is a port of the Bukkit plugin InvSee++ for Horizon.

**Derived from InvSee++** (<https://github.com/Jannyboy11/InvSee-plus-plus>, LGPL-2.1, by Jannyboy11). The commands,
messages, defaults, placeholders and log formats follow its source; the authors and the licence are in
[NOTICE](NOTICE) and [LICENSE](LICENSE). It is not affiliated with InvSee++ or its author.

## Commands

| Command | Aliases | Default level |
|---|---|---|
| `/invsee <name>\|<uuid>` | `inventorysee`, `isee` | 1 (any operator) |
| `/endersee <name>\|<uuid>` | `enderchestsee`, `esee` | 1 (any operator) |
| `/invseeplusplusreload` | `isppreload` | 1 (any operator; also from the console) |

A view opens on the next server tick. It follows the target live (every tick while they are online, every
`offline_refresh_ticks` from their file while they are not). A player who joins or leaves while viewed is switched
between live and file by Horizon.

**Editing** (level `invsee_edit` / `endersee_edit`): clicking a stack in the view moves it whole into your own
inventory; a right click takes the larger half into a free slot of yours; clicking a stack in your inventory below
the view moves it whole into the target's, and armour, elytra, shields and totems go to the slot they are worn in
(a piece already worn swaps places with it). A number key over an
empty slot of the view gives your hotbar slot. Middle click, Q, dragging and double clicks do nothing. Every move is
one checked step in Horizon (`native_move_stack`): nothing is duplicated or lost, and a move that no longer fits or
whose stack changed does nothing and says why.

## Configuration

`config/invsee-server.toml`, written with all defaults and comments on the first start, reloaded by the game when
it changes (see [`mixins/invsee/core/invsee-server.toml`](mixins/invsee/core/invsee-server.toml)):

- `[access]`: operator level per action, `exempt_invsee` / `exempt_endersee` (names or UUIDs) and `bypass_exempt`.
- `[players]`: offline and never-played players, name lookup order (`online`, `logged_out`, `usercache`, `mojang`,
  `offline`), Mojang timeout, refresh interval of offline views.
- `[titles]`, `[templates]`: the view's title (`<player>`) and layout, the same words as InvSee++'s templates.
- `[menu] placeholder_palette`: `glass panes`, `icons` or `empty`.
- `[logging]`: `LOG_NEVER` / `LOG_ON_CLOSE` / `LOG_EVERY_CHANGE`, outputs `PLUGIN_LOG_FILE`
  (`<world>/invsee/InvSee++ logs/_global.log`), `SPECTATOR_LOG_FILE` (`<world>/invsee/InvSee++ logs/<uuid>.log`), `SERVER_LOG_FILE`,
  `CONSOLE`, and the formats with `<date> <time> <level> <spectator_uuid> <spectator_name> <taken> <given> <target>`.

Permission nodes (`invseeplusplus.invsee.view|edit|bypass-exempt`, the same for `endersee`, `invseeplusplus.reload`,
`invseeplusplus.tabcomplete`) are asked through Horizon's permission check, so a permissions mod can answer them; where
none does, the operator levels of `[access]` apply. Changing those levels in the file resends every player's command
tree.

## Differences from InvSee++

See [STATUS.md](STATUS.md) for the full list and [GAPS.md](GAPS.md) for what Horizon would need. In short: whole-stack
clicks (plus split and armour placement) instead of a free cursor, the cursor and crafting grid slots are shown as
unavailable, and a never-played player's view is read-only.

A mod or plugin that keeps inventories in a database and writes them back on join (a HuskSync-like sync) can
overwrite changes made to an offline player's file.

## Build

The bundle is `mixins/invsee/`: `horizon-mixin.toml`, one helper per hook, a support module (`glue.rs`) and the crate
`core/` with all logic (no game types). It needs Horizon at `a589a3dd9` or later (server menus, API 10; player
inventories, API 132; mod configs, API 397). Install it like any native mod (the launcher's Mods page or
`horizon-launcher native-build`), or apply it with `horizon-mixins apply` to an exported Horizon source.

The crate's tests run on their own:

```sh
cd mixins/invsee/core && cargo test
```

**Untested:** the bundle has not yet been applied to Horizon or run on a server; see STATUS.md, "Test first".

## Licence

LGPL-2.1, see [LICENSE](LICENSE) (the original's licence text) and [NOTICE](NOTICE). The port derives from InvSee++,
so the whole bundle is under the same licence.

## Contributing

Pull requests and improvements are always welcome! Open an issue or a PR -- direct pushes to main are disabled, every change goes through review.
