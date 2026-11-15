import importlib.util
import contextlib
import io
import json
import os
import tempfile
import sys
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).parents[1]
_SPEC = importlib.util.spec_from_file_location(
    "unmodelled_calls_tool", ROOT / "tools" / "unmodelled_calls.py"
)
TOOL = importlib.util.module_from_spec(_SPEC)
sys.modules[_SPEC.name] = TOOL
_before = list(sys.path)
_SPEC.loader.exec_module(TOOL)
sys.path[:] = _before


class UnmodelledCallsTests(unittest.TestCase):
    FABRIC_REVIEW_PAIRS = {
        (">=0.14.3 <1.0.0", "0.15.2+26.3"): True,
        (">=0.6.3 <1.0.0", "0.6.10"): True,
        (">=0.6.3 <1.0.0", "0.6.8"): True,
        (">=0.9.3- <=0.9.3", "0.9.3-alpha.1+mc26.3"): True,
        (">=0.9.3- <=0.9.3", "0.9.3-beta.1+mc26.3"): True,
        (">=1.0.0+26.x", "1.0.0+26.x"): True,
        (">=1.1.17 <1.2.0", "1.1.25"): True,
        (">=1.9.3", "unspecified"): None,
        (">=2.3.1+26.3.x", "2.3.1+26.3.x"): True,
        (">=6.9.0 <6.10", "6.9.0"): True,
    }

    def test_real_loader_review_pairs_are_recorded(self):
        """Answers came from Fabric Loader 0.19.5's Java harness in rc265-review."""
        loader = os.environ.get("FABRIC_LOADER_0195")
        if not loader:
            self.skipTest("set FABRIC_LOADER_0195 for the real Java oracle")
        oracle = TOOL.FabricPredicateOracle(Path(loader))
        valid = {pair for pair, answer in self.FABRIC_REVIEW_PAIRS.items() if answer is not None}
        oracle.evaluate(valid)
        self.assertEqual({pair: oracle.cache[pair] for pair in valid},
                         {pair: answer for pair, answer in self.FABRIC_REVIEW_PAIRS.items()
                          if answer is not None})
        invalid = {pair for pair, answer in self.FABRIC_REVIEW_PAIRS.items() if answer is None}
        oracle.evaluate(invalid)
        self.assertFalse(oracle.cache[next(iter(invalid))])
        self.assertTrue(oracle.parse_errors)
    def test_aggregate_counts_jars_calls_and_sole_rows(self):
        corpora = {
            "wide": [
                {"verdict": "ran-hollow", "calls": [["x", 2]], "unknown": 2},
                {"verdict": "ran-hollow", "calls": [["x", 1], ["y", 3]], "unknown": 4},
                {"verdict": "ran-clean", "calls": [], "unknown": 0},
                {"verdict": "refused"},
                {"verdict": "skipped"},
            ],
            "mega": [
                {"verdict": "ran-hollow", "calls": [["y", 5]], "unknown": 5},
            ],
        }
        totals, ranked = TOOL.aggregate(corpora)

        self.assertEqual(totals["wide"], {
            "ran-hollow": 2, "ran-clean": 1, "refused": 1, "skipped": 1,
            "jars": 5, "unknown_total": 6,
        })
        self.assertEqual(totals["mega"]["unknown_total"], 5)
        by_call = {row["call"]: row for row in ranked}
        # x is the only distinct unknown of one jar; y of another. Both are
        # hit by two jars, so total volume breaks the tie.
        self.assertEqual([row["call"] for row in ranked], ["y", "x"])
        self.assertEqual(by_call["x"], {
            "call": "x", "jars": 2, "calls": 3, "sole": 1, "corpora": ["wide"],
        })
        self.assertEqual(by_call["y"], {
            "call": "y", "jars": 2, "calls": 8, "sole": 1,
            "corpora": ["mega", "wide"],
        })

    def test_a_jar_with_two_distinct_calls_is_nobodys_sole_row(self):
        corpora = {"wide": [
            {"verdict": "ran-hollow", "calls": [["a", 4], ["b", 1]], "unknown": 5},
        ]}
        _, ranked = TOOL.aggregate(corpora)
        self.assertEqual([row["sole"] for row in ranked], [0, 0])

    def test_calls_counted_before_a_refusal_are_not_ranked(self):
        # A refused jar keeps what it counted before it stopped, for reading;
        # the totals and ranking stay over runs that finished.
        corpora = {"wide": [
            {"verdict": "refused", "counted_before_error": [["z", 3]]},
            {"verdict": "ran-hollow", "calls": [["a", 1]], "unknown": 1},
        ]}
        totals, ranked = TOOL.aggregate(corpora)
        self.assertEqual(totals["wide"]["unknown_total"], 1)
        self.assertEqual([row["call"] for row in ranked], ["a"])

    def test_parked_threads_rank_by_jars_then_threads(self):
        watcher = "com/electronwill/nightconfig/core/file/FileWatcher$FsWatcher"
        corpora = {
            "mega": [
                {"jar": "a.jar", "verdict": "ran-hollow",
                 "parked_threads": [{"name": "w-1", "runnable": watcher},
                                    {"name": "w-2", "runnable": watcher}]},
                {"jar": "b.jar", "verdict": "refused",
                 "parked_threads": [{"name": None, "runnable": "demo/Updater"}]},
                {"jar": "c.jar", "verdict": "ran-clean"},
            ],
            "wide": [
                {"jar": "a.jar", "verdict": "ran-hollow",
                 "parked_threads": [{"name": "w-1", "runnable": watcher}]},
            ],
        }
        # The same file name in two corpora is two jars; a jar starting two
        # watchers is still one jar.
        self.assertEqual(TOOL.parked_summary(corpora), [
            {"runnable": watcher, "jars": 2, "threads": 3},
            {"runnable": "demo/Updater", "jars": 1, "threads": 1},
        ])

    def test_parse_corpus_splits_name_and_directory(self):
        self.assertEqual(TOOL.parse_corpus("neoforge=/tmp/x"),
                         ("neoforge", Path("/tmp/x")))

    def test_locked_neoforge_reads_the_committed_file_names(self):
        locked = TOOL.locked_neoforge()
        self.assertTrue(locked)
        self.assertIn("BiomesOPlenty-neoforge-26.3-26.3.0.0.7.jar", locked)


    def _fabric_jar(self, entrypoints: dict, environment=None, depends=None, mod_id="demo") -> Path:
        import json
        import tempfile
        import zipfile
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: __import__("shutil").rmtree(directory, ignore_errors=True))
        path = directory / "demo.jar"
        with zipfile.ZipFile(path, "w") as archive:
            metadata = {"id": mod_id, "version": "1", "entrypoints": entrypoints}
            if environment is not None:
                metadata["environment"] = environment
            if depends is not None:
                metadata["depends"] = depends
            archive.writestr("fabric.mod.json", json.dumps(metadata))
        return path

    def test_a_fabric_jar_runs_main_then_server(self):
        jar = self._fabric_jar({"server": ["demo.S"], "main": ["demo.M"]})
        self.assertEqual(TOOL.entrypoint_kind(jar, "corpus-xl"), "main,server")
        self.assertEqual(TOOL.entrypoint_kind(self._fabric_jar({"server": ["demo.S"]}),
                                              "corpus-xl"), "server")
        self.assertEqual(TOOL.entrypoint_kind(self._fabric_jar({"main": ["demo.M"]}),
                                              "corpus-xl"), "main")
        self.assertIsNone(TOOL.entrypoint_kind(self._fabric_jar({"client": ["demo.C"]}),
                                               "corpus-xl"))

    def test_client_classification_keeps_roles_and_skip_reasons(self):
        client = self._fabric_jar({"client": ["demo.C"]})
        self.assertEqual(TOOL.classify_client(client, "corpus-xl"),
                         ("client-only", "client", "client entrypoint only"))
        dual = self._fabric_jar({"main": ["demo.M"], "client": ["demo.C"],
                                 "server": ["demo.S"]})
        self.assertEqual(TOOL.classify_client(dual, "corpus-xl"),
                         ("dual-client", "main,client", ""))
        server = self._fabric_jar({"main": ["demo.M"], "server": ["demo.S"]})
        self.assertEqual(TOOL.classify_client(server, "corpus-xl"),
                         ("library", None, "no client entrypoint"))

    def test_fabric_environment_client_is_client_even_without_a_client_key(self):
        import json
        import tempfile
        import zipfile
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: __import__("shutil").rmtree(directory, ignore_errors=True))
        path = directory / "demo.jar"
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("fabric.mod.json", json.dumps({
                "id": "demo", "version": "1", "environment": "client",
                "entrypoints": {"main": ["demo.Main"]},
            }))
        self.assertEqual(TOOL.classify_client(path, "corpus-xl"),
                         ("client-only", "main", "environment: client"))
        self.assertEqual(TOOL.classify_server_skip(path, "corpus-xl"), "client-only")

    def test_fabric_environment_matches_the_physical_side(self):
        for environment in ("*", "client"):
            jar = self._fabric_jar({"client": ["demo.C"]}, environment)
            self.assertEqual(TOOL.entrypoint_kind(jar, "corpus-xl", "client"), "client")
        server = self._fabric_jar({"main": ["demo.M"], "client": ["demo.C"]}, "server")
        self.assertIsNone(TOOL.entrypoint_kind(server, "corpus-xl", "client"))
        self.assertEqual(TOOL.classify_client(server, "corpus-xl"),
                         ("server-only", None, "environment: server"))

    def test_neoforge_toml_and_dist_side_rules(self):
        import struct
        import tempfile
        import zipfile
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: __import__("shutil").rmtree(directory, ignore_errors=True))

        def mod_class(dists=()):
            u = lambda text: b"\x01" + struct.pack(">H", len(text)) + text.encode()
            pool = [u("example/ExampleMod"), b"\x07\x00\x01", u("java/lang/Object"), b"\x07\x00\x03",
                    u("Lnet/neoforged/fml/common/Mod;"), u("RuntimeVisibleAnnotations"),
                    u("value"), u("demo"), u("dist"), u("Lnet/neoforged/api/distmarker/Dist;"),
                    u("CLIENT"), u("DEDICATED_SERVER")]
            annotation = struct.pack(">H", 5)
            pairs = [(7, b"s" + struct.pack(">H", 8))]
            if dists:
                pairs.append((9, b"[" + struct.pack(">H", len(dists)) + b"".join(
                    b"e" + struct.pack(">HH", 10, 11 if d == "CLIENT" else 12) for d in dists)))
            annotation += struct.pack(">H", len(pairs))
            annotation += b"".join(struct.pack(">H", n) + value for n, value in pairs)
            attr = struct.pack(">HI", 6, len(annotation) + 2) + struct.pack(">H", 1) + annotation
            return (b"\xca\xfe\xba\xbe" + struct.pack(">HHH", 0, 52, 13) + b"".join(pool)
                    + struct.pack(">HHHHHH", 0x21, 2, 4, 0, 0, 0) + struct.pack(">H", 1) + attr)

        def jar(name, toml, mod, legacy=False):
            path = directory / name
            with zipfile.ZipFile(path, "w") as archive:
                archive.writestr("META-INF/mods.toml" if legacy else "META-INF/neoforge.mods.toml", toml)
                archive.writestr("example/ExampleMod.class", mod)
            return path

        # Distant Horizons' shape: a mod-scoped property in NeoForge's file is
        # ordinary mod metadata, not the physical-side selector.
        client_toml = jar("client.jar", '[[mods]]\nmodId = "dh"\nclientSideOnly = "true"\nside = "BOTH"\n', mod_class())
        self.assertEqual(TOOL.classify_client(client_toml, "neoforge")[0], "neo-client")
        metadata = TOOL.ledger.read_jar_entrypoints(client_toml, force_neoforge=True)
        self.assertFalse(metadata.client_side_only)
        legacy = jar("legacy.jar", 'clientSideOnly = true\n[[mods]]\nmodId = "legacy"\n', mod_class(), legacy=True)
        self.assertEqual(TOOL.classify_client(legacy, "neoforge")[0], "client-only")
        dual = jar("dual.jar", "", mod_class(("CLIENT", "DEDICATED_SERVER")))
        self.assertEqual(TOOL.classify_client(dual, "neoforge")[0], "neo-client")

    def test_client_role_runs_with_declared_dependency_closure(self):
        import stat
        import tempfile
        directory = Path(tempfile.mkdtemp())
        self.addCleanup(lambda: __import__("shutil").rmtree(directory, ignore_errors=True))
        # Build both jars in the same corpus directory so dependency_closure
        # resolves the library beside the client mod.
        lib = self._fabric_jar({}, mod_id="lib")
        app = self._fabric_jar({"client": ["demo.C"]}, "client",
                               {"lib": "*"}, "app")
        import shutil
        shutil.copy2(lib, directory / "lib.jar")
        shutil.copy2(app, directory / "app.jar")
        probe = directory / "probe.py"
        probe.write_text("#!/usr/bin/env python3\nimport json\nprint(json.dumps({'status':'ok','verdict':'ran-clean','unknown_host_call_count':0,'unknown_host_calls':[]}))\n")
        probe.chmod(probe.stat().st_mode | stat.S_IXUSR)
        report_path = directory / "report.json"
        from unittest import mock
        fake_oracle = type("Oracle", (), {"evaluate": lambda self, pairs: None,
                                           "matches": lambda self, version, predicates: True})()
        with mock.patch.object(TOOL, "find_fabric_loader", return_value=directory / "loader.jar"), \
             mock.patch.object(TOOL, "FabricPredicateOracle", return_value=fake_oracle):
            self.assertEqual(TOOL.main([
                "--role", "client", "--dependency-mode", "closure",
                "--probe", str(probe), "--jobs", "1",
                "--json", str(report_path), "--corpus", f"test={directory}",
            ]), 0)
        rows = __import__("json").loads(report_path.read_text())["corpora"]["test"]["rows"]
        row = next(row for row in rows if row["jar"] == "app.jar")
        self.assertEqual((row["role"], row["kind"]), ("client-only", "client"))
        self.assertIn("lib.jar", row["deps"])

    def test_the_row_takes_the_probe_verdict(self):
        self.assertEqual(TOOL.row_verdict({"verdict": "ran-partial",
                                           "unknown_host_call_count": 0}), "ran-partial")
        self.assertEqual(TOOL.row_verdict({"unknown_host_call_count": 0}), "ran-clean")
        self.assertEqual(TOOL.row_verdict({"unknown_host_call_count": 3}), "ran-hollow")


    @staticmethod
    def _jar(directory, name, ident, depends=None):
        path = Path(directory) / name
        metadata = {"schemaVersion": 1, "id": ident, "version": "1",
                    "entrypoints": {"main": ["demo.Entry"]}}
        if depends is not None:
            metadata["depends"] = depends
        with zipfile.ZipFile(path, "w") as archive:
            archive.writestr("fabric.mod.json", json.dumps(metadata))
        return path

    def test_both_modes_are_reported_and_missing_closure_is_refused(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._jar(root, "app.jar", "app", {"absent": ">=2"})
            probe = root / "probe"
            probe.write_text(
                "#!/bin/sh\n"
                "case \"$*\" in *--link*) n=2 ;; *) n=1 ;; esac\n"
                "printf '{\"status\":\"ok\",\"unknown_host_call_count\":%s,\"unknown_host_calls\":[]}\\n' \"$n\"\n",
                encoding="utf-8")
            probe.chmod(0o755)
            output = io.StringIO()
            with contextlib.redirect_stdout(output), contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(TOOL.main([
                    "--corpus", f"wide={root}", "--probe", str(probe),
                    "--dependency-mode", "both", "--jobs", "1",
                    "--allow-missing-fabric-oracle",
                ]), 0)
            report = json.loads(output.getvalue())
        rows = report["corpora"]["wide"]["rows"]
        by_mode = {row["mode"]: row for row in rows}
        self.assertEqual(by_mode["alone"]["verdict"], "ran-hollow")
        self.assertEqual(by_mode["alone"]["unknown"], 1)
        self.assertEqual(by_mode["closure"]["verdict"], "refused")
        self.assertIn("Fabric dependency predicate oracle unavailable",
                      by_mode["closure"]["error"])
        self.assertEqual(report["totals"]["alone"]["wide"]["ran-hollow"], 1)
        self.assertNotIn("refused", report["totals"]["closure"]["wide"])
        self.assertEqual(len(report["corpora"]["wide"]["closure_regressions"]), 1)

    def test_closure_mode_without_the_fabric_oracle_stops_loudly(self):
        """A missing loader jar must not pass for a census where every Fabric closure row refuses."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._jar(root, "app.jar", "app")
            probe = root / "probe"
            probe.write_text("#!/bin/sh\nexit 99\n", encoding="utf-8")
            probe.chmod(0o755)
            errors = io.StringIO()
            with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(errors):
                code = TOOL.main(["--corpus", f"wide={root}", "--probe", str(probe),
                                  "--dependency-mode", "closure", "--jobs", "1"])
        self.assertEqual(code, 2)
        self.assertIn("--fabric-loader", errors.getvalue())

    def test_dependency_failure_is_a_regression_outside_closure_totals(self):
        rows = [
            {"jar": "app.jar", "mode": "alone", "verdict": "ran-hollow",
             "unknown": 1, "calls": [["a", 1]]},
            {"jar": "app.jar", "mode": "closure", "verdict": "refused",
             "stage": "execute", "error": "at dep/Config#<clinit> +7",
             "deps": ["dep.jar"], "unknown": 0, "calls": []},
        ]
        regressions = TOOL.mark_closure_regressions(rows)
        self.assertEqual(len(regressions), 1)
        self.assertEqual(rows[1]["classification"], "closure-regression")
        totals, _ = TOOL.aggregate({"wide": [rows[0]]})
        self.assertEqual(totals["wide"]["ran-hollow"], 1)


if __name__ == "__main__":
    unittest.main()
