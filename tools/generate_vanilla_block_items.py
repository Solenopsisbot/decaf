#!/usr/bin/env python3
"""Generate vanilla Block.asItem ids from a Java 25 26.3 oracle run.

The input is the oracle's logger output.  The oracle must have run
SharedConstants.tryDetectVersion(), Bootstrap.bootStrap(), then enumerated
public Blocks fields and printed FIELD\\tBLOCK_ID\\tITEM_ID after
Block.asItem() had observed Items.BY_BLOCK.
"""

from __future__ import annotations

import argparse
import re
from pathlib import Path


def rows(path: Path) -> list[tuple[str, str]]:
    found: list[tuple[str, str]] = []
    for line in path.read_text().splitlines():
        payload = line.rsplit(": ", 1)[-1]
        parts = payload.split(r"\t")
        if len(parts) != 3 or not re.fullmatch(r"[A-Z0-9_]+", parts[0]):
            continue
        found.append((parts[1], parts[2]))
    if len(found) != 878 or len({block for block, _ in found}) != 878:
        raise SystemExit(f"expected 878 unique block rows, found {len(found)}")
    return sorted(found)


def render(entries: list[tuple[str, str]]) -> str:
    lines = [
        "//! Generated from a Java 25 run of the real 26.3 Item.BY_BLOCK table.",
        "//! Do not derive this by lowercasing block names: aliases and AIR entries are intentional.",
        "",
        "/// The item id returned by vanilla Block.asItem() for a registered block id.",
        "pub(crate) fn item_id(block: &str) -> Option<&'static str> {",
        "    TABLE",
        "        .binary_search_by(|(id, _)| id.cmp(&block))",
        "        .ok()",
        "        .map(|index| TABLE[index].1)",
        "}",
        "",
        "#[rustfmt::skip]",
        "const TABLE: &[(&str, &str)] = &[",
    ]
    lines.extend(f'    ("{block}", "{item}"),' for block, item in entries)
    lines.extend(["];", ""])
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("oracle", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    args.output.write_text(render(rows(args.oracle)))


if __name__ == "__main__":
    main()
