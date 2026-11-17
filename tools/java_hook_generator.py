#!/usr/bin/env python3
"""Generate the reviewed Lane 1 Sunrise listener manifest.

The mapping is the review authority. `functions.json` and `points.json` are build artifacts from
one pinned Sunrise checkout; a point row is emitted only when both indexes contain its exact Rust
path and the point has no API refusal. Hook rows retain the reviewed difference/evidence text.
"""
from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any


FUNCTION_KEYS = {"module", "file", "span", "signature_hash", "sha256"}
POINT_KEYS = {"api_refusal", "args", "at_return", "host", "listener", "name", "raw", "raw_shape", "record", "return_listener", "return_ty", "returns", "side", "target"}


def _header(value: Any, keys: set[str], name: str) -> dict[str, Any]:
    if not isinstance(value, dict) or set(value) != keys:
        raise ValueError(f"{name} must have exactly {sorted(keys)}")
    if value.get("schema") != 1 or not isinstance(value.get("build"), dict):
        raise ValueError(f"{name} must have schema=1 and a build object")
    if not isinstance(value["build"].get("commit"), str):
        raise ValueError(f"{name}.build.commit must be a string")
    return value


def _function_index(value: Any) -> dict[str, dict[str, Any]]:
    value = _header(value, {"schema", "build", "crates", "functions", "refused"}, "functions.json")
    if not isinstance(value["functions"], dict) or not isinstance(value["refused"], dict):
        raise ValueError("functions.json functions/refused must be objects")
    result = {}
    for path, item in value["functions"].items():
        if not isinstance(path, str) or not isinstance(item, dict) or set(item) != FUNCTION_KEYS:
            raise ValueError(f"functions.json.functions[{path!r}] has the wrong shape")
        result[path] = item
    return result


def _point_index(value: Any) -> dict[str, dict[str, Any]]:
    value = _header(value, {"schema", "build", "points"}, "points.json")
    if not isinstance(value["points"], list):
        raise ValueError("points.json points must be an array")
    result = {}
    for item in value["points"]:
        if not isinstance(item, dict) or set(item) != POINT_KEYS or not isinstance(item.get("target"), str):
            raise ValueError("points.json point has the wrong shape")
        result[item["target"]] = item
    return result


def _phases(anchor: str) -> list[tuple[str, str | None]]:
    raw, _, note = anchor.partition(";")
    raw = raw.strip().upper()
    note = note.strip()
    if note and any(word in note.lower() for word in ("refusal", "no row", "unavailable")):
        return []
    if raw == "HEAD":
        return [("head", note or None)]
    if raw == "RETURN":
        return [("return", note or None)]
    if raw == "TAIL":
        return [("return", "TAIL mapped to Sunrise RETURN" + (f"; {note}" if note else ""))]
    if raw in {"HEAD/RETURN", "RETURN/HEAD", "HEAD/TAIL", "RETURN/TAIL"}:
        return [("head", note or None), ("return", note or None)]
    return []


def _slug(vanilla: str, phase: str) -> str:
    value = re.sub(r"[^A-Za-z0-9]+", "_", vanilla).strip("_").lower()
    return f"java_{value}_{phase}"


def _method(row: dict[str, Any]) -> tuple[str, str]:
    vanilla = str(row["vanilla"])
    owner, name = vanilla.split("#", 1)
    return f"{owner}#{name}", str(row["descriptor"])


def _fit(row: dict[str, Any]) -> str:
    return str(row.get("fit", "")).upper()


def _is_no_fit(row: dict[str, Any]) -> bool:
    return "NO FIT" in _fit(row)


def _point_candidate(row: dict[str, Any]) -> bool:
    return "::" in str(row.get("sunrise", "")) and bool(_phases(str(row.get("anchor", ""))))


def _candidate_order(row: dict[str, Any]) -> tuple[int, int]:
    # Prefer a validated API point over a coarse existing hook, then exact review fits.
    return (0 if _point_candidate(row) else 1, 0 if _fit(row) == "EXACT FIT" else 1)


def generate(mapping: Any, functions: Any, points: Any) -> dict[str, Any]:
    rows = mapping.get("rows", []) if isinstance(mapping, dict) else mapping
    function_index = _function_index(functions)
    point_index = _point_index(points)
    groups: dict[tuple[str, str], list[dict[str, Any]]] = {}
    for row in rows if isinstance(rows, list) else []:
        if not isinstance(row, dict) or _is_no_fit(row):
            continue
        groups.setdefault(_method(row), []).append(row)

    inject: list[dict[str, Any]] = []
    hooks: list[dict[str, Any]] = []
    errors: list[str] = []
    for candidates in groups.values():
        selected = None
        for row in sorted(candidates, key=_candidate_order):
            target = str(row.get("sunrise", ""))
            phases = _phases(str(row.get("anchor", "")))
            if "::" in target and not phases:
                errors.append(f"invalid point anchor: {row.get('anchor')}")
                continue
            if not _point_candidate(row):
                selected = ("hook", row, None)
                break
            if target not in function_index:
                errors.append(f"missing function target: {target}")
                continue
            point = point_index.get(target)
            if point is None:
                errors.append(f"missing point target: {target}")
                continue
            refusal = point.get("api_refusal")
            if refusal not in (None, ""):
                errors.append(f"refused point {target}: {refusal}")
                continue
            selected = ("inject", row, point)
            break
        if selected is None:
            continue
        kind, row, point = selected
        method, descriptor = _method(row)
        common = {
            "method": method,
            "descriptor": descriptor,
            "fit": _fit(row),
            "evidence": str(row.get("evidence", "")),
        }
        if kind == "inject":
            signature = function_index[str(row["sunrise"])].get("signature_hash") or point.get("signature_hash") or point.get("hash")
            for phase, note in _phases(str(row["anchor"])):
                entry = {**common, "id": _slug(row["vanilla"], phase), "at": str(row["sunrise"]), "kind": phase, "with": _slug(row["vanilla"], phase)}
                if signature:
                    entry["hash"] = signature
                if note or common["fit"] != "EXACT FIT":
                    entry["note"] = note or common["evidence"]
                inject.append(entry)
        else:
            hooks.append({**common, "id": _slug(row["vanilla"], "hook"), "target": str(row["sunrise"]), "note": common["evidence"]})

    inject.sort(key=lambda item: item["id"])
    hooks.sort(key=lambda item: item["id"])
    return {"inject": inject, "hooks": hooks, "errors": sorted(set(errors))}


def _toml_string(value: Any) -> str:
    return json.dumps(str(value), ensure_ascii=False)


def render(plan: dict[str, Any], source: str) -> str:
    lines = [
        "# Generated by tools/java_hook_generator.py; do not edit.",
        f"source = {_toml_string(source)}",
        f"inject_count = {len(plan['inject'])}",
        f"hook_count = {len(plan['hooks'])}",
    ]
    for section, entries in (("inject", plan["inject"]), ("hook", plan["hooks"])):
        for entry in entries:
            lines.append(f"\n[[{section}]]")
            for key, value in entry.items():
                lines.append(f"{key} = {_toml_string(value)}")
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mapping", type=Path)
    parser.add_argument("--functions", required=True, type=Path)
    parser.add_argument("--points", required=True, type=Path)
    parser.add_argument("-o", "--output", type=Path)
    args = parser.parse_args(argv)
    try:
        plan = generate(json.loads(args.mapping.read_text()), json.loads(args.functions.read_text()), json.loads(args.points.read_text()))
    except ValueError as error:
        print(f"java-hook-generator: {error}", file=sys.stderr)
        return 2
    if plan["errors"]:
        for error in plan["errors"]:
            print(f"java-hook-generator: {error}", file=sys.stderr)
        return 2
    text = render(plan, args.mapping.name)
    if args.output:
        args.output.write_text(text)
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
