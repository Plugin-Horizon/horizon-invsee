# invsee for Horizon

> **Unofficial Rust port of InvSee++ for the Horizon server (https://github.com/arismoko/horizon). Not affiliated with or endorsed by the InvSee++ authors.**

A native mod for [Horizon](https://github.com/arismoko/horizon) servers: operators open another player's
inventory or ender chest as a chest view, whether that player is online or not, and take items out or put items in.
It is a port of the Bukkit plugin InvSee++ for Horizon.

**Derived from InvSee++** (<https://github.com/Jannyboy11/InvSee-plus-plus>, LGPL-2.1, by Jannyboy11). The commands,
messages, defaults, placeholders and log formats follow its source; the authors and the licence are in
[NOTICE](NOTICE) and [LICENSE](LICENSE). It is not affiliated with InvSee++ or its author.

> **Built for and working only with [arismoko/horizon](https://github.com/arismoko/horizon)** (the Horizon fork that carries the mod API). [horizon-rust/horizon](https://github.com/horizon-rust/horizon) is experimental for mods and not supported: the earlier version for its native hooks / horizon-mixins is frozen on the `horizon-plain` branch (work stopped on 2026-11-12).

## Which Horizon

This is a native trait mod: `horizon-mod.toml`, a `Cargo.toml` that depends on `horizon-mod-api`, `src/lib.rs` with the hooks, and the plugin logic in the `core/` crate (plain Rust, no engine needed for its tests).

- Pin: `[native] horizon = "54090ee36c4e2c7b43b66db864220c577b94b7e0"` in `horizon-mod.toml` (game `26.3`). That is a development build of arismoko/horizon `main` plus mod API additions that are not all merged upstream yet, so a stock `main` may lack some host methods this mod calls until they land.
- It compiled and loaded on that build and was smoke-tested there; it has not seen a long run with real players. Treat it as experimental.
- Feature descriptions below were written for the earlier horizon-rust version; where they name engine calls (`native_*`) or old file paths, read them as descriptions only. The Known gaps list is current.

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
two checked writes (source emptied, then the target filled; the source is put back if the second fails and the slot is still free, otherwise the message says so), and a move that no longer fits or
whose stack changed does nothing and says why.

## Configuration

`config/invsee-server.toml`, written with all defaults and comments on the first start, reloaded by the game when
it changes (see [`core/invsee-server.toml`](core/invsee-server.toml)):

- `[access]`: operator level per action, `exempt_invsee` / `exempt_endersee` (names or UUIDs) and `bypass_exempt`.
- `[players]`: offline and never-played players, name lookup order (`online`, `logged_out`, `usercache`, `mojang`,
  `offline`), Mojang timeout, refresh interval of offline views.
- `[titles]`, `[templates]`: the view's title (`<player>`) and layout, the same words as InvSee++'s templates.
- `[menu] placeholder_palette`: `glass panes`, `icons` or `empty`.
- `[logging]`: `LOG_NEVER` / `LOG_ON_CLOSE` / `LOG_EVERY_CHANGE`, outputs `PLUGIN_LOG_FILE`
  (`<world>/invsee/InvSee++ logs/_global.log`), `SPECTATOR_LOG_FILE` (`<world>/invsee/InvSee++ logs/<uuid>.log`), `SERVER_LOG_FILE`,
  `CONSOLE`, and the formats with `<date> <time> <level> <spectator_uuid> <spectator_name> <taken> <given> <target>`.

Permission nodes (`invseeplusplus.invsee.view|edit`, `invseeplusplus.endersee.view|edit`, `invseeplusplus.reload`,
`invseeplusplus.tabcomplete`, `invseeplusplus.bypass-exempt.invsee|endersee`, `invseeplusplus.exempt.invsee|endersee`)
are asked through Horizon's permission check, so a permissions mod can answer them. The default of the first six is
the operator level of `[access]` as read at server start (a restart applies a changed level); the exempt and
bypass-exempt nodes are declared at level 4 (InvSee++'s default is nobody, which a node default cannot say). A target
is exempt when it is in the config list or holds `invseeplusplus.exempt.<kind>` (online targets only).

## Differences from InvSee++

See the Known gaps section for the full list and the Known gaps section for what Horizon would need. In short: whole-stack
clicks (plus split and armour placement) instead of a free cursor, the cursor and crafting grid slots are shown as
unavailable, and a never-played player's view is read-only.

A mod or plugin that keeps inventories in a database and writes them back on join (a HuskSync-like sync) can
overwrite changes made to an offline player's file.

## Build

Build it as a native trait mod with arismoko's launcher (see the "native mods" how-to in the arismoko/horizon docs):

```sh
horizon-launcher native-build <path to this folder> --server-output <dir> --version <tag> --trust-native-code
```

`--trust-native-code` is your consent to compile and run this code inside the server. The logic crate tests run without the engine: `(cd core && cargo test)`.

## Known gaps

What does not work yet on arismoko/horizon (each is marked `TODO(arismoko-api: ...)` in `src/lib.rs`):

- Offline players' inventories cannot be read or written (a mod cannot read the player file yet).
- The view follows the target every tick and clicks use vanilla's rules (cursor, drag, shift click, number keys,
  double click), but it is the mod's own menu, not the target's container: throws and middle clicks do nothing, and
  the target's cursor and crafting grid are shown, not edited.
- The target's anvil, merchant and other workstation inputs cannot be read (only their placeholders are shown).
- The `permission plugin`, `proxy` and `player data save files` resolve strategies answer nothing.
- Log lines go to stderr, not to the server's log file.

## Licence

LGPL-2.1, see [LICENSE](LICENSE) (the original's licence text) and [NOTICE](NOTICE). The port derives from InvSee++,
so the whole bundle is under the same licence.
