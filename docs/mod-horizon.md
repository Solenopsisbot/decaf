# `mod-horizon/`: the native mod on the official Horizon

`mod/` runs the Java runtime inside **Sunrise** (`sunrise-mod-api`, see [mod-plan.md](mod-plan.md)).
`mod-horizon/` is the same mod built on the **official Horizon**'s mod API (`horizon-mod-api` from
horizon-rust/horizon, merged there by horizon-rust/horizon#16941). It exists because the API is now
upstream and someone wanted Java mods in the official game; that side owns it (#310):

- **Gabbotampo's side owns this build**: its bridge code, its pin, its CI job and its in-game proofs.
  Sunrise-side bridge changes made in `mod/` are not ported here by the maintainers.
- **It never holds a runtime PR.** The `mod-horizon` CI job is `continue-on-error`, and the `gates`
  job's vendor check covers `mod/` only; when an interpreter change breaks this build, the fix is a
  follow-up here.
- **Its runtime copy is generated, never committed**: `python3 tools/vendor.py --mod mod-horizon`
  writes the same `crates/horizon-native-java/src` copy as `mod/src/runtime/` into
  `mod-horizon/src/runtime/` (ignored by git). The `mod-horizon` CI job runs it first; run it locally
  before a build. Plain `vendor.py` and `vendor.py --check` touch `mod/` only, so no runtime PR has to
  regenerate or commit anything for this build.

## The SDK and the lock

Like `mod/` on Sunrise, `mod-horizon/` names the mod API by the path of the pinned build's signed SDK,
`.horizon/sdk/api/crates/horizon-mod-api` (ignored by git): the official Horizon refuses every other
form under `E_SDK_SOURCE` (horizon-rust/horizon `docs/modding/src/reference/sdk/03-in-a-project.md`).
The `mod-horizon` CI job fetches it from the release `sdk-<pin>` and checks it as that project's
`01-release.md` says (Horizon's signature on `SHA256SUMS`, `sdk.json` naming the pin, each file's size
and SHA-256), then builds with the Rust release the SDK was compiled with (`toolchain.rust`).
Locally, the same folder is what `cargo horizon-mod check` fetches when run in `mod-horizon/` (per that
page), or the CI step can be run by hand.

`Cargo.lock` is committed and the job builds `--locked`. Every registry package it shares with the
game is at the version and checksum of Horizon's own `Cargo.lock` at the pin, so the mod brings no
second copy of them.

## What differs from `mod/`

The bridge code (`src/*.rs`) is `mod/`'s, re-derived from it whenever `mod/` changes, with only:

| `mod/` (Sunrise) | `mod-horizon/` (official Horizon) |
|---|---|
| `sunrise-mod.toml`, `[native] sunrise = …` | `horizon-mod.toml`, `[native] horizon = …` |
| `sunrise-mod-api` from `.sunrise/sdk/` | `horizon-mod-api` from `.horizon/sdk/` |
| `#[mod_api::sunrise_mod]` | `#[mod_api::horizon_mod]` |
| `HitHost` (damage hooks' level) | `LevelRandomHost` |
| `BlocksEvent::register`, `ItemsEvent::register` (checked per entry) | `event.blocks.push`, `event.items.push` (the game's ownership fence refuses at start) |
| Test fakes: `AttachmentHost::validate_attachment`, `ConfigFiles::remove`, `PlayerModesBackend` | Not in the official API; `player_is_spectator` is on `PlayersBackend` |

Everything else is the same code, `game_dispatch.rs` included: the bridge takes its state through the
same `StateGuard` and dispatch gate as `mod/`, so Java callbacks re-enter the same way on both games.
Comments that name the game say Horizon instead of Sunrise.

The pin is `0a875da91d` (Nightly 2026-11-20), the newest build with a published `sdk-<sha>` release
when this was written. At that pin `cargo check --all-targets`, clippy with `-D warnings` and
`cargo test --lib` pass.

## Embedded data as Rust

The official Horizon's native package takes only `.rs` and `.toml` files under `src/`
(horizon-rust/horizon `docs/modding/src/how-to/native-mods/03-data-files.md`), and the launcher's
staging leaves every other file out, so the build fails on the first `include_str!` of a `.txt`.
`vendor.py --mod` therefore writes each embedded data file of another kind into a folder with
`horizon-mod.toml` as `<name>.rs`, one raw string literal of the same text, and turns the vendored
`include_str!("<name>")` into `include!("<name>.rs")`. The constant holds the same bytes;
`tests/test_vendor_mod.py` checks the literal against the source file. `mod/` keeps the plain copy.

## In-game proof (official launcher 0.2.95, Nightly 2026-11-20 at the pin)

Run in a scratch tree with fresh `HORIZON_*`/`XDG_*` homes, as Horizon's
`docs/mod-api/developer-acceptance-workflow.md` describes:

```sh
horizon-launcher install nightly-2026-11-20-0a875da
horizon-launcher prepare --version nightly-2026-11-20-0a875da --minecraft <your Minecraft folder>
python3 tools/vendor.py --mod mod-horizon     # then copy mod-horizon/ without .horizon/ to MOD
horizon-launcher native-build MOD --server-output ENGINE --version nightly-2026-11-20-0a875da --trust-native-code
horizon-server --dir SERVER init --eula --world proof --port 25599 --no-whitelist --offline --yes
horizon-server --dir SERVER config set engine.path=ENGINE/horizon-engine server.bind=127.0.0.1
# jars from tools/java_mod_fixture.py in SERVER/config/horizon_java/java-mods, then
horizon-server --dir SERVER start; horizon-server --dir SERVER cmd ...; horizon-server --dir SERVER stop
```

The launcher prints `Prebuilt game: only the mods compile (optimized)`: the game is the signed SDK's
prebuilt server engine, and only this mod's crate and its 20 registry crates compile (211 s here).

- **M0**: with an empty `java-mods/`, `config/horizon_java/java-runtime.json` is written through
  the config service with `"schema": 1, "jars": []`.
- **M1**: eight fixture jars are `ran-clean` with no unknown host calls (`m0_fixture`: 11 host
  calls). A file that is not a jar is `refused` with `not a readable jar: Zip("invalid Zip archive:
  Could not find EOCD")`.
- **M2a**: `m2a_fixture`'s `SERVER_STARTED` and `SERVER_STOPPING` listeners ran: `callbacks.ran`
  is 2, with 0 failed.
- **M2b**: `javahello Viola` on the console answers `Hello, Viola!`, `javahello` answers
  `Hello, world!`, and `help javahello` shows `/javahello [<name>]`. The row has 3 usages translated
  and 2 runs.
- **M2c**: `m2c_fixture`'s tick listeners ran 825 callbacks, 0 failed.
- **M3**: `own_content_fixture` declares `horizon_java:proof_item` and `horizon_java:proof_block`.
  - `setblock … horizon_java:proof_block` works, and `execute if block` finds it.
  - `item replace … with horizon_java:proof_item` puts the item in a chest.
  - `horizon_java:proof_block` is `Unknown item`: it has no block item, as in Java.
  - `m3_fixture:*` is unknown: that namespace is not the mod's, so it is in the row's
    `content.skipped`.

**With a player.** A client of the same build joined the server: the launcher's client build of
this mod for an instance, copied out of the managed install, run on a virtual display (Xvfb, the
software renderer) with `--server 127.0.0.1:25599 --name Prova`, and driven through `horizon-ctl`.
- **Joining needs the same mods and content on both sides.** The handshake refuses a client
  without `horizon_java` ("Your mods don't match this server's"), and one without the same jars
  ("Content registry mismatch"). The client reads its own `config/horizon_java/java-mods/`.
- **M2c, block break**: `horizon-ctl break` of stone with a diamond pickaxe leaves the stone in
  place (`m2c_fixture`'s `PlayerBlockBreakEvents.BEFORE` returns false for stone), and dirt breaks
  and drops.
- **M2d**: two `interact-entity` on a cow, then `kill` of 9 cows and the player, and a respawn.
  - `m2d_fixture` ran 14 callbacks, 0 failed, 0 unknown host calls. The split across interactions
    and deaths is inferred, since the per-callback lines are not logged.
  - `copyfrom_fixture` ran 2, `COPY_FROM` and `AFTER_RESPAWN`.
  - `m2c_fixture`'s total reached 1992 with the player's ticks.

**Real mods.** Five Fabric jars from the local corpus, on the same build with no other jars. None
runs clean yet. These are runtime gaps, not this bridge's: the same runtime runs in `mod/`.

| Jar | Verdict | Why |
|---|---|---|
| fabric-carpet 26.3+v260915 | `ran-partial` | `CarpetRulePrinter.onInitializeServer`: NPE on `joptsimple.OptionSpecBuilder.withRequiredArg()`; 11 unknown host calls; `/carpet` is not registered |
| FarmersDelight 3.6.30 | `refused` | `ExceptionInInitializerError` in `AbstractStoveBlock.<init>`, caused by an NPE on `StateDefinition.any()` |
| mcw-fences 1.2.1 | `refused` | the same `StateDefinition.any()` NPE in `FenceHitbox.<init>` |
| toms_storage 2.12.2 | `refused` | NPE on `BlockState.setValue` in `AbstractStorageTerminalBlock.<init>` |
| Clumps 26.3.2 | `refused` | no main, server or NeoForge entrypoint |

The three block-state failures are the area of #581 (block state properties).

## What is not done yet

- **The mod's log lines don't reach the game's log.** The mod compiles its own copy of the `log`
  crate, and no logger is installed in that copy. The game's `log` is in the SDK's
  `libmod_vocab.so`. So the `Horizon Java …` lines and the jars' `System.out` lines are lost on
  this build, and the proof above reads the report and the console instead. horizon-rust/horizon#17983
  proposes `Host::log` for this.
- **M3b**: `tools/bake.py` writes Sunrise packages (`.smod` in `sunrise-mods/`). A bake for the
  official Horizon (`.hmod` in `horizon-mods/`, its own manifest name) does not exist yet.
- An item the jar declares shows `block.horizon_java.proof_item` as its name key, where Java uses
  `item.horizon_java.proof_item`. The game picks the key, not this bridge: Horizon's command
  feedback (`crates/horizon-client/src/game/administration/commands/mod.rs` at the pin) uses
  `item.<id>` only when the language pack holds that key, and `block.<id>` otherwise. An item with
  no name in any language pack therefore prints the block form.
