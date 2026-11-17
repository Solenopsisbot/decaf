import importlib.util
import json
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
TOOL = ROOT / "tools" / "java_hook_generator.py"
spec = importlib.util.spec_from_file_location("java_hook_generator", TOOL)
generator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(generator)


class JavaHookGeneratorTests(unittest.TestCase):
    def setUp(self):
        self.mapping = {
            "rows": [
                {"vanilla": "net.minecraft.world.entity.player.Player#attack", "descriptor": "(LEntity;)V", "sunrise": "sunrise_client::game::melee::Game::attack", "anchor": "HEAD; cancellable result", "fit": "EXACT FIT", "evidence": "point"},
                {"vanilla": "Minecraft#continueAttack", "descriptor": "(Z)V", "sunrise": "attack_entity", "anchor": "HOOK", "fit": "FITS WITH A STATED DIFFERENCE", "evidence": "server dispatch difference"},
                {"vanilla": "Minecraft#startAttack", "descriptor": "()Z", "sunrise": "none", "anchor": "HOOK", "fit": "NO FIT", "evidence": "none"},
            ]
        }
        target = "sunrise_client::game::melee::Game::attack"
        self.functions = {
            "schema": 1,
            "build": {"commit": "fixture"},
            "crates": {},
            "functions": {target: {"module": "game", "file": "game.rs", "span": [0, 1], "signature_hash": "abc123", "sha256": "deadbeef"}},
            "refused": {},
        }
        self.points = {
            "schema": 1,
            "build": {"commit": "fixture"},
            "points": [{"api_refusal": None, "args": [], "at_return": False, "host": "none", "listener": "listener", "name": "attack", "raw": True, "raw_shape": {"args": [], "outcome": "cancel", "outcome_record_mutable": True, "receiver": {"kind": "none", "ty": None, "write_back": False}, "return_class": "plain", "return_record_mutable": False, "return_ty": "()"}, "record": "Args", "return_listener": "return_listener", "return_ty": "()", "returns": "cancel", "side": "none", "target": target}],
        }

    def test_exact_point_and_stated_difference_hook_emit(self):
        plan = generator.generate(self.mapping, self.functions, self.points)
        self.assertEqual([item["kind"] for item in plan["inject"]], ["head"])
        self.assertEqual(plan["inject"][0]["hash"], "abc123")
        self.assertEqual(plan["hooks"][0]["target"], "attack_entity")
        text = generator.render(plan, "lane1.json")
        self.assertIn("[[inject]]", text)
        self.assertIn("[[hook]]", text)
        self.assertNotIn("start_attack", text)

    def test_refused_or_stale_points_fail_closed(self):
        target = self.mapping["rows"][0]["sunrise"]
        refused = dict(self.points["points"][0], api_refusal="Hit has no API type")
        plan = generator.generate(self.mapping, self.functions, {**self.points, "points": [refused]})
        self.assertEqual(plan["inject"], [])
        self.assertTrue(any("refused point" in error for error in plan["errors"]))

    def test_head_return_emits_two_rows_not_replace(self):
        row = dict(self.mapping["rows"][0], anchor="HEAD/RETURN; cancellable result")
        plan = generator.generate({"rows": [row]}, self.functions, self.points)
        self.assertEqual([item["kind"] for item in plan["inject"]], ["head", "return"])
        self.assertNotIn("replace", {item["kind"] for item in plan["inject"]})

    def test_tail_maps_to_return_with_note(self):
        row = dict(self.mapping["rows"][0], anchor="TAIL; result")
        plan = generator.generate({"rows": [row]}, self.functions, self.points)
        self.assertEqual(plan["inject"][0]["kind"], "return")
        self.assertIn("TAIL mapped", plan["inject"][0]["note"])

    def test_refusal_anchor_is_not_emitted_as_head(self):
        row = dict(self.mapping["rows"][0], anchor="HEAD; no row emitted")
        plan = generator.generate({"rows": [row]}, self.functions, self.points)
        self.assertEqual(plan["inject"], [])
        self.assertTrue(any("invalid point anchor" in error for error in plan["errors"]))

    def test_index_shape_is_pinned(self):
        with self.assertRaises(ValueError):
            generator.generate(self.mapping, {"functions": []}, self.points)

    def test_cli_shape_is_deterministic(self):
        plan = generator.generate(self.mapping, self.functions, self.points)
        self.assertEqual(generator.render(plan, "lane1.json"), generator.render(plan, "lane1.json"))


if __name__ == "__main__":
    unittest.main()
