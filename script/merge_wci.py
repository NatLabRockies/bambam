#!/usr/bin/env python3
"""
Merges wci scores with edge-complete file
"""
from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

csv.field_size_limit(sys.maxsize)


def read_wci_rows(path: Path) -> tuple[list[str], list[list[str]]]:
    """Read a CSV containing WCI scores.

    Expected format:

    total_score,walk_score,traffic_speed_score,cycle_score,traffic_signal_score
    2,-2,2,0,2
    ...
    """
    with path.open("r", newline="", encoding="utf-8") as f:
        reader = csv.reader(f)
        header = next(reader)
        rows = [row for row in reader]

    return header, rows


def merge_wci(
    edges_csv: Path,
    wci_csv: Path,
    output_csv: Path,
) -> None:
    with edges_csv.open("r", newline="", encoding="utf-8") as f:
        edge_rows = list(csv.reader(f))

    if not edge_rows:
        raise ValueError(f"{edges_csv} is empty")

    edge_header = edge_rows[0]
    edge_data = edge_rows[1:]

    wci_header, wci_rows = read_wci_rows(wci_csv)

    if len(edge_data) != len(wci_rows):
        raise ValueError(
            f"Row count mismatch:\n"
            f"edges: {len(edge_data)}\n"
            f"wci:   {len(wci_rows)}"
        )

    with output_csv.open("w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(edge_header + wci_header)

        for edge, wci in zip(edge_data, wci_rows):
            writer.writerow(edge + wci)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()

    parser.add_argument("--edges", type=Path, required=True)
    parser.add_argument("--wci", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)

    return parser.parse_args()


def main() -> None:
    args = parse_args()

    merge_wci(
        args.edges.resolve(),
        args.wci.resolve(),
        args.output.resolve(),
    )

    print(f"Wrote {args.output}")


if __name__ == "__main__":
    main()