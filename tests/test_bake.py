"""The bake turns a Java jar's own assets, data and declarations into a content package.

Milestone 3b: a native mod may not own another namespace's files or declarations, so the jar's
models, textures, names, blockstates, loot and its blocks/items go into a content `.smod` of their
own (docs/m3-content-design.md, recommendation 3). The declarations come from the offline runtime
probe, like the mod's own run; these tests drive the tool with synthetic jars and a fake probe and
check the observable result: the package tree, its manifest, what the report says was left out, and
the zip-slip refusal.
"""

import contextlib
import importlib.util
import io
import json
import os
import sys
import tempfile
import threading
import tomllib
import unittest
import warnings
import zipfile
from pathlib import Path
from unittest import mock

ROOT = Path(__file__).resolve().parents[1]
BAKE = ROOT / "tools" / "bake.py"
sys.path.insert(0, str(Path(__file__).resolve().parent))
import bake_golden  # noqa: E402


def load_bake():
    spec = importlib.util.spec_from_file_location("horizon_bake", BAKE)
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


BAKE_MODULE = load_bake()

# A 1x1 PNG is enough for the copy; the fixture tool generates the real 16x16 art.
PNG = bytes.fromhex(
    "89504e470d0a1a0a0000000d49484452000000010000000108060000001f15c4"
    "890000000d49444154789c6360f8cfc000000301010018dd8db10000000049454e44ae426082"
)

BLOCK = {
    "template": "minecraft:stone",
    "properties": {"hardness": 3.0, "requires_tool": True},
    "item": True,
}
ITEM = {"template": "minecraft:stick", "max_stack_size": 16, "rarity": "rare"}


def make_jar(path: Path, entries: dict[str, bytes]) -> Path:
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
        for name, data in entries.items():
            archive.writestr(name, data)
    return path


def ruby_jar(entries: dict[str, bytes] | None = None) -> dict[str, bytes]:
    files = {
        "fabric.mod.json": b'{"id":"ruby_mod","version":"1.2.3"}',
        "ruby_mod/Entry.class": b"\xca\xfe\xba\xbe",
        "assets/ruby_mod/textures/item/ruby.png": PNG,
        "assets/ruby_mod/items/ruby.json": (
            b'{"model":{"type":"minecraft:model","model":"ruby_mod:item/ruby"}}'
        ),
        "assets/ruby_mod/models/item/ruby.json": b'{"parent":"minecraft:item/generated"}',
        "assets/ruby_mod/lang/en_us.json": b'{"item.ruby_mod.ruby":"Ruby"}',
        "data/ruby_mod/loot_table/blocks/ruby_block.json": b'{"pools":[]}',
        "data/minecraft/tags/block/mineable/pickaxe.json": b'{"values":["ruby_mod:ruby_block"]}',
    }
    if entries:
        files.update(entries)
    return files


def declarations(blocks: list[dict] | None = None, items: list[dict] | None = None) -> str:
    return json.dumps(
        {
            "status": "ok",
            "entrypoints": ["example.Content"],
            "content": {
                "blocks": blocks
                if blocks is not None
                else [{"id": "ruby_mod:ruby_block", "file": BLOCK}],
                "items": items
                if items is not None
                else [{"id": "ruby_mod:ruby", "file": ITEM}],
            },
        }
    )


def fake_probe(root: Path, output: str, exit_code: int = 0) -> Path:
    """A stand-in for `horizon-native-java --json`: it prints the report and ignores the jar."""
    path = root / "horizon-native-java"
    path.write_text(
        "#!/usr/bin/env python3\n"
        "import sys\n"
        f"sys.stdout.write({output!r} + '\\n')\n"
        f"sys.exit({exit_code})\n",
        encoding="utf-8",
    )
    path.chmod(0o755)
    return path


class BakeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.root = Path(self.scratch.name)
        self.game = self.root / "game"
        self.mods = self.game / "config" / "horizon_java" / "java-mods"
        self.mods.mkdir(parents=True)
        self.probe = fake_probe(self.root, declarations())

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def bake(self) -> dict:
        report = BAKE_MODULE.bake_directory(self.mods, self.game, probe=self.probe)
        return report.as_json()

    def test_menu_types_bake_into_sunrise_menu_declarations(self):
        """A jar's own menu types become `data/<ns>/sunrise/menu/<path>.json` beside its blocks
        and items (the live-menu bridge opens them as Sunrise owned menus); a menu type sharing a
        block's id is its own declaration; one whose layout the runtime can't declare is left
        out with the runtime's reason."""
        layout = {"slots": 3, "owned_slots": [99, 99, 99], "inputs": [0, 1], "player_inventory": True}
        report_text = json.loads(declarations())
        report_text["content"]["menus"] = [
            {"id": "ruby_mod:ruby_block", "file": layout},
        ]
        report_text["content"]["menu_refusals"] = [
            {"id": "ruby_mod:odd", "reason": "slot 0 is a ruby_mod/OddSlot"},
        ]
        self.probe = fake_probe(self.root, json.dumps(report_text))
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()

        package = self.game / "sunrise-mods" / "ruby_mod.smod"
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            menu = archive.read("data/ruby_mod/sunrise/menu/ruby_block.json")
        self.assertIn("data/ruby_mod/sunrise/block/ruby_block.json", names)
        self.assertEqual(
            menu,
            b'{"inputs":[0,1],"owned_slots":[99,99,99],"player_inventory":true,"slots":3}\n',
        )
        self.assertNotIn("data/ruby_mod/sunrise/menu/odd.json", names)
        skipped = {entry["reason"]: entry["paths"] for entry in report["jars"][0]["skipped"]}
        self.assertEqual(
            skipped["menu type whose layout is not a declaration: slot 0 is a ruby_mod/OddSlot"],
            ["ruby_mod:odd"],
        )

    def loaded_mods_probe(self, mods: list[str], items: list[dict] | None = None) -> None:
        report_text = json.loads(declarations(items=items))
        report_text["content"]["mods"] = mods
        self.probe = fake_probe(self.root, json.dumps(report_text))

    def test_packages_are_the_mods_the_runtime_loaded_and_other_namespaces_join_the_jars(self):
        """BUGS-3d34-round2 #3: Sunrise's `mod_loaded` reads the enabled packages' ids, Fabric
        Loader's its mod map (the jar's mod and its nested jars). With the probe's `mods`, the
        packages are exactly those: the nested mod gets a manifest-only package, and a namespace
        that is no mod's (a tag file for another mod) is carried by the jar's own package."""
        self.loaded_mods_probe(
            ["ruby_mod", "gem_api"],
            items=[
                {"id": "ruby_mod:ruby", "file": ITEM},
                {"id": "other_ns:gem", "file": {"template": "minecraft:stick"}},
            ],
        )
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar(
                {
                    "data/quiver_mod/tags/item/quivers.json": b'{"values":["ruby_mod:ruby"]}',
                    "assets/other_ns/lang/en_us.json": b'{"item.other_ns.gem":"Gem"}',
                }
            ),
        )
        report = self.bake()

        mods_dir = self.game / "sunrise-mods"
        self.assertEqual(sorted(path.name for path in mods_dir.iterdir()),
                         ["gem_api.smod", "ruby_mod.smod"])
        self.assertEqual(report["enabled"], ["ruby_mod", "gem_api"])
        with zipfile.ZipFile(mods_dir / "ruby_mod.smod") as archive:
            names = archive.namelist()
            manifest = archive.read("sunrise-mod.toml").decode()
        self.assertIn('id = "ruby_mod"\n', manifest)
        self.assertIn('namespaces = ["other_ns", "quiver_mod"]\n', manifest)
        for name in (
            "data/quiver_mod/tags/item/quivers.json",
            "assets/other_ns/lang/en_us.json",
            "data/other_ns/sunrise/item/gem.json",
            "data/ruby_mod/sunrise/item/ruby.json",
            "data/minecraft/tags/block/mineable/pickaxe.json",
        ):
            self.assertIn(name, names)
        with zipfile.ZipFile(mods_dir / "gem_api.smod") as archive:
            self.assertEqual(archive.namelist(), ["sunrise-mod.toml"])
            nested = archive.read("sunrise-mod.toml").decode()
        self.assertIn('id = "gem_api"\n', nested)
        self.assertNotIn("namespaces", nested)
        rows = {row["id"]: row for row in report["jars"][0]["packages"]}
        self.assertEqual(rows["ruby_mod"]["declarations"], {"blocks": 1, "items": 2})
        self.assertEqual(rows["gem_api"]["files"], 0)
        self.assertIsNone(report["jars"][0]["error"])

    def test_an_items_component_patch_is_baked_for_sunrise(self):
        """BUGS-3d34-round2 #4: the runtime's `components` (the lore an `Item.Properties` set)
        is the Sunrise declaration's, byte for byte as the probe wrote it."""
        lore = {"minecraft:lore": [{"translate": "item.ruby_mod.ruby.hint", "color": "gray"}]}
        self.probe = fake_probe(
            self.root, declarations(items=[{"id": "ruby_mod:ruby", "file": {**ITEM, "components": lore}}])
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.bake()
        with zipfile.ZipFile(self.game / "sunrise-mods" / "ruby_mod.smod") as archive:
            item = json.loads(archive.read("data/ruby_mod/sunrise/item/ruby.json"))
        self.assertEqual(item["components"], lore)

    def test_the_horizon_target_keeps_an_item_without_its_component_patch(self):
        lore = {"minecraft:lore": ["Ruby"]}
        self.probe = fake_probe(
            self.root, declarations(items=[{"id": "ruby_mod:ruby", "file": {**ITEM, "components": lore}}])
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = BAKE_MODULE.bake_directory(
            self.mods, self.game, probe=self.probe, target=BAKE_MODULE.HORIZON
        ).as_json()
        with zipfile.ZipFile(self.game / "horizon-mods" / "ruby_mod.hmod") as archive:
            item = json.loads(archive.read("data/ruby_mod/horizon/item/ruby.json"))
        self.assertEqual(item, ITEM)
        skipped = {entry["reason"]: entry["paths"] for entry in report["jars"][0]["skipped"]}
        self.assertEqual(skipped["item components (not established, left out)"], ["ruby_mod:ruby"])

    def test_a_provides_alias_the_runtime_loaded_is_a_manifest_only_package(self):
        """#668 review: Fabric's `isModLoaded` finds a `provides` alias, and Sunrise's `mod_loaded`
        reads package ids only, so the alias the probe names gets a package of its own."""
        self.loaded_mods_probe(["ruby_mod", "ruby_compat"])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(report["enabled"], ["ruby_mod", "ruby_compat"])
        with zipfile.ZipFile(self.game / "sunrise-mods" / "ruby_compat.smod") as archive:
            self.assertEqual(archive.namelist(), ["sunrise-mod.toml"])
            self.assertIn('id = "ruby_compat"\n', archive.read("sunrise-mod.toml").decode())

    def test_a_loaded_mod_that_cannot_be_a_package_is_named(self):
        """A Fabric id may hold `-`; a Sunrise package id may not, so that mod can't be loaded
        there, and the report says so. The jar's other mods still get their packages."""
        self.loaded_mods_probe(["ruby_mod", "gem-api"])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual([row["id"] for row in report["jars"][0]["packages"]], ["ruby_mod"])
        skipped = {entry["reason"]: entry["paths"] for entry in report["jars"][0]["skipped"]}
        self.assertEqual(
            skipped[
                "loaded mod whose id is not a Sunrise package id ([a-z0-9_], not reserved): "
                "Sunrise cannot count it as loaded"
            ],
            ["gem-api"],
        )

    def test_a_namespace_another_jar_baked_refuses_the_package_that_would_carry_it(self):
        make_jar(self.mods / "a_mod.jar", {"fabric.mod.json": b'{"id":"a_mod"}',
                                           "assets/shared_ns/lang/en_us.json": b"{}"})
        make_jar(self.mods / "b_mod.jar", {"fabric.mod.json": b'{"id":"b_mod"}',
                                           "assets/shared_ns/lang/en_us.json": b"{}"})
        report_a = json.loads(declarations(blocks=[], items=[]))
        report_a["content"]["mods"] = ["a_mod"]
        # One probe for both jars: each names itself by the jar it is given.
        path = self.root / "horizon-native-java"
        path.write_text(
            "#!/usr/bin/env python3\nimport json, sys, pathlib\n"
            f"report = json.loads({json.dumps(report_a)!r})\n"
            "report['content']['mods'] = [pathlib.Path(sys.argv[2]).stem]\n"
            "print(json.dumps(report))\n",
            encoding="utf-8",
        )
        path.chmod(0o755)
        self.probe = path
        report = self.bake()
        jars = {row["jar"]: row for row in report["jars"]}
        self.assertEqual([p["id"] for p in jars["a_mod.jar"]["packages"]], ["a_mod"])
        self.assertEqual(jars["b_mod.jar"]["packages"], [])
        self.assertEqual(
            jars["b_mod.jar"]["error"],
            "namespace shared_ns is already baked from a_mod.jar; "
            "two content mods may not own one namespace",
        )

    def test_without_the_probes_mods_each_namespace_is_its_own_package(self):
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar({"data/quiver_mod/tags/item/quivers.json": b'{"values":[]}'}),
        )
        self.bake()
        self.assertEqual(
            sorted(path.name for path in (self.game / "sunrise-mods").iterdir()),
            ["quiver_mod.smod", "ruby_mod.smod"],
        )

    def test_assets_data_and_declarations_bake_into_the_namespace_package(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()

        self.assertEqual(report["enabled"], ["ruby_mod"])
        package = self.game / "sunrise-mods" / "ruby_mod.smod"
        self.assertTrue(package.is_file())
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            manifest = archive.read("sunrise-mod.toml").decode()
            declaration = json.loads(
                archive.read("data/ruby_mod/sunrise/item/ruby.json")
            )
        # The package is the jar's content tree, its declarations and the manifest: no classes,
        # no Fabric metadata, no arbitrary files.
        expected = {
            "data/ruby_mod/sunrise/item/ruby.json",
            "data/ruby_mod/sunrise/block/ruby_block.json",
        } | {name for name in ruby_jar() if name.startswith(("assets/", "data/"))}
        self.assertEqual(names[0], "sunrise-mod.toml")
        self.assertEqual(names[1:], sorted(expected))
        self.assertEqual(declaration, ITEM)
        self.assertIn('id = "ruby_mod"', manifest)
        self.assertIn('api = "0.2"', manifest)
        self.assertIn('side = "both"', manifest)
        self.assertIn('capabilities = ["registries"]', manifest)
        # The declaration is exactly what the runtime recorded, and the loot table comes through
        # byte for byte at the 26.3 path.
        with zipfile.ZipFile(package) as archive:
            self.assertEqual(
                json.loads(archive.read("data/ruby_mod/sunrise/block/ruby_block.json")), BLOCK
            )
            self.assertEqual(
                archive.read("data/ruby_mod/loot_table/blocks/ruby_block.json"), b'{"pools":[]}'
            )
        self.assertEqual(
            report["jars"][0]["packages"][0]["declarations"], {"blocks": 1, "items": 1}
        )
        # The report names the files it did not copy, with reasons.
        skipped = {entry["reason"]: entry for entry in report["jars"][0]["skipped"]}
        self.assertIn("not an assets/ or data/ file", skipped)
        examples = [path for entry in skipped.values() for path in entry["paths"]]
        self.assertIn("ruby_mod/Entry.class", examples)
        # The package is deterministic: a second bake writes the same bytes.
        before = package.read_bytes()
        self.bake()
        self.assertEqual(package.read_bytes(), before)
        # `sunrise-mods.txt` turns it on without disturbing other lines.
        self.assertEqual((self.game / "sunrise-mods.txt").read_text(), "mod.ruby_mod:true\n")

    def test_the_block_item_is_not_declared_twice(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.probe = fake_probe(
            self.root,
            declarations(
                items=[
                    {"id": "ruby_mod:ruby", "file": ITEM},
                    {"id": "ruby_mod:ruby_block", "file": {"template": "minecraft:stone"}},
                ]
            ),
        )
        report = self.bake()
        with zipfile.ZipFile(self.game / "sunrise-mods" / "ruby_mod.smod") as archive:
            names = archive.namelist()
        self.assertIn("data/ruby_mod/sunrise/block/ruby_block.json", names)
        self.assertIn("data/ruby_mod/sunrise/item/ruby.json", names)
        self.assertNotIn("data/ruby_mod/sunrise/item/ruby_block.json", names)
        reasons = {entry["reason"] for entry in report["jars"][0]["skipped"]}
        self.assertIn("the block's own item (its block declares it)", reasons)

    def test_unsafe_entry_paths_refuse_the_jar_whole(self):
        unsafe_paths = (
            "../escape.json",
            "/absolute.json",
            "assets/ruby_mod/../../x.json",
            "a\\b.json",
        )
        for unsafe in unsafe_paths:
            with self.subTest(unsafe=unsafe):
                entries = {"assets/ruby_mod/lang/en_us.json": b"{}", unsafe: b"{}"}
                make_jar(self.mods / "evil.jar", entries)
                report = self.bake()
                row = report["jars"][0]
                self.assertIsNotNone(row["error"])
                self.assertIn("unsafe entry path", row["error"])
                self.assertEqual(row["packages"], [])
                self.assertFalse((self.game / "sunrise-mods" / "evil.smod").exists())
                self.assertFalse((self.root / "escape.json").exists())

    def test_recipes_of_a_mods_own_type_stay_out(self):
        # Sunrise refuses to start on a recipe whose serializer is not registered, and it registers
        # none for any mod's type: a recipe of the jar's own type, or of another mod's (a jar that
        # adds recipes for a library's serializer), stays out. Vanilla's (namespaced or not) stay in.
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar(
                {
                    "data/ruby_mod/recipe/altar.json": b'{"type": "ruby_mod:altar", "base": "x"}',
                    "data/ruby_mod/recipe/hand.json": b'{"type": "minecraft:crafting_shapeless"}',
                    "data/ruby_mod/recipe/bare.json": b'{"type": "crafting_shaped"}',
                    "data/ruby_mod/recipe/lib.json": b'{"type": "other_lib:infuse"}',
                }
            ),
        )
        report = self.bake()
        with zipfile.ZipFile(self.game / "sunrise-mods" / "ruby_mod.smod") as archive:
            names = set(archive.namelist())
        self.assertNotIn("data/ruby_mod/recipe/altar.json", names)
        self.assertIn("data/ruby_mod/recipe/hand.json", names)
        self.assertIn("data/ruby_mod/recipe/bare.json", names)
        reasons = {entry["reason"]: entry["paths"] for entry in report["jars"][0]["skipped"]}
        self.assertEqual(
            reasons["recipe of a mod's own type ruby_mod:altar: no serializer for it in the game yet"],
            ["data/ruby_mod/recipe/altar.json"],
        )
        self.assertNotIn("data/ruby_mod/recipe/lib.json", names)
        self.assertEqual(
            reasons["recipe of a mod's own type other_lib:infuse: no serializer for it in the game yet"],
            ["data/ruby_mod/recipe/lib.json"],
        )

    def test_unsupported_kinds_are_skipped_and_reported(self):
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar(
                {
                    "assets/ruby_mod/classes/Ruby.class": b"\xca\xfe\xba\xbe",
                    "assets/ruby_mod/nonsense/thing.bin": b"x",
                    "data/ruby_mod/sunrise/block/other.json": b"{}",
                    "assets/minecraft/models/item/stone.json": b"{}",
                    "assets/ruby_mod/sounds.json": b"{}",
                }
            ),
        )
        report = self.bake()
        with zipfile.ZipFile(self.game / "sunrise-mods" / "ruby_mod.smod") as archive:
            names = set(archive.namelist())

        self.assertIn("assets/ruby_mod/sounds.json", names)
        self.assertNotIn("assets/ruby_mod/classes/Ruby.class", names)
        self.assertNotIn("assets/ruby_mod/nonsense/thing.bin", names)
        # The jar's own `sunrise/` declarations are not copied; the runtime's are.
        self.assertNotIn("data/ruby_mod/sunrise/block/other.json", names)
        self.assertIn("data/ruby_mod/sunrise/block/ruby_block.json", names)
        self.assertNotIn("assets/minecraft/models/item/stone.json", names)

        reasons = {entry["reason"] for entry in report["jars"][0]["skipped"]}
        self.assertIn("code or an executable, never copied", reasons)
        self.assertIn("unsupported asset kind", reasons)
        self.assertIn("vanilla's namespace: only tag and atlas additions are a mod's", reasons)
        self.assertIn(
            "content declarations: the runtime declares this jar's blocks and items", reasons
        )

    def test_a_namespace_that_cannot_be_a_package_id_is_refused_by_name(self):
        make_jar(
            self.mods / "ruby-mod.jar",
            {
                "assets/ruby-mod/lang/en_us.json": b'{"item.ruby-mod.ruby":"Ruby"}',
                "fabric.mod.json": b'{"id":"ruby-mod"}',
            },
        )
        self.probe = fake_probe(
            self.root,
            declarations(blocks=[], items=[{"id": "ruby-mod:ruby", "file": ITEM}]),
        )
        report = self.bake()
        row = report["jars"][0]
        self.assertEqual(row["packages"], [])
        self.assertIn("not a content package id", row["error"])
        self.assertFalse((self.game / "sunrise-mods" / "ruby-mod.smod").exists())

    def test_a_probe_that_cannot_run_the_jar_refuses_it(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.probe = fake_probe(self.root, "not a report", exit_code=2)
        report = self.bake()
        row = report["jars"][0]
        self.assertIn("could not run the jar's entrypoints", row["error"])
        self.assertEqual(row["packages"], [])
        self.assertFalse((self.game / "sunrise-mods" / "ruby_mod.smod").exists())

    def test_class_files_are_never_copied_even_under_an_allowed_kind(self):
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar(
                {
                    "assets/ruby_mod/textures/item/Ruby.class": b"\xca\xfe\xba\xbe",
                    "assets/ruby_mod/models/item/ruby.jar": b"PK\x05\x06",
                }
            ),
        )
        self.bake()
        with zipfile.ZipFile(self.game / "sunrise-mods" / "ruby_mod.smod") as archive:
            names = archive.namelist()
        self.assertNotIn("assets/ruby_mod/textures/item/Ruby.class", names)
        self.assertNotIn("assets/ruby_mod/models/item/ruby.jar", names)


TEMPLATE = (
    "block template the bake cannot check (it takes minecraft:stone and minecraft:crafting_table, R9)"
)
SOUNDS = "block sounds the bake cannot check (not one of the 878 vanilla blocks it checked, R7)"
HARDNESS = "block property hardness is not a finite f32 of 0 or more, or -1 (refused, R6)"
RESISTANCE = "block property explosion_resistance is not a finite f32 of 0 or more (refused, R6)"
FRICTION = "block property friction is not a number from 0 to 2 (refused, R6)"
JSON_REASON = BAKE_MODULE.HORIZON_JSON_REASON


def block(**fields) -> dict:
    """A block declaration the official game loads, with `fields` changed."""
    file = {
        "template": "minecraft:stone",
        "properties": {},
        "state_properties": {},
        "default_state": {},
        "item": True,
    }
    file.update(fields)
    return file


class HorizonTargetTests(unittest.TestCase):
    """`--target horizon`: packages the official Horizon build reads (the clean-room package
    specification), and everything that build refuses left out with a counted reason."""

    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.root = Path(self.scratch.name)
        self.game = self.root / "game"
        self.mods = self.game / "config" / "horizon_java" / "java-mods"
        self.mods.mkdir(parents=True)
        self.probe = fake_probe(self.root, declarations())

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def main(self, *arguments: str) -> int:
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            return BAKE_MODULE.main(
                ["--game-dir", str(self.game), "--probe", str(self.probe), *arguments]
            )

    def bake(self) -> dict:
        report = BAKE_MODULE.bake_directory(
            self.mods, self.game, probe=self.probe, target=BAKE_MODULE.HORIZON
        )
        return report.as_json()

    def declare(self, blocks: list[dict] | None = None, items: list[dict] | None = None) -> None:
        self.probe = fake_probe(self.root, declarations(blocks or [], items or []))

    def names(self, package: str = "ruby_mod") -> list[str]:
        with zipfile.ZipFile(self.game / "horizon-mods" / f"{package}.hmod") as archive:
            return archive.namelist()

    @staticmethod
    def reasons(report: dict) -> dict[str, list[str]]:
        return {entry["reason"]: entry["paths"] for entry in report["jars"][0]["skipped"]}

    def test_main_bakes_the_official_package_layout(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.assertEqual(self.main("--target", "horizon"), 0)

        package = self.game / "horizon-mods" / "ruby_mod.hmod"
        self.assertTrue(package.is_file())
        self.assertFalse((self.game / "sunrise-mods").exists())
        self.assertFalse((self.game / "sunrise-mods.txt").exists())
        with zipfile.ZipFile(package) as archive:
            names = archive.namelist()
            manifest = tomllib.loads(archive.read("horizon-mod.toml").decode())
            infos = archive.infolist()
            block_file = json.loads(archive.read("data/ruby_mod/horizon/block/ruby_block.json"))
            item_file = json.loads(archive.read("data/ruby_mod/horizon/item/ruby.json"))
        # Assets and the runtime's declarations only: every `data/` file of the jar stays out.
        expected = {
            "data/ruby_mod/horizon/item/ruby.json",
            "data/ruby_mod/horizon/block/ruby_block.json",
        } | {name for name in ruby_jar() if name.startswith("assets/")}
        # The manifest at the top, no directory level above `assets/`/`data/` (A1, 4.1).
        self.assertEqual(names[0], "horizon-mod.toml")
        self.assertEqual(names[1:], sorted(expected))
        self.assertEqual(
            manifest,
            {
                "id": "ruby_mod",
                "name": "ruby_mod",
                "version": "1.0.0",
                "description": (
                    "Assets and data baked from ruby_mod.jar for the ruby_mod namespace. "
                    "Mod ids of the jar: ruby_mod."
                ),
                "authors": [],
                "api": "0.2",
                "side": "both",
                "capabilities": ["registries"],
            },
        )
        # Deflated, mode 0644, regular files only (A2, A8, A9).
        for info in infos:
            self.assertEqual(info.compress_type, zipfile.ZIP_DEFLATED)
            self.assertEqual(info.external_attr >> 16, 0o644)
        self.assertEqual(block_file, BLOCK)
        self.assertEqual(item_file, ITEM)
        # `mod.<id>:true` in `horizon-mods.txt` turns it on (L5).
        self.assertEqual((self.game / "horizon-mods.txt").read_text(), "mod.ruby_mod:true\n")
        report_path = self.game / "config" / "horizon_java" / "bake-report.json"
        report = json.loads(report_path.read_text())
        self.assertEqual(report["target"], "horizon")
        self.assertEqual(report["out"], "horizon-mods")
        self.assertEqual(report["enabled"], ["ruby_mod"])
        row = report["jars"][0]["packages"][0]
        self.assertEqual(row["path"], "horizon-mods/ruby_mod.hmod")
        self.assertEqual(row["declarations"], {"blocks": 1, "items": 1})
        skipped = {entry["reason"]: entry["paths"] for entry in report["jars"][0]["skipped"]}
        self.assertEqual(
            skipped[BAKE_MODULE.HORIZON_DATA_REASON],
            ["data/ruby_mod/loot_table/blocks/ruby_block.json",
             "data/minecraft/tags/block/mineable/pickaxe.json"],
        )
        self.assertEqual(report["stale"], [])
        # Deterministic, like the Sunrise package.
        before = package.read_bytes()
        self.main("--target", "horizon")
        self.assertEqual(package.read_bytes(), before)

    def test_the_default_target_writes_what_the_original_bake_wrote(self):
        # `tests/data/bake_sunrise_golden.json` is the output of tools/bake.py at 91040874, before
        # the horizon target, for an input that carries every horizon-only rule's trigger.
        golden = json.loads(bake_golden.GOLDEN.read_text())
        self.assertEqual(golden.pop("generated_by"), "tools/bake.py at 91040874")
        for arguments in ((), ("--target", "sunrise")):
            with self.subTest(arguments=arguments), tempfile.TemporaryDirectory() as scratch:
                self.assertEqual(bake_golden.run(BAKE_MODULE, Path(scratch), *arguments), golden)

    def test_an_explicit_out_is_kept_for_the_horizon_target(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        out = self.root / "elsewhere"
        self.assertEqual(self.main("--target", "horizon", "--out", str(out)), 0)
        self.assertTrue((out / "ruby_mod.hmod").is_file())
        self.assertFalse((self.game / "horizon-mods").exists())

    def test_executable_names_and_contents_are_left_out(self):
        # A10, A11: one such file refuses the whole package, whatever its name or folder.
        magic = {
            "elf": b"\x7fELF\x02\x01",
            "mz": b"MZ is a word",
            "shebang": b"#!/bin/sh\n",
            "wasm": b"\x00asm\x01\x00",
            "cafe": b"\xca\xfe\xba\xbe",
            "macho64": b"\xcf\xfa\xed\xfe",
            "macho32": b"\xce\xfa\xed\xfe",
            "machobe": b"\xfe\xed\xfa\xcf",
        }
        extra = {f"assets/ruby_mod/textures/item/{name}.png": data for name, data in magic.items()}
        extra["assets/ruby_mod/textures/item/run.com"] = b"plain text\n"
        extra["assets/ruby_mod/textures/item/run.ps1"] = b"plain text\n"
        # Not executables for the official game: `#!` later in a file, a zip, gzip.
        extra["assets/ruby_mod/lang/x.json"] = b'{"a":"#!"}'
        extra["assets/ruby_mod/textures/item/zip.png"] = b"PK\x03\x04"
        extra["assets/ruby_mod/textures/item/gz.png"] = b"\x1f\x8b\x08"
        make_jar(self.mods / "ruby_mod.jar", ruby_jar(extra))
        report = self.bake()
        names = set(self.names())
        reasons = self.reasons(report)
        for name in magic:
            self.assertNotIn(f"assets/ruby_mod/textures/item/{name}.png", names)
        self.assertEqual(
            sum(1 for entry in report["jars"][0]["skipped"]
                if entry["reason"].startswith("starts like an executable")
                for _ in range(entry["count"])),
            len(magic),
        )
        self.assertNotIn("assets/ruby_mod/textures/item/run.com", names)
        self.assertNotIn("assets/ruby_mod/textures/item/run.ps1", names)
        self.assertEqual(
            reasons["an executable name the official game refuses the package for (A10)"],
            ["ruby_mod/Entry.class", "assets/ruby_mod/textures/item/run.com",
             "assets/ruby_mod/textures/item/run.ps1"],
        )
        for kept in ("lang/x.json", "textures/item/zip.png", "textures/item/gz.png"):
            self.assertIn(f"assets/ruby_mod/{kept}", names)

    def test_the_official_games_own_namespaces(self):
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar(
                {
                    "data/horizon/tags/block/villager_secondary_poi/farmer.json": b'{"values":[]}',
                    "data/c/tags/item/gems.json": b'{"values":["ruby_mod:ruby"]}',
                    "data/horizon_java/loot_table/x.json": b'{"pools":[]}',
                    "assets/minecraft/atlases/blocks.json": b'{"sources":[]}',
                    "assets/minecraft/lang/en_us.json": b"{}",
                    "assets/c/lang/en_us.json": b"{}",
                    "assets/horizon/lang/en_us.json": b"{}",
                    "assets/horizon_java/lang/en_us.json": b"{}",
                }
            ),
        )
        self.declare(
            blocks=[
                {"id": "ruby_mod:ruby_block", "file": BLOCK},
                {"id": "c:gem_block", "file": BLOCK},
                {"id": "horizon:gem_block", "file": BLOCK},
                {"id": "horizon_java:proof_block", "file": BLOCK},
            ],
            items=[{"id": "horizon_java:proof_item", "file": ITEM}],
        )
        report = self.bake()
        names = set(self.names())
        # N3: vanilla's atlases are a mod's to add to; every other shared-namespace file is not,
        # and the `c` and `horizon` tags are data, which stays out.
        self.assertIn("assets/minecraft/atlases/blocks.json", names)
        reasons = self.reasons(report)
        self.assertEqual(
            sorted(reasons[BAKE_MODULE.HORIZON_DATA_REASON]),
            ["data/c/tags/item/gems.json",
             "data/horizon/tags/block/villager_secondary_poi/farmer.json",
             "data/horizon_java/loot_table/x.json",
             "data/minecraft/tags/block/mineable/pickaxe.json",
             "data/ruby_mod/loot_table/blocks/ruby_block.json"],
        )
        self.assertEqual(
            reasons["vanilla's namespace: only tag and atlas additions are a mod's"],
            ["assets/minecraft/lang/en_us.json", "assets/c/lang/en_us.json",
             "assets/horizon/lang/en_us.json"],
        )
        self.assertEqual(
            reasons["the official game's built-in horizon_java mod owns this namespace"],
            ["assets/horizon_java/lang/en_us.json"],
        )
        self.assertEqual(
            reasons["a namespace the official game keeps: no declarations there"],
            ["c:gem_block", "horizon:gem_block", "horizon_java:proof_block",
             "horizon_java:proof_item"],
        )
        self.assertEqual([package["id"] for package in report["jars"][0]["packages"]], ["ruby_mod"])
        self.assertEqual(
            {name for name in names
             if not name.startswith("assets/ruby_mod/") and "/horizon/" not in name},
            {"horizon-mod.toml", "assets/minecraft/atlases/blocks.json"},
        )
        self.assertFalse((self.game / "horizon-mods" / "horizon_java.hmod").exists())
        self.assertFalse((self.game / "horizon-mods" / "c.hmod").exists())

    def test_a_trailing_newline_is_not_a_plain_path(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar({"assets/ruby_mod/lang/x.json\n": b"{}"}))
        report = self.bake()
        self.assertNotIn("assets/ruby_mod/lang/x.json\n", self.names())
        self.assertIn(
            "assets/ruby_mod/lang/x.json\n", self.reasons(report)["not a plain content path"]
        )

    def test_a_valid_block_with_states_and_shapes_is_written_as_recorded(self):
        machine = block(
            properties={
                "hardness": 2,
                "explosion_resistance": 6.0,
                "friction": 0.6,
                "requires_tool": True,
                "occludes": False,
                "light": 15,
                "map_color": 61,
                "sounds": "minecraft:amethyst_block",
            },
            state_properties={
                "lit": {"type": "boolean"},
                "age": {"type": "enum", "values": ["0", "1", "2"]},
            },
            default_state={"lit": "true"},
            shapes={"properties": ["lit"], "values": {"true": [], "false": [[0, 0, 0, 1, 0.5, 1]]}},
            collision_shapes={"property": "age", "default": [[-0.5, 0, 0, 1.5, 2, 1]] * 16},
        )
        self.declare(
            blocks=[
                {"id": "ruby_mod:machine", "file": machine},
                {"id": "ruby_mod:ghost", "file": block(item=False, block_item="minecraft:diamond")},
            ],
            items=[
                {"id": "ruby_mod:ghost", "file": {"template": "minecraft:stick"}},
                {"id": "ruby_mod:ruby", "file": dict(ITEM, crafting_remainder="ruby_mod:machine")},
                {"id": "ruby_mod:dust", "file": {"template": "minecraft:paper", "rarity": "epic",
                                                "crafting_remainder": "minecraft:paper"}},
            ],
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(
            report["jars"][0]["packages"][0]["declarations"], {"blocks": 2, "items": 3}
        )
        with zipfile.ZipFile(self.game / "horizon-mods" / "ruby_mod.hmod") as archive:
            self.assertEqual(
                json.loads(archive.read("data/ruby_mod/horizon/block/machine.json")), machine
            )
        # An item file beside a block without its item is the official game's `bi_sep` case.
        self.assertIn("data/ruby_mod/horizon/item/ghost.json", self.names())

    def test_every_block_the_official_game_refuses_or_stops_on_is_left_out(self):
        enum = {"type": "enum", "values": ["x", "y"]}
        cases = {
            "unknown key": (
                block(bogus_key=1),
                "block key the official game refuses (unknown field, 3.2)",
            ),
            "not established key": (
                block(components={}),
                "block key components (not established, left out)",
            ),
            "no template": ({"properties": {}}, TEMPLATE),
            "template without namespace": (block(template="stone"), TEMPLATE),
            "unknown template": (block(template="minecraft:nonexistent_block"), TEMPLATE),
            "air template": (block(template="minecraft:air"), TEMPLATE),
            "template whose item has another id": (block(template="minecraft:wall_torch"), TEMPLATE),
            "itemless block on an itemless template": (
                block(template="minecraft:water", item=False),
                TEMPLATE,
            ),
            "a real block the bake cannot check": (block(template="minecraft:white_wool"), TEMPLATE),
            "a list template": (block(template=["minecraft:stone"]), TEMPLATE),
            "null item": (block(item=None), "block item is not a boolean (refused, 3.2)"),
            "block_item with the block's item": (
                block(block_item="minecraft:diamond"),
                "block_item on a block that has its own item (not established, left out)",
            ),
            "unknown block_item": (
                block(item=False, block_item="minecraft:nonexistent_item"),
                "block_item is not a vanilla item (fatal in the official game, 3.2)",
            ),
            "unknown property": (
                block(properties={"bogus_key": 1}),
                "block property the official game refuses (unknown field, 3.2)",
            ),
            "string hardness": (block(properties={"hardness": "3"}), HARDNESS),
            "boolean friction": (block(properties={"friction": True}), FRICTION),
            "string occludes": (
                block(properties={"occludes": "false"}),
                "block property occludes is not a boolean (refused, 3.2)",
            ),
            "light 16": (
                block(properties={"light": 16}),
                "block property light is not an integer 0 to 15 (refused, 3.2, R6)",
            ),
            "map_color 62": (
                block(properties={"map_color": 62}),
                "block property map_color is not an integer 0 to 61 (refused, 3.2, R6)",
            ),
            "unknown sounds": (block(properties={"sounds": "minecraft:bogus_sound_type"}), SOUNDS),
            "sounds of a block outside the table": (
                block(properties={"sounds": "minecraft:copper_block"}),
                SOUNDS,
            ),
            "list sounds": (block(properties={"sounds": ["minecraft:stone"]}), SOUNDS),
            "not established property": (
                block(properties={"render_shape": "model"}),
                "block property render_shape (not established, left out)",
            ),
            "upper-case state name": (
                block(state_properties={"Facing": {"type": "boolean"}}),
                "state property name is not [a-z0-9_] (refused, 3.2)",
            ),
            "integer state type": (
                block(state_properties={"age": {"type": "integer", "values": ["0"]}}),
                "state property type is not boolean or enum (refused, 3.2)",
            ),
            "enum without values": (
                block(state_properties={"e": {"type": "enum"}}),
                "enum state property without values (refused, 3.2)",
            ),
            "enum with no value": (
                block(state_properties={"e": {"type": "enum", "values": []}}),
                "enum state property without values (refused, 3.2)",
            ),
            "upper-case value": (
                block(state_properties={"facing": {"type": "enum", "values": ["North"]}}),
                "state property value is not [a-z0-9_] (refused, 3.2)",
            ),
            "repeated value": (
                block(state_properties={"e": {"type": "enum", "values": ["a", "a"]}}),
                "state property value repeats (refused, 3.2)",
            ),
            "over 65536 states": (
                block(state_properties={f"b{index}": {"type": "boolean"} for index in range(17)}),
                "more than 65536 states in one block (TooManyStates, fatal in the official game, "
                "RS3)",
            ),
            "state on a template the bake does not know": (
                block(template="minecraft:furnace", state_properties={"lit": {"type": "boolean"}}),
                TEMPLATE,
            ),
            "default of no value": (
                block(state_properties={"lit": {"type": "boolean"}}, default_state={"lit": "yes"}),
                "default_state value is not one of its property's (fatal, 3.2)",
            ),
            "default of a property the block lacks": (
                block(default_state={"facing": "north"}),
                "default_state names a property the block lacks (fatal in the official game, 3.2)",
            ),
            "default of a template property": (
                block(template="minecraft:furnace", default_state={"facing": "south"}),
                TEMPLATE,
            ),
            "shape key": (
                block(shapes={"bogus_key": 1}),
                "block shape table key the official game refuses (unknown field, 3.2)",
            ),
            "17 boxes": (
                block(shapes={"default": [[0, 0, 0, 1, 1, 1]] * 17}),
                "block shape of over 16 boxes or a box not 6 ordered finite numbers (refused, 3.2)",
            ),
            "box of 5": (
                block(shapes={"default": [[0, 0, 0, 1, 1]]}),
                "block shape of over 16 boxes or a box not 6 ordered finite numbers (refused, 3.2)",
            ),
            "min over max": (
                block(collision_shapes={"default": [[1, 0, 0, 0, 1, 1]]}),
                "block shape of over 16 boxes or a box not 6 ordered finite numbers (refused, 3.2)",
            ),
            "impossible tuple": (
                block(
                    state_properties={"e": enum},
                    shapes={"properties": ["e"], "values": {"z": []}, "default": []},
                ),
                "block shape tuple of no state of the block (fatal in the official game, 3.2)",
            ),
            "uncovered tuple": (
                block(
                    state_properties={"e": enum}, shapes={"properties": ["e"], "values": {"x": []}}
                ),
                "block shape leaves a state uncovered, no default "
                "(fatal in the official game, 3.2)",
            ),
            "shape of a property the block lacks": (
                block(shapes={"properties": ["nope"], "default": []}),
                "block shape names a property the block lacks (fatal in the official game, 3.2)",
            ),
            "empty shape table": (
                block(shapes={}),
                "block shape table without properties, with values or no default (not established)",
            ),
        }
        blocks = [
            {"id": f"ruby_mod:case{index}", "file": file}
            for index, (file, _) in enumerate(cases.values())
        ]
        self.declare(blocks=blocks + [{"id": "ruby_mod:fine", "file": block()}])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        reasons = self.reasons(report)
        vanilla = BAKE_MODULE.vanilla_tables()
        for label, (file, reason) in cases.items():
            with self.subTest(label):
                self.assertEqual(
                    BAKE_MODULE._horizon_declaration_problem("block", file, vanilla), reason
                )
        counts = {entry["reason"]: entry["count"] for entry in report["jars"][0]["skipped"]}
        for reason in {reason for _, reason in cases.values()}:
            self.assertEqual(
                counts[reason], sum(1 for _, case in cases.values() if case == reason), reason
            )
        names = self.names()
        self.assertEqual(
            [name for name in names if "/horizon/" in name],
            ["data/ruby_mod/horizon/block/fine.json"],
        )
        skipped = sum(entry["count"] for entry in report["jars"][0]["skipped"]
                      if entry["reason"] in {reason for _, reason in cases.values()})
        self.assertEqual(skipped, len(cases))

    def test_every_item_the_official_game_refuses_or_stops_on_is_left_out(self):
        unknown_template = (
            "item template is not in the vanilla item table (not established, left out)"
        )
        cases = {
            "unknown key": (
                {"template": "minecraft:stick", "bogus_key": 1},
                "item key the official game refuses (unknown field, 3.3)",
            ),
            # `components` is not here: the item is kept without it
            # (`test_the_horizon_target_keeps_an_item_without_its_component_patch`).
            "arm_pose": (
                {"template": "minecraft:stick", "arm_pose": "bow"},
                "item key arm_pose (not established, left out)",
            ),
            "unknown template": ({"template": "minecraft:nonexistent_item"}, unknown_template),
            "air template": ({"template": "minecraft:air"}, unknown_template),
            "stack 0": (
                {"template": "minecraft:stick", "max_stack_size": 0},
                "item max_stack_size is not 1 to 99 (refused, 3.3)",
            ),
            "stack 100": (
                {"template": "minecraft:stick", "max_stack_size": 100},
                "item max_stack_size is not 1 to 99 (refused, 3.3)",
            ),
            "rarity": (
                {"template": "minecraft:stick", "rarity": "legendary"},
                "item rarity is not common, uncommon, rare or epic (refused, 3.3)",
            ),
            "remainder not a string": (
                {"template": "minecraft:stick", "crafting_remainder": 1},
                "item crafting_remainder is not an item id (fatal in the official game, 3.3)",
            ),
        }
        remainder = (
            "item crafting_remainder is not a vanilla item or one of the package's own "
            "(fatal in the official game, 3.3)"
        )
        items = [
            {"id": f"ruby_mod:case{index}", "file": file}
            for index, (file, _) in enumerate(cases.values())
        ]
        items += [
            {
                "id": "ruby_mod:unknown_rest",
                "file": {
                    "template": "minecraft:stick",
                    "crafting_remainder": "minecraft:nonexistent_item",
                },
            },
            # A remainder of an item that is itself left out takes this one with it.
            {"id": "ruby_mod:chained", "file": {"template": "minecraft:stick",
                                                "crafting_remainder": "ruby_mod:unknown_rest"}},
            {"id": "ruby_mod:foreign", "file": {"template": "minecraft:stick",
                                                "crafting_remainder": "other_mod:gem"}},
            {"id": "ruby_mod:own", "file": {"template": "minecraft:stick",
                                            "crafting_remainder": "ruby_mod:ruby_block"}},
            # The block has its item: an item file of its id refuses the content (3.2).
            {"id": "ruby_mod:ruby_block", "file": {"template": "minecraft:stick"}},
        ]
        self.declare(blocks=[{"id": "ruby_mod:ruby_block", "file": BLOCK}], items=items)
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        reasons = self.reasons(report)
        for index, (label, (_, reason)) in enumerate(cases.items()):
            with self.subTest(label):
                self.assertIn(f"ruby_mod:case{index}", reasons[reason])
        self.assertEqual(
            sorted(reasons[remainder]),
            ["ruby_mod:chained", "ruby_mod:foreign", "ruby_mod:unknown_rest"],
        )
        self.assertEqual(
            reasons["the block's own item (its block declares it)"], ["ruby_mod:ruby_block"]
        )
        self.assertEqual(
            [name for name in self.names() if "/horizon/" in name],
            ["data/ruby_mod/horizon/block/ruby_block.json", "data/ruby_mod/horizon/item/own.json"],
        )

    def test_without_the_vanilla_tables_nothing_is_declared(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        with mock.patch.object(BAKE_MODULE, "vanilla_tables", return_value=None):
            report = self.bake()
        reason = "no vanilla block and item tables to check the declaration against"
        self.assertEqual(
            sorted(self.reasons(report)[reason]), ["ruby_mod:ruby", "ruby_mod:ruby_block"]
        )
        self.assertFalse([name for name in self.names() if "/horizon/" in name])

    def test_the_vanilla_tables_are_the_full_capture(self):
        vanilla = BAKE_MODULE.vanilla_tables()
        self.assertEqual(len(vanilla.blocks), 878)
        # The official build's vanilla baseline is 1658 items (spec 0, experiment `base`).
        self.assertEqual(len(vanilla.items), 1658)
        self.assertEqual(vanilla.blocks["minecraft:stone"], "minecraft:stone")
        # The prototype `max_damage` table: the 84 items no stack over 1 of can exist (R8).
        self.assertEqual(len(vanilla.damageable), 84)
        self.assertIn("minecraft:diamond_sword", vanilla.damageable)
        self.assertNotIn("minecraft:stick", vanilla.damageable)

    def test_a_package_over_the_official_caps_is_not_written(self):
        # P5: the jar's caps leave room for its declarations to cross the package's: two
        # content files are under a cap of three, and with two declarations the package is not.
        make_jar(
            self.mods / "ruby_mod.jar",
            {
                "assets/ruby_mod/lang/en_us.json": b"{}",
                "assets/ruby_mod/models/item/ruby.json": b"{}",
            },
        )
        with mock.patch.object(BAKE_MODULE, "MAX_FILES", 3):
            report = self.bake()
        self.assertIn("more than 3 files or 512 MiB", report["jars"][0]["error"])
        self.assertFalse((self.game / "horizon-mods" / "ruby_mod.hmod").exists())

    def test_the_manifest_escapes_the_jar_name(self):
        make_jar(self.mods / 'ruby"mod\\x.jar', ruby_jar())
        self.bake()
        with zipfile.ZipFile(self.game / "horizon-mods" / "ruby_mod.hmod") as archive:
            manifest = tomllib.loads(archive.read("horizon-mod.toml").decode())
        self.assertEqual(
            manifest["description"],
            'Assets and data baked from ruby"mod\\x.jar for the ruby_mod namespace. '
            "Mod ids of the jar: ruby_mod.",
        )

    def test_enabling_turns_an_off_line_on_and_keeps_the_others(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        (self.game / "horizon-mods.txt").write_text("mod.other:true\nmod.ruby_mod:false\n")
        self.bake()
        self.assertEqual(
            (self.game / "horizon-mods.txt").read_text(), "mod.other:true\nmod.ruby_mod:true\n"
        )



def manifest_toml(package_id: str, namespaces: list[str] | None = None, description: str = "") -> str:
    text = (
        f'id = "{package_id}"\nname = "{package_id}"\nversion = "1"\napi = "0.2"\n'
        'side = "both"\ncapabilities = ["registries"]\n'
    )
    if namespaces:
        text += "namespaces = [" + ", ".join(f'"{n}"' for n in namespaces) + "]\n"
    if description:
        text += f'description = "{description}"\n'
    return text


class HorizonReviewTests(unittest.TestCase):
    """The rules the black-box review added (spec "Found in review", R1-R10)."""

    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.root = Path(self.scratch.name)
        self.game = self.root / "game"
        self.mods = self.game / "config" / "horizon_java" / "java-mods"
        self.mods.mkdir(parents=True)
        self.packages = self.game / "horizon-mods"
        self.probe = fake_probe(self.root, declarations())

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def declare(self, blocks: list | None = None, items: list | None = None) -> None:
        self.probe = fake_probe(self.root, declarations(blocks or [], items or []))

    def bake(self, **arguments) -> dict:
        report = BAKE_MODULE.bake_directory(
            self.mods, self.game, probe=self.probe, target=BAKE_MODULE.HORIZON, **arguments
        )
        return report.as_json()

    def main(self) -> tuple[int, str]:
        stderr = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
            status = BAKE_MODULE.main(
                ["--game-dir", str(self.game), "--probe", str(self.probe), "--target", "horizon"]
            )
        return status, stderr.getvalue()

    def names(self, package: str = "ruby_mod") -> list[str]:
        with zipfile.ZipFile(self.packages / f"{package}.hmod") as archive:
            return archive.namelist()

    def declared(self, package: str = "ruby_mod") -> list[str]:
        return [name for name in self.names(package) if "/horizon/" in name]

    @staticmethod
    def reasons(report: dict, jar: int = 0) -> dict[str, list[str]]:
        return {entry["reason"]: entry["paths"] for entry in report["jars"][jar]["skipped"]}

    # R1 / A5: path parts.

    def test_dot_parts_in_declarations_and_files_are_left_out(self):
        self.declare(
            blocks=[
                {"id": "ruby_mod:a/./b", "file": block()},
                {"id": "ruby_mod:a/../b", "file": block()},
                {"id": "ruby_mod:../evil", "file": block()},
                {"id": "ruby_mod:a//b", "file": block()},
                {"id": "ruby_mod:a/", "file": block()},
                {"id": "ruby_mod:a./b", "file": block()},
                {"id": "ruby_mod:ends.", "file": block()},
                {"id": "ruby_mod:fine.name", "file": block()},
            ]
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar({"assets/ruby_mod/textures/a./b.png": PNG}))
        report = self.bake()
        reasons = self.reasons(report)
        empty = "a path part is empty, . or .. (the official game refuses the package, A5)"
        dot = "a path part ends in a dot or space (the official game refuses the package, R1)"
        self.assertEqual(
            sorted(reasons[empty]),
            sorted(["ruby_mod:../evil", "ruby_mod:a/", "ruby_mod:a//b", "ruby_mod:a/../b",
                    "ruby_mod:a/./b"]),
        )
        self.assertEqual(sorted(reasons[dot]), ["assets/ruby_mod/textures/a./b.png", "ruby_mod:a./b"])
        # `ends.` becomes `ends..json`, whose one part ends in `json`; `.` inside a name is fine.
        self.assertEqual(
            self.declared(),
            ["data/ruby_mod/horizon/block/ends..json", "data/ruby_mod/horizon/block/fine.name.json"],
        )
        self.assertNotIn("assets/ruby_mod/textures/a./b.png", self.names())
        self.assertIsNone(report["jars"][0]["error"])

    def test_the_horizon_path_rule_part_by_part(self):
        problem = BAKE_MODULE._horizon_path_problem
        self.assertIsNone(problem("data/ns/horizon/block/a.b-c_d.json"))
        for name in ("data/ns//x.json", "data/./x.json", "data/../x.json", "data/ns/x/"):
            self.assertIn("empty, . or ..", problem(name), name)
        for name in ("data/ns/x./y.json", "data/ns/x /y.json", "data/ns./x.json"):
            self.assertIn("ends in a dot or space", problem(name), name)
        for name in ("data/ns/X.json", "data/ns/x.json\n", "data/ns/a b.json"):
            self.assertEqual(problem(name), "not a plain content path", name)
        # RP1: 255 bytes loaded, 256 refused the package.
        self.assertIsNone(problem("assets/ns/textures/" + "l" * 251 + ".png"))
        self.assertEqual(
            problem("assets/ns/textures/" + "l" * 252 + ".png"),
            "a path part over 255 bytes (the official game refuses the package, RP1)",
        )
        self.assertIsNone(problem("assets/" + "n" * 255 + "/x.png"))
        self.assertIsNotNone(problem("assets/" + "n" * 256 + "/x.png"))
        # RP2: exactly the parts refused and loaded in Round 3, as a file and as a folder.
        device = "a path part is a Windows device name (the official game refuses the package, RP2)"
        refused = ["con", "prn", "aux", "nul", "com1", "com9", "lpt1", "lpt9", "con.png",
                   "con.a.b", "nul.json", "com1.x", "lpt1.tar.gz"]
        refused += [f"{prefix}{digit}" for prefix in ("com", "lpt") for digit in "2345678"]
        loaded = ["com0", "lpt0", "com10", "conx", "xcon", "aux_", "con-x", "coma", "con_",
                  "nulls.png", "x.con"]
        for part in refused:
            self.assertEqual(problem(f"assets/ns/textures/{part}"), device, part)
            self.assertEqual(problem(f"assets/ns/textures/{part}/x.png"), device, part)
        for part in loaded:
            self.assertIsNone(problem(f"assets/ns/textures/{part}"), part)
            self.assertIsNone(problem(f"assets/ns/textures/{part}/x.png"), part)

    def test_long_parts_and_device_names_in_files_and_declarations_are_left_out(self):
        self.declare(
            blocks=[
                {"id": "ruby_mod:" + "d" * 250, "file": block()},  # `.json` makes 255 bytes
                {"id": "ruby_mod:" + "e" * 251, "file": block()},  # 256 bytes
                {"id": "ruby_mod:nul", "file": block()},
                {"id": "ruby_mod:sub/com1/x", "file": block()},
                {"id": "ruby_mod:com10", "file": block()},
            ]
        )
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar({
                "assets/ruby_mod/textures/" + "l" * 251 + ".png": PNG,
                "assets/ruby_mod/textures/" + "m" * 252 + ".png": PNG,
                "assets/ruby_mod/textures/con.png": PNG,
                "assets/ruby_mod/textures/lpt1/x.png": PNG,
            }),
        )
        report = self.bake()
        reasons = self.reasons(report)
        self.assertEqual(
            sorted(reasons["a path part over 255 bytes (the official game refuses the package, RP1)"]),
            sorted(["ruby_mod:" + "e" * 251, "assets/ruby_mod/textures/" + "m" * 252 + ".png"]),
        )
        self.assertEqual(
            sorted(reasons["a path part is a Windows device name (the official game refuses the "
                           "package, RP2)"]),
            ["assets/ruby_mod/textures/con.png", "assets/ruby_mod/textures/lpt1/x.png",
             "ruby_mod:nul", "ruby_mod:sub/com1/x"],
        )
        self.assertEqual(
            self.declared(),
            ["data/ruby_mod/horizon/block/com10.json",
             "data/ruby_mod/horizon/block/" + "d" * 250 + ".json"],
        )
        self.assertIn("assets/ruby_mod/textures/" + "l" * 251 + ".png", self.names())

    def test_a_name_the_jar_holds_twice_is_left_out(self):
        # RP3: the official game reads the last copy; which one the mod reads is not established.
        path = self.mods / "ruby_mod.jar"
        make_jar(path, ruby_jar())
        with warnings.catch_warnings():
            warnings.simplefilter("ignore")
            with zipfile.ZipFile(path, "a") as archive:
                archive.writestr("assets/ruby_mod/lang/en_us.json", b'{"item.ruby_mod.ruby":"Two"}')
        report = self.bake()
        self.assertEqual(
            self.reasons(report)["a name the jar holds more than once (not established, left out)"],
            ["assets/ruby_mod/lang/en_us.json", "assets/ruby_mod/lang/en_us.json"],
        )
        self.assertNotIn("assets/ruby_mod/lang/en_us.json", self.names())

    def test_a_file_that_is_also_a_folder_is_kept_only_where_both_load(self):
        # RP4: both sides of `x.json` beside `x.json/y.json` loaded for a lang file and for
        # declarations (`fp1`, `fp3`), so both stay; any other asset kind is not established.
        self.declare(
            blocks=[{"id": "ruby_mod:a", "file": block()},
                    {"id": "ruby_mod:a.json/c", "file": block()},
                    {"id": "ruby_mod:b", "file": block()}],
            items=[{"id": "ruby_mod:r", "file": {"template": "minecraft:stick",
                                                 "crafting_remainder": "ruby_mod:a"}}],
        )
        make_jar(
            self.mods / "ruby_mod.jar",
            ruby_jar({
                "assets/ruby_mod/lang/en_us.json/y.json": b"{}",
                "assets/ruby_mod/lang/en_us.jsonx/y.json": b"{}",
                "assets/ruby_mod/textures/t/deep/z.png": PNG,
                "assets/ruby_mod/textures/t": PNG,
            }),
        )
        report = self.bake()
        clash = (
            "a file and a folder of the same name (both load only for lang files and "
            "declarations, RP4; not established for this kind, left out)"
        )
        self.assertEqual(
            self.reasons(report)[clash],
            ["assets/ruby_mod/textures/t", "assets/ruby_mod/textures/t/deep/z.png"],
        )
        # The declarations `a` and `a.json/c` both load, but the game generates no blockstate for
        # `a` (RB1), so `a` stays out and its remainder with it.
        self.assertEqual(self.reasons(report)[BAKE_MODULE.HORIZON_BLOCKSTATE_CLASH_REASON],
                         ["ruby_mod:a"])
        names = self.names()
        self.assertNotIn("data/ruby_mod/horizon/block/a.json", names)
        self.assertNotIn("data/ruby_mod/horizon/item/r.json", names)
        for name in (
            "assets/ruby_mod/lang/en_us.json",
            "assets/ruby_mod/lang/en_us.json/y.json",
            "assets/ruby_mod/lang/en_us.jsonx/y.json",
            "data/ruby_mod/horizon/block/a.json/c.json",
            "data/ruby_mod/horizon/block/b.json",
        ):
            self.assertIn(name, names)
        self.assertEqual(BAKE_MODULE._clashes([("a/b", b""), ("a/bc/d", b"")]), set())
        self.assertFalse(BAKE_MODULE._clash_loads("assets/ns/lang"))
        self.assertTrue(BAKE_MODULE._clash_loads("assets/ns/lang/x"))

    def test_a_declaration_over_16_mib_is_left_out(self):
        # P5: one file over the cap refuses the namespace's content (round-2 review `bigdecl`).
        self.declare(blocks=[{"id": "ruby_mod:big", "file": block(properties={"hardness": 1})},
                             {"id": "ruby_mod:ok", "file": block()}])
        make_jar(self.mods / "ruby_mod.jar", {"assets/ruby_mod/lang/en_us.json": b"{}"})
        size = len(BAKE_MODULE._declaration_bytes(block()))
        with mock.patch.object(BAKE_MODULE, "MAX_FILE", size):
            report = self.bake()
        self.assertEqual(
            self.reasons(report)["declaration over the content file cap (16 MiB, P5)"],
            ["ruby_mod:big"],
        )
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/ok.json"])

    # R2: packages already in horizon-mods/.    # R2: packages already in horizon-mods/.

    def test_an_existing_folder_package_of_the_same_id_refuses_the_namespace(self):
        folder = self.packages / "ruby_mod"
        folder.mkdir(parents=True)
        (folder / "horizon-mod.toml").write_text(manifest_toml("ruby_mod"))
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertFalse((self.packages / "ruby_mod.hmod").exists())
        self.assertIn("horizon-mods/ruby_mod already uses this id or namespace", stderr)
        self.assertFalse((self.game / "horizon-mods.txt").exists())

    def test_an_existing_file_package_under_another_name_or_listing_the_namespace(self):
        for package_id, namespaces in (("ruby_mod", None), ("other", ["ruby_mod"])):
            with self.subTest(package_id=package_id):
                for path in self.packages.glob("*") if self.packages.exists() else []:
                    path.unlink()
                self.packages.mkdir(exist_ok=True)
                with zipfile.ZipFile(self.packages / "zz_user.hmod", "w") as archive:
                    archive.writestr("horizon-mod.toml", manifest_toml(package_id, namespaces))
                make_jar(self.mods / "ruby_mod.jar", ruby_jar())
                report = self.bake()
                self.assertIn(
                    "horizon-mods/zz_user.hmod already uses this id or namespace",
                    report["jars"][0]["error"],
                )
                self.assertFalse((self.packages / "ruby_mod.hmod").exists())

    def test_a_package_whose_manifest_cannot_be_read_refuses_every_namespace(self):
        self.packages.mkdir()
        (self.packages / "broken.hmod").write_text("not a zip")
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertIn("the manifest of horizon-mods/broken.hmod cannot be read",
                      report["jars"][0]["error"])
        self.assertFalse((self.packages / "ruby_mod.hmod").exists())

    def test_files_that_are_not_packages_are_ignored(self):
        # L2, L4: a folder without a manifest, `.zip`, upper-case `.HMOD` are not packages.
        (self.packages / "loose_folder").mkdir(parents=True)
        (self.packages / "ruby_mod.zip").write_text("x")
        (self.packages / "RUBY_MOD.HMOD").write_text("x")
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertIsNone(report["jars"][0]["error"])
        self.assertTrue((self.packages / "ruby_mod.hmod").is_file())

    def test_a_rebake_replaces_its_own_package_but_never_a_users(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.assertEqual(self.main()[0], 0)
        self.assertEqual(self.main()[0], 0)  # its own earlier package: replaced
        package = self.packages / "ruby_mod.hmod"
        with zipfile.ZipFile(package, "w") as archive:
            archive.writestr("horizon-mod.toml", manifest_toml("someone_else"))
        before = package.read_bytes()
        status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertIn("a package this bake did not write for it; not overwritten", stderr)
        self.assertEqual(package.read_bytes(), before)

    def stale(self) -> list[dict]:
        report = self.game / "config" / "horizon_java" / "bake-report.json"
        return json.loads(report.read_text())["stale"]

    def test_a_stale_package_from_an_earlier_bake_is_named_not_deleted(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.assertEqual(self.main(), (0, ""))
        self.assertEqual(self.stale(), [])  # rewritten by the second run: not stale
        self.assertEqual(self.main(), (0, ""))
        self.assertEqual(self.stale(), [])
        folder = self.packages / "user_copy"
        folder.mkdir()
        (folder / "horizon-mod.toml").write_text(manifest_toml("ruby_mod"))
        status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertTrue((self.packages / "ruby_mod.hmod").is_file())
        self.assertIn(
            "stale package from an earlier bake of ruby_mod.jar left in place: "
            "horizon-mods/ruby_mod.hmod (turned on in horizon-mods.txt", stderr
        )
        stale = [{"id": "ruby_mod", "path": "horizon-mods/ruby_mod.hmod",
                  "baked_from": "ruby_mod.jar", "mod_ids": ["ruby_mod"], "turned_on": True}]
        self.assertEqual(self.stale(), stale)
        # A jar refused whole leaves its earlier package stale too.
        make_jar(self.mods / "ruby_mod.jar", {"../x.json": b"{}"})
        shutil_rmtree(folder)
        self.assertEqual(self.main()[0], 1)
        self.assertEqual(self.stale(), stale)

    def test_a_renamed_jar_leaves_its_old_packages_stale(self):
        # A version bump renames the jar: the namespace it no longer has stays loaded, so the
        # package is named whatever jar wrote it (round-2 review, stale_t run 3).
        self.declare(blocks=[{"id": "ruby_mod:b", "file": block()},
                             {"id": "gem_mod:b", "file": block()}])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.assertEqual(self.main(), (0, ""))
        (self.mods / "ruby_mod.jar").rename(self.mods / "ruby_mod-1.1.jar")
        self.declare(blocks=[{"id": "ruby_mod:b", "file": block()}])
        status, stderr = self.main()
        self.assertEqual(status, 0)
        self.assertEqual(
            self.stale(),
            [{"id": "gem_mod", "path": "horizon-mods/gem_mod.hmod", "baked_from": "ruby_mod.jar",
              "mod_ids": ["ruby_mod"], "turned_on": True}],
        )
        self.assertIn("stale package from an earlier bake of ruby_mod.jar", stderr)
        # The rewritten package now names the new jar and is not stale.
        with zipfile.ZipFile(self.packages / "ruby_mod.hmod") as archive:
            self.assertIn("ruby_mod-1.1.jar", archive.read("horizon-mod.toml").decode())

    def test_only_a_package_this_bake_wrote_for_the_namespace_is_replaced(self):
        # The guards of `existing_packages`: the description's namespace must be the id, no
        # `namespaces`, and only that one namespace.
        description = "Assets and data baked from ruby_mod.jar for the {} namespace."
        for label, manifest in (
            ("other namespace in the description", manifest_toml("ruby_mod", None,
                                                                 description.format("other"))),
            ("namespaces listed", manifest_toml("ruby_mod", ["extra"], description.format("ruby_mod"))),
            ("no description", manifest_toml("ruby_mod")),
            # This bake's package, but for another namespace, at the path of this one.
            ("another namespace's baked package", manifest_toml("other", None,
                                                               description.format("other"))),
        ):
            with self.subTest(label):
                self.packages.mkdir(exist_ok=True)
                package = self.packages / "ruby_mod.hmod"
                with zipfile.ZipFile(package, "w") as archive:
                    archive.writestr("horizon-mod.toml", manifest)
                before = package.read_bytes()
                make_jar(self.mods / "ruby_mod.jar", ruby_jar())
                report = self.bake()
                self.assertIn("a package this bake did not write for it; not overwritten",
                              report["jars"][0]["error"])
                self.assertEqual(package.read_bytes(), before)
                # Not replaced, so the other namespace's baked package is named stale.
                stale = ["other"] if label.startswith("another") else []
                self.assertEqual([row["id"] for row in report["stale"]], stale)
        package.unlink()
        with zipfile.ZipFile(package, "w") as archive:
            archive.writestr("horizon-mod.toml",
                             manifest_toml("ruby_mod", None, description.format("ruby_mod")))
        self.assertIsNone(self.bake()["jars"][0]["error"])

    def test_a_folder_where_the_package_goes_is_never_overwritten(self):
        (self.packages / "ruby_mod.hmod").mkdir(parents=True)
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertIn("a folder sits where its package goes; not overwritten",
                      report["jars"][0]["error"])
        self.assertTrue((self.packages / "ruby_mod.hmod").is_dir())

    def test_existing_packages_are_read_in_horizon_mods_and_in_out(self):
        out = self.root / "elsewhere"
        for folder, name in ((self.packages, "a.hmod"), (out, "b.hmod")):
            folder.mkdir(parents=True)
            with zipfile.ZipFile(folder / name, "w") as archive:
                archive.writestr("horizon-mod.toml", manifest_toml(f"user_{name[0]}", ["ruby_mod"]))
            make_jar(self.mods / "ruby_mod.jar", ruby_jar())
            report = self.bake(out=out)
            self.assertIn(f"{name} already uses this id or namespace", report["jars"][0]["error"])
            (folder / name).unlink()

    def test_a_manifest_with_namespaces_that_are_not_strings_cannot_be_read(self):
        self.packages.mkdir()
        with zipfile.ZipFile(self.packages / "user.hmod", "w") as archive:
            archive.writestr("horizon-mod.toml", manifest_toml("user") + "namespaces = [1]\n")
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.assertIn("the manifest of horizon-mods/user.hmod cannot be read",
                      self.bake()["jars"][0]["error"])

    def test_every_error_of_a_jar_is_kept_and_the_cli_names_what_was_written(self):
        self.declare(blocks=[{"id": "ruby_mod:b", "file": block()},
                             {"id": "gem_mod:b", "file": block()},
                             {"id": "ore_mod:b", "file": block()}])
        for package_id in ("gem_mod", "ore_mod"):
            folder = self.packages / package_id
            folder.mkdir(parents=True)
            (folder / "horizon-mod.toml").write_text(manifest_toml(package_id))
        # gem_mod and ore_mod are the jar's own mod ids too, so their conflicts are its errors.
        make_jar(self.mods / "ruby_mod.jar", ruby_jar({"META-INF/mods.toml": (
            b'[[mods]]\nmodId = "gem_mod"\n[[mods]]\nmodId = "ore_mod"\n')}))
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            status = BAKE_MODULE.main(
                ["--game-dir", str(self.game), "--probe", str(self.probe), "--target", "horizon"]
            )
        self.assertEqual(status, 1)
        self.assertIn("ruby_mod.jar: 1 package(s) [ruby_mod]", stdout.getvalue())
        self.assertIn("ruby_mod.jar: not written: namespace gem_mod:", stderr.getvalue())
        self.assertIn("; namespace ore_mod:", stderr.getvalue())
        self.assertNotIn("refused", stderr.getvalue())
        self.assertEqual((self.game / "horizon-mods.txt").read_text(), "mod.ruby_mod:true\n")

    def test_shared_files_go_with_the_first_package_written(self):
        # `ruby-dash` sorts first and is refused; vanilla's atlas still goes with `ruby_mod`.
        self.declare(
            blocks=[{"id": "ruby_mod:ruby_block", "file": block()}],
            items=[{"id": "ruby-dash:gem", "file": {"template": "minecraft:stick"}}],
        )
        make_jar(self.mods / "ruby_mod.jar",
                 ruby_jar({"assets/minecraft/atlases/blocks.json": b'{"sources":[]}'}))
        report = self.bake()
        self.assertIn("'ruby-dash' is not a content package id", report["jars"][0]["error"])
        self.assertEqual(report["jars"][0]["error"].count("ruby-dash"), 1)
        self.assertIn("assets/minecraft/atlases/blocks.json", self.names())

    # Data files, strict JSON, mod ids.

    def test_every_data_file_is_left_out_with_one_reason(self):
        extra = {
            "data/ruby_mod/recipe/r.json": b'{"type":"minecraft:crafting_shapeless",'
            b'"ingredients":["minecraft:stick"],"result":{"id":"minecraft:stick"}}',
            "data/ruby_mod/recipe/own.json": b'{"type":"ruby_mod:mixing"}',
            "data/ruby_mod/tags/block/t.json": b'{"values":["minecraft:stone"]}',
            "data/ruby_mod/advancement/a.json": b'{"criteria":{}}',
            "data/ruby_mod/structure/s.nbt": b"\x0a\x00\x00",
            "data/ruby_mod/predicate/p.json": b'{"condition":"minecraft:random_chance","chance":1}',
            "data/ruby_mod/item_modifier/m.json": b'{"function":"minecraft:set_count","count":2}',
            "data/ruby_mod/damage_type/d.json": b'{"message_id":"x","scaling":"never","exhaustion":0}',
            "data/ruby_mod/worldgen/biome/b.json": b"{}",
            "data/ruby_mod/function/f.mcfunction": b"say hi\n",
            "data/ruby_mod/bogus_kind/x.json": b"{}",
            "data/ruby_mod/horizon/block/x.json": b'{"template":"minecraft:stone"}',
            "data/ruby_mod/sunrise/block/x.json": b'{"template":"minecraft:stone"}',
            "data/c/tags/item/gems.json": b'{"values":[]}',
            "data/other_mod/recipe/compat.json": b"{}",
        }
        make_jar(self.mods / "ruby_mod.jar", ruby_jar(extra))
        report = self.bake()
        reasons = self.reasons(report)
        every = {*extra, "data/ruby_mod/loot_table/blocks/ruby_block.json",
                 "data/minecraft/tags/block/mineable/pickaxe.json"}
        examples = reasons[BAKE_MODULE.HORIZON_DATA_REASON]
        self.assertEqual(len(examples), 8)  # the report keeps eight example paths per reason
        self.assertLessEqual(set(examples), every)
        data_count = {entry["reason"]: entry["count"] for entry in report["jars"][0]["skipped"]}
        self.assertEqual(data_count[BAKE_MODULE.HORIZON_DATA_REASON], len(extra) + 2)
        self.assertEqual([name for name in self.names() if name.startswith("data/")],
                         ["data/ruby_mod/horizon/block/ruby_block.json",
                          "data/ruby_mod/horizon/item/ruby.json"])
        self.assertEqual([package["id"] for package in report["jars"][0]["packages"]], ["ruby_mod"])

    def test_only_strict_json_is_copied(self):
        good = b'{"a": [1, 2.5, -0, "\\u00e9", {"b": null}], "c": true}'

        def nested(containers: int) -> bytes:
            # The outermost object and `containers - 1` arrays inside it.
            return b'{"a":' + b"[" * (containers - 1) + b"1" + b"]" * (containers - 1) + b"}"

        cases = {
            "comment": b"// x\n{}",
            "bom": b"\xef\xbb\xbf{}",
            "nan": b'{"a": NaN}',
            "infinity": b'{"a": -Infinity}',
            "trailing": b"{} {}",
            "trailing_comma": b'{"a": 1,}',
            "duplicate": b'{"a": 1, "a": 2}',
            "latin1": b'{"a": "\xe9"}',
            "surrogate": b'{"a": "\\ud800"}',
            "surrogate_key": b'{"\\udc00": 1}',
            "overflow": b'{"a": 1e400}',
            "big_integer": b'{"a": ' + b"9" * 400 + b"}",
            "depth128": nested(128),
            "depth128_objects": b'{"a":' * 128 + b"1" + b"}" * 128,
            "deeper": b"[" * 5000 + b"]" * 5000,
        }
        extra = {f"assets/ruby_mod/lang/{name}.json": data for name, data in cases.items()}
        extra["assets/ruby_mod/lang/good.json"] = good
        extra["assets/ruby_mod/lang/edge.json"] = b'{"a": 340282346638528859811704183484516925440}'
        # RD1: 127 arrays and objects in all loaded in the official game, 128 did not.
        extra["assets/ruby_mod/lang/depth127.json"] = nested(127)
        extra["assets/ruby_mod/lang/depth127_objects.json"] = b'{"a":' * 127 + b"1" + b"}" * 127
        extra["assets/ruby_mod/lang/depth127_empty.json"] = b"[" * 127 + b"]" * 127
        make_jar(self.mods / "ruby_mod.jar", ruby_jar(extra))
        report = self.bake()
        skipped = {
            entry["reason"]: entry["count"] for entry in report["jars"][0]["skipped"]
        }
        self.assertEqual(skipped[JSON_REASON], len(cases))
        names = self.names()
        for kept in ("good", "edge", "depth127", "depth127_objects", "depth127_empty"):
            self.assertIn(f"assets/ruby_mod/lang/{kept}.json", names)
        for name in cases:
            self.assertNotIn(f"assets/ruby_mod/lang/{name}.json", names)
        strict = BAKE_MODULE._strict_json
        self.assertEqual(strict(b"[" * 128 + b"]" * 128), (None, False))
        self.assertTrue(strict(b"[" * 127 + b"]" * 127)[1])
        self.assertEqual(strict(b"1"), (1, True))

    def test_assets_only_of_the_namespaces_the_jar_owns(self):
        self.declare(blocks=[{"id": "decl_mod:b", "file": block(template="minecraft:furnace")}])
        files = {
            "assets/ruby_mod/lang/en_us.json": b"{}",
            "assets/decl_mod/lang/en_us.json": b"{}",
            "assets/neo_mod/lang/en_us.json": b"{}",
            "assets/neo_two/lang/en_us.json": b"{}",
            "assets/forge_mod/lang/en_us.json": b"{}",
            "assets/create/lang/en_us.json": b"{}",
            "assets/othermod/textures/x.png": PNG,
            "assets/ruby_mod/bogus_kind/x.png": PNG,
            "fabric.mod.json": b'{"schemaVersion":1,"id":"ruby_mod","version":"1"}',
            "META-INF/neoforge.mods.toml": b'modLoader="javafml"\n[[mods]]\nmodId="neo_mod"\n'
            b'[[mods]]\nmodId="neo_two"\n[[dependencies.neo_mod]]\nmodId="create"\n',
            "META-INF/mods.toml": b'[[mods]]\nmodId="forge_mod"\n',
        }
        make_jar(self.mods / "ruby_mod.jar", files)
        report = self.bake()
        packages = [package["id"] for package in report["jars"][0]["packages"]]
        # decl_mod's only block is left out (its template), yet the namespace is the jar's.
        self.assertEqual(packages, ["decl_mod", "forge_mod", "neo_mod", "neo_two", "ruby_mod"])
        self.assertEqual(
            self.reasons(report)[BAKE_MODULE.HORIZON_UNOWNED_REASON],
            ["assets/create/lang/en_us.json", "assets/othermod/textures/x.png"],
        )
        self.assertFalse((self.packages / "create.hmod").exists())
        self.assertEqual(self.reasons(report)["unsupported asset kind"],
                         ["assets/ruby_mod/bogus_kind/x.png"])

    def test_mod_ids_come_only_from_metadata_the_bake_can_read(self):
        mod_ids = BAKE_MODULE._jar_mod_ids
        self.assertEqual(mod_ids([("fabric.mod.json", b'{"id":"a"}')]), {"a"})
        for name, data in (
            ("fabric.mod.json", b"// comment\n{\"id\":\"a\"}"),
            ("fabric.mod.json", b'{"id":5}'),
            ("fabric.mod.json", b'["a"]'),
            ("fabric.mod.json", b"\xff"),
            ("fabric.mod.json", b"[" * 100000 + b"]" * 100000),
            ("META-INF/neoforge.mods.toml", b"[[mods]\nmodId='a'"),
            ("META-INF/neoforge.mods.toml", b"mods = 5"),
            ("META-INF/neoforge.mods.toml", b"[[mods]]\nmodId=5"),
            ("META-INF/mods.toml", b"mods = [5]"),
            ("META-INF/mods.toml", b"\xff"),
            ("nested/fabric.mod.json", b'{"id":"a"}'),
            ("META-INF/jars/x/META-INF/mods.toml", b'[[mods]]\nmodId="a"'),
        ):
            with self.subTest(name=name, data=data[:40]):
                self.assertEqual(mod_ids([(name, data)]), set())
        self.assertEqual(
            mod_ids([("META-INF/mods.toml", b'[[mods]]\nmodId="a"\n[[mods]]\nmodId="b"')]),
            {"a", "b"},
        )

    def test_sunrise_only_suffixes_keep_their_own_reason(self):
        extra = {
            "assets/ruby_mod/textures/a.zip": b"PK\x05\x06",
            "assets/ruby_mod/textures/a.sh": b"echo\n",
            "assets/ruby_mod/textures/a.py": b"print()\n",
        }
        make_jar(self.mods / "ruby_mod.jar", ruby_jar(extra))
        reasons = self.reasons(self.bake())
        self.assertEqual(
            sorted(reasons["code or an archive, kept out as for Sunrise (the official game would "
                           "take it, A10)"]),
            sorted(extra),
        )

    # R6: numbers.

    def test_number_bounds_of_the_official_game(self):
        vanilla = BAKE_MODULE.vanilla_tables()
        problem = BAKE_MODULE._horizon_declaration_problem

        def check(key, value):
            return problem("block", block(properties={key: value}), vanilla)

        for value in (0, 0.0, -0.0, 2, 2.0, 0.6, 1e-30):
            self.assertIsNone(check("friction", value), value)
        for value in (-1e-7, -0.5, 2.0000002384185791, 3e38, float("nan"), True, "1", 10**400):
            self.assertEqual(check("friction", value), FRICTION, value)
        for value in (-1, -1.0, -0.0, 0, 3e38, BAKE_MODULE.F32_MAX):
            self.assertIsNone(check("hardness", value), value)
        for value in (-0.5, -3e38, -2, 1e39, float("inf"), float("-inf"), float("nan"),
                      10**400, -(10**400)):
            self.assertEqual(check("hardness", value), HARDNESS, value)
        for value in (0, -0.0, 3e38):
            self.assertIsNone(check("explosion_resistance", value), value)
        for value in (-1, -1e-30, 1e39, float("nan"), 10**400):
            self.assertEqual(check("explosion_resistance", value), RESISTANCE, value)
        light = "block property light is not an integer 0 to 15 (refused, 3.2, R6)"
        color = "block property map_color is not an integer 0 to 61 (refused, 3.2, R6)"
        for key, top, reason in (("light", 15, light), ("map_color", 61, color)):
            self.assertIsNone(check(key, 0))
            self.assertIsNone(check(key, top))
            for value in (-1, top + 1, 1.0, True, 10**400):
                self.assertEqual(check(key, value), reason, (key, value))
        # A box bound is an f32 too.
        self.assertIsNotNone(
            problem("block", block(shapes={"default": [[0, 0, 0, 1, 1, 10**400]]}), vanilla)
        )

    def test_numbers_from_the_probe_never_stop_the_bake(self):
        self.declare(
            blocks=[
                {"id": "ruby_mod:nan", "file": block(properties={"hardness": float("nan")})},
                {"id": "ruby_mod:inf", "file": block(properties={"friction": float("inf")})},
                {"id": "ruby_mod:huge", "file": block(properties={"hardness": 10**400})},
                {"id": "ruby_mod:big_box", "file": block(shapes={"default": [[0, 0, 0, 1, 1, 1e39]]})},
                {"id": "ruby_mod:ok", "file": block(properties={"hardness": -1})},
            ]
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(sorted(self.reasons(report)[HARDNESS]), ["ruby_mod:huge", "ruby_mod:nan"])
        self.assertEqual(self.reasons(report)[FRICTION], ["ruby_mod:inf"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/ok.json"])

    # R7, R8, R9: tables.

    def test_sounds_take_the_878_checked_blocks(self):
        vanilla = BAKE_MODULE.vanilla_tables()
        for sounds in ("minecraft:stone", "minecraft:amethyst_block", "minecraft:air"):
            file = block(properties={"sounds": sounds})
            self.assertIsNone(BAKE_MODULE._horizon_declaration_problem("block", file, vanilla))

    def test_the_two_templates_decafs_runtime_infers(self):
        vanilla = BAKE_MODULE.vanilla_tables()
        for template in ("minecraft:stone", "minecraft:crafting_table"):
            for item in (True, False):
                file = block(template=template, item=item)
                self.assertIsNone(BAKE_MODULE._horizon_declaration_problem("block", file, vanilla))

    def test_a_damageable_template_takes_no_stack_over_one(self):
        reason = (
            "item max_stack_size over 1 on a damageable template "
            "(no stack of it can exist in the official game, R8)"
        )
        self.declare(
            items=[
                {"id": "ruby_mod:sword16", "file": {"template": "minecraft:diamond_sword",
                                                    "max_stack_size": 16}},
                {"id": "ruby_mod:sword2", "file": {"template": "minecraft:diamond_sword",
                                                   "max_stack_size": 2}},
                {"id": "ruby_mod:sword1", "file": {"template": "minecraft:diamond_sword",
                                                   "max_stack_size": 1}},
                {"id": "ruby_mod:sword", "file": {"template": "minecraft:diamond_sword"}},
                {"id": "ruby_mod:stick99", "file": {"template": "minecraft:stick",
                                                    "max_stack_size": 99}},
            ]
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(sorted(self.reasons(report)[reason]), ["ruby_mod:sword16", "ruby_mod:sword2"])
        self.assertEqual(
            self.declared(),
            ["data/ruby_mod/horizon/item/stick99.json", "data/ruby_mod/horizon/item/sword.json",
             "data/ruby_mod/horizon/item/sword1.json"],
        )

    def test_block_item_air_is_left_out(self):
        vanilla = BAKE_MODULE.vanilla_tables()
        file = block(item=False, block_item="minecraft:air")
        self.assertEqual(
            BAKE_MODULE._horizon_declaration_problem("block", file, vanilla),
            "block_item minecraft:air (not established, left out)",
        )
        file = block(item=False, block_item="minecraft:diamond")
        self.assertIsNone(BAKE_MODULE._horizon_declaration_problem("block", file, vanilla))

    def test_a_remainder_of_another_namespace_in_the_same_jar_is_left_out(self):
        # The remainder must be the package's own (3.3); `other_ns:gem` goes to another package.
        self.declare(
            items=[
                {"id": "other_ns:gem", "file": {"template": "minecraft:stick"}},
                {"id": "ruby_mod:rest", "file": {"template": "minecraft:stick",
                                                 "crafting_remainder": "other_ns:gem"}},
                {"id": "ruby_mod:own", "file": {"template": "minecraft:stick",
                                                "crafting_remainder": "ruby_mod:gem"}},
                {"id": "ruby_mod:gem", "file": {"template": "minecraft:stick"}},
            ]
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(
            self.reasons(report)[
                "item crafting_remainder is not a vanilla item or one of the package's own "
                "(fatal in the official game, 3.3)"
            ],
            ["ruby_mod:rest"],
        )
        self.assertEqual(
            self.declared(),
            ["data/ruby_mod/horizon/item/gem.json", "data/ruby_mod/horizon/item/own.json"],
        )
        self.assertEqual(self.declared("other_ns"), ["data/other_ns/horizon/item/gem.json"])

    def test_a_singular_shape_property_without_default(self):
        enum = {"type": "enum", "values": ["x", "y"]}
        vanilla = BAKE_MODULE.vanilla_tables()
        covered = block(state_properties={"e": enum},
                        shapes={"property": "e", "values": {"x": [], "y": [[0, 0, 0, 1, 1, 1]]}})
        self.assertIsNone(BAKE_MODULE._horizon_declaration_problem("block", covered, vanilla))
        uncovered = block(state_properties={"e": enum}, shapes={"property": "e", "values": {"x": []}})
        self.assertEqual(
            BAKE_MODULE._horizon_declaration_problem("block", uncovered, vanilla),
            "block shape leaves a state uncovered, no default (fatal in the official game, 3.2)",
        )

    # Validation before duplicates and block items; malformed values.

    def test_a_left_out_block_does_not_take_its_items_or_duplicates_with_it(self):
        self.declare(
            blocks=[
                {"id": "ruby_mod:x", "file": block(template="minecraft:furnace")},
                {"id": "ruby_mod:dup", "file": block(properties={"light": 99})},
                {"id": "ruby_mod:dup", "file": block()},
            ],
            items=[{"id": "ruby_mod:x", "file": {"template": "minecraft:stick"}}],
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        reasons = self.reasons(report)
        self.assertNotIn("the block's own item (its block declares it)", reasons)
        self.assertNotIn("declared more than once in the jar", reasons)
        self.assertEqual(
            self.declared(),
            ["data/ruby_mod/horizon/block/dup.json", "data/ruby_mod/horizon/item/x.json"],
        )
        with zipfile.ZipFile(self.packages / "ruby_mod.hmod") as archive:
            self.assertEqual(json.loads(archive.read("data/ruby_mod/horizon/block/dup.json")),
                             block())

    def test_malformed_probe_values_are_left_out_never_a_crash(self):
        rows = [
            {"id": "ruby_mod:list_rarity", "file": {"template": "minecraft:stick",
                                                    "rarity": ["rare"]}},
            {"id": "ruby_mod:dict_rarity", "file": {"template": "minecraft:stick",
                                                    "rarity": {"a": 1}}},
            {"id": "ruby_mod:huge_stack", "file": {"template": "minecraft:stick",
                                                   "max_stack_size": 10**400}},
            {"id": "ruby_mod:list_template", "file": {"template": ["minecraft:stick"]}},
            {"id": "ruby_mod:not_object", "file": ["minecraft:stick"]},
            {"id": "ruby_mod:list_remainder", "file": {"template": "minecraft:stick",
                                                       "crafting_remainder": ["x"]}},
            {"id": 5, "file": {}},
            {"file": {}},
            {"id": "ruby_mod:no_file"},
            "a string row",
        ]
        blocks = [
            {"id": "ruby_mod:list_state", "file": block(state_properties=["a"])},
            {"id": "ruby_mod:list_default", "file": block(default_state={"a": ["x"]})},
            {"id": "ruby_mod:dict_item", "file": block(item={"a": 1})},
            {"id": "ruby_mod:list_property", "file": block(properties={"light": [1]})},
            {"id": "ruby_mod:list_shape", "file": block(shapes={"property": ["a"], "default": []})},
        ]
        self.declare(blocks=blocks, items=rows)
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        row = report["jars"][0]
        self.assertIsNone(row["error"])
        self.assertEqual(self.declared(), [])
        malformed = self.reasons(report)["probe row that is not an object with a string id and a file"]
        self.assertEqual(len(malformed), 4)
        skipped = sum(entry["count"] for entry in row["skipped"])
        self.assertGreaterEqual(skipped, len(rows) + len(blocks))

    def test_probe_output_that_is_not_an_object_refuses_the_jar(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        for output, error in (
            ("[1, 2]", "not a JSON object"),
            ('{"status": "ok", "entrypoints": ["x"], "content": [1]}', "content is not a JSON object"),
        ):
            with self.subTest(output=output):
                self.probe = fake_probe(self.root, output)
                report = self.bake()
                self.assertIn(error, report["jars"][0]["error"])
        self.probe = fake_probe(
            self.root, '{"status": "ok", "entrypoints": ["x"], "content": {"blocks": 5}}'
        )
        report = self.bake()
        self.assertIsNone(report["jars"][0]["error"])
        self.assertEqual(
            self.reasons(report)["probe row that is not an object with a string id and a file"],
            ["blocks: 5"],
        )

    def test_probe_output_python_cannot_read_refuses_the_jar_without_a_crash(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        script = self.root / "horizon-native-java"
        for label, body, error in (
            ("deep", "sys.stdout.write('[' * 200000 + ']' * 200000 + '\\n')",
             "not JSON: nested too deep"),
            ("not utf-8", "sys.stdout.buffer.write(b'{\\\"status\\\": \\\"ok\\\"}\\n\\xff\\n')",
             "output that is not UTF-8"),
        ):
            with self.subTest(label):
                script.write_text(f"#!/usr/bin/env python3\nimport sys\n{body}\n", encoding="utf-8")
                script.chmod(0o755)
                self.probe = script
                report = self.bake()
                self.assertIn(error, report["jars"][0]["error"])
                self.assertFalse((self.packages / "ruby_mod.hmod").exists())

    def test_a_remainder_is_never_air_nor_a_block_without_its_item(self):
        remainder = (
            "item crafting_remainder is not a vanilla item or one of the package's own "
            "(fatal in the official game, 3.3)"
        )
        self.declare(
            blocks=[{"id": "ruby_mod:itemless", "file": block(item=False)},
                    {"id": "ruby_mod:with_item", "file": block()}],
            items=[
                {"id": "ruby_mod:air_rest", "file": {"template": "minecraft:stick",
                                                     "crafting_remainder": "minecraft:air"}},
                {"id": "ruby_mod:itemless_rest", "file": {"template": "minecraft:stick",
                                                          "crafting_remainder": "ruby_mod:itemless"}},
                {"id": "ruby_mod:block_rest", "file": {"template": "minecraft:stick",
                                                       "crafting_remainder": "ruby_mod:with_item"}},
            ],
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(sorted(self.reasons(report)[remainder]),
                         ["ruby_mod:air_rest", "ruby_mod:itemless_rest"])
        self.assertIn("data/ruby_mod/horizon/item/block_rest.json", self.declared())

    # P5 and the manifest.

    def test_a_package_over_512_mib_once_declared_is_not_written(self):
        make_jar(self.mods / "ruby_mod.jar", {"assets/ruby_mod/lang/en_us.json": b"{}"})
        size = len(BAKE_MODULE._declaration_bytes(BLOCK)) + len(BAKE_MODULE._declaration_bytes(ITEM))
        with mock.patch.object(BAKE_MODULE, "MAX_TOTAL", size + 1):
            report = self.bake()
        self.assertIn("more than 65536 files or 512 MiB", report["jars"][0]["error"])
        self.assertFalse((self.packages / "ruby_mod.hmod").exists())
        with mock.patch.object(BAKE_MODULE, "MAX_TOTAL", size + 2):
            report = self.bake()
        self.assertIsNone(report["jars"][0]["error"])

    def test_the_manifest_escapes_control_characters(self):
        make_jar(self.mods / "ruby\x01mod\x7f.jar", ruby_jar())
        self.bake()
        with zipfile.ZipFile(self.packages / "ruby_mod.hmod") as archive:
            text = archive.read("horizon-mod.toml").decode()
        self.assertIn("ruby\\u0001mod\\u007f.jar", text)
        self.assertEqual(
            tomllib.loads(text)["description"],
            "Assets and data baked from ruby\x01mod\x7f.jar for the ruby_mod namespace. "
            "Mod ids of the jar: ruby_mod.",
        )
        # A file name byte that is not UTF-8 reaches Python as a lone surrogate, which TOML (and
        # UTF-8) cannot hold.
        self.assertEqual(BAKE_MODULE._toml_string("a\udcffb"), '"a\ufffdb"')
        self.assertEqual(tomllib.loads("x = " + BAKE_MODULE._toml_string("a\udcffb"))["x"],
                         "a\ufffdb")


# The most blocks the mods added together in the official game without "too many blocks"
# (spec Round 4, RS2).
MEASURED_MOD_BLOCKS = 64_250


def jar_probe(root: Path, outputs: dict[str, str]) -> Path:
    """A stand-in probe that prints the report of the jar it is given, by file name."""
    data = root / "probe-outputs.json"
    data.write_text(json.dumps(outputs), encoding="utf-8")
    path = root / "horizon-native-java-by-jar"
    path.write_text(
        "#!/usr/bin/env python3\n"
        "import json, os, sys\n"
        f"outputs = json.load(open({str(data)!r}))\n"
        "sys.stdout.write(outputs.get(os.path.basename(sys.argv[2]), '') + '\\n')\n",
        encoding="utf-8",
    )
    path.chmod(0o755)
    return path


def booleans(count: int, **extra) -> dict:
    """State properties: `count` booleans, plus `extra` definitions."""
    properties = {f"p{index}": {"type": "boolean"} for index in range(count)}
    properties.update(extra)
    return properties


def package_with_blocks(
    path: Path, package_id: str, blocks: dict[str, object], turn_on: bool = True
) -> None:
    """A package someone else wrote, with block files `{path: file}`, turned on in the game
    folder's `horizon-mods.txt` unless `turn_on` is false."""
    path.parent.mkdir(parents=True, exist_ok=True)
    if turn_on:
        with (path.parent.parent / "horizon-mods.txt").open("a") as stream:
            stream.write(f"mod.{package_id}:true\n")
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("horizon-mod.toml", manifest_toml(package_id))
        for name, file in blocks.items():
            body = file if isinstance(file, bytes) else json.dumps(file).encode()
            archive.writestr(f"data/{package_id}/horizon/block/{name}.json", body)


class HorizonRound4Tests(unittest.TestCase):
    """The rules of the fourth review round (spec "Round 4 (black-box)")."""

    def setUp(self) -> None:
        self.scratch = tempfile.TemporaryDirectory()
        self.root = Path(self.scratch.name)
        self.game = self.root / "game"
        self.mods = self.game / "config" / "horizon_java" / "java-mods"
        self.mods.mkdir(parents=True)
        self.packages = self.game / "horizon-mods"
        self.probe = fake_probe(self.root, declarations())

    def tearDown(self) -> None:
        self.scratch.cleanup()

    def declare(self, blocks: list | None = None, items: list | None = None) -> None:
        self.probe = fake_probe(self.root, declarations(blocks or [], items or []))

    def declare_by_jar(self, **jars: tuple[list, list]) -> None:
        self.probe = jar_probe(
            self.root,
            {f"{name}.jar": declarations(blocks, items) for name, (blocks, items) in jars.items()},
        )

    def bake(self) -> dict:
        report = BAKE_MODULE.bake_directory(
            self.mods, self.game, probe=self.probe, target=BAKE_MODULE.HORIZON
        )
        return report.as_json()

    def main(self) -> tuple[int, str]:
        stderr = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
            status = BAKE_MODULE.main(
                ["--game-dir", str(self.game), "--probe", str(self.probe), "--target", "horizon"]
            )
        return status, stderr.getvalue()

    def report_file(self) -> dict:
        path = self.game / "config" / "horizon_java" / "bake-report.json"
        return json.loads(path.read_text())

    def declared(self, package: str = "ruby_mod") -> list[str]:
        with zipfile.ZipFile(self.packages / f"{package}.hmod") as archive:
            return [name for name in archive.namelist() if "/horizon/" in name]

    @staticmethod
    def reasons(report: dict, jar: int = 0) -> dict[str, list[str]]:
        return {entry["reason"]: entry["paths"] for entry in report["jars"][jar]["skipped"]}

    def mod_jar(self, name: str, mod_id: str | None = None, extra: dict | None = None) -> None:
        files = {"fabric.mod.json": json.dumps({"id": mod_id or name}).encode()}
        files.update(extra or {})
        make_jar(self.mods / f"{name}.jar", files)

    # RL1: the whole entry name.

    def test_an_entry_name_over_the_bound_is_left_out(self):
        limit = BAKE_MODULE.HORIZON_MAX_ENTRY_BYTES
        self.assertEqual(limit, 512)
        prefix = len("data/ruby_mod/horizon/block/") + len(".json")
        fits = "/".join(["q" * 99] * 4) + "/" + "r" * (limit - prefix - 400)
        over = fits + "s"
        asset = "assets/ruby_mod/textures/" + "/".join(["t" * 99] * 5)
        asset_fits = asset[: limit - 4] + ".png"
        asset_over = asset[: limit - 3] + ".png"
        self.declare(
            blocks=[{"id": f"ruby_mod:{fits}", "file": block()},
                    {"id": f"ruby_mod:{over}", "file": block()},
                    # Over the zip name field (65535 bytes): once a traceback in zipfile.
                    {"id": "ruby_mod:" + "/".join(["q" * 250] * 270), "file": block()}],
            items=[{"id": f"ruby_mod:{over}x", "file": {"template": "minecraft:stick"}}],
        )
        make_jar(self.mods / "ruby_mod.jar",
                 ruby_jar({asset_fits: PNG, asset_over: PNG}))
        self.assertEqual(self.main(), (0, ""))
        report = self.report_file()
        reason = (
            f"an entry name over {limit} bytes (the bake's cap on every entry: a block's "
            "cache path past the system's path limit stops the official game for every mod, RL1)"
        )
        left_out = self.reasons(report)[reason]
        self.assertEqual(len(left_out), 4)
        self.assertIn(f"ruby_mod:{over}", left_out)
        self.assertIn(f"ruby_mod:{over}x", left_out)
        self.assertIn(asset_over, left_out)
        self.assertEqual(len(f"data/ruby_mod/horizon/block/{fits}.json"), limit)
        self.assertIn(f"data/ruby_mod/horizon/block/{fits}.json", self.declared())
        with zipfile.ZipFile(self.packages / "ruby_mod.hmod") as archive:
            self.assertIn(asset_fits, archive.namelist())
        self.assertFalse(list(self.packages.glob("*.tmp")))

    def test_the_entry_bound_follows_the_game_folder(self):
        # On Linux an entry loaded at 4095 - 52 - (the folder's length) bytes, not one more.
        folder = Path("/" + "g" * 3899)
        self.assertEqual(BAKE_MODULE.horizon_max_entry(folder), 4095 - 52 - 3900)
        self.assertEqual(BAKE_MODULE.horizon_max_entry(Path("/srv/game")), 512)
        self.assertEqual(BAKE_MODULE.horizon_max_entry(Path("/" + "g" * 3530)), 512)
        self.assertEqual(BAKE_MODULE.horizon_max_entry(Path("/" + "g" * 3531)), 511)
        problem = BAKE_MODULE._horizon_path_problem
        self.assertIsNone(problem("assets/a/" + "b" * 10, 19))
        self.assertIsNotNone(problem("assets/a/" + "b" * 11, 19))
        self.assertIsNone(problem("assets/a/" + "b" * 11))

    def test_a_package_that_cannot_be_written_refuses_only_its_namespace(self):
        self.declare_by_jar(a=([{"id": "a:b", "file": block()}], []),
                            b=([{"id": "b:b", "file": block()}], []))
        self.mod_jar("a")
        self.mod_jar("b")
        real = BAKE_MODULE.write_package

        def failing(path, namespace, *arguments):
            if namespace == "a":
                path.with_name(path.name + ".tmp").write_bytes(b"partial")
                raise struct_error("'H' format requires 0 <= number <= 65535")
            return real(path, namespace, *arguments)

        with mock.patch.object(BAKE_MODULE, "write_package", failing):
            status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertIn("namespace a: the package could not be written", stderr)
        self.assertEqual(sorted(path.name for path in self.packages.iterdir()), ["b.hmod"])
        self.assertEqual(self.report_file()["enabled"], ["b"])

    # RS1, RS2: block states and blocks of the whole game.

    def test_the_measured_game_limits(self):
        self.assertEqual(BAKE_MODULE.HORIZON_MOD_STATES, 1_048_576 - 35_723)
        self.assertEqual(BAKE_MODULE.HORIZON_MOD_BLOCKS, MEASURED_MOD_BLOCKS)
        # RS3: 65536 states in one block loaded, 65537 and more stopped the game.
        self.assertEqual(BAKE_MODULE.MAX_STATES, 1 << 16)

    def test_one_block_over_65536_states_and_one_over_the_run_budget(self):
        per_block = (
            "more than 65536 states in one block (TooManyStates, fatal in the official game, RS3)"
        )

        def enum(count: int) -> dict:
            return {"type": "enum", "values": [f"v{index}" for index in range(count)]}

        self.declare(blocks=[
            # Each over 65536 states: 17 booleans, 256 by 257 values, one enum of 65537 values,
            # 2^20 and 2^21 (the specification's older bound).
            {"id": "ruby_mod:b17", "file": block(state_properties=booleans(17))},
            {"id": "ruby_mod:e256x257",
             "file": block(state_properties={"a": enum(256), "b": enum(257)})},
            {"id": "ruby_mod:e65537", "file": block(state_properties={"a": enum(65537)})},
            {"id": "ruby_mod:b20", "file": block(state_properties=booleans(20))},
            {"id": "ruby_mod:b21", "file": block(state_properties=booleans(21))},
            # Exactly 65536 states, the most one block may have.
            {"id": "ruby_mod:e256x256",
             "file": block(state_properties={"a": enum(256), "b": enum(256)})},
            {"id": "ruby_mod:e65536", "file": block(state_properties={"a": enum(65536)})},
            # 2^16 more would pass the mods' share beside the two above; one state more fits.
            {"id": "ruby_mod:b16", "file": block(state_properties=booleans(16))},
            {"id": "ruby_mod:small", "file": block()},
        ])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 2 * 65536 + 1):
            report = self.bake()
        reasons = self.reasons(report)
        self.assertEqual(reasons[per_block], ["ruby_mod:b17", "ruby_mod:e256x257",
                                              "ruby_mod:e65537", "ruby_mod:b20", "ruby_mod:b21"])
        self.assertEqual(reasons[BAKE_MODULE.HORIZON_STATES_REASON], ["ruby_mod:b16"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/e256x256.json",
                                           "data/ruby_mod/horizon/block/e65536.json",
                                           "data/ruby_mod/horizon/block/small.json"])

    def test_the_budget_boundary_counts_every_jar_and_existing_package(self):
        package_with_blocks(self.packages / "theirs.hmod", "theirs", {
            "x": block(state_properties=booleans(1)),  # 2 states
            "y": {"template": "minecraft:furnace", "state_properties": "not an object"},  # 1
            "z": b"not json",  # 1
        })
        self.declare_by_jar(
            a=([{"id": "a:b", "file": block(state_properties=booleans(2))},  # 4: total 8
                {"id": "a:c", "file": block(state_properties=booleans(1))}], []),  # 10
            b=([{"id": "b:b", "file": block(state_properties=booleans(1))},  # 12: over
                {"id": "b:c", "file": block()}], []),  # 11: fits
        )
        self.mod_jar("a")
        self.mod_jar("b")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 11):
            report = self.bake()
        self.assertEqual(self.declared("a"), ["data/a/horizon/block/b.json",
                                              "data/a/horizon/block/c.json"])
        self.assertEqual(self.reasons(report, 1)[BAKE_MODULE.HORIZON_STATES_REASON], ["b:b"])
        self.assertEqual(self.declared("b"), ["data/b/horizon/block/c.json"])

    def test_the_block_count_takes_existing_packages_and_earlier_jars(self):
        folder = self.packages / "theirs"
        (folder / "data" / "theirs" / "horizon" / "block" / "sub").mkdir(parents=True)
        (folder / "horizon-mod.toml").write_text(manifest_toml("theirs"))
        (folder / "data" / "theirs" / "horizon" / "block" / "sub" / "x.json").write_text("{}")
        (self.game / "horizon-mods.txt").write_text("mod.theirs: true\n")
        self.declare_by_jar(
            a=([{"id": "a:b", "file": block()}, {"id": "a:c", "file": block()}], []),
            b=([{"id": "b:b", "file": block()}], [{"id": "b:i", "file": {"template":
                                                                        "minecraft:stick"}}]),
        )
        self.mod_jar("a")
        self.mod_jar("b")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 3):
            report = self.bake()
        self.assertEqual(self.declared("a"), ["data/a/horizon/block/b.json",
                                              "data/a/horizon/block/c.json"])
        self.assertEqual(self.reasons(report, 1)[BAKE_MODULE.HORIZON_BLOCKS_REASON], ["b:b"])
        self.assertEqual(self.declared("b"), ["data/b/horizon/item/i.json"])

    def test_a_rebake_does_not_count_the_packages_it_replaces(self):
        self.declare(blocks=[{"id": "ruby_mod:a", "file": block(state_properties=booleans(2))},
                             {"id": "ruby_mod:b", "file": block()}])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 5), \
                mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 2):
            self.assertEqual(self.main(), (0, ""))
            first = self.declared()
            self.assertEqual(self.main(), (0, ""))
        self.assertEqual(first, ["data/ruby_mod/horizon/block/a.json",
                                 "data/ruby_mod/horizon/block/b.json"])
        self.assertEqual(self.declared(), first)

    def test_a_namespace_refused_whole_takes_none_of_the_budget(self):
        # `dup` is the mod id of both jars: the second jar's package would take its id.
        self.declare_by_jar(
            a=([{"id": "dup:x", "file": block()}], []),
            b=([{"id": "dup:y", "file": block()}, {"id": "own:z", "file": block()}], []),
        )
        self.mod_jar("a", "dup")
        make_jar(self.mods / "b.jar", {"META-INF/mods.toml":
                                       b'[[mods]]\nmodId = "dup"\n[[mods]]\nmodId = "own"\n'})
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 2):
            report = self.bake()
        self.assertEqual(report["jars"][1]["error"],
                         "namespace dup is already baked from a.jar; "
                         "two content mods may not own one namespace")
        self.assertEqual(self.declared("dup"), ["data/dup/horizon/block/x.json"])
        self.assertEqual(self.declared("own"), ["data/own/horizon/block/z.json"])
        with zipfile.ZipFile(self.packages / "dup.hmod") as archive:
            self.assertIn("baked from a.jar", archive.read("horizon-mod.toml").decode())

    def test_package_usage_of_what_the_bake_cannot_read(self):
        self.packages.mkdir(parents=True)
        broken = self.packages / "broken.hmod"
        broken.write_bytes(b"PK\x03\x04 not a zip")
        # Unknown, never zero (Round 6, RB3).
        with self.assertRaisesRegex(BAKE_MODULE._Uncountable, "broken.hmod: the bake cannot read"):
            BAKE_MODULE._package_usage(broken, [0])
        self.assertEqual(BAKE_MODULE._declared_states(
            {"state_properties": {"a": {"type": "boolean"}, "b": {"type": "enum", "values": [1, 2, 3]},
                                  "c": {"type": "enum", "values": []}, "d": 5}}), 6)

    # Strict JSON of what is written.

    def test_the_bytes_of_a_declaration_are_strict_json(self):
        check = BAKE_MODULE._horizon_declaration_file
        reason = BAKE_MODULE.HORIZON_DECLARATION_JSON_REASON
        deep = 1
        for _ in range(126):
            deep = [deep]
        for value in (float("nan"), float("inf"), 10**400, "\ud800", [deep]):
            self.assertEqual(check({"template": "minecraft:stone", "x": value}), (None, reason))
        data, problem = check({"x": deep})  # 127 nested, the outermost included
        self.assertIsNone(problem)
        self.assertTrue(data.endswith(b"\n"))
        too_deep = []
        for _ in range(5000):
            too_deep = [too_deep]
        self.assertEqual(check({"x": too_deep}), (None, reason))
        # Wired in: a file every other rule let through (they are switched off here) is left out
        # for the bytes it would write, and the next one is kept.
        self.declare(blocks=[{"id": "ruby_mod:nan", "file": block(properties={"hardness":
                                                                              float("nan")})},
                             {"id": "ruby_mod:ok", "file": block()}])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        with mock.patch.object(BAKE_MODULE, "_horizon_declaration_problem", return_value=None):
            report = self.bake()
        self.assertEqual(self.reasons(report)[reason], ["ruby_mod:nan"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/ok.json"])

    def test_state_property_definitions_take_only_the_keys_the_game_reads(self):
        reason = ("state property definition with a key other than type, and values of an enum "
                  "(ignored by the official game, left out; Round 4)")
        self.declare(blocks=[
            {"id": "ruby_mod:bvalues", "file": block(state_properties={
                "p": {"type": "boolean", "values": ["true", "false"]}})},
            {"id": "ruby_mod:bjunk", "file": block(state_properties={
                "p": {"type": "boolean", "junk": 1}})},
            {"id": "ruby_mod:ejunk", "file": block(state_properties={
                "p": {"type": "enum", "values": ["a"], "default": "a"}})},
            {"id": "ruby_mod:type", "file": block(state_properties={"p": {"type": ["enum"]}})},
            {"id": "ruby_mod:ok", "file": block(state_properties={
                "p": {"type": "enum", "values": ["a", "b"]}, "q": {"type": "boolean"}})},
        ])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        reasons = self.reasons(self.bake())
        self.assertEqual(reasons[reason], ["ruby_mod:bvalues", "ruby_mod:bjunk", "ruby_mod:ejunk"])
        self.assertEqual(reasons["state property type is not boolean or enum (refused, 3.2)"],
                         ["ruby_mod:type"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/ok.json"])

    # Ordering: nothing leaves a declaration out after the remainders are decided.

    def test_a_remainder_never_names_a_declaration_left_out_later(self):
        stick = {"template": "minecraft:stick"}
        big = block(state_properties={"p": {"type": "enum", "values": ["a" * 64, "b" * 64]}})
        self.declare(
            blocks=[{"id": "ruby_mod:big", "file": big},
                    {"id": "ruby_mod:many", "file": block(state_properties=booleans(3))},
                    {"id": "ruby_mod:a", "file": block()},
                    {"id": "ruby_mod:a.json/c", "file": block()}],
            items=[{"id": "ruby_mod:r_big", "file": dict(stick, crafting_remainder="ruby_mod:big")},
                   {"id": "ruby_mod:r_many",
                    "file": dict(stick, crafting_remainder="ruby_mod:many")},
                   {"id": "ruby_mod:r_a", "file": dict(stick, crafting_remainder="ruby_mod:a")},
                   {"id": "ruby_mod:many", "file": stick}],
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        size = len(BAKE_MODULE._declaration_bytes(big)) - 1
        with mock.patch.object(BAKE_MODULE, "MAX_FILE", size), \
                mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 4):
            report = self.bake()
        reasons = self.reasons(report)
        self.assertEqual(reasons["declaration over the content file cap (16 MiB, P5)"],
                         ["ruby_mod:big"])
        self.assertEqual(reasons[BAKE_MODULE.HORIZON_STATES_REASON], ["ruby_mod:many"])
        remainder = ("item crafting_remainder is not a vanilla item or one of the package's own "
                     "(fatal in the official game, 3.3)")
        # `a` gets no generated blockstate beside `a.json/c` (RB1): left out before the
        # remainders, so `r_a` goes with it.
        self.assertEqual(reasons[BAKE_MODULE.HORIZON_BLOCKSTATE_CLASH_REASON], ["ruby_mod:a"])
        self.assertEqual(reasons[remainder], ["ruby_mod:r_big", "ruby_mod:r_a"])
        # `many` lost its block, so its item file is a plain item, and `r_many` names it.
        self.assertEqual(self.declared(), [
            "data/ruby_mod/horizon/block/a.json/c.json",
            "data/ruby_mod/horizon/item/many.json", "data/ruby_mod/horizon/item/r_many.json",
        ])

    # Crash-freedom.

    def corrupt_jars(self) -> None:
        self.mod_jar("a")  # good
        # A corrupt deflate stream: a BTYPE=11 block.
        path = self.mods / "b.jar"
        with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("fabric.mod.json", b'{"id":"b"}')
            archive.writestr("assets/b/lang/en_us.json", b'{"x":"' + b"y" * 200 + b'"}')
        data = bytearray(path.read_bytes())
        offset = data.index(b"assets/b/lang/en_us.json") + len("assets/b/lang/en_us.json")
        data[offset] = 0xFF
        path.write_bytes(bytes(data))
        # A compression method zipfile lacks.
        path = self.mods / "c.jar"
        make_jar(path, {"assets/c/lang/en_us.json": b"{}"})
        data = bytearray(path.read_bytes())
        data[8:10] = (9).to_bytes(2, "little")
        central = data.index(b"PK\x01\x02")
        data[central + 10: central + 12] = (9).to_bytes(2, "little")
        path.write_bytes(bytes(data))
        # A name flagged UTF-8 that is not.
        path = self.mods / "d.jar"
        make_jar(path, {"assets/d/lang/x.json": b"{}"})
        data = bytearray(path.read_bytes())
        for header in (b"PK\x03\x04", b"PK\x01\x02"):
            start = data.index(header)
            flags = start + (6 if header == b"PK\x03\x04" else 8)
            data[flags + 1] |= 0x08  # bit 11: the name is UTF-8
        name = data.index(b"assets/d/lang/x.json")
        data[name] = 0xFF
        name = data.index(b"assets/d/lang/x.json", name + 1)
        data[name] = 0xFF
        path.write_bytes(bytes(data))
        # A folder named like a jar.
        (self.mods / "e.jar").mkdir()

    def test_a_jar_the_bake_cannot_read_is_refused_and_the_run_goes_on(self):
        self.corrupt_jars()
        self.declare_by_jar(a=([{"id": "a:b", "file": block()}], []))
        status, stderr = self.main()
        self.assertEqual(status, 1)
        report = self.report_file()
        errors = {row["jar"]: row["error"] for row in report["jars"]}
        self.assertIsNone(errors["a.jar"])
        self.assertIn("zlib.error: Error -3 while decompressing data", errors["b.jar"] or "")
        self.assertIn("NotImplementedError", errors["c.jar"] or "")
        self.assertIn("UnicodeDecodeError", errors["d.jar"] or "")
        self.assertIn("IsADirectoryError", errors["e.jar"] or "")
        self.assertEqual(report["enabled"], ["a"])
        self.assertTrue((self.packages / "a.hmod").is_file())

    def test_what_a_step_raises_refuses_only_that_jar(self):
        # The net under every step's own guards: a step that raises refuses its jar, with the
        # packages it wrote before kept, and the next jar is baked.
        self.declare_by_jar(a=([{"id": "a:b", "file": block()}], []),
                            b=([{"id": "b:b", "file": block()}], []))
        self.mod_jar("a")
        self.mod_jar("b")
        real = BAKE_MODULE._horizon_declarations

        def failing(candidates, *arguments):
            if candidates and candidates[0][1] == "a:b":
                raise RecursionError("maximum recursion depth exceeded")
            return real(candidates, *arguments)

        with mock.patch.object(BAKE_MODULE, "_horizon_declarations", failing):
            status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertIn("a.jar: refused: the bake could not finish the jar: RecursionError: "
                      "maximum recursion depth exceeded", stderr)
        self.assertEqual(self.report_file()["enabled"], ["b"])

    def test_a_mod_id_that_is_no_package_id_takes_no_package_for_its_assets(self):
        # A mod id may hold `-` or `.`; a package id is [a-z0-9_] (1.3).
        self.declare()
        for mod_id in ("ruby-dash", "ruby.dot"):
            with self.subTest(mod_id):
                shutil_rmtree(self.mods)
                self.mods.mkdir(parents=True)
                self.mod_jar("a", mod_id, {f"assets/{mod_id}/lang/en_us.json": b"{}"})
                report = self.bake()
                self.assertIn(f"namespace {mod_id!r} is not a content package id",
                              report["jars"][0]["error"])
                self.assertEqual(report["jars"][0]["packages"], [])
                self.assertFalse((self.packages / f"{mod_id}.hmod").exists())

    def test_a_corrupt_jar_no_longer_stops_a_sunrise_bake(self):
        self.corrupt_jars()
        report = BAKE_MODULE.bake_directory(self.mods, self.game, probe=self.probe).as_json()
        errors = {row["jar"]: row["error"] for row in report["jars"]}
        self.assertTrue(errors["b.jar"].startswith("not a readable jar: zlib.error: "))
        self.assertTrue(errors["c.jar"].startswith("not a readable jar: NotImplementedError"))
        self.assertTrue(errors["d.jar"].startswith("not a readable jar: UnicodeDecodeError"))
        self.assertIsNone(errors["a.jar"])
        self.assertTrue(errors["e.jar"].startswith("not a readable jar: IsADirectoryError"))

    def test_probe_output_over_the_integer_conversion_limit_refuses_the_jar(self):
        self.probe = fake_probe(
            self.root,
            '{"status":"ok","entrypoints":["a"],"content":{"blocks":[{"id":"ruby_mod:b",'
            '"file":{"template":"minecraft:stone","properties":{"light":' + "1" * 5000 + "}}}]}}",
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertIn("ruby_mod.jar: refused: the runtime could not run the jar's entrypoints: "
                      "no server entrypoint ran (not JSON: Exceeds the limit", stderr)

    def test_a_turned_on_file_that_is_not_utf8_is_an_error_not_a_crash(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        (self.game / "horizon-mods.txt").write_bytes(b"mod.x:true\n\xff\n")
        status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertIn("error: horizon-mods.txt not updated", stderr)
        report = self.report_file()
        self.assertEqual(report["enabled"], [])
        self.assertTrue(report["errors"])

    # Namespace ownership.

    def test_the_real_owner_keeps_its_namespace_whatever_the_jar_order(self):
        self.declare_by_jar(
            aaa=([{"id": "aaa:b", "file": block()}, {"id": "zzz:x", "file": block()},
                  {"id": "free:y", "file": block()}], []),
            zzz=([{"id": "zzz:b", "file": block()}, {"id": "zzz:c", "file": block()}], []),
        )
        self.mod_jar("aaa", extra={"assets/zzz/lang/en_us.json": b"{}"})
        self.mod_jar("zzz")
        status, stderr = self.main()
        self.assertEqual((status, stderr), (0, ""))
        report = self.report_file()
        reason = ("a declaration in the namespace of another jar's mod id (that jar's package; "
                  "left out)")
        self.assertEqual(self.reasons(report)[reason], ["zzz:x"])
        self.assertEqual(
            self.reasons(report)[BAKE_MODULE.HORIZON_UNOWNED_REASON], ["assets/zzz/lang/en_us.json"]
        )
        self.assertEqual([p["id"] for p in report["jars"][0]["packages"]], ["aaa", "free"])
        self.assertEqual([p["id"] for p in report["jars"][1]["packages"]], ["zzz"])
        self.assertEqual(self.declared("zzz"), ["data/zzz/horizon/block/b.json",
                                                "data/zzz/horizon/block/c.json"])

    def test_a_foreign_namespace_held_elsewhere_is_left_out_not_an_error(self):
        # `free` is no jar's mod id: `a` sorts first and takes it, `b` keeps its own package.
        self.declare_by_jar(
            a=([{"id": "a:b", "file": block()}, {"id": "free:x", "file": block()}], []),
            b=([{"id": "b:b", "file": block()}, {"id": "free:y", "file": block()},
                {"id": "user:z", "file": block()}], []),
        )
        self.mod_jar("a")
        self.mod_jar("b", extra={"assets/free/lang/en_us.json": b"{}"})
        self.packages.mkdir(parents=True)
        with zipfile.ZipFile(self.packages / "theirs.hmod", "w") as archive:
            archive.writestr("horizon-mod.toml", manifest_toml("user"))
        status, stderr = self.main()
        self.assertEqual((status, stderr), (0, ""))
        report = self.report_file()
        self.assertEqual([p["id"] for p in report["jars"][0]["packages"]], ["a", "free"])
        self.assertEqual([p["id"] for p in report["jars"][1]["packages"]], ["b"])
        self.assertIsNone(report["jars"][1]["error"])
        reasons = self.reasons(report, 1)
        claimed = ("a namespace that is not the jar's mod id, held elsewhere (namespace free is "
                   "already baked from a.jar; two content mods may not own one namespace)")
        self.assertEqual(reasons[claimed], ["free:y", "assets/free/lang/en_us.json"])
        held = [reason for reason in reasons if "theirs.hmod already uses" in reason]
        self.assertEqual([reasons[reason] for reason in held], [["user:z"]])
        self.assertEqual(self.declared("free"), ["data/free/horizon/block/x.json"])

    # Rules the round-3 mutation run left unpinned.

    def test_a_namespace_already_baked_in_this_run_is_refused_not_overwritten(self):
        self.declare_by_jar(a=([{"id": "dup:x", "file": block()}], []),
                            b=([{"id": "dup:y", "file": block()}], []))
        self.mod_jar("a", "dup")
        self.mod_jar("b", "dup")
        status, stderr = self.main()
        self.assertEqual(status, 1)
        self.assertIn("b.jar: refused: namespace dup is already baked from a.jar", stderr)
        self.assertEqual(self.declared("dup"), ["data/dup/horizon/block/x.json"])

    def test_declaration_shapes_the_round_3_mutants_kept(self):
        stick = {"template": "minecraft:stick"}
        two = {"a": {"type": "boolean"}}
        self.declare(
            blocks=[
                {"id": "ruby_mod:props_list", "file": block(properties=[1])},
                {"id": "ruby_mod:shape_both", "file": block(state_properties=two, shapes={
                    "property": "a", "properties": ["a"], "default": []})},
                {"id": "ruby_mod:shape_dup", "file": block(state_properties=two, shapes={
                    "properties": ["a", "a"], "default": []})},
                {"id": "ruby_mod:box_flat", "file": block(shapes={
                    "default": [[0, 0.5, 0, 1, 0.5, 1]]})},
                {"id": "ruby_mod:box_turned", "file": block(shapes={
                    "default": [[0, 0.6, 0, 1, 0.5, 1]]})},
            ],
            items=[{"id": "ruby_mod:stack_true", "file": dict(stick, max_stack_size=True)},
                   {"id": "ruby_mod:stack_ok", "file": dict(stick, max_stack_size=99)}],
        )
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        reasons = self.reasons(self.bake())
        self.assertEqual(reasons["block properties is not an object (refused, 3.2)"],
                         ["ruby_mod:props_list"])
        self.assertEqual(
            reasons["block shape table with both property and properties (not established, "
                    "left out)"], ["ruby_mod:shape_both"])
        self.assertEqual(reasons["block shape names a property twice (not established, left out)"],
                         ["ruby_mod:shape_dup"])
        self.assertEqual(
            reasons["block shape of over 16 boxes or a box not 6 ordered finite numbers "
                    "(refused, 3.2)"], ["ruby_mod:box_turned"])
        self.assertEqual(reasons["item max_stack_size is not 1 to 99 (refused, 3.3)"],
                         ["ruby_mod:stack_true"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/box_flat.json",
                                           "data/ruby_mod/horizon/item/stack_ok.json"])

    def test_a_stale_package_is_turned_on_only_by_a_true_line(self):
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        self.assertEqual(self.main(), (0, ""))
        (self.mods / "ruby_mod.jar").unlink()
        for text, on in (("mod.ruby_mod:false\n", False), ("mod.ruby_mod: true \n", True),
                         ("mod.ruby_mod:truex\n", False), ("mod.other:true\n", False)):
            (self.game / "horizon-mods.txt").write_text(text)
            report = BAKE_MODULE.bake_directory(
                self.mods, self.game, probe=self.probe, target=BAKE_MODULE.HORIZON,
                turn_on=False,
            )
            self.assertEqual([stale["turned_on"] for stale in report.stale], [on], text)

    def test_without_the_max_damage_table_nothing_is_declared(self):
        item_data = self.root / "vanilla_item_data.rs"
        item_data.write_text("const OTHER: &[(&str, i32)] = &[\n];\n")
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        BAKE_MODULE._VANILLA.clear()
        try:
            with mock.patch.object(BAKE_MODULE, "VANILLA_ITEM_DATA", item_data):
                report = self.bake()
        finally:
            BAKE_MODULE._VANILLA.clear()
        reason = "no vanilla block and item tables to check the declaration against"
        self.assertEqual(sorted(self.reasons(report)[reason]),
                         ["ruby_mod:ruby", "ruby_mod:ruby_block"])


class HorizonRound5Tests(unittest.TestCase):
    """The rules of the fifth review round (spec "Round 5 (black-box)")."""

    setUp = HorizonRound4Tests.setUp
    tearDown = HorizonRound4Tests.tearDown
    declare = HorizonRound4Tests.declare
    declare_by_jar = HorizonRound4Tests.declare_by_jar
    bake = HorizonRound4Tests.bake
    report_file = HorizonRound4Tests.report_file
    declared = HorizonRound4Tests.declared
    reasons = staticmethod(HorizonRound4Tests.reasons)
    mod_jar = HorizonRound4Tests.mod_jar

    def run_main(self, out: io.TextIOBase | None = None) -> tuple[int, str]:
        stderr = io.StringIO()
        with contextlib.redirect_stdout(out or io.StringIO()), contextlib.redirect_stderr(stderr):
            status = BAKE_MODULE.main(
                ["--game-dir", str(self.game), "--probe", str(self.probe), "--target", "horizon"]
            )
        return status, stderr.getvalue()

    def manifest(self, package: str) -> str:
        with zipfile.ZipFile(self.packages / f"{package}.hmod") as archive:
            return archive.read("horizon-mod.toml").decode()

    # RO1: an earlier bake's package is replaced only by the same mod.

    def test_a_foreign_jar_never_overwrites_another_mods_baked_package(self):
        # The round-4 review: zzz.jar bakes zzz.hmod; a later run has only aaa.jar, which declares
        # zzz:x. zzz.hmod is left as it is and named stale; aaa.jar keeps its own package.
        self.declare_by_jar(
            zzz=([{"id": "zzz:b", "file": block()}, {"id": "zzz:c", "file": block()}], []),
            aaa=([{"id": "aaa:b", "file": block()}, {"id": "zzz:x", "file": block()}], []),
        )
        self.mod_jar("zzz", extra={"assets/zzz/lang/en_us.json": b"{}"})
        self.assertEqual(self.run_main(), (0, ""))
        self.assertIn("Mod ids of the jar: zzz.", self.manifest("zzz"))
        before = (self.packages / "zzz.hmod").read_bytes()
        (self.mods / "zzz.jar").unlink()
        self.mod_jar("aaa")
        status, stderr = self.run_main()
        self.assertEqual(status, 0)
        self.assertEqual((self.packages / "zzz.hmod").read_bytes(), before)
        report = self.report_file()
        self.assertEqual(report["stale"], [{
            "id": "zzz", "path": "horizon-mods/zzz.hmod", "baked_from": "zzz.jar",
            "mod_ids": ["zzz"], "turned_on": True}])
        self.assertIn("stale package from an earlier bake of zzz.jar", stderr)
        self.assertEqual([p["id"] for p in report["jars"][0]["packages"]], ["aaa"])
        reason = (
            "a namespace that is not the jar's mod id, held elsewhere (namespace zzz: "
            "horizon-mods/zzz.hmod was baked from zzz.jar (mod ids zzz), another mod's package "
            "for it; not overwritten (remove it to bake the namespace from this jar))"
        )
        self.assertEqual(self.reasons(report)[reason], ["zzz:x"])

    def test_a_jar_of_the_namespaces_mod_id_never_takes_a_package_another_jar_baked(self):
        # aaa.jar baked zzz.hmod for its zzz:x; zzz.jar, whose mod id it is, gets an error.
        self.declare_by_jar(
            aaa=([{"id": "zzz:x", "file": block()}], []),
            zzz=([{"id": "zzz:b", "file": block()}], []),
        )
        self.mod_jar("aaa")
        self.assertEqual(self.run_main(), (0, ""))
        before = (self.packages / "zzz.hmod").read_bytes()
        (self.mods / "aaa.jar").unlink()
        self.mod_jar("zzz")
        status, stderr = self.run_main()
        self.assertEqual(status, 1)
        self.assertIn("zzz.jar: refused: namespace zzz: horizon-mods/zzz.hmod was baked from "
                      "aaa.jar (mod ids aaa)", stderr)
        self.assertEqual((self.packages / "zzz.hmod").read_bytes(), before)
        self.assertEqual([row["id"] for row in self.report_file()["stale"]], ["zzz"])

    def test_the_same_mod_replaces_its_package_under_a_new_jar_name(self):
        self.declare_by_jar(**{
            "zzz-1.0": ([{"id": "zzz:b", "file": block()}, {"id": "free:x", "file": block()}],
                        []),
            "zzz-1.1": ([{"id": "zzz:c", "file": block()}, {"id": "free:y", "file": block()}],
                        []),
        })
        self.mod_jar("zzz-1.0", "zzz")
        self.assertEqual(self.run_main(), (0, ""))
        (self.mods / "zzz-1.0.jar").unlink()
        self.mod_jar("zzz-1.1", "zzz")
        self.assertEqual(self.run_main(), (0, ""))
        self.assertEqual(self.declared("zzz"), ["data/zzz/horizon/block/c.json"])
        self.assertEqual(self.declared("free"), ["data/free/horizon/block/y.json"])
        self.assertEqual(self.report_file()["stale"], [])
        self.assertIn("baked from zzz-1.1.jar", self.manifest("free"))

    def test_the_owner_rule(self):
        same = BAKE_MODULE._same_owner

        def baked(jar: str, ids: list[str] | None) -> object:
            return BAKE_MODULE.Existing(Path("x.hmod"), frozenset(["ns"]), jar,
                                        None if ids is None else frozenset(ids), "ns")

        for namespace, package, jar, ids, expected in (
            ("ns", baked("a.jar", ["ns"]), "b.jar", {"ns"}, True),  # the namespace's own mod
            ("ns", baked("a.jar", ["ns"]), "a.jar", {"other"}, False),
            ("ns", baked("a.jar", ["other"]), "a.jar", {"ns"}, False),
            ("ns", baked("a.jar", ["a", "b"]), "c.jar", {"b", "a"}, True),  # same mod ids
            ("ns", baked("a.jar", ["a", "b"]), "a.jar", {"a"}, False),
            ("ns", baked("a.jar", []), "a.jar", set(), True),  # no mod id: the jar's name
            ("ns", baked("a.jar", []), "b.jar", set(), False),
            ("ns", baked("a.jar", []), "a.jar", {"a"}, False),
            ("ns", baked("a.jar", None), "a.jar", {"x"}, True),  # an earlier round's package
            ("ns", baked("a.jar", None), "b.jar", {"ns"}, False),
            # Only ids shaped as a namespace are recorded, so only those are compared.
            ("ns", baked("a.jar", []), "a.jar", {"Not An Id"}, True),
            ("ns", baked("bad\ufffd.jar", []), "bad\udcff.jar", set(), True),
            ("ns", baked("bad\ufffd.jar", None), "bad\udcff.jar", {"x"}, True),
        ):
            with self.subTest(package=package, jar=jar, ids=ids):
                self.assertEqual(same(namespace, package, jar, frozenset(ids)), expected)

    def test_the_description_records_the_mod_ids_and_reads_back(self):
        manifest = BAKE_MODULE._manifest(
            "ns", 'x for the ns namespace. Mod ids of the jar: q".jar', BAKE_MODULE.HORIZON,
            frozenset({"b.c", "a", "Not An Id"}),
        )
        description = tomllib.loads(manifest)["description"]
        self.assertTrue(description.endswith(" for the ns namespace. Mod ids of the jar: a, b.c."))
        match = BAKE_MODULE.BAKED_DESCRIPTION.fullmatch(description)
        self.assertEqual(match.group(1), 'x for the ns namespace. Mod ids of the jar: q".jar')
        self.assertEqual((match.group(2), match.group(3)), ("ns", "a, b.c"))
        none = tomllib.loads(BAKE_MODULE._manifest("ns", "j.jar", BAKE_MODULE.HORIZON))
        self.assertIn("Mod ids of the jar: none.", none["description"])
        old = BAKE_MODULE.BAKED_DESCRIPTION.fullmatch(
            "Assets and data baked from j.jar for the ns namespace.")
        self.assertEqual((old.group(1), old.group(3)), ("j.jar", None))
        # The Sunrise manifest is what it always was.
        self.assertNotIn("Mod ids", BAKE_MODULE._manifest("ns", "j.jar", frozenset({"a"})))

    # R2: what the bake cannot decide is an error, mod id or not.

    def test_an_unreadable_manifest_is_an_error_for_a_namespace_that_is_no_mod_id(self):
        # The round-4 review's repro: one unrelated corrupt package cost a jar its namespace
        # without a word.
        self.declare(blocks=[{"id": "ns:b", "file": block()}])
        self.mod_jar("ok", "okmod", extra={"assets/ns/lang/en_us.json": b"{}"})
        self.packages.mkdir(parents=True)
        (self.packages / "other.hmod").write_bytes(b"garbage")
        status, stderr = self.run_main()
        self.assertEqual(status, 1)
        self.assertIn("ok.jar: refused: namespace ns: the manifest of horizon-mods/other.hmod "
                      "cannot be read, so it may use this id; not written", stderr)
        self.assertFalse((self.packages / "ns.hmod").exists())
        (self.packages / "other.hmod").unlink()
        self.assertEqual(self.run_main(), (0, ""))
        self.assertTrue((self.packages / "ns.hmod").is_file())

    def test_a_folder_or_another_ids_package_in_the_way_is_an_error_too(self):
        self.declare(blocks=[{"id": "ns:b", "file": block()}, {"id": "okmod:b", "file": block()}])
        self.mod_jar("ok", "okmod")
        (self.packages / "ns.hmod").mkdir(parents=True)
        report = self.bake()
        self.assertEqual(report["jars"][0]["error"],
                         "namespace ns: a folder sits where its package goes; not overwritten")
        self.assertEqual([p["id"] for p in report["jars"][0]["packages"]], ["okmod"])
        (self.packages / "ns.hmod").rmdir()
        with zipfile.ZipFile(self.packages / "ns.hmod", "w") as archive:
            archive.writestr("horizon-mod.toml", manifest_toml("someone"))
        report = self.bake()
        self.assertIn("namespace ns: horizon-mods/ns.hmod is a package this bake did not write "
                      "for it", report["jars"][0]["error"])

    def test_a_namespace_another_package_holds_is_left_out_even_beside_an_unreadable_one(self):
        self.declare(blocks=[{"id": "ns:b", "file": block()}, {"id": "okmod:b", "file": block()}])
        self.mod_jar("ok", "okmod")
        self.packages.mkdir(parents=True)
        (self.packages / "a_broken.hmod").write_bytes(b"garbage")
        with zipfile.ZipFile(self.packages / "theirs.hmod", "w") as archive:
            archive.writestr("horizon-mod.toml", manifest_toml("theirs", ["ns"]))
        report = self.bake()
        held = [reason for reason in self.reasons(report) if "theirs.hmod already uses" in reason]
        self.assertEqual([self.reasons(report)[reason] for reason in held], [["ns:b"]])
        # The jar's own namespace is still blocked by the unreadable manifest.
        self.assertIn("the manifest of horizon-mods/a_broken.hmod cannot be read",
                      report["jars"][0]["error"])
        self.assertEqual(report["jars"][0]["packages"], [])

    # RB2, RB3: what the block count reads of the packages already installed.

    def test_a_package_that_is_off_is_not_counted(self):
        package_with_blocks(self.packages / "theirs.hmod", "theirs",
                            {"x": block(), "y": block()}, turn_on=False)
        self.declare(blocks=[{"id": "ruby_mod:a", "file": block()},
                             {"id": "ruby_mod:b", "file": block()}])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        for text, counted in (
            (None, False),  # no horizon-mods.txt: every package is off (L5)
            ("mod.theirs:false\n", False),
            ("mod.theirs:TRUE\n", False),
            ("# mod.theirs:true\n", False),
            ("mod.theirs:true\n", True),
            ("  mod.theirs : true\r\n", True),  # not established: counted
            ("mod.theirs:false\nmod.theirs:true\n", True),
            (b"\xff mod.theirs:false\n", True),  # unreadable: counted
        ):
            with self.subTest(text=text):
                mods_file = self.game / "horizon-mods.txt"
                if text is None:
                    mods_file.unlink(missing_ok=True)
                elif isinstance(text, bytes):
                    mods_file.write_bytes(text)
                else:
                    mods_file.write_text(text)
                with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 3):
                    report = BAKE_MODULE.bake_directory(
                        self.mods, self.game, probe=self.probe, target=BAKE_MODULE.HORIZON,
                        turn_on=False,
                    ).as_json()
                expected = (["data/ruby_mod/horizon/block/a.json"] if counted else
                            ["data/ruby_mod/horizon/block/a.json",
                             "data/ruby_mod/horizon/block/b.json"])
                self.assertEqual(self.declared(), expected)
                if counted:
                    self.assertEqual(
                        self.reasons(report)[BAKE_MODULE.HORIZON_BLOCKS_REASON], ["ruby_mod:b"])

    def test_the_count_reads_at_most_its_bound_and_then_declares_no_block(self):
        package_with_blocks(self.packages / "theirs.hmod", "theirs",
                            {f"k{index}": block() for index in range(8)})
        folder = self.packages / "dir_pkg"
        (folder / "data" / "dir_pkg" / "horizon" / "block").mkdir(parents=True)
        (folder / "horizon-mod.toml").write_text(manifest_toml("dir_pkg"))
        with (self.game / "horizon-mods.txt").open("a") as stream:
            stream.write("mod.dir_pkg:true\n")
        for index in range(4):
            (folder / "data" / "dir_pkg" / "horizon" / "block" / f"k{index}.json").write_bytes(
                json.dumps(block()).encode())
        size = len(json.dumps(block()).encode())
        self.declare(blocks=[{"id": "ruby_mod:a", "file": block()}],
                     items=[{"id": "ruby_mod:i", "file": {"template": "minecraft:stick"}}])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        existing = BAKE_MODULE.existing_packages([self.packages])
        # Within the bound: every file read and counted, directory packages too.
        with mock.patch.object(BAKE_MODULE, "HORIZON_USAGE_BYTES", 12 * size):
            budget = BAKE_MODULE.Budget.of(existing, self.game)
        self.assertEqual(budget.unknown, {})
        self.assertEqual((budget.states, budget.blocks), (12, 12))
        # One byte under it: the count stops at the file that passes it.
        reads = []
        real = BAKE_MODULE.json.loads

        def loads(data, *arguments, **keywords):
            reads.append(len(data))
            return real(data, *arguments, **keywords)

        with mock.patch.object(BAKE_MODULE, "HORIZON_USAGE_BYTES", 12 * size - 1), \
                mock.patch.object(BAKE_MODULE.json, "loads", loads):
            budget = BAKE_MODULE.Budget.of(existing, self.game)
        self.assertEqual(len(reads), 11)
        self.assertEqual(budget.unknown, {
            (self.packages / "theirs.hmod").resolve(): "more than 0 MiB of block declarations"})
        self.assertEqual(budget.left(self.packages / "x.hmod"), (0, 0))
        self.assertEqual(budget.uncounted(self.packages / "x.hmod"),
                         ((self.packages / "theirs.hmod").resolve(),
                          "more than 0 MiB of block declarations"))
        with mock.patch.object(BAKE_MODULE, "HORIZON_USAGE_BYTES", 12 * size - 1):
            report = self.bake()
        # The namespace with a block is an error and not written at all (Round 6, RB3, RB4).
        self.assertEqual(report["jars"][0]["error"], (
            "namespace ruby_mod: not written: the blocks of horizon-mods/theirs.hmod cannot be "
            "counted (more than 0 MiB of block declarations), so its blocks could pass the game's "
            "limits (fatal, RS1, RS2, RB3); nothing written"))
        self.assertFalse((self.packages / "ruby_mod.hmod").exists())
        self.assertEqual(report["errors"], [
            "the blocks of the packages in horizon-mods/ could not be counted (more than 0 MiB of "
            "block declarations): the namespaces with blocks were not written (ruby_mod)"])
        with mock.patch.object(BAKE_MODULE, "HORIZON_USAGE_BYTES", 12 * size - 1):
            status, stderr = self.run_main()
        self.assertEqual(status, 1)
        self.assertIn("error: the blocks of the packages in horizon-mods/ could not be counted",
                      stderr)

    def test_the_count_takes_block_files_only_and_does_not_read_one_over_16_mib(self):
        two = json.dumps(block(state_properties=booleans(2))).encode()  # 4 states
        package_with_blocks(self.packages / "theirs.hmod", "theirs", {"k": two, "big": two + b" "})
        with zipfile.ZipFile(self.packages / "theirs.hmod", "a") as archive:
            archive.writestr("data/theirs/horizon/item/i.json", two)
            archive.writestr("data/theirs/tags/block/t.json", two)
        folder = self.packages / "dir_pkg"
        for part in ("horizon/block", "horizon/item", "tags/block"):
            (folder / "data" / "dir_pkg" / part).mkdir(parents=True)
            (folder / "data" / "dir_pkg" / part / "k.json").write_bytes(two)
        (folder / "data" / "dir_pkg" / "horizon" / "block" / "big.json").write_bytes(two + b" ")
        (folder / "data" / "dir_pkg" / "horizon" / "block" / "x.txt").write_bytes(two)
        (folder / "horizon-mod.toml").write_text(manifest_toml("dir_pkg"))
        with (self.game / "horizon-mods.txt").open("a") as stream:
            stream.write("mod.dir_pkg:true\n")
        existing = BAKE_MODULE.existing_packages([self.packages])
        budget = BAKE_MODULE.Budget.of(existing, self.game)
        self.assertEqual((budget.states, budget.blocks), (16, 4))
        # A file over the cap is not read and counts one state (the zip's `big`, the folder's
        # `big.json`, one byte longer than `two`).
        with mock.patch.object(BAKE_MODULE, "MAX_FILE", len(two)):
            budget = BAKE_MODULE.Budget.of(existing, self.game)
        self.assertEqual((budget.states, budget.blocks), (10, 4))
        with mock.patch.object(BAKE_MODULE, "MAX_FILE", len(two) - 1):
            budget = BAKE_MODULE.Budget.of(existing, self.game)
        self.assertEqual((budget.states, budget.blocks), (4, 4))

    def test_a_zip_with_too_many_entries_is_not_opened(self):
        package_with_blocks(self.packages / "theirs.hmod", "theirs",
                            {f"k{index}": block() for index in range(5)})
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", 5):
            # Six entries with the manifest: its id is unknown, and so is the count.
            existing = BAKE_MODULE.existing_packages([self.packages])
            self.assertEqual([package.owned for package in existing], [None])
            budget = BAKE_MODULE.Budget.of(
                [BAKE_MODULE.Existing(self.packages / "theirs.hmod", frozenset(["theirs"]),
                                      package_id="theirs")], self.game)
            self.assertIn("theirs.hmod: over 5 entries",
                          budget.unknown[(self.packages / "theirs.hmod").resolve()])
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_DIRECTORY", 10):
            self.assertEqual(
                [package.owned for package in BAKE_MODULE.existing_packages([self.packages])],
                [None])
        self.assertEqual(
            [package.owned for package in BAKE_MODULE.existing_packages([self.packages])],
            [frozenset(["theirs"])])

    def test_a_rebake_of_two_jars_charges_each_replaced_package_once(self):
        # Budget.commit takes the old package's blocks off: without that, run 2 charges `a`
        # twice and `b` loses `z` (round-4 review mutant).
        self.declare_by_jar(
            a=([{"id": "a:x", "file": block()}, {"id": "a:y", "file": block()}], []),
            b=([{"id": "b:z", "file": block()}], []),
        )
        self.mod_jar("a")
        self.mod_jar("b", extra={"assets/b/lang/en_us.json": b"{}"})
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 3):
            self.assertEqual(self.run_main(), (0, ""))
            self.assertEqual(self.run_main(), (0, ""))
        self.assertEqual(self.declared("a"), ["data/a/horizon/block/x.json",
                                              "data/a/horizon/block/y.json"])
        self.assertEqual(self.declared("b"), ["data/b/horizon/block/z.json"])

    def test_a_namespace_with_nothing_left_writes_no_package(self):
        # Its only declaration is left out and it has no file: no manifest-only package, and
        # nothing turned on (round-4 review mutant). The same when its files all clash.
        self.declare(blocks=[{"id": "ns:b", "file": block(template="minecraft:furnace")},
                             {"id": "cl:b", "file": block(template="minecraft:furnace")}])
        self.mod_jar("ok", "okmod", extra={"assets/cl/textures/t": PNG,
                                           "assets/cl/textures/t/u.png": PNG})
        status, _ = self.run_main()
        self.assertEqual(status, 0)
        self.assertFalse(self.packages.exists() and list(self.packages.iterdir()))
        self.assertFalse((self.game / "horizon-mods.txt").exists())
        self.assertEqual(self.report_file()["enabled"], [])

    # RB1: a generated blockstate that would be a folder.

    def test_a_block_whose_generated_blockstate_would_be_a_folder_is_left_out(self):
        self.declare(blocks=[
            {"id": "ruby_mod:a", "file": block()},
            {"id": "ruby_mod:a.json/c", "file": block()},
            {"id": "ruby_mod:z.json/c", "file": block()},  # the folder side first
            {"id": "ruby_mod:z", "file": block()},
            {"id": "ruby_mod:q", "file": block()},  # a chain: q and q.json/r lose theirs
            {"id": "ruby_mod:q.json/r", "file": block()},
            {"id": "ruby_mod:q.json/r.json/s", "file": block()},
            {"id": "ruby_mod:p", "file": block()},  # beside a shipped p.json/c.json: generated
            {"id": "ruby_mod:s", "file": block()},  # ships s.json beside a generated s.json/t.json
            {"id": "ruby_mod:s.json/t", "file": block()},
            {"id": "ruby_mod:m", "file": block()},  # ships its blockstate: no clash
            {"id": "ruby_mod:aa", "file": block()},  # `aa.json` is no folder of `a.json/...`
        ])
        blockstate = b'{"variants":{"":{"model":"minecraft:block/stone"}}}'
        make_jar(self.mods / "ruby_mod.jar", ruby_jar({
            "assets/ruby_mod/blockstates/p.json/c.json": blockstate,
            "assets/ruby_mod/blockstates/s.json": blockstate,
            "assets/ruby_mod/blockstates/m.json": blockstate,
        }))
        # Only the blocks the server was shown to generate no blockstate for (RB1; round-5 review
        # `rbxh`: `p` and `s` are placed and keep a blockstate).
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 8):
            report = self.bake()
        reasons = self.reasons(report)
        self.assertEqual(reasons[BAKE_MODULE.HORIZON_BLOCKSTATE_CLASH_REASON], [
            "ruby_mod:a", "ruby_mod:z", "ruby_mod:q", "ruby_mod:q.json/r"])
        # The four left out take none of the 8.
        self.assertNotIn(BAKE_MODULE.HORIZON_BLOCKS_REASON, reasons)
        self.assertEqual(self.declared(), [
            "data/ruby_mod/horizon/block/a.json/c.json", "data/ruby_mod/horizon/block/aa.json",
            "data/ruby_mod/horizon/block/m.json", "data/ruby_mod/horizon/block/p.json",
            "data/ruby_mod/horizon/block/q.json/r.json/s.json",
            "data/ruby_mod/horizon/block/s.json", "data/ruby_mod/horizon/block/s.json/t.json",
            "data/ruby_mod/horizon/block/z.json/c.json",
        ])
        with zipfile.ZipFile(self.packages / "ruby_mod.hmod") as archive:
            names = archive.namelist()
        for shipped in ("p.json/c.json", "s.json", "m.json"):
            self.assertIn(f"assets/ruby_mod/blockstates/{shipped}", names)
        # One fewer: the last block that clashes with none is left out by the count.
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 7):
            report = self.bake()
        self.assertEqual(self.reasons(report)[BAKE_MODULE.HORIZON_BLOCKS_REASON], ["ruby_mod:aa"])
        self.assertEqual(self.reasons(report)[BAKE_MODULE.HORIZON_BLOCKSTATE_CLASH_REASON], [
            "ruby_mod:a", "ruby_mod:z", "ruby_mod:q", "ruby_mod:q.json/r"])

    def test_a_block_beside_one_the_count_leaves_out_keeps_its_blockstate(self):
        # `x.json/c` does not fit the states left, so `x` gets its generated blockstate and is
        # written; with room for both, `x` is the one left out (RB1). The chain `q`: when its
        # deepest block is left out by the count, the middle one is the folder side.
        self.declare(blocks=[
            {"id": "ruby_mod:x.json/c", "file": block(state_properties=booleans(2))},
            {"id": "ruby_mod:x", "file": block()},
            {"id": "ruby_mod:q", "file": block()},
            {"id": "ruby_mod:q.json/r", "file": block()},
            {"id": "ruby_mod:q.json/r.json/s", "file": block(state_properties=booleans(2))},
        ])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 3):
            report = self.bake()
        reasons = self.reasons(report)
        self.assertEqual(reasons[BAKE_MODULE.HORIZON_STATES_REASON],
                         ["ruby_mod:x.json/c", "ruby_mod:q.json/r.json/s"])
        self.assertEqual(reasons[BAKE_MODULE.HORIZON_BLOCKSTATE_CLASH_REASON], ["ruby_mod:q"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/q.json/r.json",
                                           "data/ruby_mod/horizon/block/x.json"])
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 9):
            report = self.bake()
        self.assertEqual(self.reasons(report)[BAKE_MODULE.HORIZON_BLOCKSTATE_CLASH_REASON],
                         ["ruby_mod:x", "ruby_mod:q", "ruby_mod:q.json/r"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/q.json/r.json/s.json",
                                           "data/ruby_mod/horizon/block/x.json/c.json"])

    def test_a_block_that_ships_its_blockstate_makes_no_folder_for_rb1(self):
        # `w` = `x.json/d` ships its blockstate, which is not in the game's cache: once `x.json/c`
        # is left out by the count, `x` gets its generated blockstate and is written.
        self.declare(blocks=[
            {"id": "ruby_mod:x.json/c", "file": block(state_properties=booleans(2))},
            {"id": "ruby_mod:x", "file": block()},
            {"id": "ruby_mod:x.json/d", "file": block()},
        ])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar({
            "assets/ruby_mod/blockstates/x.json/d.json": b'{"variants":{}}'}))
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 3):
            report = self.bake()
        self.assertNotIn(BAKE_MODULE.HORIZON_BLOCKSTATE_CLASH_REASON, self.reasons(report))
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/x.json",
                                           "data/ruby_mod/horizon/block/x.json/d.json"])

    # The write-failure path, tested for what it claims (round-4 review).

    def test_a_struct_error_while_writing_leaves_no_tmp_file(self):
        self.declare_by_jar(a=([{"id": "a:b", "file": block()}], []),
                            b=([{"id": "b:b", "file": block()}], []))
        self.mod_jar("a")
        self.mod_jar("b")
        real = BAKE_MODULE.write_package
        temporary = self.packages / "a.hmod.tmp"

        def failing(path, namespace, *arguments):
            if namespace == "a":
                path.parent.mkdir(parents=True, exist_ok=True)
                temporary.write_bytes(b"partial")
                raise struct_error("'H' format requires 0 <= number <= 65535")
            return real(path, namespace, *arguments)

        with mock.patch.object(BAKE_MODULE, "write_package", failing):
            status, stderr = self.run_main()
        self.assertEqual(status, 1)
        self.assertIn("a.jar: refused: namespace a: the package could not be written: "
                      "'H' format requires 0 <= number <= 65535", stderr)
        self.assertFalse(temporary.exists())
        self.assertEqual(sorted(path.name for path in self.packages.iterdir()), ["b.hmod"])
        self.assertEqual(self.report_file()["enabled"], ["b"])

    # The CLI summary under the horizon target.

    def test_the_summary_prints_any_jar_name(self):
        name = os.fsdecode(b"bad\xff.jar")
        make_jar(self.mods / name, {"assets/ns/lang/en_us.json": b"{}"})
        self.declare(blocks=[{"id": "ns:b", "file": block()}])
        stdout = io.TextIOWrapper(io.BytesIO(), encoding="utf-8", errors="strict")
        status, _ = self.run_main(stdout)
        stdout.flush()
        self.assertEqual(status, 0)
        text = stdout.buffer.getvalue().decode("utf-8")
        self.assertIn("bad\\xff.jar: 1 package(s) [ns], 0 file(s) skipped", text)
        # Another jar with no mod id may not take that package (RO1): start again.
        (self.mods / name).unlink()
        (self.packages / "ns.hmod").unlink()
        make_jar(self.mods / "r\u00fcby.jar", {"assets/ns/lang/en_us.json": b"{}"})
        stdout = io.TextIOWrapper(io.BytesIO(), encoding="ascii", errors="strict")
        status, _ = self.run_main(stdout)
        stdout.flush()
        self.assertEqual(status, 0)
        self.assertIn("r\\xfcby.jar: 1 package(s) [ns]", stdout.buffer.getvalue().decode())


def renamed_local_header(path: Path, name: str, wrong: str) -> None:
    """Gives entry `name` of the zip at `path` a local header name `wrong` of the same length:
    Python's `zipfile` then refuses to read it, though the official game loads it (round-5 review
    `nmr7`)."""
    data = path.read_bytes()
    assert len(name) == len(wrong) and data.count(name.encode()) == 2
    path.write_bytes(data.replace(name.encode(), wrong.encode(), 1))


def game_files(game: Path) -> dict[str, bytes]:
    """Every file of the game folder but the jars, by relative path."""
    return {
        str(path.relative_to(game)): path.read_bytes()
        for path in sorted(game.rglob("*"))
        if path.is_file() and "java-mods" not in path.parts
    }


class HorizonRound6Tests(unittest.TestCase):
    """The rules of the sixth review round (spec "Round 6 (black-box)")."""

    setUp = HorizonRound4Tests.setUp
    tearDown = HorizonRound4Tests.tearDown
    declare = HorizonRound4Tests.declare
    declare_by_jar = HorizonRound4Tests.declare_by_jar
    bake = HorizonRound4Tests.bake
    report_file = HorizonRound4Tests.report_file
    declared = HorizonRound4Tests.declared
    reasons = staticmethod(HorizonRound4Tests.reasons)
    mod_jar = HorizonRound4Tests.mod_jar

    def run_main(self, *extra: str) -> tuple[int, str, str]:
        stdout, stderr = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(stdout), contextlib.redirect_stderr(stderr):
            status = BAKE_MODULE.main(["--game-dir", str(self.game), "--probe", str(self.probe),
                                       "--target", "horizon", *extra])
        return status, stdout.getvalue(), stderr.getvalue()

    def runs(self, count: int, *extra: str) -> list[tuple[tuple[int, str, str], dict]]:
        results = []
        for _ in range(count):
            results.append((self.run_main(*extra), game_files(self.game)))
        return results

    # RB4: the same inputs, the same result.

    def stage_mix(self) -> None:
        """A foreign package and three jars whose run meets the block count, RB1, a free
        namespace and items."""
        self.declare_by_jar(
            a=([{"id": "a:x", "file": block()}, {"id": "a:x.json/c", "file": block()},
                {"id": "a:y", "file": block(state_properties=booleans(2))},
                {"id": "a:z", "file": block()}],
               [{"id": "a:i", "file": {"template": "minecraft:stick"}}]),
            b=([{"id": f"b:k{index}", "file": block()} for index in range(4)], []),
            c=([{"id": "free:k", "file": block()}], []),
        )
        self.mod_jar("a", extra={"assets/a/lang/en_us.json": b'{"block.a.y":"Y"}'})
        self.mod_jar("b")
        make_jar(self.mods / "c.jar", {"assets/free/lang/en_us.json": b"{}"})

    def test_baking_twice_or_three_times_gives_the_same_bytes(self):
        # The run's own packages hold more declaration bytes than the bound for other packages:
        # a second run must not count them against it (round-5 review `lvbig`: run 2 exited 1 and
        # emptied the package).
        self.stage_mix()
        size = len(json.dumps(block()).encode())
        for extra in ((), ("--no-enable",)):
            with self.subTest(extra=extra):
                if self.packages.exists():
                    shutil_rmtree(self.packages)
                (self.game / "horizon-mods.txt").unlink(missing_ok=True)
                package_with_blocks(self.packages / "theirs.hmod", "theirs",
                                    {"t1": block(), "t2": block()})
                with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6), \
                        mock.patch.object(BAKE_MODULE, "HORIZON_USAGE_BYTES", 2 * size + 10):
                    results = self.runs(3, *extra)
                (status, stdout, stderr), files = results[0]
                self.assertEqual((status, stderr), (0, ""))
                for later in results[1:]:
                    self.assertEqual(later, results[0])
                report = json.loads(files["config/horizon_java/bake-report.json"])
                self.assertEqual(report["errors"], [])
                self.assertEqual(report["stale"], [])
                self.assertEqual(self.declared("a"), [
                    "data/a/horizon/block/x.json/c.json", "data/a/horizon/block/y.json",
                    "data/a/horizon/block/z.json", "data/a/horizon/item/i.json"])
                self.assertEqual(self.declared("b"), ["data/b/horizon/block/k0.json"])
                self.assertEqual(self.reasons(report, 1)[BAKE_MODULE.HORIZON_BLOCKS_REASON],
                                 ["b:k1", "b:k2", "b:k3"])
                self.assertEqual(self.declared("free"), [])
                mods_file = files["horizon-mods.txt"]
                self.assertEqual(mods_file, b"mod.theirs:true\n" if extra else
                                 b"mod.theirs:true\nmod.a:true\nmod.b:true\nmod.free:true\n")

    def test_a_free_namespace_another_jar_of_the_run_baked_is_decided_again(self):
        # `a.jar` declares only a left-out block in the free namespace `free`, which `q.jar` (no
        # mod id) writes. Run 2 must not take q's package from run 1 as another mod's for
        # `a.jar`: the report would name a different reason (hfuzz6 seeds 6, 14, 17).
        self.declare_by_jar(
            a=([{"id": "a:b", "file": block()},
                {"id": "free:x", "file": block(template="minecraft:furnace")}], []),
            q=([{"id": "free:y", "file": block()}], []),
        )
        self.mod_jar("a")
        make_jar(self.mods / "q.jar", {"assets/free/lang/en_us.json": b"{}"})
        results = self.runs(3)
        self.assertEqual(results[0][0][0], 0)
        self.assertEqual(results[1], results[0])
        self.assertEqual(results[2], results[0])
        report = json.loads(results[0][1]["config/horizon_java/bake-report.json"])
        self.assertEqual(self.reasons(report)[TEMPLATE], ["free:x"])
        self.assertEqual(self.declared("free"), ["data/free/horizon/block/y.json"])

    def test_a_large_own_package_is_rebaked_unchanged(self):
        # Close to the round-5 review's `lvbig` (70 MB of declarations in one package) with the
        # bound lowered instead: the bake's own package is read whatever its size (up to the
        # 512 MiB it never writes past).
        self.declare_by_jar(lv=([{"id": f"lv:b{index}", "file": block(state_properties={
            "p" * 255: {"type": "enum", "values": ["v" * 255, "w" * 255]}})}
            for index in range(20)], []))
        self.mod_jar("lv", extra={"assets/lv/lang/en_us.json": b"{}"})
        with mock.patch.object(BAKE_MODULE, "HORIZON_USAGE_BYTES", 1000):
            results = self.runs(3)
        self.assertEqual(results[0][0][0], 0)
        self.assertEqual(results[1], results[0])
        self.assertEqual(results[2], results[0])
        self.assertEqual(len(self.declared("lv")), 20)
        # Past what the bake writes at most, its own package is uncountable like any other.
        with mock.patch.object(BAKE_MODULE, "MAX_TOTAL", 1000):
            budget = BAKE_MODULE.Budget.of(BAKE_MODULE.existing_packages([self.packages]),
                                           self.game)
        self.assertEqual(list(budget.unknown.values()),
                         ["more than 0 MiB of block declarations"])
        # It is the package the run writes, so it alone does not hold back the run.
        self.assertIsNone(budget.uncounted(self.packages / "lv.hmod"))
        self.assertIsNotNone(budget.uncounted(self.packages / "other.hmod"))

    def test_an_uncountable_package_keeps_an_earlier_package_as_it_is(self):
        self.declare_by_jar(a=([{"id": "a:x", "file": block()}],
                               [{"id": "a:i", "file": {"template": "minecraft:stick"}}]),
                            n=([], [{"id": "n:i", "file": {"template": "minecraft:stick"}}]))
        self.mod_jar("a", extra={"assets/a/lang/en_us.json": b"{}"})
        self.mod_jar("n")
        self.assertEqual(self.run_main()[0], 0)
        before = (self.packages / "a.hmod").read_bytes()
        # Then a package whose block entry Python's zipfile cannot read.
        package_with_blocks(self.packages / "nmp.hmod", "nmp", {"k0": block(), "k1": block()})
        renamed_local_header(self.packages / "nmp.hmod", "data/nmp/horizon/block/k0.json",
                             "data/nmp/horizon/block/k0.jsoX")
        results = self.runs(3)
        (status, _, stderr), files = results[0]
        self.assertEqual(status, 1)
        self.assertEqual(results[1], results[0])
        self.assertEqual(results[2], results[0])
        self.assertEqual(files["horizon-mods/a.hmod"], before)
        self.assertIn("a.jar: refused: namespace a: not written: the blocks of horizon-mods/nmp.hmod "
                      "cannot be counted (nmp.hmod: the bake cannot read it (zipfile.BadZipFile: File "
                      "name in directory", stderr)
        self.assertIn("horizon-mods/a.hmod left as it is", stderr)
        self.assertIn("error: the blocks of the packages in horizon-mods/ could not be counted "
                      "(nmp.hmod: the bake cannot read it", stderr)
        self.assertIn("the namespaces with blocks were not written (a)", stderr)
        # A namespace without blocks is written; the stale line names the package left in place.
        self.assertIn("n.jar: 1 package(s) [n]", results[0][0][1])
        self.assertIn("stale package from an earlier bake of a.jar left in place", stderr)

    def test_a_replaced_uncountable_package_no_longer_holds_back_the_run(self):
        # The run's own earlier package `a.hmod` cannot be counted any more; jar `a` replaces it,
        # and jar `b` then counts every other package: `theirs` (2 blocks) after it in the folder.
        self.declare_by_jar(a=([{"id": "a:x", "file": block()}], []),
                            b=([{"id": f"b:k{index}", "file": block()} for index in range(3)], []))
        self.mod_jar("a")
        self.assertEqual(self.run_main()[0], 0)
        renamed_local_header(self.packages / "a.hmod", "data/a/horizon/block/x.json",
                             "data/a/horizon/block/x.jsoX")
        package_with_blocks(self.packages / "theirs.hmod", "theirs", {"t1": block(), "t2": block()})
        self.mod_jar("b")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 5):
            status, _, stderr = self.run_main()
        self.assertEqual((status, stderr), (0, ""))
        self.assertEqual(self.declared("a"), ["data/a/horizon/block/x.json"])
        self.assertEqual(self.declared("b"), ["data/b/horizon/block/k0.json",
                                              "data/b/horizon/block/k1.json"])

    # RB3: an entry Python cannot read makes the count unknown, never zero.

    def test_an_entry_python_cannot_read_makes_the_count_unknown(self):
        package_with_blocks(self.packages / "nmp.hmod", "nmp", {"k0": block(), "k1": block()})
        renamed_local_header(self.packages / "nmp.hmod", "data/nmp/horizon/block/k0.json",
                             "data/nmp/horizon/block/k0.jsoX")
        existing = BAKE_MODULE.existing_packages([self.packages])
        self.assertEqual(existing[0].owned, frozenset(["nmp"]))  # its manifest reads
        budget = BAKE_MODULE.Budget.of(existing, self.game)
        self.assertEqual(list(budget.unknown), [(self.packages / "nmp.hmod").resolve()])
        self.assertEqual((budget.states, budget.blocks), (0, 0))
        self.assertEqual(budget.left(self.packages / "x.hmod"), (0, 0))
        # Turned off, it is not read at all (RB2).
        (self.game / "horizon-mods.txt").write_text("mod.nmp:false\n")
        self.assertEqual(BAKE_MODULE.Budget.of(existing, self.game).unknown, {})

    def test_a_block_file_of_a_folder_package_that_is_no_regular_file(self):
        folder = self.packages / "dir_pkg"
        blocks = folder / "data" / "dir_pkg" / "horizon" / "block"
        blocks.mkdir(parents=True)
        (folder / "horizon-mod.toml").write_text(manifest_toml("dir_pkg"))
        (blocks / "a.json").write_bytes(json.dumps(block()).encode())
        (blocks / "b.json").symlink_to(blocks / "missing.json")
        with self.assertRaisesRegex(BAKE_MODULE._Uncountable, "not a regular file"):
            BAKE_MODULE._package_usage(folder, [0])
        # A pipe: never opened (opening it would wait for a writer).
        (blocks / "b.json").unlink()
        os.mkfifo(blocks / "b.json")
        done = threading.Event()

        def writer() -> None:  # lets a read that should not happen finish, with a block
            while not done.wait(0.05):
                try:
                    handle = os.open(blocks / "b.json", os.O_WRONLY | os.O_NONBLOCK)
                except OSError:
                    continue
                os.write(handle, b"{}")
                os.close(handle)
                return

        thread = threading.Thread(target=writer)
        thread.start()
        try:
            with self.assertRaisesRegex(BAKE_MODULE._Uncountable, "not a regular file"):
                BAKE_MODULE._package_usage(folder, [0])
        finally:
            done.set()
            thread.join()

    def test_a_folder_package_over_the_entry_bound_is_uncountable(self):
        folder = self.packages / "dir_pkg"
        blocks = folder / "data" / "dir_pkg" / "horizon" / "block"
        blocks.mkdir(parents=True)
        for index in range(3):
            (blocks / f"k{index}.json").write_bytes(json.dumps(block()).encode())
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", 3):
            self.assertEqual(BAKE_MODULE._package_usage(folder, [0]), (3, 3))
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", 2):
            with self.assertRaisesRegex(BAKE_MODULE._Uncountable, "more than 2 files in dir_pkg"):
                BAKE_MODULE._package_usage(folder, [0])

    def test_a_package_without_a_readable_id_is_counted(self):
        folder = self.packages / "dir_pkg"
        blocks = folder / "data" / "dir_pkg" / "horizon" / "block"
        blocks.mkdir(parents=True)
        (folder / "horizon-mod.toml").write_text("id = ")  # not TOML
        (blocks / "a.json").write_bytes(json.dumps(block()).encode())
        (self.game / "horizon-mods.txt").write_text("mod.other:true\n")
        existing = BAKE_MODULE.existing_packages([self.packages])
        self.assertIsNone(existing[0].package_id)
        budget = BAKE_MODULE.Budget.of(existing, self.game)
        self.assertEqual((budget.states, budget.blocks), (1, 1))

    def test_a_manifest_over_16_mib_is_not_read(self):
        text = manifest_toml("big") + "#" + " " * BAKE_MODULE.MAX_FILE + "\n"
        folder = self.packages / "big"
        folder.mkdir(parents=True)
        (folder / "horizon-mod.toml").write_text(text)
        with zipfile.ZipFile(self.packages / "bigz.hmod", "w", zipfile.ZIP_DEFLATED) as archive:
            archive.writestr("horizon-mod.toml", text.replace('"big"', '"bigz"'))
        self.assertEqual([package.owned for package in
                          BAKE_MODULE.existing_packages([self.packages])], [None, None])
        with mock.patch.object(BAKE_MODULE, "MAX_FILE", len(text) + 10):
            self.assertEqual([package.owned for package in
                              BAKE_MODULE.existing_packages([self.packages])],
                             [frozenset(["big"]), frozenset(["bigz"])])

    def test_the_bounds_of_the_count_and_of_zip_directories(self):
        self.assertEqual(BAKE_MODULE.HORIZON_USAGE_BYTES, 64 << 20)
        self.assertEqual(BAKE_MODULE.ZIP_MAX_ENTRIES, 131_072)
        self.assertEqual(BAKE_MODULE.ZIP_MAX_DIRECTORY, 64 << 20)
        self.assertEqual(BAKE_MODULE.STATE_STRING_MAX_BYTES, 255)
        # What the bake writes stays under the zip bounds: 65,536 files and the manifest, names
        # of at most 512 bytes (46 bytes of directory record each).
        self.assertLess(BAKE_MODULE.MAX_FILES + 1, BAKE_MODULE.ZIP_MAX_ENTRIES)
        self.assertLess((BAKE_MODULE.MAX_FILES + 1) * (46 + BAKE_MODULE.HORIZON_MAX_ENTRY_BYTES),
                        BAKE_MODULE.ZIP_MAX_DIRECTORY)

    # State property names and enum values.

    def test_state_names_and_values_over_255_bytes_are_left_out(self):
        self.declare(blocks=[
            {"id": "ruby_mod:n255", "file": block(state_properties={"n" * 255: {"type": "boolean"}})},
            {"id": "ruby_mod:n256", "file": block(state_properties={"n" * 256: {"type": "boolean"}})},
            {"id": "ruby_mod:v255", "file": block(state_properties={
                "p": {"type": "enum", "values": ["a", "v" * 255]}})},
            {"id": "ruby_mod:v256", "file": block(state_properties={
                "p": {"type": "enum", "values": ["a", "v" * 256]}})},
            {"id": "ruby_mod:n70k", "file": block(state_properties={"n" * 70000: {"type": "boolean"}})},
        ])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(self.reasons(report)[BAKE_MODULE.STATE_STRING_REASON],
                         ["ruby_mod:n256", "ruby_mod:v256", "ruby_mod:n70k"])
        self.assertEqual(self.declared(), ["data/ruby_mod/horizon/block/n255.json",
                                           "data/ruby_mod/horizon/block/v255.json"])

    # ML1: horizon-mods.txt as the game reads it.

    def test_the_lines_of_horizon_mods_txt_as_the_game_reads_them(self):
        on = [
            "mod.x:true", "mod.x:true\r", " mod.x : true ", "mod.x :true", "mod.x:\x0btrue",
            "mod.x:\x0ctrue", "mod.x:\x85true", "mod.x:\u2028true", "mod.x:\u2029true",
            "mod.x:\ttrue\t", "mod.x:\xa0true", "mod.x:\u3000true", "mod.x:\u1680true",
            "mod.x:\u2000true", "mod.x:\u202ftrue", "mod.x:\u205ftrue", "\rmod.x:true",
        ]
        off = [
            "mod.x:false", "mod.x:TRUE", "mod.x=true", "# mod.x:true", "x:true", "mod.x:1",
            "mod.x:\x1ctrue", "mod.x:\x1etrue", "mod.x:\x1ftrue", "mod.x:\u200btrue",
            "mod.x:\ufefftrue", "\ufeffmod.x:true", "mod. x:true", "mod.x:tr ue",
            "mod.x:true\rmod.y:true", "mod.x:true\x0bmod.y:true", "mod.x:true\u2028mod.y:true",
            "\x1cmod.x:true", "mod.x\x1f:true",
        ]
        for line in on:
            self.assertEqual(BAKE_MODULE._mods_file_id(line), ("x", True), repr(line))
        for line in off:
            self.assertNotEqual(BAKE_MODULE._mods_file_id(line), ("x", True), repr(line))
        self.assertEqual(BAKE_MODULE._mods_file_lines("a\r\nb\rc\x0bd\x85e\u2028f\x1cg\nh"),
                         ["a\r", "b\rc\x0bd\x85e\u2028f\x1cg", "h"])
        mods_file = self.game / "horizon-mods.txt"
        for text, expected in (
            ("mod.vt:\x0btrue\n", True),  # the round-5 review's `vtn7`: counted
            ("mod.vt:true\rmod.big:true\n", False),
            ("mod.big:true\u2028mod.vt:true\n", False),
            ("\ufeffmod.vt:true\n", False),
            ("mod.vt:true", True),  # no newline at the end
        ):
            mods_file.write_bytes(text.encode())
            self.assertEqual(BAKE_MODULE._possibly_on(self.game, "vt"), expected, repr(text))
            self.assertEqual(BAKE_MODULE._turned_on(self.game, "vt"), expected, repr(text))

    def test_enabling_never_splits_or_turns_off_another_mods_line(self):
        mods_file = self.game / "horizon-mods.txt"
        original = ("mod.vt:\x0btrue\r\nmod.w:\u2028true\nmod.ruby_mod:false\r\n# note \x85 x\n"
                    "mod.other:true\rmod.z:true\nlast").encode()
        mods_file.write_bytes(original)
        self.assertEqual(BAKE_MODULE.enable(self.game, ["ruby_mod", "new"], BAKE_MODULE.HORIZON),
                         ["ruby_mod", "new"])
        self.assertEqual(mods_file.read_bytes(), (
            "mod.vt:\x0btrue\r\nmod.w:\u2028true\nmod.ruby_mod:true\r\n# note \x85 x\n"
            "mod.other:true\rmod.z:true\nlast\nmod.new:true\n").encode())
        # Again: nothing changes, not even the file's time.
        stamp = mods_file.stat().st_mtime_ns
        BAKE_MODULE.enable(self.game, ["ruby_mod", "new", "vt"], BAKE_MODULE.HORIZON)
        self.assertEqual(mods_file.stat().st_mtime_ns, stamp)
        self.assertTrue(BAKE_MODULE._possibly_on(self.game, "vt"))
        # An id whose only line is not one the game reads as its own gets a line of its own.
        mods_file.write_bytes("\ufeffmod.a:true\nmod. b:true\n".encode())
        BAKE_MODULE.enable(self.game, ["a", "b"], BAKE_MODULE.HORIZON)
        self.assertEqual(mods_file.read_bytes(),
                         "\ufeffmod.a:true\nmod. b:true\nmod.a:true\nmod.b:true\n".encode())
        mods_file.unlink()
        BAKE_MODULE.enable(self.game, ["a"], BAKE_MODULE.HORIZON)
        self.assertEqual(mods_file.read_bytes(), b"mod.a:true\n")
        # Sunrise's file is written as it always was.
        (self.game / "sunrise-mods.txt").write_bytes(b"mod.x:true\rmod.y:false\r\n")
        BAKE_MODULE.enable(self.game, ["y"])
        self.assertEqual((self.game / "sunrise-mods.txt").read_bytes(), b"mod.x:true\nmod.y:true\n")

    # The horizon target bounds a jar's central directory too.

    def test_a_jar_over_the_directory_bounds_is_refused_under_the_horizon_target(self):
        self.declare(blocks=[{"id": "ruby_mod:a", "file": block()}])
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        entries = len(ruby_jar())
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", entries - 1):
            self.assertEqual(BAKE_MODULE._jar_metadata_ids(self.mods / "ruby_mod.jar"),
                             frozenset())
            report = self.bake()
            self.assertEqual(report["jars"][0]["error"], (
                f"over {entries - 1} entries or 64 MiB of central directory (Python's zipfile "
                "reads it whole; not opened)"))
            # Sunrise is unchanged: the same jar is baked.
            sunrise = BAKE_MODULE.bake_directory(self.mods, self.game, probe=self.probe).as_json()
            self.assertIsNone(sunrise["jars"][0]["error"])
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_DIRECTORY", 100):
            self.assertEqual(self.bake()["jars"][0]["error"], (
                "over 131072 entries or 0 MiB of central directory (Python's zipfile reads it "
                "whole; not opened)"))
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", entries):
            self.assertEqual(BAKE_MODULE._jar_metadata_ids(self.mods / "ruby_mod.jar"),
                             frozenset(["ruby_mod"]))
            self.assertIsNone(self.bake()["jars"][0]["error"])

    # The summary when the stream's encoding has no codec.

    def test_the_summary_survives_a_stream_encoding_without_a_codec(self):
        class Stream(io.StringIO):
            encoding = "no-such-codec"

        stream = Stream()
        BAKE_MODULE._say("r\u00fcby.jar: 1 package(s)", stream)
        self.assertEqual(stream.getvalue(), "r\\xfcby.jar: 1 package(s)\n")


def fluid_package(path: Path, package_id: str, fluids: dict[str, bytes],
                  blocks: dict[str, object] | None = None) -> None:
    """A package someone else wrote with fluid files `{path: bytes}` (and block files), turned
    on."""
    package_with_blocks(path, package_id, blocks or {})
    with zipfile.ZipFile(path, "a") as archive:
        for name, body in fluids.items():
            archive.writestr(f"data/{package_id}/horizon/fluid/{name}.json", body)


def understated_zip(path: Path, entries: int, stated: int) -> None:
    """A zip of `entries` stored entries whose end record states `stated` entries (the round-6
    review's lying jar, without zip64)."""
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("fabric.mod.json", b'{"id":"lie"}')
        for index in range(entries - 1):
            archive.writestr(f"a/{index:06d}", b"")
    data = bytearray(path.read_bytes())
    end = data.rfind(b"PK\x05\x06")
    data[end + 8:end + 12] = struct_pack("<HH", stated, stated)
    path.write_bytes(bytes(data))


def probe_by_kind(root: Path, outputs: dict[str, tuple[str, int]]) -> Path:
    """A stand-in probe that prints `outputs[kind]` and exits with its status, whatever the jar."""
    data = root / "probe-kinds.json"
    data.write_text(json.dumps(outputs), encoding="utf-8")
    path = root / "horizon-native-java-by-kind"
    path.write_text(
        "#!/usr/bin/env python3\n"
        "import json, sys\n"
        f"text, status = json.load(open({str(data)!r}))[sys.argv[3]]\n"
        "sys.stdout.write(text + '\\n')\n"
        "sys.exit(status)\n",
        encoding="utf-8",
    )
    path.chmod(0o755)
    return path


class HorizonRound7Tests(unittest.TestCase):
    """The rules of the seventh review round (spec "Round 7 (black-box)")."""

    setUp = HorizonRound4Tests.setUp
    tearDown = HorizonRound4Tests.tearDown
    declare = HorizonRound4Tests.declare
    declare_by_jar = HorizonRound4Tests.declare_by_jar
    bake = HorizonRound4Tests.bake
    report_file = HorizonRound4Tests.report_file
    declared = HorizonRound4Tests.declared
    reasons = staticmethod(HorizonRound4Tests.reasons)
    mod_jar = HorizonRound4Tests.mod_jar
    run_main = HorizonRound6Tests.run_main
    runs = HorizonRound6Tests.runs

    def blocks_of(self, package: str) -> int:
        return sum(1 for name in self.declared(package) if "/horizon/block/" in name)

    # FL1: fluid declarations add blocks.

    def test_which_fluid_files_add_a_block(self):
        adds = BAKE_MODULE._fluid_blocks
        for body, blocks in [
            (b"{}", 0), (b'{"world":null}', 0), (b'{"sprite":"m:block/oil","tint":1}', 0),
            (b'{"world":{}}', 1), (b'{"world":{"bucket":false}}', 1), (b'{"world":[]}', 1),
            (b'{"w\\u006frld":{}}', 1), (b"[]", 1), (b"[null,null,{}]", 1), (b"not json", 1),
            (b'{"world":false}', 1), (b'{"world":{},"world":null}', 1),
            (b'{"world":null,"world":null}', 1), (b"\xef\xbb\xbf{}", 1), (b"\xff{}", 1),
            (b"[" * 100_000, 1), (None, 1),
        ]:
            with self.subTest(body=body if body is None else body[:30]):
                self.assertEqual(adds(body), blocks)
        self.assertEqual(BAKE_MODULE.FLUID_STATES, 16)

    def test_a_turned_on_package_counts_its_world_fluids(self):
        fluid_package(self.packages / "fl.hmod", "fl", {
            "oil": b'{"world":{}}', "dir/tar": b'{"world":{"bucket":false}}', "tank": b"{}",
            "none": b'{"world":null}', "bad": b"[", "big": b'{"world":null}' + b" " * 200,
        }, {"b0": block()})
        with zipfile.ZipFile(self.packages / "fl.hmod", "a") as archive:
            # Neither a fluid file (no `.json`) nor one in `horizon/fluid/`.
            archive.writestr("data/fl/horizon/fluid/oil.txt", b'{"world":{}}')
            archive.writestr("data/fl/horizon/fluids/oil.json", b'{"world":{}}')
            archive.writestr("data/fl/horizon/fluid/", b"")
        existing = BAKE_MODULE.existing_packages([self.packages])
        with mock.patch.object(BAKE_MODULE, "MAX_FILE", 150):
            usage = BAKE_MODULE._package_usage(self.packages / "fl.hmod", [0])
            budget = BAKE_MODULE.Budget.of(existing, self.game)
        # oil, dir/tar, bad and big (over the file cap, not read) add 16 states each; b0 adds 1.
        self.assertEqual(usage, (4 * 16 + 1, 5))
        self.assertEqual((budget.states, budget.blocks), (4 * 16 + 1, 5))
        # Read, big adds none.
        self.assertEqual(BAKE_MODULE._package_usage(self.packages / "fl.hmod", [0]), (49, 4))
        # The same in a folder package.
        folder = self.root / "folder" / "horizon-mods" / "fd"
        (folder / "data" / "fd" / "horizon" / "fluid" / "dir").mkdir(parents=True)
        (folder / "horizon-mod.toml").write_text(manifest_toml("fd"))
        for name, body in {"oil": b'{"world":{}}', "dir/tar": b"[]", "tank": b"{}"}.items():
            (folder / "data" / "fd" / "horizon" / "fluid" / f"{name}.json").write_bytes(body)
        (folder / "data" / "fd" / "horizon" / "fluid" / "x.txt").write_bytes(b'{"world":{}}')
        self.assertEqual(BAKE_MODULE._package_usage(folder, [0]), (32, 2))
        (folder / "data" / "fd" / "horizon" / "fluid" / "gone.json").symlink_to("missing")
        with self.assertRaisesRegex(BAKE_MODULE._Uncountable, "not a regular file"):
            BAKE_MODULE._package_usage(folder, [0])
        # Fluid bytes count against the read bound as block bytes do (12 + 2 + 2 bytes here).
        (folder / "data" / "fd" / "horizon" / "fluid" / "gone.json").unlink()
        self.assertEqual(BAKE_MODULE._package_usage(folder, [0], 16), (32, 2))
        with self.assertRaisesRegex(BAKE_MODULE._Uncountable, "more than 0 MiB"):
            BAKE_MODULE._package_usage(folder, [0], 15)

    def test_world_fluids_leave_room_for_fewer_blocks(self):
        # The round-6 review's fl2, scaled down: a turned-on package of world fluids and a jar
        # that would fill the block count without them.
        fluid_package(self.packages / "fl.hmod", "fl",
                      {f"oil{index}": b'{"world":{}}' for index in range(2)})
        self.declare_by_jar(mb=([{"id": f"mb:k{index}", "file": block()} for index in range(6)],
                                []))
        self.mod_jar("mb")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6):
            report = self.bake()
        self.assertEqual(self.blocks_of("mb"), 4)
        self.assertEqual(self.reasons(report)[BAKE_MODULE.HORIZON_BLOCKS_REASON],
                         ["mb:k4", "mb:k5"])
        # The states too: 2 x 16 of 40 leave 8 one-state blocks.
        (self.packages / "mb.hmod").unlink()
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 40):
            self.bake()
        self.assertEqual(self.blocks_of("mb"), 6)
        (self.packages / "mb.hmod").unlink()
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 36):
            self.bake()
        self.assertEqual(self.blocks_of("mb"), 4)
        # Turned off, the fluids do not count.
        (self.game / "horizon-mods.txt").write_text("mod.fl:false\n")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6):
            self.bake()
        self.assertEqual(self.blocks_of("mb"), 6)

    # ZB1: the central directory's records, counted.

    def test_a_zip_that_understates_its_entries_is_refused(self):
        jar = self.mods / "lie.jar"
        understated_zip(jar, 9, 1)
        with zipfile.ZipFile(jar) as archive:
            self.assertEqual(len(archive.infolist()), 9)  # zipfile reads every record
        with jar.open("rb") as stream:
            end = zipfile._EndRecData(stream)
            self.assertEqual(end[zipfile._ECD_ENTRIES_TOTAL], 1)
            self.assertEqual(BAKE_MODULE._central_records(stream, end), 9)
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", 8):
            with self.assertRaises(BAKE_MODULE.ZipTooLarge):
                BAKE_MODULE._open_zip(jar)
            self.assertEqual(BAKE_MODULE._jar_metadata_ids(jar), frozenset())
            report = self.bake()
            self.assertEqual(report["jars"][0]["error"], (
                "over 8 entries or 64 MiB of central directory (Python's zipfile reads it "
                "whole; not opened)"))
            # A package in horizon-mods/ that understates its entries: its count is unknown.
            self.packages.mkdir(parents=True, exist_ok=True)
            understated_zip(self.packages / "theirs.hmod", 9, 1)
            with self.assertRaisesRegex(BAKE_MODULE._Uncountable, "over 8 entries"):
                BAKE_MODULE._package_usage(self.packages / "theirs.hmod", [0])
            # Sunrise is unchanged: it opens the jar with zipfile, whatever its count.
            sunrise = BAKE_MODULE.bake_directory(self.mods, self.game, probe=self.probe)
            self.assertIsNone(sunrise.as_json()["jars"][0]["error"])
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", 9):
            self.assertEqual(BAKE_MODULE._jar_metadata_ids(jar), frozenset(["lie"]))
        # Records with extra fields and comments are walked by all three lengths.
        noted = self.root / "noted.zip"
        with zipfile.ZipFile(noted, "w") as archive:
            for index in range(6):
                info = zipfile.ZipInfo(f"n/{index}")
                info.comment = b"c" * (7 * index + 1)
                info.extra = struct_pack("<HH", 0xCAFE, 3) + b"xyz"
                archive.writestr(info, b"")
        with noted.open("rb") as stream:
            self.assertEqual(BAKE_MODULE._central_records(stream, zipfile._EndRecData(stream)), 6)

    def test_the_record_walk_stops_where_zipfile_does(self):
        jar = self.root / "x.zip"
        understated_zip(jar, 5, 5)
        data = bytearray(jar.read_bytes())
        records = [index for index in range(len(data)) if data[index:index + 4] == b"PK\x01\x02"]
        self.assertEqual(len(records), 5)
        data[records[3]:records[3] + 4] = b"PK\x09\x09"  # a bad signature: zipfile stops there
        jar.write_bytes(bytes(data))
        with jar.open("rb") as stream:
            end = zipfile._EndRecData(stream)
            self.assertEqual(BAKE_MODULE._central_records(stream, end), 3)
        with self.assertRaises(zipfile.BadZipFile):
            zipfile.ZipFile(jar)
        # A prepended stub (an executable jar): zipfile finds the directory after it, and so
        # does the walk.
        stub = self.root / "stub.zip"
        understated_zip(self.root / "plain.zip", 4, 1)
        stub.write_bytes(b"#!/bin/sh\nexit 0\n" + (self.root / "plain.zip").read_bytes())
        with stub.open("rb") as stream:
            end = zipfile._EndRecData(stream)
            self.assertEqual(BAKE_MODULE._central_records(stream, end), 4)
        self.assertEqual(len(zipfile.ZipFile(stub).infolist()), 4)
        with mock.patch.object(BAKE_MODULE, "ZIP_MAX_ENTRIES", 3), \
                self.assertRaises(BAKE_MODULE.ZipTooLarge):
            BAKE_MODULE._open_zip(stub)
        # A record cut short at the directory's end: zipfile refuses it, and the walk stops.
        plain = (self.root / "plain.zip").read_bytes()
        end = plain.rfind(b"PK\x05\x06")
        record = struct_pack("<I", int.from_bytes(plain[end + 12:end + 16], "little") + 4)
        cut = self.root / "cut.zip"
        cut.write_bytes(plain[:end] + b"PK\x01\x02" + plain[end:end + 12] + record
                        + plain[end + 16:])
        with cut.open("rb") as stream:
            end_record = zipfile._EndRecData(stream)
            self.assertEqual(BAKE_MODULE._central_records(stream, end_record), 4)
        bake = BAKE_MODULE.JarBake(jar="cut.jar")
        with self.assertRaisesRegex(BAKE_MODULE.UnsafeJar,
                                    "^not a readable jar: Truncated central directory$"):
            BAKE_MODULE._zip_entries(cut, bake, bounded=True)
        # A directory longer than what comes before the end record: zipfile refuses it.
        long = self.root / "long.zip"
        long.write_bytes(plain[:end + 12] + struct_pack("<I", end + 1) + plain[end + 16:])
        with self.assertRaisesRegex(BAKE_MODULE.UnsafeJar,
                                    "^not a readable jar: Bad offset for central directory$"):
            BAKE_MODULE._zip_entries(long, bake, bounded=True)

    # RB5: the packages a run replaces, decided before it.

    def stage_update(self) -> None:
        """The round-6 review's `conv`, scaled down (6 blocks instead of 64,250): run 0 bakes
        j-1.0 (mod id jmod, 5 blocks); then j-1.1 has 1 block and a.jar (amod, 4 blocks) is
        added. `a.jar` sorts first."""
        make_jar(self.mods / "j-1.0.jar", {"fabric.mod.json": b'{"id":"jmod"}'})
        self.declare_by_jar(**{"j-1.0": ([{"id": f"jmod:k{i}", "file": block(item=False)}
                                          for i in range(5)], [])})
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6):
            self.assertEqual(self.run_main()[0], 0)
        self.assertEqual(self.blocks_of("jmod"), 5)
        (self.mods / "j-1.0.jar").unlink()
        make_jar(self.mods / "j-1.1.jar", {"fabric.mod.json": b'{"id":"jmod"}'})
        self.mod_jar("a", "amod")
        self.declare_by_jar(**{
            "j-1.1": ([{"id": "jmod:k0", "file": block(item=False)}], []),
            "a": ([{"id": f"amod:k{i}", "file": block(item=False)} for i in range(4)], []),
        })

    def test_an_update_that_shrinks_a_package_gives_the_next_runs_result_at_once(self):
        self.stage_update()
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6):
            results = self.runs(3)
        (status, stdout, stderr), _ = results[0]
        self.assertEqual((status, stderr), (0, ""))
        self.assertEqual(results[1], results[0])
        self.assertEqual(results[2], results[0])
        self.assertEqual((self.blocks_of("amod"), self.blocks_of("jmod")), (4, 1))
        self.assertEqual(self.report_file()["errors"], [])

    def test_a_package_its_jar_does_not_replace_counts_from_the_start(self):
        # j-1.1 is now refused (its probe output is no report), so jmod.hmod stays with its 5
        # blocks: a.jar, which sorts first, has room for 1 block only, and the game's count is
        # never passed.
        self.stage_update()
        self.declare_by_jar(**{
            "a": ([{"id": f"amod:k{i}", "file": block(item=False)} for i in range(4)], []),
        })
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6):
            results = self.runs(2)
        self.assertEqual(results[1], results[0])
        self.assertEqual((self.blocks_of("amod"), self.blocks_of("jmod")), (1, 5))
        report = self.report_file()
        self.assertIn("no server entrypoint ran", report["jars"][1]["error"])
        self.assertEqual([stale["id"] for stale in report["stale"]], ["jmod"])
        # Two trials ran (the second without jmod pending), and each jar was probed once.
        calls = []
        real = BAKE_MODULE.probe_declarations

        def counting(probe, jar, malformed=None):
            calls.append(jar.name)
            return real(probe, jar, malformed)

        trials = []
        real_bake = BAKE_MODULE._bake_jar_horizon

        def counting_bake(jar, out, claimed, probe, run):
            if run.dry:
                trials.append(jar.name)
            return real_bake(jar, out, claimed, probe, run)

        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6), \
                mock.patch.object(BAKE_MODULE, "probe_declarations", counting), \
                mock.patch.object(BAKE_MODULE, "_bake_jar_horizon", counting_bake):
            self.assertEqual(self.run_main()[0], 1)
        self.assertEqual(trials, ["a.jar", "j-1.1.jar"] * 2)
        self.assertEqual(calls, ["a.jar", "j-1.1.jar"])

    def test_one_jar_baked_alone_settles_its_packages_too(self):
        # `bake_jar` without a run: j-1.0's package jmod is not replaced by j-1.1, which now
        # writes only `other`: jmod's 5 blocks count against it.
        self.stage_update()
        (self.mods / "a.jar").unlink()
        self.declare_by_jar(**{"j-1.1": ([{"id": f"other:k{i}", "file": block(item=False)}
                                          for i in range(3)], [])})
        existing = BAKE_MODULE.existing_packages([self.packages])
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6):
            bake = BAKE_MODULE.bake_jar(self.mods / "j-1.1.jar", self.packages, {}, self.probe,
                                        BAKE_MODULE.HORIZON, existing, self.game)
        self.assertEqual([(package.id, package.blocks) for package in bake.packages],
                         [("other", 1)])

    def test_a_package_that_fails_to_be_written_is_an_error(self):
        self.stage_update()
        real = BAKE_MODULE.write_package

        def failing(path, *rest, **named):
            if path.name == "jmod.hmod":
                raise OSError("disk full")
            return real(path, *rest, **named)

        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6), \
                mock.patch.object(BAKE_MODULE, "write_package", failing):
            report = self.bake()
        self.assertEqual(report["errors"], [
            "the packages horizon-mods/jmod.hmod were to be replaced and were not: with them "
            "turned on, the blocks written in this run can pass the game's limits (RS1, RS2, "
            "RB5); turn these packages off or remove them, then bake again"])
        self.assertEqual(self.blocks_of("jmod"), 5)
        # Turned off, the package counts nowhere: its write failing is only the jar's error.
        (self.game / "horizon-mods.txt").write_text("mod.jmod:false\n")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6), \
                mock.patch.object(BAKE_MODULE, "write_package", failing):
            report = self.bake()
        self.assertEqual(report["errors"], [])
        self.assertIn("could not be written: disk full", report["jars"][1]["error"])

    def test_an_uncountable_package_the_run_replaces_holds_no_jar_back(self):
        # jmod.hmod (this bake's, its owner j-1.1 in the run) has an entry Python cannot read:
        # its count is unknown, but j-1.1 replaces it, so a.jar, which sorts first, is baked.
        self.stage_update()
        renamed_local_header(self.packages / "jmod.hmod", "data/jmod/horizon/block/k0.json",
                             "data/jmod/horizon/block/k0.jsoX")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6):
            results = self.runs(2)
        self.assertEqual(results[0][0][0], 0)
        self.assertEqual(results[1], results[0])
        self.assertEqual((self.blocks_of("amod"), self.blocks_of("jmod")), (4, 1))

    def test_the_trial_writes_nothing_and_probes_each_jar_once(self):
        self.stage_update()
        calls = []
        real = BAKE_MODULE.probe_declarations

        def counting(probe, jar, malformed=None):
            calls.append(jar.name)
            return real(probe, jar, malformed)

        before = game_files(self.game)
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_BLOCKS", 6), \
                mock.patch.object(BAKE_MODULE, "probe_declarations", counting):
            run = BAKE_MODULE.HorizonRun.start(sorted(self.mods.glob("*.jar")),
                                               BAKE_MODULE.existing_packages([self.packages]),
                                               self.game)
            self.assertEqual(run.budget.pending, {(self.packages / "jmod.hmod").resolve()})
            # jmod's 5 old blocks count against no jar.
            self.assertEqual(run.budget.left(self.packages / "amod.hmod"),
                             (BAKE_MODULE.HORIZON_MOD_STATES, 6))
            run.settle(sorted(self.mods.glob("*.jar")), self.packages, self.probe)
            self.assertEqual(game_files(self.game), before)
            self.assertEqual(run.budget.pending, {(self.packages / "jmod.hmod").resolve()})
            claimed: dict[str, str] = {}
            for jar in sorted(self.mods.glob("*.jar")):
                BAKE_MODULE.bake_jar(jar, self.packages, claimed, self.probe,
                                     BAKE_MODULE.HORIZON, run.existing, self.game, run)
            self.assertEqual(run.budget.pending, set())
            self.assertEqual(run.probed, {})  # taken out as the run bakes each jar
        self.assertEqual(calls, ["a.jar", "j-1.1.jar"])
        self.assertEqual((self.blocks_of("amod"), self.blocks_of("jmod")), (4, 1))

    def test_commit_takes_a_replaced_packages_old_states_off(self):
        # The round-6 review's `atl/c.py`: with 10 states, a (one 4-state block) and b (two) give
        # a 1 block and b 1 block, on every run.
        four = block(item=False, state_properties=booleans(2))
        self.declare_by_jar(a=([{"id": "a:k0", "file": four}], []),
                            b=([{"id": f"b:k{i}", "file": four} for i in range(2)], []))
        self.mod_jar("a")
        self.mod_jar("b")
        with mock.patch.object(BAKE_MODULE, "HORIZON_MOD_STATES", 10):
            for _ in range(3):
                report = self.bake()
                self.assertEqual([(package["id"], package["declarations"]["blocks"])
                                  for jar in report["jars"]
                                  for package in jar["packages"]], [("a", 1), ("b", 1)])
        budget = BAKE_MODULE.Budget(states=8, blocks=2,
                                    existing={Path("/p").resolve(): (4, 1)})
        budget.commit(Path("/p"), 3, 1)
        self.assertEqual((budget.states, budget.blocks), (7, 2))

    def test_a_copy_has_its_own_unknown_entries(self):
        path = Path("/p").resolve()
        budget = BAKE_MODULE.Budget(unknown={path: "reason"})
        copied = budget.copy()
        del copied.unknown[path]
        self.assertEqual(budget.unknown, {path: "reason"})

    # The rest of the round-6 review's surviving mutants.

    def test_vanilla_atlases_go_only_into_the_first_package_written(self):
        self.declare_by_jar(x=([{"id": "xa:a", "file": block()},
                                {"id": "xb:b", "file": block()}], []))
        make_jar(self.mods / "x.jar", {
            "assets/xa/lang/en_us.json": b"{}", "assets/xb/lang/en_us.json": b"{}",
            "assets/minecraft/atlases/blocks.json": b'{"sources":[]}',
        })
        self.bake()
        atlas = "assets/minecraft/atlases/blocks.json"
        with zipfile.ZipFile(self.packages / "xa.hmod") as archive:
            self.assertIn(atlas, archive.namelist())
        with zipfile.ZipFile(self.packages / "xb.hmod") as archive:
            self.assertNotIn(atlas, archive.namelist())

    def test_enable_rewrites_the_first_line_that_turns_a_package_off(self):
        mods_file = self.game / "horizon-mods.txt"
        mods_file.write_bytes(b"mod.a:false\nmod.b:true\nmod.a:false\r\n")
        BAKE_MODULE.enable(self.game, ["a"], BAKE_MODULE.HORIZON)
        self.assertEqual(mods_file.read_bytes(), b"mod.a:true\nmod.b:true\nmod.a:false\r\n")

    # PE1: the runtime's JSON error report.

    def test_the_probes_json_error_names_the_entrypoint_that_failed(self):
        failure = ("main net.example.Mod: uncaught exception java/lang/ExceptionInInitializerError"
                   " (at x#<init> +10;\ncaused by \x1b[1mNPE)")
        self.probe = probe_by_kind(self.root, {
            "main": (json.dumps({"status": "error", "stage": "execute", "error": failure}), 1),
            "server": (json.dumps({"status": "error", "stage": "discover", "kind": "server",
                                   "error": "no entrypoint"}), 1),
            "neoforge": (json.dumps({"status": "error", "stage": "discover",
                                     "kind": "neoforge", "error": "no entrypoint"}), 1),
        })
        make_jar(self.mods / "ruby_mod.jar", ruby_jar())
        report = self.bake()
        self.assertEqual(report["jars"][0]["error"], (
            "the runtime could not run the jar's entrypoints: no server entrypoint ran (main: exit "
            "status 1: execute: main net.example.Mod: uncaught exception "
            "java/lang/ExceptionInInitializerError (at x#<init> +10; caused by [1mNPE))"))
        # Sunrise's reason is as it was: the exit status (its stderr is empty).
        sunrise = BAKE_MODULE.bake_directory(self.mods, self.game, probe=self.probe).as_json()
        self.assertEqual(sunrise["jars"][0]["error"],
                         "the runtime could not run the jar's entrypoints: no server entrypoint "
                         "ran (exit status 1)")
        # Two entrypoint kinds that ran and failed: the first.
        self.probe = probe_by_kind(self.root, {
            "main": (json.dumps({"status": "error", "stage": "execute", "error": "one"}), 1),
            "server": (json.dumps({"status": "error", "stage": "execute", "error": "two"}), 3),
            "neoforge": ("", 1)})
        self.assertEqual(self.bake()["jars"][0]["error"], (
            "the runtime could not run the jar's entrypoints: no server entrypoint ran (main: exit "
            "status 1: execute: one)"))
        # Only discover errors: the last kind's.
        self.probe = probe_by_kind(self.root, {
            kind: (json.dumps({"status": "error", "stage": "discover", "error": "no entrypoint"}),
                   2) for kind in ("main", "server", "neoforge")})
        self.assertEqual(self.bake()["jars"][0]["error"], (
            "the runtime could not run the jar's entrypoints: no server entrypoint ran (neoforge: "
            "exit status 2: discover: no entrypoint)"))
        # A long error is cut; output that is no such report keeps the old reason.
        self.probe = probe_by_kind(self.root, {
            "main": (json.dumps({"status": "error", "stage": "execute", "error": "e" * 900}), 1),
            "server": ("not json", 1), "neoforge": (json.dumps({"status": "ok"}), 1)})
        error = self.bake()["jars"][0]["error"]
        self.assertIn("(main: exit status 1: execute: " + "e" * 500 + "...)", error)
        self.assertEqual(BAKE_MODULE._probe_failure(json.dumps({"status": "ok"})), None)
        self.assertEqual(BAKE_MODULE._probe_failure(json.dumps({"status": "ok", "error": "x"})),
                         None)
        self.assertEqual(BAKE_MODULE._probe_failure(json.dumps({"status": "error"})), None)
        self.assertEqual(BAKE_MODULE._probe_failure("[" * 100_000), None)
        self.assertEqual(BAKE_MODULE._probe_failure("1"), None)
        self.assertEqual(BAKE_MODULE._probe_failure(json.dumps(
            {"status": "error", "stage": "s" * 99, "error": 5})), ("s" * 40 + "...",
                                                                  "s" * 40 + "...: 5"))

    def test_a_deeply_nested_probe_error_does_not_raise(self):
        result = BAKE_MODULE._probe_failure(
            '{"status":"error","stage":"main","error":'
            + '{"a":' * 50000 + '1' + '}' * 50000 + '}')
        self.assertIsNone(result)


class CentralDirectoryOffsetTests(unittest.TestCase):
    def assert_points_at_the_central_directory(self, path: Path) -> None:
        with path.open("rb") as stream:
            end = zipfile._EndRecData(stream)
            offset, concat = BAKE_MODULE._directory_offset(end, stream)
            if hasattr(zipfile, "_handle_prepended_data"):
                self.assertEqual((offset, concat), zipfile._handle_prepended_data(end))
            stream.seek(offset + concat)
            self.assertEqual(stream.read(4), b"PK\x01\x02")

    def test_a_normal_zip(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "normal.zip"
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr("a.txt", b"a")
            self.assert_points_at_the_central_directory(path)

    def test_a_zip_with_prepended_data(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "prepended.zip"
            path.write_bytes(b"\0" * 1000)
            with path.open("ab") as stream, zipfile.ZipFile(stream, "w") as archive:
                archive.writestr("a.txt", b"a")
            self.assert_points_at_the_central_directory(path)

    def test_a_zip64_zip(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "zip64.zip"
            with mock.patch.object(zipfile, "ZIP_FILECOUNT_LIMIT", 1), \
                    zipfile.ZipFile(path, "w", allowZip64=True) as archive:
                for index in range(3):
                    archive.writestr(f"a{index}.txt", b"a")
            with path.open("rb") as stream:
                self.assertEqual(zipfile._EndRecData(stream)[zipfile._ECD_SIGNATURE],
                                 zipfile.stringEndArchive64)
            self.assert_points_at_the_central_directory(path)

    def test_a_zip64_zip_with_prepended_data(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "prepended-zip64.zip"
            path.write_bytes(b"\0" * 1000)
            with mock.patch.object(zipfile, "ZIP_FILECOUNT_LIMIT", 1), \
                    path.open("ab") as stream, \
                    zipfile.ZipFile(stream, "w", allowZip64=True) as archive:
                for index in range(3):
                    archive.writestr(f"a{index}.txt", b"a")
            with path.open("rb") as stream:
                self.assertEqual(zipfile._EndRecData(stream)[zipfile._ECD_SIGNATURE],
                                 zipfile.stringEndArchive64)
            self.assert_points_at_the_central_directory(path)


def struct_error(message: str) -> Exception:
    import struct

    return struct.error(message)


def struct_pack(layout: str, *values: int) -> bytes:
    import struct

    return struct.pack(layout, *values)


def shutil_rmtree(path: Path) -> None:
    import shutil

    shutil.rmtree(path)

if __name__ == "__main__":
    unittest.main()
