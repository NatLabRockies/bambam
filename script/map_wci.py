"""Create interactive WCI maps from two edge CSV files."""

# Usage:
#   1. Run merge_wci.py to generate a CSV containing WCI scores and geometry.
#   2. Generate an interactive map:
#        python wci_map.py --input merged_wci.csv --output wci_map.html
#   3. Open the resulting HTML file in a browser to explore WCI scores.
#
# The input CSV must contain a 'linestring' WKT geometry column and WCI fields
# such as wci_total, wci_walk, wci_speed, wci_cycle, and wci_signal.

from __future__ import annotations

import argparse
from pathlib import Path

import geopandas as gpd
import pandas as pd


def load_wci_gdf(path: Path) -> gpd.GeoDataFrame:
    """Load an OSM edges CSV containing WKT linestring geometry."""
    df = pd.read_csv(path)

    if "linestring" not in df.columns:
        raise ValueError(f"{path} does not contain a 'linestring' column")

    return gpd.GeoDataFrame(
        df,
        geometry=gpd.GeoSeries.from_wkt(df["linestring"]),
        crs="EPSG:4326",
    ).drop(columns="linestring")


def create_wci_map(
    gdf: gpd.GeoDataFrame,
    output: Path,
    title: str,
) -> None:
    """Create and save an interactive WCI map."""
    m = gdf.explore(
        column="wci_total",
        cmap="viridis_r",
        tiles="CartoDB positron",
        legend=True,
        tooltip=[
            "name",
            "highway",
            "wci_total",
            "wci_walk",
            "wci_speed",
            "wci_cycle",
            "wci_signal"
        ],
    )

    m.save(output)
    print(f"Saved {output}")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Generate interactive WCI comparison maps."
    )

    parser.add_argument(
        "--input",
        type=Path,
        required=True,
        help="CSV containing WCI scores",
    )

    parser.add_argument(
        "--output",
        type=Path,
        default=Path("wci_map.html"),
        help="Output HTML for map",
    )

    return parser.parse_args()


def main() -> None:
    args = parse_args()

    gdf = load_wci_gdf(args.input)

    create_wci_map(
        gdf,
        args.output,
        "WCI",
    )

if __name__ == "__main__":
    main()