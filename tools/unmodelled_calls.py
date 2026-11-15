#!/usr/bin/env python3
"""Rank the unmodelled host calls the native probe still answers with a neutral value.

    unmodelled_calls.py [--corpus name=DIR]... [--probe PATH] [--json OUT]
                        [--game-version 26.3] [--limit N] [--jobs N]
                        [--dependency-mode both|alone|closure]

Runs the probe once per jar alone and once with the jar's declared dependency
closure (unless `--dependency-mode` selects one), resolved against the other
corpus jars (the same resolution `java_mod_server_ledger.py --with-deps` uses),
and aggregates the probe's `unknown_host_calls`. The modes are reported as
separate rows and totals so a closure win cannot be mistaken for an interpreter win:

    call           owner#name descriptor, as the probe reports it
    jars           how many jars hit the call at least once
    calls          total invocations over every jar
    sole           jars where this call is the only distinct unknown left
                   (so modelling it alone would make them ran-clean)

A Fabric jar runs every server stage it declares, in the loader's order
(`Hooks.startServer`: `main`, then `server`), passed to the probe as
`main,server`; a NeoForge jar runs its `@Mod` classes. The verdict is the one
the game's mod gives (`verdict.rs`): `ran-clean` when every stage ran with no
unknown host call, `ran-hollow` with at least one, `ran-partial` when a stage
failed and another ran, `refused` when nothing ran. A jar with no server-side
entrypoint is `skipped` (client-only or a library, out of scope for a server
run). A closure with an absent or incompatible required provider is refused with
`missing dependency X` and is never run.

Default corpora: the Fabric `~/horizon-corpus/{corpus-xl,mega}` and the NeoForge
jars pinned by `tools/real_mod_fixtures/neoforge_corpus.lock.json`. Nothing is
downloaded and no jar is copied into the checkout.

Exit 0 with a report; 1 when the probe binary or a corpus directory is missing.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import subprocess
import sys
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import hybrid_pack  # noqa: E402
import java_mod_server_ledger as ledger  # noqa: E402

ROOT = Path(__file__).resolve().parents[1]
NEOFORGE_LOCK = ROOT / "tools" / "real_mod_fixtures" / "neoforge_corpus.lock.json"
HOME_CORPUS = Path.home() / "horizon-corpus"
DEFAULT_CORPORA = (
    f"corpus-xl={HOME_CORPUS / 'corpus-xl'}",
    f"mega={HOME_CORPUS / 'mega'}",
    f"neoforge={HOME_CORPUS / 'neoforge'}",
)
PROBE_TIMEOUT = 180.0
MAX_REPORTED = 400
FABRIC_LOADER_NAME = "fabric-loader-0.19.5.jar"


class FabricOracleUnavailable(RuntimeError):
    pass


class FabricPredicateOracle:
    """Batch the exact Fabric Loader 0.19.5 predicate oracle once per run."""

    def __init__(self, loader: Path):
        self.loader = loader
        self.cache: dict[tuple[str, str], bool] = {}
        self.parse_errors: dict[tuple[str, str], str] = {}
        self.java = shutil.which("java")
        self.javac = shutil.which("javac")
        if not self.java or not self.javac:
            raise FabricOracleUnavailable("Java 25 (java and javac) is unavailable")
        source = ROOT / "tools" / "FabricPredicateOracle.java"
        cache = Path.home() / ".cache" / "horizon-java" / "fabric-predicate-oracle"
        cache.mkdir(parents=True, exist_ok=True)
        class_file = cache / "FabricPredicateOracle.class"
        if not class_file.is_file() or class_file.stat().st_mtime < source.stat().st_mtime:
            result = subprocess.run(
                [self.javac, "--release", "21", "-cp", str(loader), "-d", str(cache), str(source)],
                capture_output=True, text=True, check=False,
            )
            if result.returncode:
                raise FabricOracleUnavailable(
                    f"could not compile Fabric predicate oracle: {(result.stderr or result.stdout).strip()[-300:]}"
                )
        self.class_path = cache

    def evaluate(self, pairs: set[tuple[str, str]]) -> None:
        pending = sorted(pair for pair in pairs if pair not in self.cache)
        if not pending:
            return
        lines = "".join(json.dumps({"predicate": predicate, "version": version}) + "\n"
                        for predicate, version in pending)
        result = subprocess.run(
            [self.java, "-cp", os.pathsep.join((str(self.loader), str(self.class_path))),
             "FabricPredicateOracle", str(self.loader)],
            input=lines, capture_output=True, text=True, check=False,
        )
        if result.returncode:
            raise FabricOracleUnavailable(
                f"Fabric predicate oracle failed: {(result.stderr or result.stdout).strip()[-300:]}"
            )
        answers = []
        for raw in result.stdout.splitlines():
            try:
                answers.append(json.loads(raw))
            except ValueError as error:
                raise FabricOracleUnavailable(f"invalid Fabric oracle output: {raw[:200]}") from error
        if len(answers) != len(pending):
            raise FabricOracleUnavailable("Fabric predicate oracle returned the wrong number of answers")
        for pair, answer in zip(pending, answers):
            if "error" in answer:
                self.parse_errors[pair] = str(answer["error"])
                self.cache[pair] = False
            else:
                self.cache[pair] = bool(answer.get("ok"))

    def matches(self, version: str, predicates: tuple[str, ...]) -> bool:
        pairs = {(predicate, version) for predicate in predicates}
        self.evaluate(pairs)
        return any(self.cache[pair] for pair in pairs)


def find_fabric_loader(corpora: list[tuple[str, Path]], explicit: Path | None) -> Path | None:
    candidates = [explicit] if explicit else []
    for _, directory in corpora:
        candidates.append(directory / FABRIC_LOADER_NAME)
        candidates.extend(directory.glob(f"**/{FABRIC_LOADER_NAME}"))
    candidates.extend(path / FABRIC_LOADER_NAME for path in ledger.DEFAULT_DEP_DIRS)
    for candidate in candidates:
        if candidate and candidate.is_file():
            return candidate.resolve()
    return None


def fabric_pairs(index: dict[str, list]) -> set[tuple[str, str]]:
    pairs = set()
    for providers in index.values():
        for provider in providers:
            requirements = provider.declaration.requirements
            for dependency, predicate in requirements:
                for candidate in index.get(dependency.lower(), []):
                    pairs.add((predicate, candidate.declaration.version))
    return pairs


def parse_corpus(value: str) -> tuple[str, Path]:
    name, _, directory = value.partition("=")
    if not name or not directory:
        raise argparse.ArgumentTypeError("a corpus is NAME=DIR")
    return name, Path(directory).expanduser()


def locked_neoforge() -> set[str]:
    if not NEOFORGE_LOCK.is_file():
        return set()
    data = json.loads(NEOFORGE_LOCK.read_text())
    rows = data.get("jars") if isinstance(data, dict) else None
    if not isinstance(rows, list):
        return set()
    return {str(row.get("file")) for row in rows if isinstance(row, dict) and row.get("file")}


def corpus_jars(name: str, directory: Path) -> list[Path]:
    if not directory.is_dir():
        raise SystemExit(f"{name}: {directory} is not a directory")
    jars = sorted(p for p in directory.iterdir() if p.suffix.lower() == ".jar" and p.is_file())
    if name == "neoforge":
        locked = locked_neoforge()
        if locked:
            jars = [jar for jar in jars if jar.name in locked]
    return jars


SERVER_STAGES = ("main", "server")
CLIENT_STAGES = ("main", "client")


def entrypoint_kind(path: Path, corpus: str, role: str = "server") -> str | None:
    """The stages a dedicated server runs for this jar, comma-joined in loader order.

    Fabric Loader's `Hooks.startServer` invokes every `main` entrypoint, then
    every `server` one, so a jar with both runs both (the game's mod does the
    same since #31). A jar without Fabric server stages runs its NeoForge
    `@Mod` classes.
    """
    entrypoints = ledger.read_jar_entrypoints(path, force_neoforge=corpus == "neoforge")
    if entrypoints is None:
        return None
    if role == "client":
        if corpus == "neoforge":
            return "neoforge-client" if entrypoints.neoforge else None
        if entrypoints.fabric_environment == "server":
            return None
        stages = [kind for kind in CLIENT_STAGES if kind in entrypoints.kinds]
        return ",".join(stages) if stages else None
    if entrypoints.neoforge_client_only or entrypoints.client_side_only:
        return None
    stages = [kind for kind in SERVER_STAGES if kind in entrypoints.kinds]
    if stages:
        return ",".join(stages)
    if entrypoints.neoforge:
        return "neoforge"
    return None


def classify_client(path: Path, corpus: str) -> tuple[str, str | None, str]:
    entrypoints = ledger.read_jar_entrypoints(path, force_neoforge=corpus == "neoforge")
    if entrypoints is None:
        return "unreadable", None, "metadata unreadable"
    if corpus == "neoforge":
        if not entrypoints.neoforge:
            return "library", None, "no @Mod class"
        if entrypoints.neoforge_client_only or entrypoints.client_side_only:
            return "client-only", "neoforge-client", "client-only @Mod"
        if entrypoints.neoforge_server_only:
            return "server-only", None, "server-only @Mod"
        return "neo-client", "neoforge-client", ""
    if entrypoints.fabric_environment == "server":
        return "server-only", None, "environment: server"
    kinds = set(entrypoints.kinds)
    stages = [kind for kind in CLIENT_STAGES if kind in kinds]
    if entrypoints.client_side_only:
        return ("client-only", ",".join(stages), "environment: client") if stages else (
            "client-only-noentry", None, "environment: client")
    if "client" not in kinds:
        return "library", None, "no client entrypoint"
    if "main" not in kinds and "server" not in kinds:
        return "client-only", ",".join(stages), "client entrypoint only"
    return "dual-client", ",".join(stages), ""


def classify_server_skip(path: Path, corpus: str) -> str:
    entrypoints = ledger.read_jar_entrypoints(path, force_neoforge=corpus == "neoforge")
    if entrypoints is None:
        return "metadata unreadable"
    if entrypoints.client_side_only or entrypoints.neoforge_client_only \
            or "client" in entrypoints.kinds:
        return "client-only"
    if corpus == "neoforge" and not entrypoints.neoforge:
        return "library (no @Mod class)"
    if not entrypoints.kinds and not entrypoints.neoforge:
        return "library"
    return "no server entrypoint"


def row_verdict(result: dict) -> str:
    """The label of one probe run that returned `status: ok`.

    The probe reports the game's own verdict (`ran-partial` included); an older
    probe without one is labelled from its unknown-call count.
    """
    verdict = result.get("verdict")
    if verdict in {"ran-clean", "ran-hollow", "ran-partial", "refused"}:
        return verdict
    return "ran-clean" if result.get("unknown_host_call_count") == 0 else "ran-hollow"


def probe_one(probe: Path, jar: Path, kind: str, deps: list[Path],
              game_version: str) -> dict:
    command = [str(probe), "--json", "--game-version", game_version]
    for dep in deps:
        command += ["--link", str(dep)]
    command += [str(jar), kind]
    try:
        result = subprocess.run(command, capture_output=True, text=True,
                                timeout=PROBE_TIMEOUT, check=False)
    except (OSError, subprocess.SubprocessError) as error:
        return {"status": "refused", "stage": "probe", "error": str(error)}
    for raw in reversed(result.stdout.splitlines()):
        line = raw.strip()
        if line.startswith("{"):
            try:
                report = json.loads(line)
            except ValueError:
                continue
            if isinstance(report, dict):
                return report
    return {"status": "refused", "stage": "probe",
            "error": (result.stderr or result.stdout).strip()[-400:]}


def missing_reason(missing: list[tuple[str, str]],
                   requirements: tuple[tuple[str, str, str], ...] = ()) -> str:
    ranges = {key: requirement for key, _, requirement in requirements}
    names = []
    for key, by in missing:
        suffix = f" [{ranges[key]}]" if ranges.get(key) not in (None, "", "*") else ""
        names.append(f"{key}{suffix}" if not by else f"{key}{suffix} (needed by {by})")
    return "missing dependency " + ", ".join(names)


def result_row(jar: Path, kind: str, result: dict, deps: list[Path],
               missing: list[tuple[str, str]],
               requirements: tuple[tuple[str, str, str], ...], mode: str,
               jar_role: str | None = None) -> dict:
    """Normalize one census mode while retaining probe evidence."""
    if result.get("status") == "ok":
        unknown = result.get("unknown_host_call_count")
        calls = result.get("unknown_host_calls")
        if not isinstance(unknown, int) or not isinstance(calls, list):
            calls = []
        row = {
            "jar": jar.name, "mode": mode, "verdict": row_verdict(result),
            "classification": "alone-result" if mode == "alone" else "closure-result",
            "kind": kind, "unknown": unknown, "calls": calls,
            "parked_threads": result.get("parked_threads") or [],
            "deps": [dep.name for dep in deps], "missing": missing,
        }
        if result.get("failed"):
            row["failed"] = [str(item)[:400] for item in result["failed"]]
        if jar_role:
            row["role"] = jar_role
        for field in ("client_keys", "hud_layers", "listeners", "unfired", "refused", "frame"):
            if field in result:
                row[field] = result[field]
        return row
    row = {
        "jar": jar.name, "mode": mode, "verdict": "refused", "kind": kind,
        "classification": "alone-result" if mode == "alone" else "closure-result",
        "stage": result.get("stage"), "error": str(result.get("error"))[:400],
        "deps": [dep.name for dep in deps], "missing": missing,
    }
    before = result.get("unknown_host_calls")
    if isinstance(before, list) and before:
        row["counted_before_error"] = before
    if result.get("parked_threads"):
        row["parked_threads"] = result["parked_threads"]
    if jar_role:
        row["role"] = jar_role
    return row


VERDICT_RANK = {"ran-clean": 0, "ran-hollow": 1, "ran-partial": 2,
                "refused": 3, "skipped": 4}


def mark_closure_regressions(rows: list[dict]) -> list[dict]:
    """Mark valid dependency-induced failures without changing census totals."""
    by_jar: dict[str, dict[str, dict]] = defaultdict(dict)
    for row in rows:
        by_jar[row.get("jar", "")][row.get("mode", "")] = row
    regressions = []
    for modes in by_jar.values():
        alone, closure = modes.get("alone"), modes.get("closure")
        if not alone or not closure:
            continue
        alone_rank = VERDICT_RANK.get(alone.get("verdict"), 99)
        closure_rank = VERDICT_RANK.get(closure.get("verdict"), 99)
        alone_unknown = alone.get("unknown") or 0
        closure_unknown = closure.get("unknown") or 0
        if closure_rank > alone_rank or (closure_rank == alone_rank and
                                         closure_unknown > alone_unknown):
            closure["classification"] = "closure-regression"
            closure["failure"] = closure.get("error", "")
            regressions.append({
                "jar": closure.get("jar"), "alone_verdict": alone.get("verdict"),
                "closure_verdict": closure.get("verdict"),
                "alone_unknown": alone_unknown, "closure_unknown": closure_unknown,
                "failure": closure.get("failure", ""),
                "failure_kind": ("missing-dependency" if closure.get("stage") == "dependency"
                                  else "dependency-execution"),
                "deps": closure.get("deps", []),
            })
    return regressions


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--corpus", action="append", type=parse_corpus,
                        help="NAME=DIR, repeatable (default: the three measured corpora)")
    parser.add_argument("--probe", type=Path,
                        default=ROOT / "target" / "fast" / "horizon-native-java")
    parser.add_argument("--game-version", default="26.3")
    parser.add_argument("--role", choices=("server", "client"), default="server")
    parser.add_argument("--json", type=Path, help="write the full per-jar report here")
    parser.add_argument("--limit", type=int, default=0,
                        help="probe at most N jars per corpus (0 = all)")
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--fabric-loader", type=Path,
                        help="path to the real Fabric Loader 0.19.5 jar")
    parser.add_argument("--dependency-mode", choices=("both", "alone", "closure"),
                        default="both", help="run alone, closure, or both (default: both)")
    parser.add_argument("--allow-missing-fabric-oracle", action="store_true",
                        help="run closure mode even without the Fabric Loader oracle; every Fabric "
                             "closure row is then refused (only for tests and partial machines)")
    return parser


def main(argv: list[str] | None = None) -> int:
    args = build_parser().parse_args(argv)
    corpora = args.corpus or [parse_corpus(value) for value in DEFAULT_CORPORA]
    probe = args.probe.resolve()
    if not probe.is_file():
        print(f"missing probe binary {probe}", file=sys.stderr)
        return 1

    # Providers live in the corpora under test and in the pools the mega corpus
    # was fetched with, exactly as `java_mod_server_ledger.DEFAULT_DEP_DIRS`.
    # A NeoForge jar resolves inside the NeoForge corpus only: the Fabric corpora
    # carry Fabric builds of the same libraries (TerraBlender, GlitchCore), and
    # linking those beside a NeoForge mod measures the wrong class path.
    indexes: dict[str, dict] = {}
    for name, directory in corpora:
        if name in indexes:
            continue
        if name == "neoforge":
            provider_dirs = [directory]
        else:
            provider_dirs = [path for other, path in corpora if other != "neoforge"]
            provider_dirs += [path for path in ledger.DEFAULT_DEP_DIRS
                              if str(path) not in {str(d) for d in provider_dirs}]
        indexes[name] = ledger.provider_index(ledger.corpus_jars(provider_dirs))

    fabric_oracle: FabricPredicateOracle | None = None
    fabric_oracle_error = ""
    if any(name != "neoforge" for name, _ in corpora):
        loader = find_fabric_loader(corpora, args.fabric_loader)
        if loader is None:
            fabric_oracle_error = f"missing {FABRIC_LOADER_NAME}"
        else:
            try:
                fabric_oracle = FabricPredicateOracle(loader)
                pairs = set()
                for name, _ in corpora:
                    if name != "neoforge":
                        pairs.update(fabric_pairs(indexes[name]))
                fabric_oracle.evaluate(pairs)
            except FabricOracleUnavailable as error:
                fabric_oracle_error = str(error)
        # Without the oracle every Fabric closure row refuses, which reads like a census result
        # but only says the loader jar was not found (arcweld ran like that for a day). Stop
        # loudly instead, unless the caller asked for it.
        if (fabric_oracle_error and args.dependency_mode != "alone"
                and not args.allow_missing_fabric_oracle):
            print(f"error: closure mode needs the Fabric Loader oracle: {fabric_oracle_error}; "
                  f"pass --fabric-loader PATH (or --dependency-mode alone, or "
                  f"--allow-missing-fabric-oracle)", file=sys.stderr)
            return 2

    report: dict[str, dict] = {}
    for name, directory in corpora:
        index = indexes[name]
        jars = corpus_jars(name, directory)
        if args.limit:
            jars = jars[:args.limit]
        rows: list[dict] = []
        print(f"{name}: {len(jars)} jars", file=sys.stderr)
        todo = []
        for jar in jars:
            if args.role == "client":
                jar_role, kind, reason = classify_client(jar, name)
            else:
                jar_role, reason = None, classify_server_skip(jar, name)
                kind = entrypoint_kind(jar, name, args.role)
            if kind is None:
                for mode in (("alone", "closure") if args.dependency_mode == "both"
                             else (args.dependency_mode,)):
                    rows.append({"jar": jar.name, "mode": mode, "verdict": "skipped",
                                 "reason": reason, **({"role": jar_role} if jar_role else {})})
                continue
            closure_error = ""
            if name != "neoforge" and fabric_oracle_error:
                closure = ledger.Closure()
                closure_error = f"Fabric dependency predicate oracle unavailable: {fabric_oracle_error}"
            else:
                matcher = (fabric_oracle.matches if name != "neoforge"
                           else hybrid_pack.maven_version_satisfies)
                closure = ledger.dependency_closure(jar, index, version_matcher=matcher)
            missing = list(closure.missing)
            requirements = closure.missing_requirements
            modes = ("alone", "closure") if args.dependency_mode == "both" else (args.dependency_mode,)
            for mode in modes:
                if mode == "alone":
                    todo.append((jar, kind, jar_role, [], [], (), mode, ""))
                else:
                    todo.append((jar, kind, jar_role, list(closure.jars), missing, requirements, mode, closure_error))

        def run(item):
            jar, kind, jar_role, deps, missing, requirements, mode, closure_error = item
            if closure_error:
                result = {"status": "refused", "stage": "dependency", "error": closure_error}
            elif missing:
                result = {"status": "refused", "stage": "dependency",
                          "error": missing_reason(missing, requirements)}
            else:
                result = probe_one(probe, jar, kind, deps, args.game_version)
            return result_row(jar, kind, result, deps, missing, requirements, mode, jar_role)

        with ThreadPoolExecutor(max_workers=max(1, args.jobs)) as pool:
            rows.extend(pool.map(run, todo))
        regressions = mark_closure_regressions(rows)
        report[name] = {"directory": str(directory), "rows": rows,
                        "closure_regressions": regressions}

    rows_by_corpus = {name: data["rows"] for name, data in report.items()}
    modes = ("alone", "closure") if args.dependency_mode == "both" else (args.dependency_mode,)
    totals = {}
    ranked = {}
    parked = {}
    for mode in modes:
        selected = {name: [row for row in rows if row.get("mode") == mode]
                    for name, rows in rows_by_corpus.items()}
        if mode == "closure":
            selected = {name: [row for row in rows
                               if row.get("classification") != "closure-regression"]
                        for name, rows in selected.items()}
        totals[mode], ranked[mode] = aggregate(selected)
        parked[mode] = parked_summary(selected)
    payload = {"dependency_mode": args.dependency_mode, "totals": totals,
               "ranked": {mode: rows[:MAX_REPORTED] for mode, rows in ranked.items()},
               "parked": parked, "corpora": report}
    print(json.dumps(payload, indent=2))
    if args.json:
        args.json.write_text(json.dumps(payload, indent=2))
    return 0


def aggregate(corpora: dict[str, list[dict]]) -> tuple[dict, list[dict]]:
    """Per-corpus label counts and the ranked unknown-call table.

    An unknown call is ranked by how many jars it blocks, then by the jars it
    is the only distinct unknown of, then by its total volume. A jar counts
    once per call however many times the call ran inside it.
    """
    hits: dict[str, dict] = defaultdict(lambda: {"jars": 0, "calls": 0, "sole": 0,
                                                 "corpora": set()})
    totals = {}
    for name, rows in corpora.items():
        counts = defaultdict(int)
        for row in rows:
            counts[row["verdict"]] += 1
            calls = row.get("calls") or []
            distinct = {str(call) for call, _ in calls}
            for call, count in calls:
                hit = hits[str(call)]
                hit["jars"] += 1
                hit["calls"] += int(count)
                hit["corpora"].add(name)
                if len(distinct) == 1:
                    hit["sole"] += 1
        totals[name] = {
            **dict(counts),
            "jars": len(rows),
            "unknown_total": sum(row.get("unknown") or 0 for row in rows),
        }
    ranked = sorted(
        ({"call": call, **{key: value for key, value in hit.items() if key != "corpora"},
          "corpora": sorted(hit["corpora"])}
         for call, hit in hits.items()),
        key=lambda entry: (-entry["sole"], -entry["jars"], -entry["calls"], entry["call"]),
    )
    return totals, ranked


def parked_summary(corpora: dict[str, list[dict]]) -> list[dict]:
    """What the counted `Thread.start` calls would have run, ranked by jars.

    The probe names each started thread's `Runnable` (`parked_threads`): a parked
    night-config file watcher (`FileWatcher$FsWatcher`) is not the gap an update
    checker is, though both count as `java/lang/Thread#start()V`. A jar counts
    once per runnable however many threads of it it started; refused jars count
    too, since the threads they started before stopping are as real.
    """
    hits: dict[str, dict] = defaultdict(lambda: {"jars": set(), "threads": 0})
    for name, rows in corpora.items():
        for row in rows:
            for thread in row.get("parked_threads") or []:
                if not isinstance(thread, dict):
                    continue
                hit = hits[str(thread.get("runnable"))]
                hit["jars"].add((name, row.get("jar")))
                hit["threads"] += 1
    return sorted(
        ({"runnable": runnable, "jars": len(hit["jars"]), "threads": hit["threads"]}
         for runnable, hit in hits.items()),
        key=lambda entry: (-entry["jars"], -entry["threads"], entry["runnable"]),
    )


if __name__ == "__main__":
    raise SystemExit(main())
