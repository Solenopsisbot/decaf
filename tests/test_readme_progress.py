"""Tests for tools/readme_progress.py: the README's generated progress tables."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
import readme_progress  # noqa: E402


def _census():
    def row(**counts):
        return counts

    server = {
        "corpus-xl/closure/server": row(**{"ran-clean": 3, "ran-hollow": 2, "ran-partial": 1, "refused": 4, "skipped": 9, "unknown_total": 11}),
        "mega/closure/server": row(**{"ran-clean": 1, "unknown_total": 0}),
        "neoforge/closure/server": row(**{"refused": 2, "unknown_total": 5}),
        # The `alone` rows must not leak into the table.
        "corpus-xl/alone/server": row(**{"ran-clean": 100}),
    }
    client = {
        "corpus-xl/closure/client-only": row(**{"ran-clean": 1, "refused": 1, "unknown_total": 2}),
        "corpus-xl/closure/dual-client": row(**{"ran-hollow": 5, "unknown_total": 7}),
        "corpus-xl/closure/library": row(**{"skipped": 40}),
        "mega/closure/client-only": row(**{"ran-partial": 2}),
        "neoforge/closure/neo-client": row(**{"ran-clean": 4}),
    }
    return {
        "roles": {
            "server": {"merged": {"sha": "abcdef1234567", "totals": server}},
            "client": {"merged": {"sha": "abcdef1234567", "totals": client}},
        }
    }


class RenderTests(unittest.TestCase):
    def test_server_row_sums_verdicts_and_leaves_out_skipped_and_alone(self):
        block = readme_progress.render(_census(), "2026-11-17")
        self.assertIn("| Wide Fabric set | 10 | 3 | 2 | 1 | 4 | 11 |", block)

    def test_client_row_adds_client_only_and_dual_client_but_not_libraries(self):
        block = readme_progress.render(_census(), "2026-11-17")
        self.assertIn("| Wide Fabric set | 7 | 1 | 5 | 0 | 1 | 9 |", block)
        self.assertIn("| NeoForge 26.3 set | 4 | 4 | 0 | 0 | 0 | 0 |", block)

    def test_missing_corpus_is_an_error_not_a_zero_row(self):
        census = _census()
        del census["roles"]["server"]["merged"]["totals"]["mega/closure/server"]
        with self.assertRaises(ValueError):
            readme_progress.render(census, "2026-11-17")


class RewriteTests(unittest.TestCase):
    def test_only_the_marked_block_changes(self):
        readme = f"intro\n{readme_progress.START}\nold\n{readme_progress.END}\noutro\n"
        out = readme_progress.rewrite(readme, f"{readme_progress.START}\nnew\n{readme_progress.END}")
        self.assertEqual(out, f"intro\n{readme_progress.START}\nnew\n{readme_progress.END}\noutro\n")

    def test_a_readme_without_markers_is_refused(self):
        with self.assertRaises(ValueError):
            readme_progress.rewrite("no markers here", "x")


if __name__ == "__main__":
    unittest.main()
