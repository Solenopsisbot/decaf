#!/usr/bin/env python3
"""Compare an unmodelled-call census with a PR merged into current main."""
from __future__ import annotations

import argparse
import atexit
import json
import os
import shutil
import signal
import subprocess
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_TARGET_ROOT = Path.home() / "ava" / "targets" / "census-compare"
DRIVER = ROOT / "tools" / "unmodelled_calls.py"


def run(command: list[str], cwd: Path | None = None, *, check: bool = True) -> subprocess.CompletedProcess[str]:
    return subprocess.run(command, cwd=cwd, text=True, check=check)


def git(cwd: Path, *args: str, check: bool = True) -> str:
    result = subprocess.run(["git", *args], cwd=cwd, text=True, capture_output=True, check=check)
    return result.stdout.strip()


def resolve_head(repo: Path, value: str) -> tuple[str, str]:
    ref = f"pull/{value}/head" if value.isdigit() else value
    if value.isdigit():
        fetched = subprocess.run(["git", "fetch", "origin", f"{ref}:refs/remotes/origin/{ref}"], cwd=repo, text=True)
        if fetched.returncode:
            raise SystemExit(f"could not fetch PR #{value}: pull/{value}/head")
        ref = f"refs/remotes/origin/{ref}"
    try:
        sha = git(repo, "rev-parse", "--verify", f"{ref}^{{commit}}")
    except subprocess.CalledProcessError as error:
        raise SystemExit(f"cannot resolve head ref {value!r}") from error
    return ref, sha


def row_key(corpus: str, row: dict, role: str) -> tuple[str, str, str, str]:
    return (corpus, str(row.get("mode", "default")), str(row.get("role", role)), str(row.get("jar", "")))


def call_counts(row: dict | None) -> dict[str, int]:
    result: dict[str, int] = defaultdict(int)
    for item in (row or {}).get("calls") or []:
        if isinstance(item, (list, tuple)) and len(item) >= 2:
            result[str(item[0])] += int(item[1])
        elif isinstance(item, dict) and "call" in item:
            result[str(item["call"])] += int(item.get("count", 0))
    return dict(result)


def payload_rows(payload: dict, role: str) -> dict[tuple[str, str, str, str], dict]:
    rows: dict[tuple[str, str, str, str], dict] = {}
    for corpus, data in (payload.get("corpora") or {}).items():
        for row in data.get("rows", []):
            rows[row_key(str(corpus), row, role)] = row
    return rows


def differing_rows(payloads: list[dict], role: str) -> dict[tuple[str, str, str, str], list[dict]]:
    """Return rows whose complete observable result differs between repetitions."""
    all_keys = set().union(*(payload_rows(payload, role) for payload in payloads))
    flaky: dict[tuple[str, str, str, str], list[dict]] = {}
    for key in sorted(all_keys):
        snapshots = [payload_rows(payload, role).get(key) for payload in payloads]
        if any(snapshot != snapshots[0] for snapshot in snapshots[1:]):
            flaky[key] = snapshots
    return flaky


def totals(payload: dict, role: str, excluded: set[tuple[str, str, str, str]] = set()) -> dict[str, dict]:
    result: dict[str, dict] = {}
    for corpus, data in (payload.get("corpora") or {}).items():
        groups: dict[tuple[str, str], list[dict]] = defaultdict(list)
        for row in data.get("rows", []):
            key = row_key(str(corpus), row, role)
            if key not in excluded:
                groups[(key[1], key[2])].append(row)
        for (mode, row_role), rows in groups.items():
            counts: dict[str, int] = defaultdict(int)
            for row in rows:
                counts[str(row.get("verdict", "refused"))] += 1
            group_key = f"{corpus}/{mode}/{row_role}"
            result[group_key] = {"corpus": str(corpus), "mode": mode, "role": row_role,
                                 **dict(counts), "jars": len(rows),
                                 "unknown_total": sum(int(row.get("unknown") or 0) for row in rows)}
    return result


def moved_jars(base: dict, merged: dict, role: str, flaky: set[tuple[str, str, str, str]]) -> list[dict]:
    """Every deterministic row whose verdict, unknown count or counted-call multiset changed.

    A count-only change matters as much as a verdict change: an exact model usually removes calls
    from a row that stays hollow (#298's Pair calls did), and every vanished call must be attributed
    EXACT or SILENT. A row present on only one side (a jar that started or stopped being censused) is
    reported too, with a missing verdict.
    """
    before, after = payload_rows(base, role), payload_rows(merged, role)
    moved = []
    for key in sorted(set(before) | set(after)):
        if key in flaky:
            continue
        left, right = before.get(key), after.get(key)
        old_calls, new_calls = call_counts(left), call_counts(right)
        same_verdict = (left or {}).get("verdict") == (right or {}).get("verdict")
        same_unknown = (left or {}).get("unknown") == (right or {}).get("unknown")
        if left is not None and right is not None and same_verdict and same_unknown and old_calls == new_calls:
            continue
        # Per-call deltas: `vanished` holds calls counted fewer times after the merge (by how many),
        # `appeared` calls counted more times; a call that disappears entirely is the full count.
        vanished = [{"call": call, "count": old_calls[call] - new_calls.get(call, 0)}
                    for call in sorted(old_calls) if old_calls[call] > new_calls.get(call, 0)]
        appeared = [{"call": call, "count": new_calls[call] - old_calls.get(call, 0)}
                    for call in sorted(new_calls) if new_calls[call] > old_calls.get(call, 0)]
        moved.append({"corpus": key[0], "mode": key[1], "role": key[2], "jar": key[3],
                      "base_verdict": (left or {}).get("verdict", "(absent)"),
                      "merged_verdict": (right or {}).get("verdict", "(absent)"),
                      "base_unknown": (left or {}).get("unknown"), "merged_unknown": (right or {}).get("unknown"),
                      "attribution": "", "vanished": vanished, "appeared": appeared})
    return moved


def flaky_rows(flaky: dict[tuple[str, str, str, str], list[dict]]) -> list[dict]:
    return [{"corpus": key[0], "mode": key[1], "role": key[2], "jar": key[3], "runs": runs}
            for key, runs in sorted(flaky.items())]


def compare_payloads(base: dict, merged: dict, repetitions: dict[str, list[dict]], role: str) -> dict:
    all_flaky: dict[tuple[str, str, str, str], list[dict]] = {}
    for payloads in repetitions.values():
        all_flaky.update(differing_rows(payloads, role))
    excluded = set(all_flaky)
    return {"base": {"sha": base.get("sha"), "totals": totals(base, role, excluded)},
            "merged": {"sha": merged.get("sha"), "totals": totals(merged, role, excluded)},
            "moved": moved_jars(base, merged, role, excluded), "flaky": flaky_rows(all_flaky)}


def markdown(report: dict) -> str:
    lines = [f"Base SHA: `{report['base']['sha']}`", f"Merged SHA: `{report['merged']['sha']}`", "",
             "## Unknown totals (flaky rows excluded)", "", "| Corpus | Mode | Role | Base unknown | Merged unknown | Base verdicts | Merged verdicts |", "|---|---|---|---:|---:|---|---|"]
    groups = sorted(set(report["base"]["totals"]) | set(report["merged"]["totals"]))
    for group in groups:
        left, right = report["base"]["totals"].get(group, {}), report["merged"]["totals"].get(group, {})
        source = left or right
        def labels(data: dict) -> str:
            return ", ".join(f"{key}={data[key]}" for key in sorted(data) if key not in {"corpus", "mode", "role", "jars", "unknown_total"}) or "—"
        lines.append(f"| {source.get('corpus', group)} | {source.get('mode', '—')} | {source.get('role', '—')} | {left.get('unknown_total', 0)} | {right.get('unknown_total', 0)} | {labels(left)} | {labels(right)} |")
    lines += ["", "## Deterministic changed rows", "", "Every row whose verdict, unknown count or counted calls changed (flaky rows excluded). Vanished/appeared are per-call count deltas.", "", "| Corpus | Mode | Role | Jar | Base → merged | Unknown | Vanished calls | Appeared calls | EXACT/SILENT attribution |", "|---|---|---|---|---|---|---|---|---|"]
    for item in report["moved"]:
        def calls(values: list[dict]) -> str:
            return "<br>".join(f"`{entry['call']}` ({entry['count']})" for entry in values) or "—"
        lines.append(f"| {item['corpus']} | {item['mode']} | {item['role']} | `{item['jar']}` | {item['base_verdict']} → {item['merged_verdict']} | {item['base_unknown']} → {item['merged_unknown']} | {calls(item['vanished'])} | {calls(item['appeared'])} | {item['attribution']} |")
    if not report["moved"]:
        lines.append("| — | — | — | — | — | — | — | — | — |")
    lines += ["", "## Flaky rows", ""]
    if report["flaky"]:
        lines += ["| Corpus | Mode | Role | Jar | Repetitions |", "|---|---|---|---|---|"]
        for item in report["flaky"]:
            lines.append(f"| {item['corpus']} | {item['mode']} | {item['role']} | `{item['jar']}` | {len(item['runs'])} |")
    else:
        lines.append("None.")
    return "\n".join(lines) + "\n"


def parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--base", default="origin/main")
    p.add_argument("--head", required=True, help="ref or pull request number")
    p.add_argument("--target-root", type=Path, default=DEFAULT_TARGET_ROOT)
    p.add_argument("--corpus", action="append", required=True, metavar="NAME=DIR")
    p.add_argument("--fabric-loader", type=Path)
    p.add_argument("--provider-dir", type=Path, action="append", default=[])
    p.add_argument("--role", choices=("server", "client", "both"), default="both")
    p.add_argument("--dependency-mode", choices=("both", "alone", "closure"), default="both")
    p.add_argument("--jobs", type=int, default=4)
    p.add_argument("--repeat", type=int, default=2)
    p.add_argument("--json", type=Path)
    p.add_argument("--markdown", type=Path)
    return p


def build_probe(checkout: Path, target: Path, output: Path) -> None:
    target.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy(); env["CARGO_TARGET_DIR"] = str(target)
    command = ["cargo", "build", "--locked", "--profile", "fast", "-p", "horizon-native-java", "--bin", "horizon-native-java"]
    subprocess.run(command, cwd=checkout, env=env, check=True)
    binary = target / "fast" / ("horizon-native-java.exe" if os.name == "nt" else "horizon-native-java")
    if not binary.is_file():
        raise SystemExit(f"cargo did not produce {binary}")
    shutil.copy2(binary, output)
    output.chmod(output.stat().st_mode | 0o111)


def run_census(base_checkout: Path, probe: Path, args: argparse.Namespace, role: str, path: Path) -> dict:
    command = [sys.executable, str(base_checkout / "tools" / "unmodelled_calls.py"), "--probe", str(probe), "--json", str(path), "--jobs", str(args.jobs), "--role", role, "--dependency-mode", args.dependency_mode]
    for corpus in args.corpus:
        command += ["--corpus", corpus]
    if args.fabric_loader:
        command += ["--fabric-loader", str(args.fabric_loader)]
    for provider in args.provider_dir:
        command += ["--provider-dir", str(provider)]
    subprocess.run(command, cwd=base_checkout, check=True)
    return json.loads(path.read_text())


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.repeat < 1:
        raise SystemExit("--repeat must be at least 1")
    repo = ROOT
    # Compare against the CURRENT main, not whatever this clone last fetched.
    subprocess.run(["git", "fetch", "origin"], cwd=repo, check=False)
    base_ref, base_sha = resolve_head(repo, args.base)
    head_ref, head_sha = resolve_head(repo, args.head)
    print(f"Base SHA: {base_sha}")
    print(f"Head SHA: {head_sha}")
    root = args.target_root.expanduser().resolve(); root.mkdir(parents=True, exist_ok=True)
    wt_root = Path.home() / "ava" / "wt" / f"census-compare-run-{os.getpid()}"; wt_root.mkdir(parents=True, exist_ok=False)
    worktrees: list[Path] = []
    def cleanup() -> None:
        for worktree in reversed(worktrees):
            subprocess.run(["git", "worktree", "remove", "--force", str(worktree)], cwd=repo, check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        shutil.rmtree(wt_root, ignore_errors=True)
    atexit.register(cleanup)
    def interrupted(signum, _frame):
        cleanup(); raise SystemExit(128 + signum)
    signal.signal(signal.SIGINT, interrupted); signal.signal(signal.SIGTERM, interrupted)
    base_checkout, merged_checkout = wt_root / "base", wt_root / "merged"
    run(["git", "worktree", "add", "--detach", str(base_checkout), base_sha], cwd=repo); worktrees.append(base_checkout)
    run(["git", "worktree", "add", "--detach", str(merged_checkout), base_sha], cwd=repo); worktrees.append(merged_checkout)
    merge = subprocess.run(["git", "merge", "--no-edit", head_ref], cwd=merged_checkout, text=True, capture_output=True)
    if merge.returncode:
        print("error: PR must merge current main first; merge conflict in temporary worktree", file=sys.stderr)
        print(merge.stdout + merge.stderr, file=sys.stderr)
        return 2
    merged_sha = git(merged_checkout, "rev-parse", "HEAD")
    print(f"Merged SHA: {merged_sha}")
    base_probe, merged_probe = root / "probe-base", root / "probe-merged"
    build_probe(base_checkout, root / "base", base_probe)
    build_probe(merged_checkout, root / "merged", merged_probe)
    roles = ["server", "client"] if args.role == "both" else [args.role]
    reports: dict[str, dict] = {}
    output = ""
    for role in roles:
        repetitions: dict[str, list[dict]] = {"base": [], "merged": []}
        for label, probe in (("base", base_probe), ("merged", merged_probe)):
            for number in range(args.repeat):
                # Both probes run under the BASE checkout's driver, so the flags mean the same thing.
                path = root / f"{role}-{label}-{number}.json"
                payload = run_census(base_checkout, probe, args, role, path)
                payload["sha"] = base_sha if label == "base" else merged_sha
                repetitions[label].append(payload)
        report = compare_payloads(repetitions["base"][0], repetitions["merged"][0], repetitions, role)
        report["head_sha"] = head_sha
        report["merged_sha"] = merged_sha
        reports[role] = report
        output += f"# Role: {role}\n\n" + markdown(report) + "\n"
    print(output, end="")
    if args.json:
        args.json.write_text(json.dumps({"head_sha": head_sha, "merged_sha": merged_sha, "roles": reports}, indent=2) + "\n")
    if args.markdown:
        args.markdown.write_text(output)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
