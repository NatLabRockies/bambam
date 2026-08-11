"""
Merges modal metric scores (e.g., WCI, LTS) with an edges-complete.csv file.
"""

# Usage:
#   Merge modal metric scores (WCI, LTS, etc.) with an OSM edges-complete CSV:
#
#       python merge_metric.py \
#           --edges edges-complete.csv \
#           --metric metric-output-file.csv \
#           --output edges-complete-with-metric.csv
#
# The input files must have matching row order and row counts:
#   - --edges: edge network CSV containing geometry and OSM edge attributes
#   - --metric: modal metric score CSV produced by bulk_compute_modal_metric
#   - --output: combined CSV output

from __future__ import annotations

import argparse
import csv
import sys
from pathlib import Path

csv.field_size_limit(sys.maxsize)


def read_metric_rows(path: Path) -> tuple[list[str], list[list[str]]]:
    """Read a CSV containing modal metric outputs (e.g., WCI, LTS)."""
    with path.open("r", newline="", encoding="utf-8") as f:
        reader = csv.reader(f)
        try:
            header = next(reader)
        except StopIteration:
            raise ValueError(f"{path} is empty")
        rows = [row for row in reader]

    return header, rows


def merge_modal_metric(
    edges_csv: Path,
    metric_csv: Path,
    output_csv: Path,
) -> None:
    """Combine edge geometry and attributes with modal metric output rows."""
    with edges_csv.open("r", newline="", encoding="utf-8") as f:
        edge_rows = list(csv.reader(f))

    if not edge_rows:
        raise ValueError(f"{edges_csv} is empty")

    edge_header = edge_rows[0]
    edge_data = edge_rows[1:]

    metric_header, metric_rows = read_metric_rows(metric_csv)

    if len(edge_data) != len(metric_rows):
        raise ValueError(
            f"Row count mismatch:\n"
            f"edges:  {len(edge_data)}\n"
            f"metric: {len(metric_rows)}"
        )

    with output_csv.open("w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f)
        writer.writerow(edge_header + metric_header)

        for edge, metric in zip(edge_data, metric_rows):
            writer.writerow(edge + metric)


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Merge modal metric outputs with edges-complete.csv"
    )

    parser.add_argument(
        "--edges",
        type=Path,
        required=True,
        help="Path to edge network CSV (edges-complete.csv)",
    )
    parser.add_argument(
        "--metric",
        dest="metric",
        type=Path,
        required=True,
        help="Path to modal metric CSV file output by bulk_compute_modal_metric",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Path to output combined CSV",
    )

    return parser.parse_args()


def main() -> None:
    args = parse_args()

    merge_modal_metric(
        args.edges.resolve(),
        args.metric.resolve(),
        args.output.resolve(),
    )

    print(f"Wrote {args.output}")


if __name__ == "__main__":
    main()