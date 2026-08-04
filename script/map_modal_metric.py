"""Create interactive Modal Metric maps (e.g., WCI, LTS) from merged edge CSV files."""

# Usage:
#   1. Run merge_metric.py to generate a CSV containing OSM geometry and modal metric data.
#   2. Generate an interactive map:
#        python map_modal_metric.py --input edges-complete-with-metric.csv --output map.html
#      Or specify a metric column and color map explicitly:
#        python map_modal_metric.py --input merged_lts.csv --column lts --cmap RdYlGn_r --output lts_map.html
#   3. Open the resulting HTML file in a browser to explore scores.

from __future__ import annotations

import argparse
from pathlib import Path

import geopandas as gpd
import pandas as pd


# Known modal metric primary columns and their associated metric components
KNOWN_METRICS = {
    "wci_total": ["wci_total", "wci_walk", "wci_speed", "wci_cycle", "wci_signal"],
    "lts": ["lts"],
}


def load_metric_gdf(path: Path) -> gpd.GeoDataFrame:
    """Load an OSM edges CSV containing WKT linestring geometry."""
    df = pd.read_csv(path)

    if "linestring" not in df.columns:
        raise ValueError(f"{path} does not contain a 'linestring' column")

    return gpd.GeoDataFrame(
        df,
        geometry=gpd.GeoSeries.from_wkt(df["linestring"]),
        crs="EPSG:4326",
    ).drop(columns="linestring")


def detect_metric_column(df: pd.DataFrame, requested_column: str | None = None) -> str:
    """Detect or validate the primary modal metric column to visualize."""
    if requested_column:
        if requested_column not in df.columns:
            raise ValueError(f"Requested column '{requested_column}' not found in CSV.")
        return requested_column

    # Auto-detect known modal metric columns
    for primary_col in KNOWN_METRICS:
        if primary_col in df.columns:
            return primary_col

    # Fallback to any column starting with 'wci' or 'lts'
    fallback_cols = [c for c in df.columns if c.startswith(("wci", "lts"))]
    if fallback_cols:
        return fallback_cols[0]

    raise ValueError(
        "Could not auto-detect a modal metric column. "
        "Please specify one using the --column argument."
    )


def build_tooltip_columns(df: pd.DataFrame, primary_column: str) -> list[str]:
    """Construct a list of tooltip columns present in the GeoDataFrame."""
    # Standard edge metadata attributes if present
    base_attrs = [
        col
        for col in ["name", "highway", "cycleway", "oneway" "maxspeed", "maxspeed_raw"] 
        if col in df.columns
    ]

    # Find associated metric columns
    if primary_column in KNOWN_METRICS:
        metric_cols = [c for c in KNOWN_METRICS[primary_column] if c in df.columns]
    else:
        # Include any wci_ or lts_ prefixed columns if custom metric
        metric_cols = [
            c for c in df.columns if c.startswith(("wci_", "lts_")) or c in ("lts", "wci_total")
        ]

    # Combine unique columns preserving order
    tooltip_cols = base_attrs + [c for c in metric_cols if c not in base_attrs]
    return tooltip_cols


def create_modal_metric_map(
    gdf: gpd.GeoDataFrame,
    metric_column: str,
    output: Path,
    cmap: str = "viridis_r",
) -> None:
    """Create and save an interactive modal metric map."""
    tooltip_cols = build_tooltip_columns(gdf, metric_column)

    m = gdf.explore(
        column=metric_column,
        cmap=cmap,
        tiles="CartoDB positron",
        legend=True,
        tooltip=tooltip_cols,
    )

    m.save(output)
    print(f"Saved {output} (visualizing column '{metric_column}')")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate interactive modal metric (WCI, LTS, etc.) maps."
    )

    parser.add_argument(
        "--input",
        type=Path,
        required=True,
        help="CSV containing geometry and modal metric scores",
    )

    parser.add_argument(
        "--column",
        type=str,
        default=None,
        help="Primary metric column to map (auto-detected if omitted, e.g. wci_total, lts)",
    )

    parser.add_argument(
        "--cmap",
        type=str,
        default="viridis_r",
        help="Colormap for the interactive map (default: viridis_r)",
    )

    parser.add_argument(
        "--output",
        type=Path,
        default=Path("modal_metric_map.html"),
        help="Output HTML file path for map",
    )

    return parser.parse_args()


def main() -> None:
    args = parse_args()

    gdf = load_metric_gdf(args.input)
    target_column = detect_metric_column(gdf, args.column)

    create_modal_metric_map(
        gdf=gdf,
        metric_column=target_column,
        output=args.output,
        cmap=args.cmap,
    )


if __name__ == "__main__":
    main()