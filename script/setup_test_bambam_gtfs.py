import os
import json
import argparse
import osmnx as ox
import pandas as pd
import geopandas as gpd
from pathlib import Path
from shapely.geometry import Point, LineString
from nrel.routee.compass.io import generate_compass_dataset

from transit_output_extract_geojson import process_csv_into_geometry

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Setup GTFS bambam test.")
    parser.add_argument("dir", type=Path, help="Path to working directory (parent of gtfs folder)")
    parser.add_argument("--hull_buffer", default=325, type=float, help="Buffer (in meters) for the convex hull of stop points")
    parser.add_argument("--output_geometries", action="store_true", help="If set, store geometries for stop locations, hull, and network edges and vertices as geojson objects.")
    args = parser.parse_args()
    
    os.makedirs(f"{args.dir}/geometries", exist_ok=True)

    print("Reading stops.txt into a geodataframe")
    raw_df = pd.read_csv(f"{args.dir}/gtfs/stops.txt", sep=",")
    gdf = gpd.GeoSeries(raw_df.apply(lambda r: Point(r.stop_lon, r.stop_lat), axis=1), crs="EPSG:4326")

    print("Estimate UTM CRS and compute buffered convex hull")
    utm_crs = gdf.estimate_utm_crs()
    hull_geometry = gdf.to_crs(utm_crs).geometry.union_all().convex_hull.buffer(args.hull_buffer)
    hull_gdf = gpd.GeoDataFrame(geometry=[hull_geometry], crs=utm_crs).to_crs("EPSG:4326")

    print("Extract convex hull as extent")
    with open(f"{args.dir}/geometries/query.json", "w") as f:
        json.dump({"extent": hull_gdf.iloc[0].geometry.wkt}, f, indent=2)

    print("Download osmnx graph")
    g = ox.graph_from_polygon(hull_gdf.geometry.iloc[0], network_type="drive")
    generate_compass_dataset(g, output_directory=f"{args.dir}/compass")

    if args.output_geometries:
        print("Writing geometries to `geometries` folder")
        gdf.to_file(f"{args.dir}/geometries/stops.geojson", driver="GeoJSON")
        hull_gdf.to_file(f"{args.dir}/geometries/hull.geojson", driver="GeoJSON")
        process_csv_into_geometry(f'{args.dir}/compass/edges-compass.csv.gz', f'{args.dir}/compass/vertices-compass.csv.gz', f"{args.dir}/geometries", "-compass", output_vertices=True)