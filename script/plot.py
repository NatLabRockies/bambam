# plot.py
# plot outputs of a BAMBAM run, assuming a result.csv file generated based on default output arguments.

from typing import Optional
from pathlib import Path
import argparse

import h3
import pandas as pd
import geopandas as gpd
from shapely import wkt, wkb, Polygon
import matplotlib.pyplot as plt
import seaborn as sns
sns.set_theme()
import contextily as ctx
from tqdm import tqdm

default_modes = "walk,bike,drive,transit"
default_acts = "retail,entertainment,services,food,healthcare,jobs"
default_timebins = "10,20,30,40"

parser = argparse.ArgumentParser("BAMBAM CSV output plots")
parser.add_argument("infile", default="result.csv", help="CSV output of BAMBAM run with default CSV output policy")
parser.add_argument("outdir", default="", help="directory to write plots")
parser.add_argument("--plot-isochrones", action="store_true", help="plot each isochrone for each grid_id/mode combination")
parser.add_argument("--plot-activities", action="store_true", help="plot each mode/activity distribution over the study grid")
parser.add_argument("--modes", default=default_modes, help="comma-delimited list of mode names")
parser.add_argument("--activities", default=default_acts, help="comma-delimited list of activity names")
parser.add_argument("--timebins", default=default_timebins, help="comma-delimited list of time bins")

def act_plot(gdf: gpd.GeoDataFrame, mode: str, act: str, timebin: int, outdir: Path) -> None:
    col = f"opps_{act}_{timebin}"
    mode_gdf = gdf[gdf["mode"]==mode]
    fig, ax = plt.subplots()
    mode_gdf.to_crs("EPSG:3857").plot(col, cmap="summer_r", alpha=0.667, linewidth=0.0, legend=True, ax=ax)
    ctx.add_basemap(ax=ax)
    ax.set_xticks([])
    ax.set_yticks([])
    fig.savefig(outdir / f"plot_opps_{mode}_{act}_{timebin}.png", dpi=300, bbox_inches="tight")
    plt.close()
    return

def runtimes_plot(df: pd.DataFrame, outdir: Path) -> None:
    ax = sns.scatterplot(df, x="iterations", y="runtime", hue="mode")
    ax.figure.savefig(outdir / "plot_runtime_per_iteration.png", dpi=300, bbox_inches="tight")
    plt.close()
    return

def isochrone_plot(iso_df: pd.DataFrame, grid_id: str, mode: str, timebins: list[int]) -> None:
    row = iso_df.loc[grid_id, mode]
    geoms = [wkb.loads(row[f"isochrone_{t}"])
             for t in timebins]
    iso_gdf = gpd.GeoDataFrame({"geometry": geoms, "timebin": timebins}, crs="EPSG:4326").sort_values("timebin", ascending=False)
    fig, ax = plt.subplots()
    iso_gdf.to_crs("EPSG:3857").plot("timebin", cmap="bone_r", alpha=0.444, linewidth=1.0, edgecolor="k", legend=True, ax=ax)
    ctx.add_basemap(ax=ax)
    ax.set_xticks([])
    ax.set_yticks([])
    fig.savefig(outdir / f"plot_isos_{grid_id}_{mode}.png", dpi=300, bbox_inches="tight")
    plt.close()

def to_polygon(hex_id: str) -> Polygon:
    pts = h3.cell_to_boundary(hex_id)
    return Polygon(list(map(lambda p: (p[1], p[0]), pts)))

if __name__ == "__main__":
    args = parser.parse_args()
    
    modes = list(args.modes.split(","))
    acts = list(args.activities.split(","))
    timebins = list(map(lambda t: int(t), args.timebins.split(",")))
    outdir = Path(args.outdir)
    if not outdir.exists():
        outdir.mkdir(parents=True)

    df = pd.read_csv(args.infile)
    df["geometry"] = df.grid_id.apply(to_polygon)
    gdf = gpd.GeoDataFrame(df, crs="EPSG:4326")

    runtimes_plot(df, outdir)

    if args.plot_activities:

        plot_triples = [(mode, act, timebin)
                    for mode in modes
                    for act in acts
                    for timebin in timebins]
        
        for (mode, act, timebin) in tqdm(plot_triples, desc="mode/activity plots"):
            act_plot(gdf, mode, act, timebin, outdir)

    if args.plot_isochrones:
        iso_df = df.copy().set_index(["grid_id", "mode"])
        grid_ids = df.grid_id.unique()

        plot_tuples = [(grid_id, mode)
                        for grid_id in grid_ids
                        for mode in modes]

        for (grid_id, mode) in tqdm(plot_tuples, desc="isochrone plots"):
            isochrone_plot(iso_df, grid_id, mode, timebins)
