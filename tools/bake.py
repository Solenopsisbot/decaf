#!/usr/bin/env python3
"""Bake a Java mod jar's assets and data into a Horizon content package.

A native mod may not own another namespace's files or declarations (`Content::check_namespace`
and the generated declaration fence in the fork), so a jar's own models, textures, names,
blockstates, loot, blocks and items never reach the game by themselves. This tool closes that gap at
install time: it runs each jar's server entrypoints through the offline runtime
(``horizon-native-java --json <jar>``, the code the mod vendors), reads the typed blocks and items
it recorded, keeps the resource kinds a content package accepts, and writes one package per
namespace. The package declares its own blocks and items next to the jar's assets, and the game
loads it as an ordinary content mod; the mod's runtime declaration for that namespace is skipped.

    python3 tools/bake.py --probe target/debug/horizon-native-java --game-dir /tmp/m3b-env/game-dir
    python3 tools/bake.py --game-dir SCRATCH --java-mods SCRATCH/config/horizon_java/java-mods
    python3 tools/bake.py --target horizon --probe PROBE --game-dir SERVER-FOLDER

``--target`` picks the game that reads the packages:

* ``sunrise`` (the default, this fork): ``<id>.smod`` with ``sunrise-mod.toml`` in
  ``<game-dir>/sunrise-mods/``, turned on in ``sunrise-mods.txt``, declarations in
  ``data/<ns>/sunrise/{block,item}/*.json``. Its output is byte for byte what the bake wrote before
  the horizon target existed (``tests/data/bake_sunrise_golden.json``) for a probe that names no
  loaded mods. When the probe names them (``content.mods``: the jar's own mod, the nested jars
  the runtime loaded and their ``provides`` aliases), the packages are exactly those ids, a nested
  mod or an alias included even with no file of its own, and every other namespace of the jar rides in the jar's own package's manifest
  ``namespaces``. Sunrise answers ``mod_loaded`` from package ids, so it then agrees with what the
  jar's code was told (``_mod_packages``, BUGS-3d34-round2 #3).
* ``horizon`` (the official horizon-rust/horizon build ``0a875da9``, content API ``0.2``):
  ``<id>.hmod`` with ``horizon-mod.toml`` in ``<game-dir>/horizon-mods/``, turned on with
  ``mod.<id>:true`` in ``horizon-mods.txt``, declarations in ``data/<ns>/horizon/{block,item}/``.

The horizon target follows the clean-room package specification (Horizon's docs and black-box runs
of its binaries, never its code): sections 1-3, the rules R1-R10 of its "Found in review
(black-box)" section and those of its "Round 3 (black-box)", "Round 4 (black-box)", "Round 5
(black-box)", "Round 6 (black-box)" and "Round 7 (black-box)" sections, named where each is
applied.
The official game refuses a whole package for one bad file, drops every package's content for some
bad tags, and stops for every mod on some bad declarations and data, so this target writes only
what the bake can check exactly, and leaves everything else out with a counted reason in the
report. It writes:

* ``assets/`` files of a namespace the jar owns, of the asset kinds Sunrise copies, and vanilla's
  atlases (``assets/minecraft/atlases/``, N3). A namespace is the jar's when its runtime declares
  something there or it is one of the jar's mod ids (``fabric.mod.json`` ``id``, ``[[mods]]
  modId`` of ``META-INF/neoforge.mods.toml`` and ``META-INF/mods.toml``); a package for any other
  namespace would take another mod's id (Round 3). A runtime declaration in a namespace that is
  another jar's mod id in the same run is that jar's: it is left out, so it neither takes that
  jar's package nor refuses it, whichever jar sorts first (Round 4). A namespace that is none of
  the jar's mod ids and that another mod holds is left out with its files, without an error, and
  the jar's own packages stand. Another mod holds it when an earlier jar of the run baked it, a
  readable package in ``horizon-mods/`` has it as its id or namespace, or an earlier bake wrote its
  package for another mod (see the owner rule below). A namespace the bake cannot decide is an
  error for that namespace, mod id or not (see R2 below). Every ``.json`` asset is
  strict JSON: UTF-8, no byte order mark, no comment, no ``NaN``, no duplicate key, nothing after
  the value, at most 127 arrays and objects nested in one another (R3, RD1);
* the runtime's block and item declarations the game loads (3.2, 3.3, R6-R9). Block templates are
  the two decaf's runtime infers, ``minecraft:stone`` and ``minecraft:crafting_table`` (both
  without state properties of their own); ``sounds`` one of the 878 blocks of
  ``vanilla_block_items_java25.tsv`` (all accepted by the game, R7); item templates and
  ``block_item`` an item of ``vanilla_item_ids_java25.txt`` other than ``minecraft:air``; a
  ``max_stack_size`` over 1 is left out on the 84 damageable templates of ``vanilla_item_data.rs``
  (R8). Numbers are checked against the game's bounds: ``friction`` 0 to 2, ``hardness`` 0 or more
  or -1, ``explosion_resistance`` 0 or more, ``light`` 0-15 and ``map_color`` 0-61 as integers
  (R6). A state property definition holds ``type``, and ``values`` for an enum, nothing else
  (the game ignores other keys, SP1); a state property name and an enum value are at most 255
  bytes (the bake's bound; the game stops past 65,535, Round 6). The bytes written are checked
  again: a file over 16 MiB (P5) or one that is not strict JSON by the asset rules is left out. A
  block has at most 65,536 states (RS3). Then the game's counts (RS1, RS2): a world holds
  1,048,576 block states, vanilla's 35,723 included (from the game's message and a baseline run),
  and the mods may add 64,250 blocks (measured); the bake counts the blocks of this run and those
  of the packages already in ``horizon-mods/`` that may be turned on, and leaves out each block
  that would pass either count, in the probe's order. A package counts its block declarations and
  its fluid declarations (``data/<ns>/horizon/fluid/*.json``): a fluid with a ``world`` object adds
  a block of 16 states (and a bucket item) against both counts, one without (or with ``"world":
  null``) adds none, and any other fluid file counts as one block of 16 states (FL1, Round 7). The
  other kinds the game takes in ``data/<ns>/horizon/`` (``item``, ``creative_tab``, ``mob_effect``,
  ``entity_type``, ``menu``) add no block. A package that is off does not count in the game (RB2):
  one is off when ``horizon-mods.txt`` reads and has no line that turns it on as the game reads the
  file (lines split at ``\n`` only, key and value trimmed of Unicode white space, ML1), and every
  package is off when there is no ``horizon-mods.txt`` at all, as in the game (L5). A package the
  run replaces stops counting: the bake decides before the first jar which of its own earlier
  packages the run's jars replace (their recorded owner, RO1, is a jar of the run), and their old
  size counts against no jar, after a trial of the run, without writing, has shown that each of
  them is replaced (one that is not counts again), so a run after a jar's update writes what the
  next run writes (RB5). If writing a package the run replaces fails (for example a folder package
  in its place, or a full disk), the jars baked before its owner have already used that package's
  old room, the old package stays on disk and stays turned on, and the game folder can then pass
  the game's block limit; the run reports it as an error naming the package, and turning that
  package off or removing it, then baking again, gives a folder within the limits. It reads the
  block declarations of those packages one at a time, at most 16 MiB each and 64 MiB in all for
  the packages it did not write (its own earlier packages, which
  it may replace, are not held to that bound, RB4), and opens no zip whose directory lists over
  131,072 entries or 64 MiB (the bake counts the directory's records itself, never trusting the
  count the zip states, ZB1). Past a bound, or when it cannot read a package or one of its block
  files, that package's count is unknown: a namespace of the run with blocks is then an error and
  not written, so an earlier package of it stays as it was, and the run has an error that says so
  (RB3). Run again with the same inputs, the bake writes the same packages and exits the same way
  (RB4). It cannot count native mods' blocks, nor the states of another package's template, so the
  game can still run out with those installed. A block whose generated blockstate name is also the
  folder of the generated blockstate of another block it writes (``a`` beside ``a.json/c``) gets
  none from the server (RB1): that block is left out; a block that ships its blockstate gets none
  generated and needs none. Every one of these checks runs before duplicates, a block's own item
  and item remainders are decided, among the declarations that stay: a left-out declaration does
  not take a valid one with it, and no kept item names one left out.

It leaves out:

* every ``data/`` file: tags, loot tables, advancements, predicates, item modifiers, structures,
  recipes and every registry kind, with one reason, "data the bake cannot validate for the
  official game (references, cycles, schemas)". A missing or recursive loot table, a tag cycle,
  an advancement predicate in list form, a predicate the bake would leave out, a recipe with an
  unknown load condition or an unregistered serializer: each stops the game or drops every
  package's content (R3-R5, the round-2 reviews, Round 3 RC1-RC2);
* names the game refuses as executables (``.exe .dll .dylib .so .bat .cmd .com .ps1 .jar .class
  .wasm``, A10) and files that start like one (ELF, ``MZ``, ``#!``, wasm, ``CAFEBABE``, Mach-O,
  A11). ``.zip``, ``.sh`` and ``.py`` are taken by the game (A10) but stay out as they do for
  Sunrise, under a reason that says so;
* a path part, in a jar file or in a declaration's entry name, that is empty, ``.``, ``..``, ends
  in a dot or space (A5, R1), is not ``[a-z0-9_.-]`` matched whole (P1), is over 255 bytes (RP1)
  or is a Windows device name before its first dot (``con``, ``prn``, ``aux``, ``nul``,
  ``com1``-``com9``, ``lpt1``-``lpt9``, RP2); a whole entry name over 512 bytes, or over the
  bound the game folder's path leaves (RL1: 4095 - 52 - its length, on Linux); a name the jar
  holds twice (RP3); an asset file that is also the folder of another asset file, and that
  folder's files, except lang files (RP4: both sides loaded for lang files and declarations,
  which are kept, but for blocks see RB1; for other kinds it is not established);
* assets of ``c`` and ``horizon`` and vanilla's other than atlases, and declarations in ``c`` or
  ``horizon`` (N3-N5); the namespace ``horizon_java``, the engine's built-in mod;
* a package over 65,536 files or 512 MiB once its declarations are in (P5): an error, not written;
* a namespace another mod holds (above): left out when it is none of the jar's mod ids, an error
  when it is one; a namespace the bake cannot decide, whatever the jar's mod ids: next to a
  package whose manifest it cannot read, or where a folder or a package of another id sits at its
  package's path (R2): an error for that namespace, not written. A package this bake wrote earlier
  for the same namespace, at the same path, is replaced only when it is the same mod's (Round 5,
  RO1): its manifest's description records the jar and the jar's mod ids, and the namespace must
  be a mod id of both jars, or of neither and the mod ids the same, or with no mod id on either
  side the jar's name the same (a package of an earlier round records only the jar: the same
  name). When its owner's jar is in the run too, the namespace is decided again among the run's
  jars in their order, as on a first run (RB4). A package of an earlier bake that this run did
  not rewrite (its namespace or jar is now refused, another mod's jar wanted it, the jar no longer
  has it, or the jar is gone) is named under ``stale`` in the report and on stderr, with the mod
  ids it records; the bake never deletes it.

A package the bake writes is turned on in ``horizon-mods.txt`` (``mod.<id>:true``) unless
``--no-enable`` is given, even when the user turned it off by hand (``mod.<id>:false``): that is
intended, and the same as for Sunrise. Only the first line that names it is rewritten.

What the specification lists as not established is left out too, never written on a guess: block
keys other than ``template``, ``properties``, ``item``, ``block_item``, ``state_properties``,
``default_state``, ``shapes`` and ``collision_shapes``; ``properties`` keys other than
``hardness``, ``explosion_resistance``, ``friction``, ``requires_tool``, ``occludes``, ``light``,
``map_color`` and ``sounds``; ``components``; item keys other than ``template``,
``max_stack_size``, ``rarity`` and ``crafting_remainder``. The manifest is the minimal one of 1.3
(``id``, ``name``, ``version``, ``api = "0.2"``, ``side = "both"``, ``capabilities =
["registries"]``) plus two optional keys the game reads, ``description`` (escaped for TOML) and an
empty ``authors`` list; every package of R10 loaded with both. Kept as for Sunrise, not
established for Horizon: the asset kind allowlist. Under this target the CLI prints a jar's
written packages even when some of its namespaces were not written, and names those as "not
written" rather than calling the jar refused; a file name byte that is not UTF-8 shows as
``\\xNN``.

The probe is ``--probe``, ``$HORIZON_NATIVE_JAVA`` or a cargo build under ``target/``; without one
the bake refuses the jar rather than writing a package whose items do not exist.

The bake is deterministic: same jar bytes, same package bytes. A jar with an unsafe entry path
(zip-slip) is refused whole and named in the report; nothing is extracted from it. So is a jar the
bake cannot read (a corrupt stream, an unsupported compression method, a name that is not the
UTF-8 it claims, a folder named ``.jar``), for both targets. Under the horizon target nothing a jar
or the probe holds stops the run: the jar is refused with a reason (a jar whose central directory
is over 131,072 entries or 64 MiB too, counted as for packages, ZB1), the next one is baked, and
the report is written. When the probe exits with an error and prints its JSON error report, the
refusal is on one line and names the first entrypoint kind that ran and failed, the exit status,
and that report's stage and error text, the error text cut at 500 characters (PE1; Sunrise's
reason is as it was). Class files, nested jars and executable payloads are never copied, and
every entry left out is in
``config/horizon_java/bake-report.json`` with its reason.
"""

from __future__ import annotations

import argparse
import contextlib
import json
import lzma
import math
import os
import re
import struct
import subprocess
import sys
import tomllib
import zipfile
import zlib
from dataclasses import dataclass, field
from pathlib import Path

# The folder the game reads content packages from, beside `options.txt`
# (sunrise-mod-package `mod_list::DIR`), and the file that turns one on (`FILE`).
MODS_DIR = "sunrise-mods"
MODS_FILE = "sunrise-mods.txt"
REPORT = Path("config") / "horizon_java" / "bake-report.json"


@dataclass(frozen=True)
class Target:
    """The game a package is baked for: where it goes and how it is spelled."""

    name: str
    mods_dir: str  # the folder of packages in the game folder
    mods_file: str  # the file whose `mod.<id>:true` lines turn them on
    extension: str
    manifest: str  # the manifest's name at the top of the archive
    declarations: str  # the folder under `data/<ns>/` the blocks and items are declared in


SUNRISE = Target("sunrise", MODS_DIR, MODS_FILE, ".smod", "sunrise-mod.toml", "sunrise")
# The official game (spec 1.1 L1, L2, L5; 1.3; 3.1 D1).
HORIZON = Target(
    "horizon", "horizon-mods", "horizon-mods.txt", ".hmod", "horizon-mod.toml", "horizon"
)
TARGETS = {target.name: target for target in (SUNRISE, HORIZON)}

# The content fence's path rule: `assets/` or `data/`, then `/`-separated parts of
# `[a-z0-9_.-]`, none of them `.` or `..` (sunrise-mod-package `content::check_path`).
# Known gap of the Sunrise target, kept so its output stays byte for byte what it was: this
# pattern (and `plain_path`, which uses `re.match`) accepts the parts `.` and `..`, a part that
# ends in a dot, and a trailing newline. A jar entry with a `.` or `..` part never gets here
# (`safe_entry` refuses the jar), but a declaration id such as `ns:a/./b` or `ns:a/../b` is
# written to `data/ns/sunrise/...` as it is. Whether Sunrise's own `check_path` refuses that
# package is not checked here. The horizon target checks every part whole (`_horizon_path_problem`).
PLAIN_PART = re.compile(r"^[a-z0-9_.-]+$")
NAMESPACE = re.compile(r"^[a-z0-9_.-]{1,64}$")
# A `.smod` manifest id is narrower than a namespace: `[a-z0-9_]{1,64}` and not a Windows
# device name (sunrise-mod-package `manifest::valid_mod_id`).
PACKAGE_ID = re.compile(r"^[a-z0-9_]{1,64}$")
RESERVED_IDS = {"con", "prn", "aux", "nul"} | {
    f"{prefix}{digit}" for prefix in ("com", "lpt") for digit in "123456789"
}

# What a content package may carry under `assets/<namespace>/`: the pack-layer kinds a Java mod
# ships for its blocks, items and sounds. `sounds.json` is the one file at the namespace root.
ASSET_KINDS = set(
    """
    atlases blockstates equipment font items lang models particles post_effect shaders sounds
    texts textures waypoint_style
    """.split()
)
ASSET_FILES = {"sounds.json"}
ASSET_BLOCKSTATES = "blockstates"

# `data/<namespace>/` kinds the fork reads (vanilla 26.3's data folders, minus `sunrise/`: those
# are content declarations, which the runtime already supplies for this jar).
DATA_KINDS = set(
    """
    advancement banner_pattern cat_sound_variant cat_variant chat_type chicken_sound_variant
    chicken_variant cow_sound_variant cow_variant damage_type decorated_pot_pattern dialog
    dimension_type enchantment enchantment_provider frog_variant instrument item_modifier
    jukebox_song loot_table painting_variant pig_sound_variant pig_variant predicate recipe
    structure tags test_environment test_instance trade_set trial_spawner trim_material
    trim_pattern villager_trade wolf_sound_variant wolf_variant world_clock worldgen
    zombie_nautilus_variant
    """.split()
)

# Vanilla's and the convention tags a mod may add to whichever namespace package it owns.
SHARED_NAMESPACES = {"minecraft", "c"}

# Never copied, whatever folder they sit in: code and executables. The kind allowlist already
# leaves them out; naming them gives the report the real reason.
REFUSED_SUFFIXES = {
    ".class",
    ".jar",
    ".zip",
    ".wasm",
    ".exe",
    ".dll",
    ".dylib",
    ".so",
    ".bat",
    ".cmd",
    ".sh",
    ".py",
}

MAX_FILE = 16 << 20  # the fork's `content::MAX_FILE`
MAX_TOTAL = 512 << 20  # the fork's `content::MAX_TOTAL`
MAX_FILES = 65_536  # the fork's `content::MAX_FILES`

# --- The official game (`--target horizon`): rules of the clean-room package specification. ---

# The names it refuses the whole package for as executables, whatever the content (A10).
HORIZON_REFUSED_SUFFIXES = (
    ".exe", ".dll", ".dylib", ".so", ".bat", ".cmd", ".com", ".ps1", ".jar", ".class", ".wasm"
)
# Sunrise's list also keeps out `.zip`, `.sh` and `.py`. The official game takes them (A10); the
# horizon target keeps them out as Sunrise does, under a reason that says so.
HORIZON_SUNRISE_ONLY_SUFFIXES = (".zip", ".sh", ".py")
# Leading bytes it refuses the whole package for, whatever the file's name (A11): ELF, MZ, `#!`,
# wasm, `CA FE BA BE`, and the three Mach-O magics.
HORIZON_EXECUTABLE_MAGIC = (
    b"\x7fELF",
    b"MZ",
    b"#!",
    b"\x00asm",
    b"\xca\xfe\xba\xbe",
    b"\xcf\xfa\xed\xfe",
    b"\xce\xfa\xed\xfe",
    b"\xfe\xed\xfa\xcf",
)
# The official engine's built-in native mod, always first in its inventory (spec 0). A package of
# that id is not established either way; it is left out.
HORIZON_BUILTIN_IDS = {"horizon_java"}
# The one shared-namespace kind the horizon target copies: vanilla's atlases, which a package may
# add to (N3). Every other `minecraft`, `c` or `horizon` file is data, or an asset that is not a
# mod's (N3-N5).
HORIZON_SHARED_NAMESPACES = SHARED_NAMESPACES | {"horizon"}
# Every `data/` file stays out under the horizon target. Tags, loot tables, advancements,
# predicates, item modifiers, structures and the registry kinds can stop the official game, or
# drop every package's content, through references, cycles and schemas the bake has no tables for
# (spec R3-R5, and the reviews: a missing or recursive loot table, a tag cycle, a predicate the
# bake left out); recipes too: an unknown load condition, an unregistered vanilla serializer or a
# ragged shaped pattern stops it (Round 3, RC1-RC3).
HORIZON_DATA_REASON = (
    "data the bake cannot validate for the official game (references, cycles, schemas): "
    "one bad file can stop it for every mod (R3-R5, Round 3)"
)
HORIZON_JSON_REASON = (
    "not strict JSON: UTF-8 without a byte order mark, no comment, no NaN, no duplicate key, "
    "at most 127 nested arrays and objects (R3, Round 3)"
)
# The deepest nesting the official game's JSON parser takes, counting every array and object
# including the outermost: 127 loaded and 128 did not, for a loot table (round-2 reviews) and a
# recipe (Round 3, RD1).
MAX_JSON_DEPTH = 127
# The longest path part, in bytes, the official game takes: 255 loaded and 256 refused the
# package, for a file and a declaration's entry (Round 3, RP1).
MAX_PART_BYTES = 255
# The game writes each block's generated blockstate to
# `<game folder>/.horizon-cache/mod-content/<16 hex>/assets/<ns>/blockstates/<path>.json`, as long
# as the block's entry `data/<ns>/horizon/block/<path>.json`, and stops for every mod with "File
# name too long (os error 36)" when that is over the system's path limit: on Linux, an entry
# loaded at 4095 - 52 - (the game folder's length) bytes and stopped it one byte longer, for two
# folder lengths (Round 4, RL1). So the bound depends on where the game is installed. The bake
# takes that bound for `--game-dir` and never more than `HORIZON_MAX_ENTRY_BYTES`, so a package
# still loads when the game folder moves to a longer path, or to a system whose limit is lower
# (macOS's is 1024 bytes; not run). The cap applies to every entry the bake writes: item
# entries (8000 bytes) and asset paths (4541) loaded on a server, but no client was run.
PATH_MAX_BYTES = 4095
HORIZON_CACHE_OVERHEAD = 52
HORIZON_MAX_ENTRY_BYTES = 512
# The block states a world holds, every mod's and vanilla's together: more stops the official
# game for every mod ("1084301 block states, more than the 1048576 a world can hold (35723 in the
# game; the mods: …)", round-3 review). A baseline run with no package counts 35723 states and
# 1658 items, so the mods share the rest, 1012853 states (Round 4, RS1). That bound is read from
# the game's own message; no run loaded that many, since a block of 2^17 states already takes
# 2.8 GB in the engine.
HORIZON_WORLD_STATES = 1 << 20
HORIZON_GAME_STATES = 35_723
HORIZON_MOD_STATES = HORIZON_WORLD_STATES - HORIZON_GAME_STATES
# The most blocks the mods may add together: 64250 one-state mod blocks loaded and 64251 stopped
# the game with "Horizon crashed: too many blocks" (Round 4, RS2).
HORIZON_MOD_BLOCKS = 64_250
# What the bake reads of the packages already in `horizon-mods/` to count their blocks: block
# declarations are read one at a time, each at most `MAX_FILE`, and at most this many bytes in
# all over the packages this bake did not write; past it the count is unknown (Round 5, RB3). A
# package this bake wrote (its description says so, `BAKED_DESCRIPTION`) is not held to it: one
# such package may hold up to 512 MiB of declarations (`MAX_TOTAL`, P5), so a bound shared with
# them would make a second run of the same inputs fail on the first run's own output (Round 6,
# RB4). Each of those is read up to `MAX_TOTAL`, which the bake never writes past.
HORIZON_USAGE_BYTES = 64 << 20
# A package or, under the horizon target, a jar whose central directory lists more entries, or
# takes more bytes, than this is not opened: Python's `zipfile` reads the whole directory into
# memory first. The official game refuses a package of more than 65536 files anyway (P5), and
# what the bake writes stays under both (65,537 entries, names of at most 512 bytes).
ZIP_MAX_ENTRIES = 2 * 65_536
ZIP_MAX_DIRECTORY = 64 << 20
HORIZON_UNOWNED_REASON = (
    "assets of a namespace that is neither one of the jar's mod ids nor one its runtime declares "
    "(a package for it would take another mod's id, Round 3)"
)
# The jar files that name its mod ids (fabric.mod.json `id`, `[[mods]] modId` of the NeoForge and
# Forge `mods.toml`).
FABRIC_METADATA = "fabric.mod.json"
TOML_METADATA = ("META-INF/neoforge.mods.toml", "META-INF/mods.toml")

# The keys a declaration may carry (3.2, 3.3, from the game's own refusal messages) and those
# whose rule the specification establishes; a key of the first set only is left out.
HORIZON_BLOCK_KEYS = set(
    """
    template properties shapes collision_shapes outline_context_shapes collision_context_shapes
    interaction_shapes state_light state_properties default_state connects_to thickness wood_type
    wall_block sign_text_color sign_text_glow base_block item open_sound close_sound block_item
    pick_item components
    """.split()
)
HORIZON_BLOCK_CHECKED = set(
    """
    template properties item block_item state_properties default_state shapes collision_shapes
    """.split()
)
HORIZON_BLOCK_PROPERTIES = set(
    """
    hardness explosion_resistance light dynamic_light requires_tool friction map_color sounds
    random_ticks render_shape push_reaction occludes destroy_overlay
    """.split()
)
HORIZON_PROPERTY_BOOLEANS = {"requires_tool", "occludes"}
HORIZON_PROPERTY_RANGES = {"light": 15, "map_color": 61}
HORIZON_ITEM_KEYS = set(
    "template max_stack_size rarity components crafting_remainder block arm_pose".split()
)
HORIZON_ITEM_CHECKED = set("template max_stack_size rarity crafting_remainder".split())
HORIZON_RARITIES = {"common", "uncommon", "rare", "epic"}
HORIZON_SHAPE_KEYS = {"property", "properties", "values", "default"}
# The keys of a state property definition the game reads (Round 4, SP1).
STATE_DEFINITION_KEYS = {"boolean": {"type"}, "enum": {"type", "values"}}
STATE_NAME = re.compile(r"^[a-z0-9_]+$")
# The longest state property name and enum value the bake writes. The game takes each up to 65535
# bytes and stops for every mod at 65536 ("Horizon crashed: …/occlusion.nbt: NBT at byte …: table
# state identities differ"; values: round-5 review `lv65535`/`lv65536`; names: Round 6 `n65535`,
# `n65536`), each string on its own (two values of 40000 bytes loaded). The bake's bound is a
# conservative 255 bytes, not the game's: the engine's memory grows with these strings, and a block
# of 65536 states whose 16 names and 32 values are 255 bytes each was killed at the 4 GB cap of the
# test servers (Round 6 `mp`, not established past that).
STATE_STRING_MAX_BYTES = 255
# The most states one block may have: a block of two enums of 256 values (65536 states) loaded,
# and 256 by 257 (65792), 17 booleans (131072) and one enum of 65537 values each stopped the game
# with "Horizon crashed: block <id>: TooManyStates" (Round 4, RS3). The specification's 2^20
# (3.2, `sem7`) is the bound of an earlier check with its own message.
MAX_STATES = 1 << 16
MAX_BOXES = 16
F32_MAX = 3.4028234663852886e38

# The block templates the horizon target takes: the two decaf's runtime infers (lib.rs
# `INFERRED_BLOCK_TEMPLATE`, and `minecraft:crafting_table` for a vanilla `CraftingTableBlock`),
# with their own state properties: `minecraft:stone` is a plain `Block` and
# `minecraft:crafting_table` a `CraftingTableBlock`, neither of which declares
# `createBlockStateDefinition` (`vanilla_block_state_definers.rs`). Both loaded in the official
# game (R9). Any other template (a jar's own native metadata can name one) is one the bake cannot
# check: decaf has no table of every 26.3 block (wool and copper blocks are not `Blocks`
# constants) nor of the templates' state properties, and the game stops on an unknown one.
TEMPLATE_STATE_PROPERTIES: dict[str, frozenset[str]] = {
    "minecraft:stone": frozenset(),
    "minecraft:crafting_table": frozenset(),
}
# Vanilla 26.3's `Blocks` constants with their `Block.asItem()` ids, its registered item ids and
# the prototype `max_damage` of its damageable items, all captured with Java 25 against the real
# server classes.
VANILLA_TABLES = Path(__file__).resolve().parents[1] / "crates" / "horizon-native-java" / "src"
VANILLA_BLOCK_ITEMS = VANILLA_TABLES / "vanilla_block_items_java25.tsv"
VANILLA_ITEMS = VANILLA_TABLES / "vanilla_item_ids_java25.txt"
VANILLA_ITEM_DATA = VANILLA_TABLES / "vanilla_item_data.rs"
VANILLA_MAX_DAMAGE = re.compile(
    r"^const VANILLA_MAX_DAMAGE: &\[\(&str, i32\)\] = &\[\n(.*?)^\];", re.M | re.S
)
VANILLA_MAX_DAMAGE_ROW = re.compile(r'^\s*\("(minecraft:[a-z0-9_]+)", \d+\),$', re.M)

# The manifest description of a package this tool baked; a package whose description reads so
# for its own id is one the bake may replace or name as stale (R2).
# Since Round 5 the description also names the jar's mod ids (those shaped as a namespace), so a
# later run can tell whose package it is; a package of an earlier round has only the jar's name.
BAKED_DESCRIPTION = re.compile(
    r"^Assets and data baked from (.*) for the ([a-z0-9_]{1,64}) namespace\."
    r"(?: Mod ids of the jar: (none|[a-z0-9_.-]{1,64}(?:, [a-z0-9_.-]{1,64})*)\.)?$",
    re.S,
)

# The runtime probe (`crates/horizon-native-java`) runs a jar's entrypoints offline and prints its
# typed blocks and items as content declarations. The bake never re-implements the interpreter.
PROBE_ENV = "HORIZON_NATIVE_JAVA"
PROBE = "horizon-native-java"
SERVER_KINDS = ("main", "server", "neoforge")

# The fixed entry timestamp: a package's bytes must not depend on when the bake ran.
ZIP_TIME = (1980, 1, 1, 0, 0, 0)
ZIP_MODE = 0o644 << 16


# What reading a jar can raise besides `zipfile.BadZipFile` (see `_zip_entries`).
JAR_READ_ERRORS = (
    zipfile.LargeZipFile,
    zlib.error,
    lzma.LZMAError,
    NotImplementedError,
    ValueError,
    OSError,
    EOFError,
    struct.error,
)


def _error_name(error: BaseException) -> str:
    """`zlib.error`, `NotImplementedError`: an exception's type as Python spells it."""
    kind = type(error)
    return kind.__name__ if kind.__module__ == "builtins" else f"{kind.__module__}.{kind.__name__}"


class UnsafeJar(Exception):
    """A jar entry could escape the package (zip-slip) or is not a plain file."""


class ProbeError(Exception):
    """The offline runtime could not say what content a jar declares."""


@dataclass
class Skipped:
    """One reason entries were left out of a jar's bake, with a few example paths."""

    reason: str
    count: int = 0
    paths: list[str] = field(default_factory=list)

    def add(self, path: str) -> None:
        self.count += 1
        if len(self.paths) < 8:
            self.paths.append(path)

    def as_json(self) -> dict:
        return {"reason": self.reason, "count": self.count, "paths": self.paths}


@dataclass
class Package:
    id: str
    namespace: str
    path: Path
    files: list[str]
    bytes: int
    blocks: int = 0
    items: int = 0

    def as_json(self, game_dir: Path) -> dict:
        return {
            "id": self.id,
            "namespace": self.namespace,
            "path": _relative(self.path, game_dir),
            "files": len(self.files),
            "bytes": self.bytes,
            "declarations": {"blocks": self.blocks, "items": self.items},
        }


@dataclass
class JarBake:
    jar: str
    packages: list[Package] = field(default_factory=list)
    skipped: dict[str, Skipped] = field(default_factory=dict)
    error: str | None = None
    namespaces: list[str] = field(default_factory=list)

    def skip(self, reason: str, path: str) -> None:
        self.skipped.setdefault(reason, Skipped(reason)).add(path)

    def fail(self, error: str) -> None:
        """Adds an error, keeping the earlier ones (horizon; Sunrise keeps only the last)."""
        self.error = error if self.error is None else f"{self.error}; {error}"

    def as_json(self, game_dir: Path) -> dict:
        row = {
            "jar": self.jar,
            "namespaces": self.namespaces,
            "packages": [package.as_json(game_dir) for package in self.packages],
            "skipped": [skipped.as_json() for skipped in self.skipped.values()],
            "error": self.error,
        }
        return row


@dataclass
class BakeReport:
    java_mods: Path
    game_dir: Path
    out: Path
    jars: list[JarBake] = field(default_factory=list)
    enabled: list[str] = field(default_factory=list)
    target: Target = SUNRISE
    # Packages an earlier bake wrote that this run left in place (horizon only, so a Sunrise
    # report keeps its bytes).
    stale: list[dict] = field(default_factory=list)
    # Errors of the run itself, not of one jar (horizon only).
    errors: list[str] = field(default_factory=list)

    @property
    def failed(self) -> bool:
        return bool(self.errors) or any(jar.error for jar in self.jars)

    def as_json(self) -> dict:
        report = {
            "schema": 1,
            "java_mods": _relative(self.java_mods, self.game_dir),
            "out": _relative(self.out, self.game_dir),
            "enabled": self.enabled,
            "jars": [jar.as_json(self.game_dir) for jar in self.jars],
        }
        if self.target is not SUNRISE:
            # Named only off the default, so a Sunrise report keeps its bytes.
            report["target"] = self.target.name
            report["stale"] = self.stale
            report["errors"] = self.errors
        return report


def _relative(path: Path, base: Path) -> str:
    try:
        return path.resolve().relative_to(base.resolve()).as_posix()
    except ValueError:
        return path.as_posix()


def safe_entry(name: str) -> bool:
    """Whether a zip entry name is a plain relative path: no absolute path, no `..`, no `\\`."""
    if not name or "\x00" in name or "\\" in name:
        return False
    if name.startswith("/") or (len(name) > 1 and name[1] == ":"):
        return False
    parts = name.rstrip("/").split("/")
    return all(part not in ("", ".", "..") for part in parts)


def plain_path(name: str) -> bool:
    """The fork's content path rule (`content::check_path`), without the `assets|data` prefix."""
    return all(PLAIN_PART.match(part) for part in name.split("/"))


def _refused_suffix(name: str) -> bool:
    lower = name.lower()
    return any(lower.endswith(suffix) for suffix in REFUSED_SUFFIXES)


def _horizon_refused_name(name: str) -> str | None:
    """Why the horizon target leaves a file out by its name, or None."""
    lower = name.lower()
    if lower.endswith(HORIZON_REFUSED_SUFFIXES):
        return "an executable name the official game refuses the package for (A10)"
    if lower.endswith(HORIZON_SUNRISE_ONLY_SUFFIXES):
        return "code or an archive, kept out as for Sunrise (the official game would take it, A10)"
    return None


def _horizon_path_problem(name: str, max_bytes: int | None = None) -> str | None:
    """Why the official game refuses a package with this entry name, part by part, or None: no
    empty, `.` or `..` part (A5), none ending in a dot or space (R1), each `[a-z0-9_.-]` matched
    whole, so a trailing newline is not one (P1), at most 255 bytes (RP1), and no Windows device
    name before the part's first dot: `con`, `prn`, `aux`, `nul`, `com1`-`com9`, `lpt1`-`lpt9`
    (RP2; `com0`, `com10`, `conx`, `con_` and `con-x` loaded). With `max_bytes`, the whole name
    is at most that long (RL1, `horizon_max_entry`)."""
    if max_bytes is not None and len(name.encode("utf-8")) > max_bytes:
        return (
            f"an entry name over {max_bytes} bytes (the bake's cap on every entry: a block's "
            "cache path past the system's path limit stops the official game for every mod, RL1)"
        )
    for part in name.split("/"):
        if part in ("", ".", ".."):
            return "a path part is empty, . or .. (the official game refuses the package, A5)"
        if part.endswith((".", " ")):
            return "a path part ends in a dot or space (the official game refuses the package, R1)"
        if not PLAIN_PART.fullmatch(part):
            return "not a plain content path"
        if len(part.encode("utf-8")) > MAX_PART_BYTES:
            return "a path part over 255 bytes (the official game refuses the package, RP1)"
        if part.split(".", 1)[0] in RESERVED_IDS:
            return (
                "a path part is a Windows device name (the official game refuses the package, RP2)"
            )
    return None


def _is_symlink(info: zipfile.ZipInfo) -> bool:
    return (info.external_attr >> 16) & 0o170000 == 0o120000


def _zip_entries(jar: Path, bake: JarBake, bounded: bool = False) -> list[tuple[str, bytes]]:
    """Reads the jar's plain files, refusing the whole jar on an unsafe entry.

    Unsafe means a path that could escape the package (`..`, absolute, a drive letter, a
    backslash, NUL) or an entry that is not a regular file (a symlink). Refusing the jar whole is
    deliberate: a jar that tries any of these is not one to extract half of. With `bounded` (the
    horizon target) a jar whose central directory is over `ZIP_MAX_ENTRIES` entries or
    `ZIP_MAX_DIRECTORY` bytes is refused before Python's `zipfile` reads it whole (Round 6).
    """
    entries: list[tuple[str, bytes]] = []
    total = 0
    try:
        try:
            opened = _open_zip(jar) if bounded else zipfile.ZipFile(jar)
        except ZipTooLarge as error:
            raise UnsafeJar(f"{error} (Python's zipfile reads it whole; not opened)") from error
        with opened as archive:
            for info in archive.infolist():
                name = info.filename
                if not safe_entry(name):
                    raise UnsafeJar(f"unsafe entry path: {name!r}")
                if _is_symlink(info):
                    raise UnsafeJar(f"symlink entry: {name!r}")
                if info.is_dir():
                    continue
                if info.flag_bits & 0x1:
                    raise UnsafeJar(f"encrypted entry: {name!r}")
                if info.file_size > MAX_FILE:
                    bake.skip("over the content file cap (16 MiB)", name)
                    continue
                total += info.file_size
                if total > MAX_TOTAL:
                    raise UnsafeJar("content over the package total cap (512 MiB)")
                entries.append((name, archive.read(info)))
    except zipfile.BadZipFile as error:
        raise UnsafeJar(f"not a readable jar: {error}") from error
    except JAR_READ_ERRORS as error:
        # A corrupt deflate stream (`zlib.error`), a compression method zipfile lacks
        # (`NotImplementedError`), a name flagged UTF-8 that is not (`UnicodeDecodeError`), a jar
        # that is a folder or cannot be opened (`OSError`), a stream that ends early (`EOFError`).
        # These used to stop the whole run with a traceback, for Sunrise too; refusing the jar
        # changes nothing for a jar that did not raise.
        raise UnsafeJar(f"not a readable jar: {_error_name(error)}: {error}") from error
    if len(entries) > MAX_FILES:
        raise UnsafeJar("more files than the content file cap")
    return entries


def _classify(name: str) -> tuple[str, str, str] | None:
    """`assets|data`, namespace, and the path under it, for a name inside a content tree."""
    parts = name.split("/")
    if len(parts) < 3 or parts[0] not in ("assets", "data"):
        return None
    return parts[0], parts[1], "/".join(parts[2:])


def _allowed(top: str, namespace: str, rest: str, path: str, bake: JarBake) -> bool:
    if namespace in SHARED_NAMESPACES:
        # Only tag additions and vanilla atlas entries are a mod's to make in a shared namespace
        # (`content::check_namespace`); everything else stays vanilla's.
        if top == "data" and rest.startswith("tags/"):
            return True
        if top == "assets" and namespace == "minecraft" and rest.startswith("atlases/"):
            return True
        bake.skip("vanilla's namespace: only tag and atlas additions are a mod's", path)
        return False
    if not NAMESPACE.match(namespace):
        bake.skip("not a content namespace ([a-z0-9_.-]{1,64})", path)
        return False
    kind, _, _ = rest.partition("/")
    if top == "assets":
        if kind in ASSET_KINDS or rest in ASSET_FILES:
            return True
        bake.skip("unsupported asset kind", path)
        return False
    # Sunrise reads declarations from `data/<ns>/sunrise/`; a jar written for Horizon may still
    # carry `data/<ns>/horizon/`. Neither is copied: the runtime's declarations are the package's.
    if kind in ("sunrise", "horizon"):
        bake.skip("content declarations: the runtime declares this jar's blocks and items", path)
        return False
    if kind in DATA_KINDS:
        return True
    bake.skip("unsupported data kind", path)
    return False


def _custom_recipe_type(top: str, rest: str, data: bytes) -> str | None:
    """The `type` of a recipe file whose serializer is not vanilla's, else None.

    A type without a namespace is vanilla's (`crafting_shaped` is `minecraft:crafting_shaped`).
    A file that is not a recipe, or not JSON with a string `type`, is left to the game."""
    if top != "data" or not rest.startswith("recipe/") or not rest.endswith(".json"):
        return None
    try:
        kind = json.loads(data).get("type")
    except (ValueError, AttributeError):
        return None
    if not isinstance(kind, str) or ":" not in kind:
        return None
    return None if kind.split(":", 1)[0] == "minecraft" else kind


def _toml_string(text: str) -> str:
    """A TOML basic string: a quote, a backslash or a control character in a jar's file name would
    otherwise break the manifest, and the official game skips a package whose manifest does not
    parse without a word (1.3, TOML syntax error)."""
    out = []
    for char in text:
        code = ord(char)
        if char in '"\\':
            out.append("\\" + char)
        elif code < 0x20 or code == 0x7F:
            out.append(f"\\u{code:04x}")
        elif 0xD800 <= code <= 0xDFFF:
            out.append("\ufffd")  # a file name byte that is not UTF-8
        else:
            out.append(char)
    return '"' + "".join(out) + '"'


def _recorded_ids(mod_ids: frozenset[str] | set[str] | None) -> frozenset[str]:
    """The mod ids a horizon package's description records: those shaped as a namespace (only
    those can own one), so the description reads back unambiguously."""
    return frozenset(mod_id for mod_id in mod_ids or () if NAMESPACE.fullmatch(mod_id))


def _shown_name(name: str) -> str:
    """A jar's file name as the manifest's description reads back (`_toml_string`)."""
    return "".join("\ufffd" if 0xD800 <= ord(char) <= 0xDFFF else char for char in name)


def _manifest(
    namespace: str,
    jar: str,
    target: Target = SUNRISE,
    mod_ids: frozenset[str] | None = None,
    namespaces: list[str] | None = None,
) -> str:
    if target is HORIZON:
        # The minimal accepted manifest of 1.3, every key one the game reads. The description
        # records the jar and its mod ids: whose package this is (`_existing_conflict`).
        ids = ", ".join(sorted(_recorded_ids(mod_ids))) or "none"
        description = _toml_string(
            f"Assets and data baked from {jar} for the {namespace} namespace. "
            f"Mod ids of the jar: {ids}."
        )
        return (
            f'id = "{namespace}"\n'
            f'name = "{namespace}"\n'
            'version = "1.0.0"\n'
            f"description = {description}\n"
            "authors = []\n"
            'api = "0.2"\n'
            'side = "both"\n'
            'capabilities = ["registries"]\n'
        )
    if namespaces is not None:
        # A package per loaded mod (`_mod_packages`): its id is the mod's, and the jar's other
        # namespaces are listed so the package may carry their files (`content::check_namespace`).
        carried = f", with the namespaces {', '.join(namespaces)}" if namespaces else ""
        description = f"Assets and data baked from {jar} for the mod {namespace}{carried}."
        listed = (
            f"namespaces = [{', '.join(_toml_string(name) for name in namespaces)}]\n"
            if namespaces
            else ""
        )
        return (
            f'id = "{namespace}"\n'
            f'name = "{namespace}"\n'
            'version = "1.0.0"\n'
            f"description = {_toml_string(description)}\n"
            "authors = []\n"
            'api = "0.2"\n'
            'side = "both"\n'
            'capabilities = ["registries"]\n'
            f"{listed}"
        )
    return (
        f'id = "{namespace}"\n'
        f'name = "{namespace}"\n'
        'version = "1.0.0"\n'
        f'description = "Assets and data baked from {jar} for the {namespace} namespace."\n'
        "authors = []\n"
        'api = "0.2"\n'
        'side = "both"\n'
        'capabilities = ["registries"]\n'
    )


def find_probe(explicit: Path | None) -> Path | None:
    """The runtime probe: the flag, `$HORIZON_NATIVE_JAVA`, or a cargo build in this checkout."""
    if explicit is not None:
        return explicit if explicit.is_file() else None
    configured = os.environ.get(PROBE_ENV)
    if configured:
        path = Path(configured)
        if path.is_file():
            return path
    for folder in ("target/release", "target/debug", ".local/parity-target/release", ".local/parity-target/debug"):
        path = Path(folder) / PROBE
        if path.is_file():
            return path
    return None


def probe_declarations(
    probe: Path, jar: Path, malformed: list[str] | None = None
) -> list[tuple[str, str, dict]]:
    """Runs the jar's server entrypoints offline and returns its (kind, id, content file) blocks
    and items. The probe is the same runtime the mod vendors, so what it records is exactly what
    the mod would have declared at Registration.

    With `malformed` (the horizon target), output the bake cannot read never raises past here:
    output that is not UTF-8, not JSON (nested too deep for Python's parser included) or not an
    object is a failed run, a `content` that is not an object refuses the jar, and each row that
    is not an object with a string `id` and a `file` is left out and named in `malformed`.
    Without it (Sunrise) the reading is exactly what it always was."""
    last = "no output"
    ran: str | None = None  # the horizon target's first entrypoint that ran and failed
    for kind in SERVER_KINDS:
        try:
            if malformed is None:
                run = subprocess.run(
                    [str(probe), "--json", str(jar), kind],
                    capture_output=True,
                    text=True,
                    timeout=120,
                )
                stdout, stderr = run.stdout, run.stderr
            else:
                run = subprocess.run(
                    [str(probe), "--json", str(jar), kind], capture_output=True, timeout=120
                )
                stderr = run.stderr.decode("utf-8", "replace")
                try:
                    stdout = run.stdout.decode("utf-8")
                except UnicodeDecodeError:
                    last = "output that is not UTF-8"
                    continue
        except (OSError, subprocess.TimeoutExpired) as error:
            raise ProbeError(str(error)) from error
        lines = [line for line in stdout.splitlines() if line.strip()]
        if run.returncode != 0 or not lines:
            errors = [line for line in stderr.splitlines() if line.strip()]
            last = errors[-1] if errors else f"exit status {run.returncode}"
            if malformed is not None and lines:
                failure = _probe_failure(lines[-1])
                if failure is not None:
                    # The runtime reports a mod's failure on stdout and exits 1 (Round 7, PE1).
                    last = f"{kind}: exit status {run.returncode}: {failure[1]}"
                    if failure[0] != "discover" and ran is None:
                        ran = last  # an entrypoint ran and failed: the reason worth naming
            continue
        try:
            report = json.loads(lines[-1])
        except json.JSONDecodeError as error:
            last = f"not JSON: {error}"
            continue
        except (RecursionError, ValueError) as error:
            # Nested too deep for Python's parser, or an integer literal over Python's
            # int-string conversion limit (4300 digits), which is a plain `ValueError`.
            if malformed is None:
                raise
            last = (
                "not JSON: nested too deep"
                if isinstance(error, RecursionError)
                else f"not JSON: {error}"
            )
            continue
        if malformed is not None and not isinstance(report, dict):
            last = "not a JSON object"
            continue
        if report.get("status") != "ok":
            last = str(report.get("error", report.get("status")))
            continue
        if not report.get("entrypoints"):
            last = "no server entrypoint"
            continue
        content = report.get("content") or {}
        if malformed is not None and not isinstance(content, dict):
            raise ProbeError("the probe's content is not a JSON object")
        declarations = []
        # `menus` are the jar's own menu types as Sunrise content menu declarations
        # (`data/<ns>/sunrise/menu/<path>.json`, the layout of the menu each `MenuType` builds);
        # `menu_refusals` the ones whose layout can't be one exactly, kept as `menu-refused`
        # rows (`{"reason": ...}`) so the bake names them.
        for name, key in (
            ("block", "blocks"),
            ("item", "items"),
            ("menu", "menus"),
            ("menu-refused", "menu_refusals"),
        ):
            rows = content.get(key, [])
            if malformed is not None and not isinstance(rows, list):
                malformed.append(f"{key}: {str(rows)[:80]}")
                continue
            field = "reason" if name == "menu-refused" else "file"
            for entry in rows:
                if malformed is not None and not (
                    isinstance(entry, dict) and isinstance(entry.get("id"), str) and field in entry
                ):
                    malformed.append(str(entry)[:80])
                    continue
                if name == "menu-refused":
                    declarations.append((name, entry["id"], {"reason": entry["reason"]}))
                    continue
                declarations.append((name, entry["id"], entry["file"]))
        if malformed is None:
            # `mods` (Sunrise only): the Fabric/NeoForge mods the runtime loaded from this jar,
            # the jar's own first, then each nested jar it selected (`fabric.mod.json` `jars`,
            # JarJar) and every `provides` alias, each id once. They are exactly the ids it answered `isModLoaded` true
            # for, so the bake makes one package for each and no other (`_mod_packages`): a
            # `mod_loaded` load condition then reads the same on Sunrise as in the jar's code.
            # A probe that names none (an older runtime, the golden test's stand-in) leaves the
            # bake as it was, one package per namespace. They come back as `mod` rows.
            mods = content.get("mods", [])
            for mod_id in mods if isinstance(mods, list) else []:
                if isinstance(mod_id, str):
                    declarations.append(("mod", mod_id, {}))
        return declarations
    raise ProbeError(f"no server entrypoint ran ({ran or last})")


PROBE_ERROR_MAX = 500


def _probe_failure(line: str) -> tuple[str, str] | None:
    """The stage and text of the runtime's JSON error report (`{"status": "error", "stage": ...,
    "error": ...}`) on a line of its output, the text on one line of at most `PROBE_ERROR_MAX`
    characters, or None when the line is no such report."""
    try:
        report = json.loads(line)
    except (RecursionError, ValueError):
        return None
    if not isinstance(report, dict) or report.get("status") != "error" or "error" not in report:
        return None

    def one_line(value: object, bound: int) -> str:
        text = " ".join("".join(c if c.isprintable() else " " for c in str(value)).split())
        return text if len(text) <= bound else text[:bound] + "..."

    try:
        stage = one_line(report.get("stage"), 40)
        return stage, f"{stage}: {one_line(report['error'], PROBE_ERROR_MAX)}"
    except RecursionError:
        return None


def write_package(
    path: Path,
    namespace: str,
    jar: str,
    files: list[tuple[str, bytes]],
    target: Target = SUNRISE,
    mod_ids: frozenset[str] | None = None,
    namespaces: list[str] | None = None,
) -> int:
    """Writes a deterministic package (a zip: the manifest then the sorted files)."""
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + ".tmp")
    with zipfile.ZipFile(temporary, "w", zipfile.ZIP_DEFLATED) as archive:
        manifest = _manifest(namespace, jar, target, mod_ids, namespaces)
        info = zipfile.ZipInfo(target.manifest, ZIP_TIME)
        info.external_attr = ZIP_MODE
        info.compress_type = zipfile.ZIP_DEFLATED
        archive.writestr(info, manifest.encode("utf-8"))
        for name, data in sorted(files):
            info = zipfile.ZipInfo(name, ZIP_TIME)
            info.external_attr = ZIP_MODE
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, data)
    os.replace(temporary, path)
    return path.stat().st_size


def _declaration_bytes(file: dict) -> bytes:
    """The declaration file, deterministic (sorted keys, compact) and newline-terminated."""
    return json.dumps(file, sort_keys=True, separators=(",", ":")).encode() + b"\n"


def _executable_content(data: bytes) -> bool:
    """Whether a file starts like an executable the official game refuses (A11)."""
    return data.startswith(HORIZON_EXECUTABLE_MAGIC)


class _NotStrict(ValueError):
    """A JSON text Python reads but a strict parser does not."""


def _strict_json(data: bytes) -> tuple[object, bool]:
    """The value of a strict JSON file and True, or (None, False) for one that is not: a UTF-8
    byte order mark, bytes that are not UTF-8, a syntax error (a comment, a trailing comma,
    anything after the value), `NaN` or `Infinity`, a number out of the `f64` range, a lone
    surrogate escape, a duplicate key, or more than `MAX_JSON_DEPTH` arrays and objects nested
    in one another, the outermost included (R3, RD1)."""
    if data.startswith(b"\xef\xbb\xbf"):
        return None, False

    def refuse_constant(name: str) -> object:
        raise _NotStrict(name)

    def finite(text: str) -> float:
        value = float(text)
        if not math.isfinite(value):
            raise _NotStrict(text)
        return value

    def unique(pairs: list[tuple[str, object]]) -> dict:
        keys = [key for key, _ in pairs]
        if len(set(keys)) != len(keys):
            raise _NotStrict("duplicate key")
        return dict(pairs)

    try:
        text = data.decode("utf-8")
        value = json.loads(
            text, parse_constant=refuse_constant, parse_float=finite, object_pairs_hook=unique
        )
    except (ValueError, RecursionError):
        # `UnicodeDecodeError` and `json.JSONDecodeError` are `ValueError`s.
        return None, False
    # Each item with the number of arrays and objects it is in, itself included when it is one.
    stack: list[tuple[object, int]] = [(value, 1)]
    while stack:
        item, depth = stack.pop()
        if isinstance(item, (dict, list)) and depth > MAX_JSON_DEPTH:
            return None, False
        try:
            if isinstance(item, str):
                item.encode("utf-8")
            elif isinstance(item, bool):
                pass
            elif isinstance(item, int):
                float(item)
            elif isinstance(item, dict):
                for key, child in item.items():
                    key.encode("utf-8")
                    stack.append((child, depth + 1))
            elif isinstance(item, list):
                stack.extend((child, depth + 1) for child in item)
        except (UnicodeEncodeError, OverflowError):
            return None, False
    return value, True


@dataclass(frozen=True)
class Vanilla:
    """Vanilla 26.3's block ids with their `Block.asItem()` ids, its item ids, and the items
    whose prototype has `max_damage` (damageable)."""

    blocks: dict[str, str]
    items: frozenset[str]
    damageable: frozenset[str] = frozenset()


_VANILLA: list[Vanilla | None] = []


def vanilla_tables() -> Vanilla | None:
    """The vanilla tables a declaration is checked against, None when one is missing (the
    horizon target then declares nothing rather than guess)."""
    if not _VANILLA:
        try:
            rows = VANILLA_BLOCK_ITEMS.read_text(encoding="utf-8").splitlines()
            items = VANILLA_ITEMS.read_text(encoding="utf-8").split()
            item_data = VANILLA_ITEM_DATA.read_text(encoding="utf-8")
        except OSError:
            _VANILLA.append(None)
        else:
            blocks = {}
            for row in rows:
                block, _, item = row.partition("\t")
                if block and item:
                    blocks[block] = item
            table = VANILLA_MAX_DAMAGE.search(item_data)
            damageable = VANILLA_MAX_DAMAGE_ROW.findall(table.group(1)) if table else []
            if damageable:
                _VANILLA.append(Vanilla(blocks, frozenset(items), frozenset(damageable)))
            else:
                _VANILLA.append(None)
    return _VANILLA[0]


def _number(value: object) -> bool:
    """A JSON number the game reads as an `f32`: finite and in `f32` range. NaN and the
    infinities fail the comparison, and a huge integer compares exactly, without a conversion
    that could overflow."""
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        return False
    return -F32_MAX <= value <= F32_MAX


# Each number property, its rule and the bound the official game refuses the content outside
# of (R6): `friction` 0 to 2, `hardness` 0 or more or exactly -1, `explosion_resistance` 0 or
# more; 1e39 is refused as not a number.
HORIZON_PROPERTY_NUMBERS = {
    "hardness": (
        lambda value: value == -1 or value >= 0,
        "block property hardness is not a finite f32 of 0 or more, or -1 (refused, R6)",
    ),
    "explosion_resistance": (
        lambda value: value >= 0,
        "block property explosion_resistance is not a finite f32 of 0 or more (refused, R6)",
    ),
    "friction": (
        lambda value: 0 <= value <= 2,
        "block property friction is not a number from 0 to 2 (refused, R6)",
    ),
}


def _boxes_problem(boxes: object) -> str | None:
    """Why the official game refuses a shape's boxes (3.2 `shapes`), or None."""
    reason = "block shape of over 16 boxes or a box not 6 ordered finite numbers (refused, 3.2)"
    if not isinstance(boxes, list) or len(boxes) > MAX_BOXES:
        return reason
    for box in boxes:
        if not isinstance(box, list) or len(box) != 6 or not all(_number(value) for value in box):
            return reason
        if any(box[axis] > box[axis + 3] for axis in range(3)):
            return reason
    return None


def _property_problem(
    name: str, declared: dict, template_properties: frozenset | None, what: str
) -> str | None:
    """Why `name`, read by a default state or a shape, cannot be checked or would crash the game."""
    if name in declared:
        return None
    if template_properties is None or name in template_properties:
        # A template property's values are not in the bake's tables: nothing to check it by.
        return f"{what} names a template property the bake does not know (left out)"
    return f"{what} names a property the block lacks (fatal in the official game, 3.2)"


def _shape_problem(
    table: object, declared: dict, template_properties: frozenset | None
) -> str | None:
    """Why the official game refuses or crashes on a `shapes`/`collision_shapes` table (3.2)."""
    if not isinstance(table, dict):
        return "block shape table is not an object (refused, 3.2)"
    if set(table) - HORIZON_SHAPE_KEYS:
        return "block shape table key the official game refuses (unknown field, 3.2)"
    if "property" in table and "properties" in table:
        return "block shape table with both property and properties (not established, left out)"
    names = [table["property"]] if "property" in table else table.get("properties", [])
    if not isinstance(names, list) or not all(isinstance(name, str) for name in names):
        return "block shape properties is not a list of names (refused, 3.2)"
    if len(set(names)) != len(names):
        return "block shape names a property twice (not established, left out)"
    for name in names:
        problem = _property_problem(name, declared, template_properties, "block shape")
        if problem:
            return problem
    values = table.get("values", {})
    if not isinstance(values, dict):
        return "block shape values is not an object (refused, 3.2)"
    if not names and (values or "default" not in table):
        return "block shape table without properties, with values or no default (not established)"
    for key, boxes in values.items():
        parts = key.split(",")
        if len(parts) != len(names) or any(
            part not in declared[name] for part, name in zip(parts, names)
        ):
            return "block shape tuple of no state of the block (fatal in the official game, 3.2)"
        problem = _boxes_problem(boxes)
        if problem:
            return problem
    if "default" in table:
        return _boxes_problem(table["default"])
    if len(values) < math.prod(len(declared[name]) for name in names):
        return "block shape leaves a state uncovered, no default (fatal in the official game, 3.2)"
    return None


STATE_STRING_REASON = (
    "state property name or enum value over 255 bytes (the bake's bound; the official game stops "
    "for every mod past 65535, Round 6)"
)


def _horizon_block_problem(file: object, vanilla: Vanilla) -> str | None:
    """Why the official game refuses, or stops on, a block declaration (3.2, R6, R7, R9), or why
    the bake cannot tell; None when the file is one it loads."""
    if not isinstance(file, dict):
        return "block declaration is not a JSON object (refused, 3.1)"
    for key in sorted(file):
        if key not in HORIZON_BLOCK_KEYS:
            return "block key the official game refuses (unknown field, 3.2)"
        if key not in HORIZON_BLOCK_CHECKED:
            return f"block key {key} (not established, left out)"
    template = file.get("template")
    if not isinstance(template, str) or template not in TEMPLATE_STATE_PROPERTIES:
        # A name without `minecraft:` refuses the content and an unknown vanilla name stops the
        # game (3.2); the bake checks only the templates decaf's runtime infers (R9).
        return (
            "block template the bake cannot check "
            "(it takes minecraft:stone and minecraft:crafting_table, R9)"
        )
    item = file.get("item", True)
    if not isinstance(item, bool):
        return "block item is not a boolean (refused, 3.2)"
    if template not in vanilla.items or vanilla.blocks.get(template) != template:
        # Both templates have an item of their own id in the capture; without one the game stops
        # on a block that has its item (`minecraft:air`, 3.2).
        return "block template without an item of its own id in the vanilla tables (left out)"
    if "block_item" in file:
        block_item = file["block_item"]
        if item:
            return "block_item on a block that has its own item (not established, left out)"
        if block_item == "minecraft:air":
            return "block_item minecraft:air (not established, left out)"
        if not isinstance(block_item, str) or block_item not in vanilla.items:
            return "block_item is not a vanilla item (fatal in the official game, 3.2)"
    properties = file.get("properties", {})
    if not isinstance(properties, dict):
        return "block properties is not an object (refused, 3.2)"
    for key, value in sorted(properties.items()):
        if key not in HORIZON_BLOCK_PROPERTIES:
            return "block property the official game refuses (unknown field, 3.2)"
        if key in HORIZON_PROPERTY_NUMBERS:
            accepted, reason = HORIZON_PROPERTY_NUMBERS[key]
            if not _number(value) or not accepted(value):
                return reason
        elif key in HORIZON_PROPERTY_BOOLEANS:
            if not isinstance(value, bool):
                return f"block property {key} is not a boolean (refused, 3.2)"
        elif key in HORIZON_PROPERTY_RANGES:
            top = HORIZON_PROPERTY_RANGES[key]
            if isinstance(value, bool) or not isinstance(value, int) or not 0 <= value <= top:
                return f"block property {key} is not an integer 0 to {top} (refused, 3.2, R6)"
        elif key == "sounds":
            if not isinstance(value, str) or value not in vanilla.blocks:
                # An unknown block stops the game (3.2); the 878 of the table all load (R7).
                return (
                    "block sounds the bake cannot check "
                    "(not one of the 878 vanilla blocks it checked, R7)"
                )
        else:
            return f"block property {key} (not established, left out)"
    state_properties = file.get("state_properties", {})
    if not isinstance(state_properties, dict):
        return "block state_properties is not an object (refused, 3.2)"
    declared: dict[str, set[str]] = {}
    for name, definition in state_properties.items():
        if not STATE_NAME.fullmatch(name):
            return "state property name is not [a-z0-9_] (refused, 3.2)"
        if len(name) > STATE_STRING_MAX_BYTES:
            return STATE_STRING_REASON
        if not isinstance(definition, dict):
            return "state property definition is not an object (refused, 3.2)"
        kind = definition.get("type")
        if not isinstance(kind, str) or kind not in STATE_DEFINITION_KEYS:
            return "state property type is not boolean or enum (refused, 3.2)"
        if set(definition) - STATE_DEFINITION_KEYS[kind]:
            # The game reads `type`, and `values` of an enum; it ignores every other key, and
            # `values` on a boolean whatever its value (Round 4, SP1). What an ignored key holds is
            # not checked by anything, so a definition with one is left out.
            return (
                "state property definition with a key other than type, and values of an enum "
                "(ignored by the official game, left out; Round 4)"
            )
        if kind == "boolean":
            values = ["true", "false"]
        else:  # an enum
            values = definition.get("values")
            if not isinstance(values, list) or not values:
                return "enum state property without values (refused, 3.2)"
            if not all(isinstance(value, str) and STATE_NAME.fullmatch(value) for value in values):
                return "state property value is not [a-z0-9_] (refused, 3.2)"
            if any(len(value) > STATE_STRING_MAX_BYTES for value in values):
                return STATE_STRING_REASON
            if len(set(values)) != len(values):
                return "state property value repeats (refused, 3.2)"
        declared[name] = set(values)
    if math.prod(len(values) for values in declared.values()) > MAX_STATES:
        return (
            "more than 65536 states in one block (TooManyStates, fatal in the official game, RS3)"
        )
    template_properties = TEMPLATE_STATE_PROPERTIES[template]
    if set(declared) & template_properties:
        return "state property already on the template (fatal in the official game, 3.2)"
    default_state = file.get("default_state", {})
    if not isinstance(default_state, dict):
        return "block default_state is not an object (refused, 3.2)"
    for name, value in default_state.items():
        problem = _property_problem(name, declared, template_properties, "default_state")
        if problem:
            return problem
        if not isinstance(value, str) or value not in declared[name]:
            return "default_state value is not one of its property's (fatal, 3.2)"
    for key in ("shapes", "collision_shapes"):
        if key in file:
            problem = _shape_problem(file[key], declared, template_properties)
            if problem:
                return problem
    return None


def _horizon_item_problem(file: object, vanilla: Vanilla) -> str | None:
    """Why the official game refuses an item declaration (3.3, R8), or why the bake cannot tell;
    the `crafting_remainder` id is checked once every declaration of the package is known."""
    if not isinstance(file, dict):
        return "item declaration is not a JSON object (refused, 3.1)"
    for key in sorted(file):
        if key not in HORIZON_ITEM_KEYS:
            return "item key the official game refuses (unknown field, 3.3)"
        if key not in HORIZON_ITEM_CHECKED:
            return f"item key {key} (not established, left out)"
    template = file.get("template")
    if (
        not isinstance(template, str)
        or template == "minecraft:air"
        or template not in vanilla.items
    ):
        # The game accepts an unknown `minecraft:` template; what it copies is not established.
        return "item template is not in the vanilla item table (not established, left out)"
    if "max_stack_size" in file:
        size = file["max_stack_size"]
        if isinstance(size, bool) or not isinstance(size, int) or not 1 <= size <= 99:
            return "item max_stack_size is not 1 to 99 (refused, 3.3)"
        if size > 1 and template in vanilla.damageable:
            return (
                "item max_stack_size over 1 on a damageable template "
                "(no stack of it can exist in the official game, R8)"
            )
    if "rarity" in file and not (
        isinstance(file["rarity"], str) and file["rarity"] in HORIZON_RARITIES
    ):
        return "item rarity is not common, uncommon, rare or epic (refused, 3.3)"
    if "crafting_remainder" in file and not isinstance(file["crafting_remainder"], str):
        return "item crafting_remainder is not an item id (fatal in the official game, 3.3)"
    return None


def _horizon_declaration_problem(kind: str, file: object, vanilla: Vanilla | None) -> str | None:
    if vanilla is None:
        return "no vanilla block and item tables to check the declaration against"
    try:
        if kind == "block":
            return _horizon_block_problem(file, vanilla)
        return _horizon_item_problem(file, vanilla)
    except (TypeError, ValueError, OverflowError, RecursionError, KeyError, AttributeError):
        # Every check above guards its types; this is the net under them, so no value the probe
        # prints can stop the bake.
        return "declaration value the bake cannot read (left out)"


HORIZON_DECLARATION_JSON_REASON = (
    "declaration that is not strict JSON once written (NaN, Infinity, a number out of range, a "
    "lone surrogate, over 127 nested arrays and objects): the official game refuses the "
    "namespace's content (Round 4)"
)
HORIZON_STATES_REASON = (
    "more block states than the official game holds once vanilla's 35723, the packages already "
    "in horizon-mods/ and this run's earlier blocks are counted (1048576 in all; fatal, RS1)"
)
HORIZON_BLOCKS_REASON = (
    "more blocks than the official game holds once the packages already in horizon-mods/ and "
    "this run's earlier blocks are counted (\"too many blocks\", fatal, RS2)"
)


def _declared_states(file: object) -> int:
    """The state combinations a block file declares (2 for a boolean, an enum's value count), at
    least 1. For a declaration the bake checked this is the block's state count: both templates
    have no states of their own. For another package's file it is a lower bound: the states of
    its template are not in the bake's tables, and what does not read as a definition counts 1."""
    states = 1
    definitions = file.get("state_properties") if isinstance(file, dict) else None
    for definition in definitions.values() if isinstance(definitions, dict) else []:
        if not isinstance(definition, dict):
            continue
        values = definition.get("values")
        if definition.get("type") == "boolean":
            states *= 2
        elif definition.get("type") == "enum" and isinstance(values, list) and values:
            states *= len(values)
    return states


def _horizon_declaration_file(file: object) -> tuple[bytes | None, str | None]:
    """The bytes the bake writes for a checked declaration, or the reason it leaves them out: a
    file over 16 MiB (P5) or bytes that are not strict JSON by the same rules as assets (R3,
    RD1). The checks before this one leave no free-form value in a declaration, so this is the
    guard on what is written rather than on what was read."""
    try:
        data = _declaration_bytes(file)
    except (ValueError, TypeError, OverflowError, RecursionError):
        return None, HORIZON_DECLARATION_JSON_REASON
    if len(data) > MAX_FILE:
        # One content file over 16 MiB refuses the namespace's content (P5, round-2 review).
        return None, "declaration over the content file cap (16 MiB, P5)"
    if not _strict_json(data)[1]:
        return None, HORIZON_DECLARATION_JSON_REASON
    return data, None


HORIZON_BLOCKSTATE_CLASH_REASON = (
    "a block whose blockstate the official game generates at a name that is also the folder of "
    "the generated blockstate of another block of the package: the server generates none for it "
    "(RB1), and a block without one is not established on a client; left out"
)


def _blockstate_name(declaration_id: str) -> str:
    """The blockstate of a block: the name the official game generates it at when its package
    ships none (RB1), and the asset that is that blockstate when it ships one."""
    namespace, path = declaration_id.split(":", 1)
    return f"assets/{namespace}/{ASSET_BLOCKSTATES}/{path}.json"


def _folders(names: list[str]) -> set[str]:
    """Every folder the files `names` sit in."""
    return {"/".join(name.split("/")[:index]) for name in names
            for index in range(1, name.count("/") + 1)}


def _blockstate_clashes(blocks: list[str], shipped: set[str]) -> set[str]:
    """The blocks (ids) of `blocks` the official game generates no blockstate for when the package
    holds exactly `blocks` and ships the blockstates `shipped` (RB1). What the server runs showed:
    it writes `assets/<ns>/blockstates/<path>.json` under `.horizon-cache/mod-content/` for every
    block whose package ships no blockstate of that name, and nothing else of the package (`gc5`);
    when that name is also a folder there, made by the generated blockstate of another block (`a`
    beside `a.json/c`, either order; `q`, `q.json/r` beside `q.json/r.json/s`), the folder wins and
    the block gets none (`gc5`, `rb5`). A block that ships its blockstate gets none generated and
    needs none. A shipped blockstate is not copied to the cache, so it makes no folder there: next
    to a shipped `p.json/c.json` the server generates `p.json` for `p`, and `s` with a shipped
    `s.json` beside a generated `s.json/t.json` keeps its own (round-5 review `rbxh`); both are
    kept. What a client makes of the package's assets beside the cache is not established."""
    generated = [_blockstate_name(declaration_id) for declaration_id in blocks
                 if _blockstate_name(declaration_id) not in shipped]
    folders = _folders(generated)
    return {declaration_id for declaration_id in blocks
            if _blockstate_name(declaration_id) not in shipped
            and _blockstate_name(declaration_id) in folders}


def _counted_blocks(
    blocks: list[tuple[str, object]],
    shipped: set[str],
    states_left: int,
    blocks_left: int,
    bake: JarBake,
) -> tuple[set[str], int, int]:
    """The blocks (ids, of `(id, file)` in the probe's order) that are written: those that fit the
    game's counts (RS1, RS2) and get a blockstate (RB1), with their states and blocks; every other
    one is skipped with its reason.

    A block is left out for RB1 only when a block whose generated blockstate makes its own name a
    folder is written. So the blocks that clash with no other are counted first, in order; a block
    whose name is a folder of a written block's is then left out for RB1, and those that clashed
    only with blocks the counts left out are counted next, in order, against what is left, until
    none is left. What is counted is exactly what is written, and every pass takes blocks in order
    against what is left, so a run with the room an earlier run of the same inputs had writes the
    same blocks again (Round 6, RB4); `Budget.pending` gives a run after an update the room of the
    next run (Round 7, RB5)."""
    written: list[str] = []
    states = count = 0
    pending = list(blocks)
    while pending:
        clashing = _blockstate_clashes([declaration_id for declaration_id, _ in pending], shipped)
        later = []
        for declaration_id, file in pending:
            if declaration_id in clashing:
                later.append((declaration_id, file))
                continue
            block_states = _declared_states(file)
            if count + 1 > blocks_left:
                bake.skip(HORIZON_BLOCKS_REASON, declaration_id)
                continue
            if states + block_states > states_left:
                bake.skip(HORIZON_STATES_REASON, declaration_id)
                continue
            states += block_states
            count += 1
            written.append(declaration_id)
        if len(later) == len(pending):
            # Never: the longest name of a pass is no folder of another's, so every pass decides
            # at least one block. Should it happen, the jar is refused rather than looping.
            raise RuntimeError("the blockstate passes of RB1 did not end")
        folders = _folders([_blockstate_name(declaration_id) for declaration_id in written
                            if _blockstate_name(declaration_id) not in shipped])
        pending = []
        for declaration_id, file in later:
            if _blockstate_name(declaration_id) in folders:
                bake.skip(HORIZON_BLOCKSTATE_CLASH_REASON, declaration_id)
            else:
                pending.append((declaration_id, file))
    return set(written), states, count


def _horizon_declarations(
    candidates: list[tuple[str, str, str, str, object]],
    bake: JarBake,
    states_left: int | None = None,
    blocks_left: int | None = None,
    shipped: set[str] | None = None,
) -> tuple[list[tuple[str, str, bytes]], int, int]:
    """The declarations of one namespace the official game loads, as (kind, id, bytes), with
    the block states and blocks they add; every other one is skipped with its reason, so none
    can refuse the package's content or stop the game. `shipped` is the namespace's asset names.

    Every check that leaves a single declaration out runs first: the file's rules, the bytes
    written (size, strict JSON), a repeated id, then the room left in the game's block state and
    block counts (`states_left`, `blocks_left`; the run's budget, RS1-RS2) together with the
    blockstate the game generates (RB1, `_counted_blocks`). Only then are a block's own item and
    the item remainders decided, among the declarations that stay, so a left-out declaration
    never takes a valid one with it and no kept item names one left out."""
    vanilla = vanilla_tables()
    states_left = HORIZON_MOD_STATES if states_left is None else states_left
    blocks_left = HORIZON_MOD_BLOCKS if blocks_left is None else blocks_left
    valid = []
    for kind, declaration_id, _, _, file in candidates:
        if kind == "item" and isinstance(file, dict) and "components" in file:
            # The runtime records a component patch (`minecraft:lore`) for Sunrise; the
            # official game's `components` is not established, so the item goes without it
            # rather than being left out whole.
            file = {key: value for key, value in file.items() if key != "components"}
            bake.skip("item components (not established, left out)", declaration_id)
        problem = _horizon_declaration_problem(kind, file, vanilla)
        data = None
        if problem is None:
            data, problem = _horizon_declaration_file(file)
        if problem:
            bake.skip(problem, declaration_id)
        else:
            valid.append((kind, declaration_id, file, data))
    # The official game takes an item file beside a block of that id without its item (3.2,
    # `bi_sep`), so a declaration repeats only within its kind.
    unique = []
    declared = set()
    for candidate in valid:
        key = candidate[:2]
        if key in declared:
            bake.skip("declared more than once in the jar", candidate[1])
            continue
        declared.add(key)
        unique.append(candidate)
    # The game's state and block counts, in the probe's order (RS1, RS2), and RB1.
    written, states, blocks = _counted_blocks(
        [(declaration_id, file) for kind, declaration_id, file, _ in unique if kind == "block"],
        {name for name in shipped or () if name.split("/")[2:3] == [ASSET_BLOCKSTATES]},
        states_left,
        blocks_left,
        bake,
    )
    counted = [candidate for candidate in unique
               if candidate[0] != "block" or candidate[1] in written]
    # A block with `item: true` declares its own item; an item file for the same id would
    # refuse the namespace (3.2).
    block_items = {
        declaration_id
        for kind, declaration_id, file, _ in counted
        if kind == "block" and file.get("item", True)
    }
    kept = []
    for candidate in counted:
        if candidate[0] == "item" and candidate[1] in block_items:
            bake.skip("the block's own item (its block declares it)", candidate[1])
            continue
        kept.append(candidate)
    # A remainder names a vanilla item or one of the package's own (3.3); an item left out can
    # take another's remainder with it, so this runs until nothing changes.
    while True:
        own = {
            declaration_id
            for kind, declaration_id, file, _ in kept
            if kind == "item" or file.get("item", True)
        }
        dropped = []
        for candidate in kept:
            kind, declaration_id, file, _ = candidate
            remainder = file.get("crafting_remainder") if kind == "item" else None
            if remainder is None:
                continue
            namespace = declaration_id.split(":", 1)[0]
            vanilla_item = remainder != "minecraft:air" and remainder in vanilla.items
            own_item = remainder in own and remainder.split(":", 1)[0] == namespace
            if not vanilla_item and not own_item:
                dropped.append(candidate)
        if not dropped:
            break
        for candidate in dropped:
            kept.remove(candidate)
            bake.skip(
                "item crafting_remainder is not a vanilla item or one of the package's own "
                "(fatal in the official game, 3.3)",
                candidate[1],
            )
    return (
        [(kind, declaration_id, data) for kind, declaration_id, _, data in kept],
        states,
        blocks,
    )


@dataclass(frozen=True)
class Existing:
    """A package already in `horizon-mods/` before this run (L2: a `.hmod` file or a folder with
    `horizon-mod.toml`)."""

    path: Path
    # Its id and `namespaces`; None when its manifest cannot be read.
    owned: frozenset[str] | None
    # The jar a package this tool baked names in its description, else None.
    baked_from: str | None = None
    # The mod ids of that jar the description records (Round 5); None for a package of an
    # earlier round, which records only the jar.
    baked_ids: frozenset[str] | None = None
    # The manifest's `id`, None when it cannot be read.
    package_id: str | None = None


class ZipTooLarge(ValueError):
    """A zip whose central directory is past `ZIP_MAX_ENTRIES` or `ZIP_MAX_DIRECTORY`."""


# A central directory record: its signature, then 42 bytes whose last 18 hold the name, extra
# field and comment lengths at offsets 28, 30 and 32 (APPNOTE 4.3.12).
CENTRAL_RECORD = b"PK\x01\x02"
CENTRAL_RECORD_SIZE = 46


def _directory_offset(end: list, stream: object) -> tuple[int, int]:
    """The central directory's offset in the file and the bytes prepended to it: the pair
    `zipfile` computes before it reads the directory, from `end[_ECD_LOCATION] - size - offset`
    (zipfile 3.11.15+, 3.13.13+ and 3.14; its `_EndRecData64` already stores the right
    location). Older point releases subtracted the zip64 record sizes, so for a zip64 record
    whose primary offset is not a central record, try that older rule and keep it only when it
    lands on one."""
    size_cd = end[zipfile._ECD_SIZE]
    offset_cd = end[zipfile._ECD_OFFSET]
    concat = end[zipfile._ECD_LOCATION] - size_cd - offset_cd
    if end[zipfile._ECD_SIGNATURE] == zipfile.stringEndArchive64 and size_cd > 0:
        try:
            stream.seek(offset_cd + concat)
            primary = stream.read(4)
        except (OSError, ValueError):
            primary = b""
        if primary != CENTRAL_RECORD:
            older = concat - (zipfile.sizeEndCentDir64 + zipfile.sizeEndCentDir64Locator)
            try:
                stream.seek(offset_cd + older)
                if stream.read(4) == CENTRAL_RECORD:
                    concat = older
            except (OSError, ValueError):
                pass
    return offset_cd, concat


def _central_records(stream: object, end: list) -> int:
    """The records of the central directory Python's `zipfile` would read: the directory's
    `end[_ECD_SIZE]` bytes (already checked against `ZIP_MAX_DIRECTORY`), from where `zipfile`
    puts it, walked by each record's signature and lengths as `zipfile` walks them. The count the
    end record (or the zip64 one) states is not used: `zipfile` does not use it either, so a zip
    that under-states it would pass a bound on it and still be parsed whole (round-6 review).
    Counting stops past `ZIP_MAX_ENTRIES`, at a record `zipfile` would refuse (a bad signature or
    a truncated record: it raises `BadZipFile` there itself) and at the directory's end."""
    size = end[zipfile._ECD_SIZE]
    offset, concat = _directory_offset(end, stream)
    if offset + concat < 0:
        return 0  # `zipfile` refuses it ("Bad offset for central directory")
    stream.seek(offset + concat)
    directory = stream.read(size)
    records = position = 0
    while position < size:
        if (
            position + CENTRAL_RECORD_SIZE > len(directory)
            or directory[position:position + 4] != CENTRAL_RECORD
        ):
            break
        name, extra, comment = struct.unpack_from("<HHH", directory, position + 28)
        position += CENTRAL_RECORD_SIZE + name + extra + comment
        records += 1
        if records > ZIP_MAX_ENTRIES:
            break
    return records


def _open_zip(path: Path) -> zipfile.ZipFile:
    """A package's (or, under the horizon target, a jar's) zip, opened only when its central
    directory is small enough to read whole: at most `ZIP_MAX_DIRECTORY` bytes and
    `ZIP_MAX_ENTRIES` records, counted by walking the directory (`_central_records`) before
    `zipfile` parses it (Round 7, ZB1); raises `ZipTooLarge` otherwise."""
    with path.open("rb") as stream:
        end = zipfile._EndRecData(stream)  # the end record, zip64 sizes included
        if end is None:
            raise zipfile.BadZipFile("no end of central directory")
        if (
            end[zipfile._ECD_SIZE] > ZIP_MAX_DIRECTORY
            or _central_records(stream, end) > ZIP_MAX_ENTRIES
        ):
            raise ZipTooLarge(
                f"over {ZIP_MAX_ENTRIES} entries or {ZIP_MAX_DIRECTORY >> 20} MiB of central "
                "directory"
            )
    return zipfile.ZipFile(path)


def _read_manifest(path: Path) -> dict:
    if path.is_dir():
        manifest = path / HORIZON.manifest
        if manifest.stat().st_size > MAX_FILE:
            raise ValueError("manifest over 16 MiB")
        text = manifest.read_text(encoding="utf-8")
    else:
        with _open_zip(path) as archive:
            if archive.getinfo(HORIZON.manifest).file_size > MAX_FILE:
                raise ValueError("manifest over 16 MiB")
            text = archive.read(HORIZON.manifest).decode("utf-8")
    return tomllib.loads(text)


def existing_packages(folders: list[Path]) -> list[Existing]:
    """The packages in `folders` (`horizon-mods/` and `--out`), read once before the run."""
    found = []
    seen = set()
    for folder in folders:
        if not folder.is_dir():
            continue
        for path in sorted(folder.iterdir()):
            resolved = path.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            if path.is_dir():
                if not (path / HORIZON.manifest).exists():
                    continue  # not a package (L2)
            elif not path.name.endswith(HORIZON.extension):
                continue  # not a package: `.zip`, `.HMOD`, no extension (L4)
            try:
                manifest = _read_manifest(path)
                package_id = manifest["id"]
                namespaces = manifest.get("namespaces", [])
                if not isinstance(package_id, str) or not isinstance(namespaces, list):
                    raise ValueError("id or namespaces")
                if not all(isinstance(namespace, str) for namespace in namespaces):
                    raise ValueError("namespaces")
            except Exception:  # noqa: BLE001 - any failure leaves the id unknown
                found.append(Existing(path, None))
                continue
            description = manifest.get("description")
            baked = (
                BAKED_DESCRIPTION.fullmatch(description) if isinstance(description, str) else None
            )
            baked_from = baked_ids = None
            if baked and baked.group(2) == package_id and not namespaces:
                baked_from = baked.group(1)
                if baked.group(3) is not None:
                    baked_ids = frozenset(
                        [] if baked.group(3) == "none" else baked.group(3).split(", ")
                    )
            found.append(
                Existing(path, frozenset([package_id, *namespaces]), baked_from, baked_ids,
                         package_id)
            )
    return found


def _same_owner(
    namespace: str, package: Existing, jar: str, mod_ids: frozenset[str]
) -> bool:
    """Whether an earlier bake's package of `namespace` is this jar's to replace (Round 5, RO1):
    the mod whose id the namespace is wrote it, or, for a namespace that is no mod id of either
    jar, a jar of the same mod ids; with no mod id on either side, a jar of the same name. A
    package of an earlier round records only its jar, so only a jar of that name replaces it."""
    ids = _recorded_ids(mod_ids)
    if package.baked_ids is None:
        return package.baked_from == _shown_name(jar)
    if namespace in ids or namespace in package.baked_ids:
        return namespace in ids and namespace in package.baked_ids
    if ids or package.baked_ids:
        return ids == package.baked_ids
    return package.baked_from == _shown_name(jar)


# What `_existing_conflict` found: another mod holds the namespace (`HELD`), or the bake cannot
# tell whether one does, or cannot write the package where it goes (`BLOCKED`).
HELD = "held"
BLOCKED = "blocked"


def _existing_conflict(
    namespace: str,
    package_path: Path,
    existing: list[Existing],
    game_dir: Path,
    jar: str = "",
    mod_ids: frozenset[str] = frozenset(),
    run_jars: dict[str, frozenset[str]] | None = None,
) -> tuple[str, str] | None:
    """Why the package of `namespace` may not be written at `package_path` (R2), as (`HELD` or
    `BLOCKED`, reason), or None. A package that holds the namespace is named before one that
    blocks it: a namespace another mod holds is never this jar's, whatever else is in the way.

    An earlier bake's package whose owner (`_same_owner`) is another jar of this run (`run_jars`,
    jar name to mod ids) does not hold the namespace: that jar bakes it again in this run, so the
    jars decide it as on a first run, in their order (the first that writes it has it), and a
    second run of the same inputs decides it the same way (Round 6, RB4)."""
    target = package_path.resolve()
    held = []
    blocked = []
    for package in existing:
        name = _relative(package.path, game_dir)
        if package.path.resolve() == target:
            if package.baked_from is not None and package.owned == {namespace}:
                if _same_owner(namespace, package, jar, mod_ids):
                    continue  # this mod's earlier package of the namespace: replaced
                if any(name != jar and _same_owner(namespace, package, name, ids)
                       for name, ids in (run_jars or {}).items()):
                    continue  # its owner is baked in this run too (RB4)
                ids = ", ".join(sorted(package.baked_ids or ())) or "none"
                recorded = "" if package.baked_ids is None else f" (mod ids {ids})"
                held.append(
                    f"namespace {namespace}: {name} was baked from {package.baked_from}"
                    f"{recorded}, another mod's package for it; not overwritten (remove it to "
                    "bake the namespace from this jar)"
                )
            elif package.owned is not None and namespace in package.owned:
                held.append(
                    f"namespace {namespace}: {name} is a package this bake did not write for it; "
                    "not overwritten"
                )
            else:
                blocked.append(
                    f"namespace {namespace}: {name} is a package this bake did not write for it; "
                    "not overwritten"
                )
            continue
        if package.owned is None:
            blocked.append(
                f"namespace {namespace}: the manifest of {name} cannot be read, so it may use "
                "this id; not written"
            )
        elif namespace in package.owned:
            held.append(
                f"namespace {namespace}: {name} already uses this id or namespace (two packages "
                "of one id stop the official game, two of one namespace lose one's content; "
                "1.3, N7, R2); not written"
            )
    if package_path.is_dir() and not any(
        package.path.resolve() == target for package in existing
    ):
        blocked.append(
            f"namespace {namespace}: a folder sits where its package goes; not overwritten"
        )
    if held:
        return HELD, held[0]
    if blocked:
        return BLOCKED, blocked[0]
    return None


def _turned_on(game_dir: Path, package_id: str) -> bool:
    """Whether `horizon-mods.txt` has a line that turns the package on (L6, L7, Round 6 ML1)."""
    path = game_dir / HORIZON.mods_file
    try:
        text = path.read_bytes().decode("utf-8")
    except (OSError, UnicodeDecodeError):
        return False
    return any(_mods_file_id(line) == (package_id, True) for line in _mods_file_lines(text))


def bake_jar(
    jar: Path,
    out: Path,
    claimed: dict[str, str],
    probe: Path | None,
    target: Target = SUNRISE,
    existing: list[Existing] | None = None,
    game_dir: Path | None = None,
    run: HorizonRun | None = None,
) -> JarBake:
    """Bakes one jar into one package per namespace; `claimed` refuses a shared namespace. Under
    the horizon target, `run` carries what the run's jars share (`bake_directory` makes it once);
    without one the jar is baked as if it were the only one."""
    if target is HORIZON:
        if run is None:
            run = HorizonRun.start([jar], existing or [], game_dir or out)
            run.settle([jar], out, probe)
        return _bake_jar_horizon(jar, out, claimed, probe, run)
    # The Sunrise target is not crash-free, and is left so on purpose: its output must stay byte
    # for byte that of the bake before the horizon target (91040874). A jar file name that is not
    # UTF-8 raises `UnicodeEncodeError` in `write_package` (leaving `<ns>.smod.tmp`), a probe row
    # whose `file` is not an object or whose `id` is not a string raises `AttributeError` or
    # `TypeError` below, and probe output that is not UTF-8 raises `UnicodeDecodeError` in
    # `probe_declarations`; each stops the run with a traceback before the report is written
    # (round-5 review; the same at 91040874). The horizon target refuses such a jar instead.
    bake = JarBake(jar=jar.name)
    try:
        entries = _zip_entries(jar, bake)
    except UnsafeJar as error:
        bake.error = str(error)
        return bake
    if probe is None:
        bake.error = (
            f"no runtime probe: pass --probe <{PROBE}> or set {PROBE_ENV}, so the jar's blocks "
            "and items can be declared in its package"
        )
        return bake
    try:
        declarations = probe_declarations(probe, jar)
    except ProbeError as error:
        bake.error = f"the runtime could not run the jar's entrypoints: {error}"
        return bake

    by_namespace: dict[str, list[tuple[str, bytes]]] = {}
    shared: list[tuple[str, bytes]] = []
    for name, data in entries:
        if _refused_suffix(name):
            bake.skip("code or an executable, never copied", name)
            continue
        if not plain_path(name):
            bake.skip("not a plain content path", name)
            continue
        classified = _classify(name)
        if classified is None:
            bake.skip("not an assets/ or data/ file", name)
            continue
        top, namespace, rest = classified
        if not _allowed(top, namespace, rest, name, bake):
            continue
        recipe_type = _custom_recipe_type(top, rest, data)
        if recipe_type is not None:
            # Sunrise refuses to start when a recipe's serializer is not registered (it loads
            # recipes strictly, where vanilla logs and skips), and nothing bridges a jar's own
            # recipe serializer to the game yet, so such a recipe stays out of the package.
            bake.skip(
                f"recipe of a mod's own type {recipe_type}: no serializer for it in the game yet",
                name,
            )
            continue
        if namespace in SHARED_NAMESPACES:
            shared.append((name, data))
        else:
            by_namespace.setdefault(namespace, []).append((name, data))

    # The runtime's typed registrations become the package's own declarations
    # (`data/<namespace>/sunrise/{block,item}/<path>.json`), which the content loader reads without
    # the native ownership fence. A block with `item: true` declares its own item; an item file for
    # the same id would refuse the namespace.
    block_items = {
        declaration_id
        for kind, declaration_id, file in declarations
        if kind == "block" and file.get("item", True)
    }
    declared = set()
    loaded_mods = list(dict.fromkeys(i for kind, i, _ in declarations if kind == "mod"))
    for kind, declaration_id, file in declarations:
        if kind == "mod":
            continue
        if kind == "menu-refused":
            # The live-menu bridge opens a jar's menu only as a declared Sunrise menu type; a
            # layout the runtime can't declare exactly is left out, and its opens are refused.
            bake.skip(f"menu type whose layout is not a declaration: {file['reason']}", declaration_id)
            continue
        namespace, path = declaration_id.split(":", 1) if ":" in declaration_id else ("", "")
        if not namespace or not path:
            bake.skip("declaration id is not namespace:path", declaration_id)
            continue
        if namespace == "minecraft":
            bake.skip("vanilla's namespace: a mod does not declare vanilla content", declaration_id)
            continue
        if not NAMESPACE.match(namespace) or not plain_path(path):
            bake.skip("declaration id is not a content namespace and path", declaration_id)
            continue
        if not PACKAGE_ID.match(namespace) or namespace in RESERVED_IDS:
            # A manifest id is `[a-z0-9_]`; a package id its declarations can join has the same
            # shape (`mod_content::accept_declared` matches the id). `-` and `.` have no package.
            bake.error = (
                f"namespace {namespace!r} is not a content package id ([a-z0-9_], not a device "
                "name): its declarations cannot join a package (docs/mod-plan.md, "
                "Fork seams needed)"
            )
            continue
        if kind == "item" and declaration_id in block_items:
            bake.skip("the block's own item (its block declares it)", declaration_id)
            continue
        # A menu type is its own registry: it may share an id with a block or an item.
        declared_key = (kind == "menu", declaration_id)
        if declared_key in declared:
            bake.skip("declared more than once in the jar", declaration_id)
            continue
        declared.add(declared_key)
        declaration_path = f"data/{namespace}/sunrise/{kind}/{path}.json"
        by_namespace.setdefault(namespace, []).append(
            (declaration_path, _declaration_bytes(file))
        )

    bake.namespaces = sorted(by_namespace)
    grouped = _mod_packages(loaded_mods, by_namespace, bake)
    if grouped is not None:
        return _write_mod_packages(jar, out, claimed, target, bake, grouped, by_namespace, shared)
    if not bake.namespaces:
        if shared:
            bake.skip(
                "vanilla tag or atlas additions with no jar namespace to bake into", shared[0][0]
            )
        return bake

    for namespace in bake.namespaces:
        if not PACKAGE_ID.match(namespace) or namespace in RESERVED_IDS:
            # Runtime declarations join a package by id (`mod_content::accept_declared`), and a
            # manifest id is `[a-z0-9_]`; a `-` or `.` namespace has no package shape that joins.
            bake.error = (
                f"namespace {namespace!r} is not a content package id ([a-z0-9_], not a device "
                "name): a runtime declaration cannot join its assets (docs/mod-plan.md, "
                "Fork seams needed)"
            )
            continue
        if namespace in claimed:
            bake.error = (
                f"namespace {namespace} is already baked from {claimed[namespace]}; "
                "two content mods may not own one namespace"
            )
            continue
        files = by_namespace[namespace]
        if namespace == bake.namespaces[0]:
            files = files + shared
        package_path = out / f"{namespace}{target.extension}"
        size = write_package(package_path, namespace, jar.name, files, target)
        prefix = f"data/{namespace}/{target.declarations}/"
        package = Package(
            id=namespace,
            namespace=namespace,
            path=package_path,
            files=[name for name, _ in sorted(files)],
            bytes=size,
            blocks=sum(1 for name, _ in files if name.startswith(prefix + "block/")),
            items=sum(1 for name, _ in files if name.startswith(prefix + "item/")),
        )
        bake.packages.append(package)
        claimed[namespace] = jar.name
    if shared and not bake.packages:
        bake.skip("vanilla tag or atlas additions with no package to carry them", shared[0][0])
    return bake


# Namespaces no Sunrise manifest may list in `namespaces` (sunrise-mod-package
# `manifest::RESERVED_NAMESPACES`); `minecraft` and `c` are `SHARED_NAMESPACES` and never reach
# a package of their own.
SUNRISE_RESERVED_NAMESPACES = {"minecraft", "c", "sunrise"}


def _mod_packages(
    loaded_mods: list[str], by_namespace: dict[str, list[tuple[str, bytes]]], bake: JarBake
) -> list[tuple[str, list[str]]] | None:
    """The Sunrise packages of a jar whose probe named the mods it loaded: one per mod id, in the
    probe's order (the jar's own mod first), each with the other namespaces it carries. None
    keeps one package per namespace (no mods named, or the jar's own id cannot be a package id).

    Sunrise answers a `mod_loaded` load condition from the ids of the enabled packages
    (sunrise-items `recipe_sources`: `EnabledMods::server_ids`, against the condition's `modid`),
    and Fabric Loader 0.20.1 from its mod map, which holds the jar's mod, every nested jar it
    loaded (`fabric.mod.json` `jars`) and each one's `provides` aliases. So the package ids must
    be exactly those ids:
    - a nested mod is a package even when it brings no file (Runes' nested `bundleapi`: a
      manifest only), so `mod_loaded bundleapi` holds as it does for the jar's code;
    - so is a `provides` alias: a Sunrise manifest has no aliases, and only an enabled package of
      that id makes `mod_loaded <alias>` true;
    - a namespace that is no mod's (a tag file another mod reads, such as Runes'
      `data/spell_engine/tags/`) is listed in the jar's own package's manifest `namespaces`
      instead of being a package, so `mod_loaded spell_engine` stays false as on Fabric.
    A mod id that cannot be a package id (`-` is a Fabric id character, not a manifest one) is
    named in the report: no package can make it a loaded mod on Sunrise."""
    if not loaded_mods:
        return None
    primary = loaded_mods[0]

    def package_id(mod_id: str) -> bool:
        return (
            PACKAGE_ID.match(mod_id) is not None
            and mod_id not in RESERVED_IDS
            and mod_id not in SUNRISE_RESERVED_NAMESPACES
            and mod_id != "horizon_java"
        )

    if not package_id(primary):
        return None
    packages: dict[str, list[str]] = {}
    for mod_id in loaded_mods:
        if package_id(mod_id):
            packages[mod_id] = []
        else:
            bake.skip(
                "loaded mod whose id is not a Sunrise package id ([a-z0-9_], not reserved): "
                "Sunrise cannot count it as loaded",
                mod_id,
            )
    for namespace in sorted(by_namespace):
        if namespace in packages:
            continue
        if namespace in SUNRISE_RESERVED_NAMESPACES:
            for name, _ in by_namespace[namespace]:
                bake.skip("Sunrise's reserved namespace: no package may own it", name)
            continue
        packages[primary].append(namespace)
    return list(packages.items())


def _write_mod_packages(
    jar: Path,
    out: Path,
    claimed: dict[str, str],
    target: Target,
    bake: JarBake,
    grouped: list[tuple[str, list[str]]],
    by_namespace: dict[str, list[tuple[str, bytes]]],
    shared: list[tuple[str, bytes]],
) -> JarBake:
    """Writes `_mod_packages`' packages: each mod's own namespace and the namespaces it carries,
    the vanilla and `c` tag additions in the jar's own package. A namespace another jar of the
    run already baked refuses the package that would carry it, as one per namespace did."""
    for index, (mod_id, extra) in enumerate(grouped):
        owned = [mod_id, *extra]
        taken = next((namespace for namespace in owned if namespace in claimed), None)
        if taken is not None:
            bake.error = (
                f"namespace {taken} is already baked from {claimed[taken]}; "
                "two content mods may not own one namespace"
            )
            continue
        files = [entry for namespace in owned for entry in by_namespace.get(namespace, [])]
        if index == 0:
            files = files + shared
        package_path = out / f"{mod_id}{target.extension}"
        size = write_package(package_path, mod_id, jar.name, files, target, namespaces=extra)
        prefixes = [f"data/{namespace}/{target.declarations}/" for namespace in owned]

        def declared(kind: str) -> int:
            return sum(
                1
                for name, _ in files
                if any(name.startswith(prefix + kind + "/") for prefix in prefixes)
            )

        bake.packages.append(
            Package(
                id=mod_id,
                namespace=mod_id,
                path=package_path,
                files=[name for name, _ in sorted(files)],
                bytes=size,
                blocks=declared("block"),
                items=declared("item"),
            )
        )
        for namespace in owned:
            claimed[namespace] = jar.name
    if shared and not bake.packages:
        bake.skip("vanilla tag or atlas additions with no package to carry them", shared[0][0])
    return bake


def _jar_mod_ids(entries: list[tuple[str, bytes]]) -> set[str]:
    """The mod ids the jar names for itself: `id` of `fabric.mod.json`, and each `[[mods]]`
    `modId` of `META-INF/neoforge.mods.toml` and `META-INF/mods.toml`. A file that does not
    parse, or a value of another type, names none; nothing here can stop the bake."""
    ids: set[str] = set()
    for name, data in entries:
        try:
            if name == FABRIC_METADATA:
                metadata = json.loads(data.decode("utf-8"))
                if isinstance(metadata, dict) and isinstance(metadata.get("id"), str):
                    ids.add(metadata["id"])
            elif name in TOML_METADATA:
                mods = tomllib.loads(data.decode("utf-8")).get("mods", [])
                for mod in mods if isinstance(mods, list) else []:
                    if isinstance(mod, dict) and isinstance(mod.get("modId"), str):
                        ids.add(mod["modId"])
        except (ValueError, RecursionError):
            # `UnicodeDecodeError`, `json.JSONDecodeError` and `tomllib.TOMLDecodeError` are
            # `ValueError`s.
            continue
    return ids


def _horizon_allowed(
    top: str, namespace: str, rest: str, path: str, bake: JarBake, owned: set[str]
) -> bool:
    """Whether a jar file goes into a horizon package: assets of an owned namespace, of a kind a
    content package carries, and vanilla's atlases (N3); never a `data/` file."""
    if top == "data":
        bake.skip(HORIZON_DATA_REASON, path)
        return False
    if namespace in HORIZON_SHARED_NAMESPACES:
        if namespace == "minecraft" and rest.startswith("atlases/"):
            return True
        bake.skip("vanilla's namespace: only tag and atlas additions are a mod's", path)
        return False
    if namespace in HORIZON_BUILTIN_IDS:
        bake.skip("the official game's built-in horizon_java mod owns this namespace", path)
        return False
    if not NAMESPACE.fullmatch(namespace):
        bake.skip("not a content namespace ([a-z0-9_.-]{1,64})", path)
        return False
    if namespace not in owned:
        bake.skip(HORIZON_UNOWNED_REASON, path)
        return False
    if rest.partition("/")[0] in ASSET_KINDS or rest in ASSET_FILES:
        return True
    bake.skip("unsupported asset kind", path)
    return False


def _clashes(files: list[tuple[str, bytes]]) -> set[str]:
    """The names of a package that are a file and also a folder of another file (`a.json` and
    `a.json/b.json`), with that folder's files."""
    names = {name for name, _ in files}
    folders = {name.rsplit("/", 1)[0] for name in names}
    folders = {"/".join(folder.split("/")[: index + 1])
               for folder in folders for index in range(folder.count("/") + 1)}
    clashing = names & folders
    return {name for name in names if name in clashing or any(
        name.startswith(folder + "/") for folder in clashing)}


def _clash_loads(name: str) -> bool:
    """Whether the official game was seen to load both sides of a file/folder clash of this kind:
    declarations and lang files (RP4 `fp1`, `fp3`). Only a tag lost its file, and no `data/`
    file but the declarations is written."""
    parts = name.split("/")
    return parts[0] == "data" or (len(parts) > 3 and parts[0] == "assets" and parts[2] == "lang")


# Block declarations of a package in `horizon-mods/` (D1).
BLOCK_DECLARATION = re.compile(r"^data/[^/]+/horizon/block/.+\.json$")
# A fluid declaration (`data/<ns>/horizon/fluid/<path>.json`, Round 7 FL1): with a `world` it adds
# the block `<ns>:<path>`, a copy of `minecraft:water` with its 16 states (`level` 0-15), counted
# against both of the game's limits as any block, and the item `<ns>:<path>_bucket` unless
# `"bucket": false`. A file that reads as an object without `world`, or with `"world": null`,
# adds neither (a tank-only fluid). Every other file is counted as one block of 16 states: an
# object whose `world` is `{}` or an array, a file that is an array (the game reads it as the
# fields in order) and one Python cannot read are not all established, and a file the game
# refuses adds nothing, so the count may be over, never under. The other declaration kinds the
# game takes in `data/<ns>/horizon/` (`item`, `creative_tab`, `mob_effect`, `entity_type`, `menu`)
# add no block (seen: no state, and an item only for `item`).
FLUID_DECLARATION = re.compile(r"^data/[^/]+/horizon/fluid/.+\.json$")
FLUID_STATES = 16


def _fluid_blocks(data: bytes | None) -> int:
    """The blocks a fluid declaration of another package adds: 0 only when it reads as a JSON
    object (UTF-8, no repeated key) whose `world` is missing or null, else 1 (FL1)."""
    if data is None:
        return 1

    def unique(pairs: list[tuple[str, object]]) -> dict:
        if len({key for key, _ in pairs}) != len(pairs):
            raise ValueError("a repeated key")  # the game refuses it; Python keeps the last
        return dict(pairs)

    try:
        file = json.loads(data.decode("utf-8"), object_pairs_hook=unique)
    except (ValueError, RecursionError):
        return 1
    return 0 if isinstance(file, dict) and file.get("world") is None else 1


class _Uncountable(Exception):
    """The bake cannot count the blocks of a package already in `horizon-mods/`: it is past what
    the bake reads to count them (`HORIZON_USAGE_BYTES`, `ZIP_MAX_ENTRIES`, `ZIP_MAX_DIRECTORY`),
    or holds something the bake cannot read (RB3)."""


def _package_usage(path: Path, read: list[int], limit: int | None = None) -> tuple[int, int]:
    """The block states and blocks a package already in `horizon-mods/` declares. Each block file
    counts one block and `_declared_states` states; a file over 16 MiB (which the game does not
    load, P5) or not JSON counts one state. Each fluid file counts `_fluid_blocks` blocks of
    `FLUID_STATES` states (Round 7, FL1; one over 16 MiB counts one). Files are read and parsed
    one at a time, and `read[0]`, the bytes read so far against `limit` (default
    `HORIZON_USAGE_BYTES`), may not pass it.

    Raises `_Uncountable` past `limit` or `ZIP_MAX_ENTRIES` files, and whenever the bake cannot
    read the package or one of its block or fluid files (a zip Python's `zipfile` cannot open, an
    entry it cannot read, such a file in a folder package that is not a regular file). The game may
    still read what Python cannot: a package whose manifest reads but one of whose block entries
    has a local header name other than its directory name loaded every block in the official game
    (round-5 review `nmr7`), so such a count is unknown, never zero (Round 6, RB3)."""
    states = blocks = 0
    limit = HORIZON_USAGE_BYTES if limit is None else limit

    def count(data: bytes | None, fluid: bool = False) -> None:
        nonlocal states, blocks
        if data is not None:
            read[0] += len(data)
            if read[0] > limit:
                raise _Uncountable(f"more than {limit >> 20} MiB of block declarations")
        if fluid:
            added = _fluid_blocks(data)
            states += FLUID_STATES * added
            blocks += added
            return
        try:
            file = json.loads(data) if data is not None else None
        except (ValueError, RecursionError):
            file = None
        states += _declared_states(file)
        blocks += 1

    try:
        if path.is_dir():
            data_folder = path / "data"
            seen = 0
            for root, folders, names in os.walk(data_folder):
                folders.sort()
                relative = Path(root).relative_to(data_folder).parts
                for name in sorted(names):
                    seen += 1
                    if seen > ZIP_MAX_ENTRIES:
                        raise _Uncountable(f"more than {ZIP_MAX_ENTRIES} files in {path.name}")
                    if len(relative) < 3 or relative[1:3] not in (
                        ("horizon", "block"), ("horizon", "fluid")
                    ):
                        continue
                    if not name.endswith(".json"):
                        continue
                    fluid = relative[2] == "fluid"
                    file_path = Path(root) / name
                    if not file_path.is_file():
                        # A broken link, a pipe: what the game reads there is not established.
                        raise _Uncountable(
                            f"{path.name}: a block or fluid file that is not a regular file"
                        )
                    if file_path.stat().st_size > MAX_FILE:
                        count(None, fluid)
                        continue
                    count(file_path.read_bytes(), fluid)
        else:
            try:
                archive = _open_zip(path)
            except ValueError as error:
                raise _Uncountable(f"{path.name}: {error}") from error
            with archive:
                for info in archive.infolist():
                    fluid = bool(FLUID_DECLARATION.match(info.filename))
                    if info.is_dir() or not (fluid or BLOCK_DECLARATION.match(info.filename)):
                        continue
                    # `ZipExtFile` stops at the header's size, so `file_size` bounds the read.
                    count(archive.read(info) if info.file_size <= MAX_FILE else None, fluid)
    except _Uncountable:
        raise
    except (zipfile.BadZipFile, RuntimeError, *JAR_READ_ERRORS) as error:
        raise _Uncountable(
            f"{path.name}: the bake cannot read it ({_error_name(error)}: {error})"
        ) from error
    return states, blocks


# What the official game trims from each side of the key and the value of a `horizon-mods.txt`
# line: Unicode's White_Space characters (Round 6, ML1). Seen trimmed: space, tab, `\v`, `\f`, `\r`,
# U+0085, U+00A0, U+1680, U+2000, U+2028, U+2029, U+202F, U+205F, U+3000; seen not trimmed (the
# package stays off): U+001C, U+001E, U+001F, U+200B, U+FEFF. U+2001-U+200A are taken from the
# same Unicode set, not run.
MODS_FILE_SPACE = (
    "\t\n\x0b\x0c\r \x85\xa0\u1680" + "".join(chr(code) for code in range(0x2000, 0x200B))
    + "\u2028\u2029\u202f\u205f\u3000"
)


def _mods_file_lines(text: str) -> list[str]:
    """The lines of `horizon-mods.txt` as the official game splits them: at `\n` only (Round 6,
    ML1: `\r`, `\v`, `\f`, U+0085, U+001C-U+001F, U+2028 and U+2029 split nothing; a `\r` before
    `\n` stays in the line, whose value it trails)."""
    return text.split("\n")


def _mods_file_id(line: str) -> tuple[str | None, bool]:
    """The package id a `horizon-mods.txt` line names (`mod.<id>` before the first `:`, trimmed
    of `MODS_FILE_SPACE`), or None, and whether it turns it on (the value, trimmed, is exactly
    `true`; L6, Round 6 ML1: `mod.x :true` is on, `mod.i k:true` and a byte order mark before
    `mod.` are not)."""
    key, separator, value = line.partition(":")
    key = key.strip(MODS_FILE_SPACE)
    if not separator or not key.startswith("mod."):
        return None, False
    return key[len("mod."):], value.strip(MODS_FILE_SPACE) == "true"


def _possibly_on(game_dir: Path, package_id: str | None) -> bool:
    """Whether the game may load the package (L5-L7, Round 5 RB2): a package that is off does
    not count toward the game's limits. A package is off only when `horizon-mods.txt` reads and
    has no line that turns it on as the game reads it (`_mods_file_lines`, `_mods_file_id`); a
    missing file turns every package off; an unreadable file or a manifest without an id counts
    as on."""
    if package_id is None:
        return True
    try:
        # Bytes, then decoded: `read_text` would turn a lone `\r` into a line break.
        text = (game_dir / HORIZON.mods_file).read_bytes().decode("utf-8")
    except FileNotFoundError:
        return False
    except (OSError, UnicodeDecodeError):
        return True
    return any(_mods_file_id(line) == (package_id, True) for line in _mods_file_lines(text))


def _enable_horizon(game_dir: Path, ids: list[str]) -> list[str]:
    """`enable` for `horizon-mods.txt`. Lines are the game's (`_mods_file_lines`), and only the
    packages' own lines change: an id the file already turns on is left alone, the first line
    naming an id that is off becomes `mod.<id>:true` (keeping a trailing `\r`), and any other id
    gets a new line at the end. Every other byte of the file stays as it was, so no other mod's
    line is split, joined or turned off (Round 6, ML1)."""
    path = game_dir / HORIZON.mods_file
    text = path.read_bytes().decode("utf-8") if path.exists() else ""
    lines = _mods_file_lines(text)
    written = []
    for mod_id in ids:
        named = [index for index, line in enumerate(lines) if _mods_file_id(line)[0] == mod_id]
        if not any(_mods_file_id(lines[index])[1] for index in named):
            line = f"mod.{mod_id}:true"
            if named:
                index = named[0]
                lines[index] = line + ("\r" if lines[index].endswith("\r") else "")
            elif lines[-1] == "":
                lines[-1:] = [line, ""]
            else:
                lines += [line, ""]  # the last line had no `\n`: it gets one, unchanged
        written.append(mod_id)
    new = "\n".join(lines)
    if new != text:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(new.encode("utf-8"))
    return written


@dataclass
class Budget:
    """The block states and blocks the mods may still add to the official game (RS1, RS2): the
    packages already in `horizon-mods/` that may be turned on and the packages this run writes
    count, and a package this run replaces stops counting once it is replaced. Native mods are not
    counted (the bake cannot see them). A package whose count is unknown (`_Uncountable`) is in
    `unknown`, with the reason: while one other than the package being written is left, nothing
    is left for blocks (RB3).

    `pending` holds the earlier packages of this bake that a jar of the run replaces, decided
    before the first jar is baked (`HorizonRun.settle`): their old size is still in `states` and
    `blocks`, but no jar's room counts it, and an unknown one holds no jar back, so a jar that
    sorts before the one replacing such a package has the room it has on the next run (Round 7,
    RB5)."""

    states: int = 0
    blocks: int = 0
    existing: dict[Path, tuple[int, int]] = field(default_factory=dict)
    unknown: dict[Path, str] = field(default_factory=dict)
    pending: set[Path] = field(default_factory=set)

    @classmethod
    def of(cls, existing: list[Existing], game_dir: Path | None = None) -> Budget:
        budget = cls()
        read = [0]
        for package in existing:
            if game_dir is not None and not _possibly_on(game_dir, package.package_id):
                continue  # off: the game does not count it (RB2)
            path = package.path.resolve()
            try:
                if package.baked_from is not None:
                    # This bake's own earlier output (RB4): read up to what it writes at most.
                    usage = _package_usage(package.path, [0], MAX_TOTAL)
                else:
                    usage = _package_usage(package.path, read)
            except _Uncountable as error:
                budget.unknown[path] = str(error)
                continue
            budget.existing[path] = usage
            budget.states += usage[0]
            budget.blocks += usage[1]
        return budget

    def copy(self) -> Budget:
        return Budget(self.states, self.blocks, dict(self.existing), dict(self.unknown),
                      set(self.pending))

    def uncounted(self, package_path: Path) -> tuple[Path, str] | None:
        """The first package other than the one at `package_path`, and not one the run replaces,
        whose count is unknown."""
        target = package_path.resolve()
        for path, reason in self.unknown.items():
            if path != target and path not in self.pending:
                return path, reason
        return None

    def left(self, package_path: Path) -> tuple[int, int]:
        """The states and blocks a package written at `package_path` may add: the package's own
        old size and those of the packages the run replaces do not count against it."""
        if self.uncounted(package_path) is not None:
            return 0, 0
        states, blocks = self.states, self.blocks
        for path in self.pending | {package_path.resolve()}:
            old_states, old_blocks = self.existing.get(path, (0, 0))
            states -= old_states
            blocks -= old_blocks
        return HORIZON_MOD_STATES - states, HORIZON_MOD_BLOCKS - blocks

    def commit(self, package_path: Path, states: int, blocks: int) -> None:
        self.unknown.pop(package_path.resolve(), None)
        self.pending.discard(package_path.resolve())
        old_states, old_blocks = self.existing.pop(package_path.resolve(), (0, 0))
        self.states += states - old_states
        self.blocks += blocks - old_blocks


def _jar_metadata_ids(jar: Path) -> frozenset[str]:
    """The jar's mod ids (`_jar_mod_ids`), read before any jar is baked; none when the jar cannot
    be read."""
    try:
        with _open_zip(jar) as archive:  # bounded: `_zip_entries` refuses a jar past the bounds
            entries = [
                (info.filename, archive.read(info))
                for info in archive.infolist()
                if info.filename in (FABRIC_METADATA, *TOML_METADATA)
                and not info.is_dir()
                and not info.flag_bits & 0x1
                and info.file_size <= MAX_FILE
            ]
    except (zipfile.BadZipFile, RuntimeError, *JAR_READ_ERRORS):
        return frozenset()
    return frozenset(_jar_mod_ids(entries))


def horizon_max_entry(game_dir: Path) -> int:
    """The longest entry name the bake writes for a game in `game_dir` (RL1)."""
    folder = len(str(game_dir.resolve()).encode("utf-8", "surrogateescape"))
    return min(HORIZON_MAX_ENTRY_BYTES, PATH_MAX_BYTES - HORIZON_CACHE_OVERHEAD - folder)


@dataclass
class HorizonRun:
    """What one `--target horizon` run shares between its jars."""

    existing: list[Existing]
    game_dir: Path
    budget: Budget
    # Each jar's mod ids, by jar file name, read before the first jar is baked.
    mod_ids: dict[str, frozenset[str]]
    max_entry: int
    # The namespaces with blocks not written for an unknown count, with its reason (RB3).
    uncounted: list[tuple[str, str]] = field(default_factory=list)
    # Each jar's probe result (declarations and malformed rows, or the `ProbeError`), by path,
    # kept by the trials of `settle` so that every trial and the run itself read the same one;
    # the run takes each out as it bakes the jar.
    probed: dict[Path, object] = field(default_factory=dict)
    # A trial run of `settle`: every decision is made, no package is written.
    dry: bool = False

    @classmethod
    def start(cls, jars: list[Path], existing: list[Existing], game_dir: Path) -> HorizonRun:
        run = cls(
            existing,
            game_dir,
            Budget.of(existing, game_dir),
            {jar.name: _jar_metadata_ids(jar) for jar in jars},
            horizon_max_entry(game_dir),
        )
        # The earlier packages of this bake that a jar of the run is the owner of (RO1): the
        # first guess of what the run replaces (RB5).
        counted = set(run.budget.existing) | set(run.budget.unknown)
        for package in existing:
            path = package.path.resolve()
            if package.baked_from is None or path not in counted or len(package.owned) != 1:
                continue
            (namespace,) = package.owned
            if any(_same_owner(namespace, package, name, ids) for name, ids in run.mod_ids.items()):
                run.budget.pending.add(path)
        return run

    def probe(self, probe: Path, jar: Path) -> tuple[list[tuple[str, str, dict]], list[str]]:
        """`probe_declarations` of the horizon target, run once per jar and run."""
        if jar in self.probed:
            result = self.probed[jar] if self.dry else self.probed.pop(jar)
        else:
            malformed: list[str] = []
            try:
                result = (probe_declarations(probe, jar, malformed), malformed)
            except ProbeError as error:
                result = error
            if self.dry:
                self.probed[jar] = result
        if isinstance(result, ProbeError):
            raise result
        declarations, malformed = result
        return list(declarations), list(malformed)

    def settle(self, jars: list[Path], out: Path, probe: Path | None) -> None:
        """Keeps in `budget.pending` only the packages the run does replace (RB5). The run is
        tried without writing anything, with the packages still pending not counted against the
        jars; a pending package that the trial does not replace (its jar refused, its namespace
        now held, not written or gone) is counted again, and the run is tried again, until every
        package still pending is replaced. The run then makes the decisions of the last trial, so
        no package whose old size it did not count stays beside what it writes."""
        while self.budget.pending:
            trial = HorizonRun(self.existing, self.game_dir, self.budget.copy(), self.mod_ids,
                               self.max_entry, probed=self.probed, dry=True)
            claimed: dict[str, str] = {}
            replaced = set()
            for jar in jars:
                bake = _bake_jar_horizon(jar, out, claimed, probe, trial)
                replaced |= {package.path.resolve() for package in bake.packages}
            if self.budget.pending <= replaced:
                return
            self.budget.pending &= replaced

    def owner(self, namespace: str, jar: str) -> str | None:
        """Another jar of the run whose mod id is `namespace`, the first by name, or None."""
        for name, ids in sorted(self.mod_ids.items()):
            if name != jar and namespace in ids:
                return name
        return None


def _bake_jar_horizon(
    jar: Path, out: Path, claimed: dict[str, str], probe: Path | None, run: HorizonRun
) -> JarBake:
    """`bake_jar` for the official game: only what the bake checks goes in (see the module text).
    Nothing a jar holds stops the run: what reading it raises refuses that jar, with the
    packages it already wrote kept in the report, and the next jar is baked."""
    bake = JarBake(jar=jar.name)
    try:
        _bake_jar_horizon_into(bake, jar, out, claimed, probe, run)
    except (RuntimeError, *JAR_READ_ERRORS) as error:
        # The net under the guards of each step (`RecursionError` is a `RuntimeError`).
        bake.fail(f"the bake could not finish the jar: {_error_name(error)}: {error}")
    return bake


def _bake_jar_horizon_into(
    bake: JarBake,
    jar: Path,
    out: Path,
    claimed: dict[str, str],
    probe: Path | None,
    run: HorizonRun,
) -> None:
    try:
        entries = _zip_entries(jar, bake, bounded=True)
    except UnsafeJar as error:
        bake.fail(str(error))
        return
    if probe is None:
        bake.fail(
            f"no runtime probe: pass --probe <{PROBE}> or set {PROBE_ENV}, so the jar's blocks "
            "and items can be declared in its package"
        )
        return
    try:
        declarations, malformed = run.probe(probe, jar)
    except ProbeError as error:
        bake.fail(f"the runtime could not run the jar's entrypoints: {error}")
        return
    for row in malformed:
        bake.skip("probe row that is not an object with a string id and a file", row)
    mod_ids = run.mod_ids.get(jar.name)
    if mod_ids is None:
        mod_ids = frozenset(_jar_mod_ids(entries))

    # The runtime's typed registrations become the package's own declarations
    # (`data/<namespace>/horizon/{block,item}/<path>.json`, D1, P4).
    candidates: dict[str, list[tuple[str, str, str, str, object]]] = {}
    declared_namespaces: set[str] = set()
    for kind, declaration_id, file in declarations:
        if kind in ("menu", "menu-refused"):
            # The official game has no content menu declarations (sections 1-3).
            bake.skip("menu type: the official game declares no content menus", declaration_id)
            continue
        namespace, path = declaration_id.split(":", 1) if ":" in declaration_id else ("", "")
        if not namespace or not path:
            bake.skip("declaration id is not namespace:path", declaration_id)
            continue
        if namespace == "minecraft":
            bake.skip("vanilla's namespace: a mod does not declare vanilla content", declaration_id)
            continue
        if namespace in HORIZON_SHARED_NAMESPACES or namespace in HORIZON_BUILTIN_IDS:
            # `c` and `horizon` declarations refuse the content (N4, N5); `horizon_java` is the
            # official game's built-in mod.
            bake.skip("a namespace the official game keeps: no declarations there", declaration_id)
            continue
        if not NAMESPACE.fullmatch(namespace):
            bake.skip("declaration id is not a content namespace and path", declaration_id)
            continue
        if namespace not in mod_ids and run.owner(namespace, jar.name):
            # Another jar of this run names the namespace as its mod id: its package is that
            # jar's, whichever jar sorts first, and this declaration does not refuse it.
            bake.skip(
                "a declaration in the namespace of another jar's mod id (that jar's package; "
                "left out)",
                declaration_id,
            )
            continue
        declared_namespaces.add(namespace)
        entry = f"data/{namespace}/{HORIZON.declarations}/{kind}/{path}.json"
        # `ns:a/` would become `a/.json`, a file whose id ends in `/`: an empty part of the id.
        problem = _horizon_path_problem(
            entry if "" not in path.split("/") else "/", run.max_entry
        )
        if problem:
            bake.skip(problem, declaration_id)
            continue
        # A `-` or `.` namespace has no package of its id (1.3): the namespace loop below refuses
        # it once, with or without files of its own.
        candidates.setdefault(namespace, []).append((kind, declaration_id, namespace, path, file))

    # A package only for the namespaces the jar owns: its runtime declarations' (left out or not,
    # unless another jar's mod id) and its mod ids.
    owned = declared_namespaces | mod_ids
    counts: dict[str, int] = {}
    for name, _ in entries:
        counts[name] = counts.get(name, 0) + 1
    assets: dict[str, list[tuple[str, bytes]]] = {}
    shared: list[tuple[str, bytes]] = []
    for name, data in entries:
        if counts[name] > 1:
            # The official game reads the last copy of a repeated name (RP3); which one the mod
            # itself reads is not established.
            bake.skip("a name the jar holds more than once (not established, left out)", name)
            continue
        reason = _horizon_refused_name(name) or _horizon_path_problem(name, run.max_entry)
        if reason:
            bake.skip(reason, name)
            continue
        classified = _classify(name)
        if classified is None:
            bake.skip("not an assets/ or data/ file", name)
            continue
        top, namespace, rest = classified
        if not _horizon_allowed(top, namespace, rest, name, bake, owned):
            continue
        if _executable_content(data):
            # One such file refuses the whole package in the official game (A11).
            bake.skip("starts like an executable (ELF, MZ, #!, wasm, CAFEBABE, Mach-O)", name)
            continue
        if name.endswith(".json") and not _strict_json(data)[1]:
            bake.skip(HORIZON_JSON_REASON, name)
            continue
        if namespace in HORIZON_SHARED_NAMESPACES:
            shared.append((name, data))
        else:
            assets.setdefault(namespace, []).append((name, data))

    bake.namespaces = sorted(set(assets) | set(candidates))
    for namespace in bake.namespaces:
        if not PACKAGE_ID.match(namespace) or namespace in RESERVED_IDS:
            bake.fail(
                f"namespace {namespace!r} is not a content package id ([a-z0-9_], not a device "
                "name): its files and declarations cannot be a package (docs/mod-plan.md, "
                "Fork seams needed)"
            )
            continue
        package_path = out / f"{namespace}{HORIZON.extension}"
        if namespace in claimed:
            conflict = (
                HELD,
                f"namespace {namespace} is already baked from {claimed[namespace]}; "
                "two content mods may not own one namespace",
            )
        else:
            conflict = _existing_conflict(
                namespace, package_path, run.existing, run.game_dir, jar.name, mod_ids,
                run.mod_ids,
            )
        if conflict and conflict[0] == HELD and namespace not in mod_ids:
            # Not one of the jar's mod ids and another mod holds it: its declarations and files
            # are left out and the jar's own packages stand (Round 4). What only blocks the
            # package (a manifest the bake cannot read, a folder or another id's package at its
            # path) is an error below, mod id or not.
            reason = f"a namespace that is not the jar's mod id, held elsewhere ({conflict[1]})"
            for _, declaration_id, _, _, _ in candidates.get(namespace, []):
                bake.skip(reason, declaration_id)
            for name, _ in assets.get(namespace, []):
                bake.skip(reason, name)
            continue
        if conflict:
            bake.fail(conflict[1])
            continue
        # RP4: both sides of a clash loaded for lang files and declarations; for any other asset
        # kind it is not established, and both sides stay out. Decided before the declarations,
        # which take the shipped blockstates that remain into account (RB1).
        own = list(assets.get(namespace, []))
        clashing = {name for name in _clashes(own) if not _clash_loads(name)}
        for name in sorted(clashing):
            bake.skip(
                "a file and a folder of the same name (both load only for lang files and "
                "declarations, RP4; not established for this kind, left out)",
                name,
            )
        own = [(name, data) for name, data in own if name not in clashing]
        # Every declaration is decided here, before any file of the package is: nothing later
        # leaves one out, so a kept item never names a block or item left out.
        uncounted = run.budget.uncounted(package_path)
        if uncounted is not None and any(
            kind == "block" for kind, *_ in candidates.get(namespace, [])
        ):
            # RB3: with a package whose blocks the bake cannot count, the game's limits could be
            # passed. The namespace is not written at all, so an earlier package of it stays as it
            # was rather than losing its blocks (Round 6, RB4).
            kept_as_is = (
                f"{_relative(package_path, run.game_dir)} left as it is"
                if package_path.exists() else "nothing written"
            )
            bake.fail(
                f"namespace {namespace}: not written: the blocks of "
                f"{_relative(uncounted[0], run.game_dir)} cannot be counted ({uncounted[1]}), so "
                f"its blocks could pass the game's limits (fatal, RS1, RS2, RB3); {kept_as_is}"
            )
            run.uncounted.append((namespace, uncounted[1]))
            continue
        states_left, blocks_left = run.budget.left(package_path)
        kept, states, blocks = _horizon_declarations(
            candidates.get(namespace, []),
            bake,
            states_left,
            blocks_left,
            {name for name, _ in own},
        )
        if not own and not kept:
            continue  # nothing of the namespace is left to write
        files = own
        # The vanilla atlas files go with the first package written, not merely the first
        # namespace, which may be refused. They are another namespace's names, so no clash with
        # the namespace's own (RP4 holds within one namespace's folders).
        if not bake.packages:
            files += shared
        prefix = f"data/{namespace}/{HORIZON.declarations}/"
        files += [(f"{prefix}{kind}/{declaration_id.split(':', 1)[1]}.json", data)
                  for kind, declaration_id, data in kept]
        if len(files) > MAX_FILES or sum(len(data) for _, data in files) > MAX_TOTAL:
            # The jar's caps leave room for its declarations to cross the package's (P5).
            bake.fail(
                f"namespace {namespace}: more than {MAX_FILES} files or 512 MiB in its "
                "package, whose content the official game refuses; not written"
            )
            continue
        try:
            size = 0 if run.dry else write_package(
                package_path, namespace, jar.name, files, HORIZON, mod_ids
            )
        except (OSError, ValueError, struct.error, zipfile.LargeZipFile) as error:
            temporary = package_path.with_name(package_path.name + ".tmp")
            with contextlib.suppress(OSError):
                temporary.unlink()
            bake.fail(f"namespace {namespace}: the package could not be written: {error}")
            continue
        bake.packages.append(
            Package(
                id=namespace,
                namespace=namespace,
                path=package_path,
                files=[name for name, _ in sorted(files)],
                bytes=size,
                blocks=sum(1 for name, _ in files if name.startswith(prefix + "block/")),
                items=sum(1 for name, _ in files if name.startswith(prefix + "item/")),
            )
        )
        claimed[namespace] = jar.name
        run.budget.commit(package_path, states, blocks)
    if shared and not bake.packages:
        if bake.namespaces:
            bake.skip("vanilla atlas additions with no package to carry them", shared[0][0])
        else:
            bake.skip("vanilla atlas additions with no jar namespace to bake into", shared[0][0])


def enable(game_dir: Path, ids: list[str], target: Target = SUNRISE) -> list[str]:
    """Turns the packages on in the target's mods file (`sunrise-mods.txt`, or the official
    game's `horizon-mods.txt`, L5-L7: a `true` line wins); returns the ids it wrote."""
    if not ids:
        return []
    if target is HORIZON:
        return _enable_horizon(game_dir, ids)
    path = game_dir / target.mods_file
    lines = path.read_text(encoding="utf-8").splitlines() if path.exists() else []
    written = []
    for mod_id in ids:
        line = f"mod.{mod_id}:true"
        wanted = f"mod.{mod_id}:"
        replaced = False
        for index, existing in enumerate(lines):
            if existing.strip().startswith(wanted):
                if existing.strip() != line:
                    lines[index] = line
                replaced = True
                break
        if not replaced:
            lines.append(line)
        written.append(mod_id)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")
    return written


def bake_directory(
    java_mods: Path,
    game_dir: Path,
    out: Path | None = None,
    turn_on: bool = True,
    probe: Path | None = None,
    target: Target = SUNRISE,
) -> BakeReport:
    """Bakes every `*.jar` under `java_mods` into `out` (default: the target's folder in
    `game_dir`, `sunrise-mods/` or `horizon-mods/`)."""
    out = out if out is not None else game_dir / target.mods_dir
    report = BakeReport(java_mods=java_mods, game_dir=game_dir, out=out, target=target)
    jars = sorted(java_mods.glob("*.jar")) if java_mods.is_dir() else []
    probe = find_probe(probe)
    claimed: dict[str, str] = {}
    existing: list[Existing] = []
    run = None
    if target is HORIZON:
        # Read once, before this run writes anything: what it writes is not "existing" (R2).
        existing = existing_packages([game_dir / HORIZON.mods_dir, out])
        run = HorizonRun.start(jars, existing, game_dir)
        run.settle(jars, out, probe)
    for jar in jars:
        report.jars.append(bake_jar(jar, out, claimed, probe, target, existing, game_dir, run))
    if run is not None and run.budget.pending:
        # Only when writing a package failed where its trial wrote it (RB5): its old size was
        # not counted against the jars before it, and it is still there.
        report.errors.append(
            "the packages "
            + ", ".join(sorted(_relative(path, game_dir) for path in run.budget.pending))
            + " were to be replaced and were not: with them turned on, the blocks written in "
            "this run can pass the game's limits (RS1, RS2, RB5); turn these packages off or "
            "remove them, then bake again"
        )
    if run is not None and run.uncounted:
        # Said once for the run as well as for each namespace (RB3).
        report.errors.append(
            f"the blocks of the packages in {HORIZON.mods_dir}/ could not be counted "
            f"({run.uncounted[0][1]}): the namespaces with blocks were not written "
            f"({', '.join(namespace for namespace, _ in run.uncounted)})"
        )
    if turn_on:
        ids = [package.id for jar in report.jars for package in jar.packages]
        if target is HORIZON:
            try:
                report.enabled = enable(game_dir, ids, target)
            except (OSError, ValueError) as error:
                # A `horizon-mods.txt` that is not UTF-8 or cannot be written: the report still
                # names the packages, and says they are not turned on.
                report.errors.append(f"{target.mods_file} not updated: {error}")
        else:
            report.enabled = enable(game_dir, ids, target)
    if target is HORIZON:
        _name_stale(report, existing)
    return report


def _name_stale(report: BakeReport, existing: list[Existing]) -> None:
    """Names each package an earlier bake wrote that this run did not rewrite: its namespace or
    its jar is now refused (a namespace with blocks next to a package the bake cannot count
    included, RB3), the jar no longer has it, or the jar is gone or renamed (a version bump). The game still loads it when it is turned on; the bake never deletes a file in the game
    folder."""
    written = {package.path.resolve() for jar in report.jars for package in jar.packages}
    for package in existing:
        if package.baked_from is None or package.path.resolve() in written:
            continue
        (package_id,) = package.owned
        report.stale.append(
            {
                "id": package_id,
                "path": _relative(package.path, report.game_dir),
                "baked_from": package.baked_from,
                # The jar's mod ids its description records; None for a package of an earlier
                # round, which records only the jar.
                "mod_ids": None if package.baked_ids is None else sorted(package.baked_ids),
                "turned_on": _turned_on(report.game_dir, package_id),
            }
        )


def _write_report(report: BakeReport, path: Path) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    text = json.dumps(report.as_json(), indent=2, sort_keys=False)
    path.write_text(text + "\n", encoding="utf-8")


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--game-dir",
        type=Path,
        default=Path("."),
        help="the game folder: packages go in its sunrise-mods/ or horizon-mods/ (default: .)",
    )
    parser.add_argument(
        "--java-mods",
        type=Path,
        help="the jars to bake (default: <game-dir>/config/horizon_java/java-mods)",
    )
    parser.add_argument(
        "--target",
        choices=sorted(TARGETS),
        default=SUNRISE.name,
        help="the game the packages are for: sunrise (.smod) or the official horizon (.hmod) "
        "(default: sunrise)",
    )
    parser.add_argument(
        "--out",
        type=Path,
        help="where the packages go (default: <game-dir>/sunrise-mods or <game-dir>/horizon-mods)",
    )
    parser.add_argument(
        "--no-enable",
        action="store_true",
        help="do not add mod.<id>:true lines to sunrise-mods.txt or horizon-mods.txt (without "
        "it, a package the bake writes is turned on again even when it was turned off by hand)",
    )
    parser.add_argument(
        "--report",
        type=Path,
        help="the report file (default: config/horizon_java/bake-report.json)",
    )
    parser.add_argument(
        "--probe",
        type=Path,
        help=f"the {PROBE} runtime probe (default: ${PROBE_ENV} or a cargo build)",
    )
    args = parser.parse_args(argv)

    target = TARGETS[args.target]
    game_dir = args.game_dir.resolve()
    java_mods = args.java_mods or game_dir / "config" / "horizon_java" / "java-mods"
    out = args.out or game_dir / target.mods_dir
    report = bake_directory(java_mods, game_dir, out, not args.no_enable, args.probe, target)
    report_path = args.report or game_dir / REPORT
    _write_report(report, report_path)

    if target is HORIZON:
        say = _say
    else:
        # The Sunrise target prints as it always did.
        def say(text: str, stream: object = None) -> None:
            print(text, file=stream)

    for error in report.errors:
        say(f"error: {error}", sys.stderr)
    for stale in report.stale:
        state = "turned on" if stale["turned_on"] else "not turned on"
        say(
            f"stale package from an earlier bake of {stale['baked_from']} left in place: "
            f"{stale['path']} ({state} in {target.mods_file}; this run did not rewrite it)",
            sys.stderr,
        )
    for jar in report.jars:
        if jar.error and target is HORIZON and jar.packages:
            # Some namespaces were written and turned on; the others are named, not "refused".
            say(f"{jar.jar}: not written: {jar.error}", sys.stderr)
        elif jar.error:
            say(f"{jar.jar}: refused: {jar.error}", sys.stderr)
            continue
        names = ", ".join(package.id for package in jar.packages) or "no packages"
        skipped = sum(skipped.count for skipped in jar.skipped.values())
        say(f"{jar.jar}: {len(jar.packages)} package(s) [{names}], {skipped} file(s) skipped")
    say(f"report: {report_path}")
    return 1 if report.failed else 0


def _say(text: str, stream: object = None) -> None:
    """Prints a line of the horizon target's summary whatever a jar's file name holds: a byte that
    is not UTF-8 (a lone surrogate from the file system) is shown as `\\xNN`, and a character the
    stream cannot encode as an escape, so the summary never fails once the work is done."""
    stream = sys.stdout if stream is None else stream
    try:
        data = text.encode("utf-8", "surrogateescape")
    except UnicodeEncodeError:
        data = text.encode("utf-8", "backslashreplace")
    text = data.decode("utf-8", "backslashreplace")
    encoding = getattr(stream, "encoding", None) or "utf-8"
    try:
        text = text.encode(encoding, "backslashreplace").decode(encoding)
    except LookupError:
        text = text.encode("ascii", "backslashreplace").decode("ascii")
    print(text, file=stream)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
