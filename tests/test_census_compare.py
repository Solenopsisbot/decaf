import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).parents[1]
spec = importlib.util.spec_from_file_location("census_compare", ROOT / "tools" / "census_compare.py")
tool = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tool)


def payload(rows, sha="sha"):
    return {"sha": sha, "corpora": {"corpus": {"rows": rows}}}


class CensusCompareTests(unittest.TestCase):
    def test_diff_names_moved_jar_and_call_deltas(self):
        base = payload([{"jar": "a.jar", "mode": "closure", "verdict": "ran-hollow", "calls": [["x#f()", 3]]}])
        merged = payload([{"jar": "a.jar", "mode": "closure", "verdict": "ran-clean", "calls": []}])
        report = tool.compare_payloads(base, merged, {"base": [base], "merged": [merged]}, "server")
        self.assertEqual(report["moved"][0]["jar"], "a.jar")
        self.assertEqual(report["moved"][0]["vanished"], [{"call": "x#f()", "count": 3}])
        self.assertEqual(report["moved"][0]["appeared"], [])

    def test_repetition_difference_is_flaky_and_excluded(self):
        stable = {"jar": "stable.jar", "verdict": "ran-clean", "calls": []}
        first = payload([{"jar": "flaky.jar", "verdict": "ran-clean", "calls": []}, stable])
        second = payload([{"jar": "flaky.jar", "verdict": "ran-hollow", "calls": [["x", 1]]}, stable])
        merged = payload([{"jar": "flaky.jar", "verdict": "ran-clean", "calls": []}, stable])
        report = tool.compare_payloads(first, merged, {"base": [first, second], "merged": [merged, merged]}, "server")
        self.assertEqual(report["moved"], [])
        self.assertEqual(report["flaky"][0]["jar"], "flaky.jar")

    def test_markdown_has_totals_and_attribution_column(self):
        base = payload([{"jar": "a.jar", "verdict": "ran-hollow", "unknown": 2, "calls": [["x", 2]]}])
        merged = payload([{"jar": "a.jar", "verdict": "ran-clean", "unknown": 0, "calls": []}])
        report = tool.compare_payloads(base, merged, {"base": [base], "merged": [merged]}, "server")
        text = tool.markdown(report)
        self.assertIn("Unknown totals", text)
        self.assertIn("a.jar", text)
        self.assertIn("EXACT/SILENT attribution", text)
        self.assertIn("| corpus | default | server | 2 | 0 |", text)

    def test_count_only_change_in_a_still_hollow_row_is_reported(self):
        # An exact model often removes some calls from a row that stays hollow: that must still be listed,
        # with the per-call decrease, so it can be attributed EXACT or SILENT.
        base = payload([{"jar": "pair.jar", "verdict": "ran-hollow", "unknown": 10,
                         "calls": [["Pair#getFirst", 7], ["Other#x", 3]]}])
        merged = payload([{"jar": "pair.jar", "verdict": "ran-hollow", "unknown": 5,
                           "calls": [["Pair#getFirst", 2], ["Other#x", 3]]}])
        report = tool.compare_payloads(base, merged, {"base": [base], "merged": [merged]}, "server")
        self.assertEqual(len(report["moved"]), 1)
        row = report["moved"][0]
        self.assertEqual(row["vanished"], [{"call": "Pair#getFirst", "count": 5}])
        self.assertEqual(row["appeared"], [])
        self.assertEqual((row["base_unknown"], row["merged_unknown"]), (10, 5))

    def test_identical_rows_are_not_reported(self):
        same = {"jar": "same.jar", "verdict": "ran-hollow", "unknown": 1, "calls": [["x", 1]]}
        report = tool.compare_payloads(payload([same]), payload([dict(same)]),
                                       {"base": [payload([same])], "merged": [payload([same])]}, "server")
        self.assertEqual(report["moved"], [])

    def test_row_on_one_side_only_is_reported(self):
        base = payload([])
        merged = payload([{"jar": "new.jar", "verdict": "ran-hollow", "unknown": 2, "calls": [["y", 2]]}])
        report = tool.compare_payloads(base, merged, {"base": [base], "merged": [merged]}, "server")
        self.assertEqual(report["moved"][0]["base_verdict"], "(absent)")
        self.assertEqual(report["moved"][0]["appeared"], [{"call": "y", "count": 2}])

    def test_refusal_that_stops_elsewhere_is_reported(self):
        # Same verdict and no `calls`, but the refusal's counted-before-error calls and its error moved.
        base = payload([{"jar": "beta.jar", "verdict": "refused", "error": "A at X+3",
                         "counted_before_error": [["a#f", 14]]}])
        merged = payload([{"jar": "beta.jar", "verdict": "refused", "error": "B at Y+9",
                           "counted_before_error": [["b#g", 1]]}])
        report = tool.compare_payloads(base, merged, {"base": [base], "merged": [merged]}, "server")
        row = report["moved"][0]
        self.assertEqual(row["vanished"], [{"call": "a#f", "count": 14}])
        self.assertEqual(row["appeared"], [{"call": "b#g", "count": 1}])
        self.assertEqual([c["field"] for c in row["other_changes"]], ["error"])
        self.assertIn("B at Y+9", tool.markdown(report))

    def test_changed_failure_evidence_alone_is_reported(self):
        base = payload([{"jar": "hud.jar", "verdict": "ran-partial", "unknown": 0, "calls": [], "failed": ["x"]}])
        merged = payload([{"jar": "hud.jar", "verdict": "ran-partial", "unknown": 0, "calls": [], "failed": ["y"]}])
        report = tool.compare_payloads(base, merged, {"base": [base], "merged": [merged]}, "server")
        self.assertEqual([c["field"] for c in report["moved"][0]["other_changes"]], ["failed"])


if __name__ == "__main__":
    unittest.main()
