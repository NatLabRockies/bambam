"""
This script consists of functions to prepare and integrate the OSM network and 
Tomtom network.

Author: Sailesh Acharya, Guangchen Zhao.

Last update: 2025-3-19
"""

# import modules
import geopandas as gpd
import numpy as np
import math
import pandas as pd
import os
from shapely.wkt import loads
from dbfread import DBF
import glob
from geopandas.tools import sjoin
import sys
import osmnx as ox


# function to obatin city boundary
def get_city_boundary(
    city_name, csv_directory, buffer_boundary
):
    """
    function to get city boundary from directory 
    """
    
    # read the CSV
    file_path = os.path.join(csv_directory, f"{city_name}.city_grid.csv")
    city_grid_df = pd.read_csv(file_path)
    
    # parse the WKT geometry and create a GeoDataFrame
    city_grid_df["geom"] = city_grid_df["geom"].apply(loads)
    city_grid = gpd.GeoDataFrame(city_grid_df, geometry="geom", crs="EPSG:4326")
    
    # obain city boundary
    city_boundary = city_grid["geom"].unary_union

    # buffer the boundary
    if buffer_boundary == True:
        # boundary
        boundary = gpd.GeoSeries([city_boundary])
        boundary.set_crs("EPSG:4326", inplace=True)

        # project to EPSG:3857
        boundary = boundary.to_crs("EPSG:3857")

        # buffer the geometry by 50 miles
        boundary = boundary.buffer(50 * 1609.34)

        # re-project back to EPSG:4326
        boundary = boundary.to_crs("EPSG:4326")

        # output the buffered geometry
        boundary = boundary.geometry.values[0]
    else:
        boundary = city_boundary

    # return the boundary
    return boundary


# function to prepare osm network
def prepare_osm_network(
    city_name, csv_directory, buffer_boundary
):
    """
    function to prepare osm data
    """

    # import osm network
    folder_path = f"../OSM network/{city_name}/"
    network_path = f"{folder_path}/{city_name}_OSM_missing_speeds.csv"
    # folder_path = f"../OSM network/{city_name}_wd_am/" # d_h
    # network_path = f"{folder_path}/{city_name}_wd_am_OSM_missing_speeds.csv" # d_h
    osm_network = pd.read_csv(
        network_path,
        engine="python",
        encoding="latin1",
    )

    # osm network as gpd
    osm_network["geometry"] = osm_network["wkt"].apply(loads)
    osm_network = gpd.GeoDataFrame(osm_network, geometry="geometry")
    osm_network = osm_network.set_crs("EPSG:4326")

    # keep osm network within city boundary
    boundary = get_city_boundary(
        city_name, csv_directory, buffer_boundary
    )
    boundary = gpd.GeoDataFrame(geometry=[boundary], crs="EPSG:4326")
    osm_network = sjoin(osm_network, boundary, how="inner")

    # keep drive eligible links only
    osm_network = osm_network[osm_network["drive-mode"] == True]

    # keep the first value of drive eligible highway class
    drive_highway = [
        "motorway_link",
        "motorway",
        "trunk_link",
        "trunk",
        "primary",
        "primary_link",
        "secondary",
        "secondary_link",
        "tertiary",
        "tertiary_link",
        "residential",
        "service",
        "track",
        "road",
        "unclassified",
    ]
    osm_network["highway"] = osm_network.apply(
        lambda x: next((key for key in drive_highway if key in str(x)), None), axis=1
    )

    # update lanes column
    # keeping first values if multiple exists
    osm_network["lanes"] = osm_network["lanes"].replace(np.nan, "None")
    osm_network["lanes"] = osm_network["lanes"].apply(
        lambda x: int(x.split()[0]) if isinstance(x, str) and " " in x else x
    )
    osm_network["lanes"] = osm_network["lanes"].replace("None", np.nan)

    # keep required columns only
    osm_network = osm_network[
        ["id", "lanes", "highway", "one-way", "speed-kph", "geometry"]
    ]

    return osm_network


# function to prepare Tomtom network
def prepare_tomtom_network(
    city_name, csv_directory, buffer_boundary
):
    """
    function to prepare Tomtom data
    """
    # import boundary
    boundary = get_city_boundary(
        city_name, csv_directory, buffer_boundary
    )
    boundary = gpd.GeoDataFrame(geometry=[boundary], crs="EPSG:4326")
    
    # import Tomtom network
    tomtom = gpd.GeoDataFrame()
    
    d_h_list = ["wd_am", "wd_af", "wd_pm", "we_am", "we_af", "we_pm"]
    
    for d_h in d_h_list:
        
        tomtom_d_h = gpd.GeoDataFrame()
        
        pattern = f"../network/{city_name}_{d_h}_*/"
    
        for folder_path in glob.glob(pattern):
            dbf_file = glob.glob(os.path.join(folder_path, f"{city_name}_*_0_Base Set_0.dbf"))
            network_file = os.path.join(folder_path, "network.shp")
        
            if dbf_file and os.path.exists(network_file):  # Ensure both files exist
                print(f"Reading files from folder: {folder_path}")
                dbf_file_path = dbf_file[0]  # There should be only one matching file
        
                # Read .dbf and network files
                speed_i = pd.DataFrame(iter(DBF(dbf_file_path)))
                network_i = gpd.read_file(network_file)
        
                # Merge the data
                join_i = pd.merge(
                    speed_i, network_i, left_on="BS_Id", right_on="Id", how="left"
                )
        
                # Collect results
                tomtom_d_h = pd.concat([tomtom_d_h, join_i], ignore_index=True)
            else:
                print(f"Missing files in folder: {folder_path}")
    
        tomtom_d_h = gpd.GeoDataFrame(tomtom_d_h, geometry="geometry")
        tomtom_d_h = tomtom_d_h.to_crs("EPSG:4326")
    
        # keep network links within city boundary
        tomtom_d_h = sjoin(tomtom_d_h, boundary, how="inner")
    
        # rename column names before merging them together
        d_h_1 = d_h.replace("_", "-")
        tomtom_d_h.rename(
            columns={
                "BS_Id": "tom-id",
                # "BS_AvgSp": f"mean-{d_h_1}",
                "BS_MedSp": f"med-{d_h_1}",
                # "BS_P85sp": f"85pr-{d_h_1}",
                "BS_Hits": f"prsz-{d_h_1}",
            },
            inplace=True,
        )
    
        # keep required columns only
        tomtom_d_h = tomtom_d_h[
            ["tom-id", 
             # f"mean-{d_h_1}", 
             f"med-{d_h_1}", 
             # f"85pr-{d_h_1}", 
             f"prsz-{d_h_1}", 
             "geometry"]
        ]
    
        # merge these files togther
        try:
            tomtom = tomtom.merge(tomtom_d_h, on=["tom-id", "geometry"], how="outer")
            print(f"{city_name}_{d_h} merged")
        except:
            tomtom = tomtom_d_h
            print(f"Starting with {city_name}_{d_h}")
    # convert to gpd
    tomtom = tomtom[[col for col in tomtom.columns if col != "geometry"] + ["geometry"]]
    tomtom = gpd.GeoDataFrame(tomtom, geometry="geometry")

    return tomtom


# function to join OSM and Tomtom networks
def join_osm_tomtom_networks(
    city_name, csv_directory, buffer_boundary
):
    """
    function to join OSM and Tomtom networks for a city
    """

    # import osm network
    print(f"Preparing OSM network for {city_name}.")
    osm = prepare_osm_network(
        city_name, csv_directory, buffer_boundary
    )

    # import tomtom network
    print(f"Preparing Tomtom network for {city_name}.")
    tom = prepare_tomtom_network(
        city_name, csv_directory, buffer_boundary
    )

    # begin joining osm and tomtom networks
    print(f"Joining networks for {city_name}.")

    # functions to calculate bearings, headings, and mean headings
    def calculate_bearing(p1, p2):
        lat1, lon1 = p1
        lat2, lon2 = p2
        delta_lon = lon2 - lon1
        x = math.cos(math.radians(lat2)) * math.sin(math.radians(delta_lon))
        y = math.cos(math.radians(lat1)) * math.sin(math.radians(lat2)) - math.sin(
            math.radians(lat1)
        ) * math.cos(math.radians(lat2)) * math.cos(math.radians(delta_lon))
        bearing = math.degrees(math.atan2(x, y))
        return bearing

    def calculate_heading(line):
        coordinates = line.coords
        headings = []
        for i in range(len(coordinates) - 1):
            p1 = coordinates[i]
            p2 = coordinates[i + 1]
            heading = calculate_bearing(p1, p2)
            headings.append(heading)
        return headings

    def calculate_mean_heading(headings):
        headings_rad = [math.radians(h) for h in headings]
        mean_heading_rad = math.atan2(
            sum([math.sin(h) for h in headings_rad]),
            sum([math.cos(h) for h in headings_rad]),
        )
        mean_heading_deg = math.degrees(mean_heading_rad)
        mean_heading_deg = mean_heading_deg % 360
        return mean_heading_deg

    # add columns of headings
    tom["head"] = tom["geometry"].apply(calculate_heading)
    osm["head"] = osm["geometry"].apply(calculate_heading)
    tom["mean-head"] = tom["head"].apply(calculate_mean_heading)
    osm["mean-head"] = osm["head"].apply(calculate_mean_heading)

    # set missing speeds as NA
    # missing speeds were coded -299 in osm query
    osm["speed-kph"] = np.where(osm["speed-kph"] == -299, np.nan, osm["speed-kph"])

    # speed in kph to mph
    osm["osm-sl-mph"] = osm["speed-kph"] / 1.609  # speed in mph

    # filter and rename column names from osm
    osm = osm[
        [
            "id",
            "lanes",
            "highway",
            "one-way",
            "osm-sl-mph",
            "geometry",
            "head",
            "mean-head",
        ]
    ]

    # set geometris to EPSG:3857
    osm.to_crs("EPSG:3857", inplace=True)
    tom.to_crs("EPSG:3857", inplace=True)

    # create a buffer of 5m on the sides of osm links
    osm_buffer = osm.copy()
    osm_buffer["geometry"] = osm_buffer["geometry"].buffer(5)

    # apportion the intersected buffers equally among intersected buffers
    # TODO

    # intersect osm and tomtom links
    net = gpd.overlay(osm_buffer, tom, how="intersection", keep_geom_type=False)
    del osm_buffer, tom

    # filter out matches with mean headings > 45
    net["diff-head"] = abs(net["mean-head_1"] - net["mean-head_2"])
    net = net[net["diff-head"] <= 45]

    # weighted average speeds by length of links within buffer
    net["length"] = net["geometry"].length
    columns = {
        # "mean-wd-am",
        "med-wd-am",
        # "85pr-wd-am",
        # "mean-wd-af",
        "med-wd-af",
        # "85pr-wd-af",
        # "mean-wd-pm",
        "med-wd-pm",
        # "85pr-wd-pm",
        # "mean-wd-nt",
        # "med-wd-nt",
        # "85pr-wd-nt",
        # "mean-we-am",
        "med-we-am",
        # "85pr-we-am",
        # "mean-we-af",
        "med-we-af",
        # "85pr-we-af",
        # "mean-we-pm",
        "med-we-pm",
        # "85pr-we-pm",
        # "mean-we-nt",
        # "med-we-nt",
        # "85pr-we-nt",
    }
    for col in columns:
        net[f"wt-{col}"] = (
            net[f"{col}"] * net["length"] / net.groupby("id")["length"].transform("sum")
        )
        net[f"{col}"] = net.groupby("id")[f"wt-{col}"].transform("sum")
        net = net.drop(columns=[f"wt-{col}"])
    net = net.drop(columns=["geometry", "length"])
    net = net.sort_values(by="id")
    net = net.drop_duplicates(subset="id", keep="first")
    net = net.merge(osm[["id", "geometry"]], on="id", how="inner")

    # network as geodataframe
    net = gpd.GeoDataFrame(net, geometry="geometry")

    # keep all osm links in network file
    osm_subset = osm[~osm["id"].isin(net["id"])]
    joined_net = pd.concat([net, osm_subset], ignore_index=True)
    del net, osm, osm_subset

    # joined network as a geodataframe
    joined_net = joined_net.drop(
        columns=[
            "head_1",
            "mean-head_1",
            "head_2",
            "mean-head_2",
            "diff-head",
            "head",
            "mean-head",
        ]
    )
    joined_net = gpd.GeoDataFrame(joined_net, geometry="geometry")

    # save the joined network file
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    network_path = f"{folder_path}/joined_network.shp"
    joined_net.to_file(
        network_path,
        driver="ESRI Shapefile",
    )

    # print message
    print(f"OSM and TomTom networks joined for {city_name}.")


# function to prepare census (population and employment) data
def prepare_census_data(city_name, state_abb):
    """
    function to prepare census data
    """
    # print message
    print(f"Preparing census data for {city_name}...")

    # import population data
    population = pd.read_csv(
        "./data/Raw census data/%s/ACSDT5Y2022.B01001-Data.csv" % state_abb
    )

    # import employment data
    employment = pd.read_csv(
        "./data/Raw census data/%s/ACSDT5Y2022.B08007-Data.csv" % state_abb
    )

    # import mapping shapefile
    map = gpd.read_file(
        glob.glob("./data/Raw census data/%s/cb_2022_*_bg_500k.shp" % state_abb)[0]
    )
    map = map.to_crs("EPSG:4326")

    # keep required rows and columns in population file
    population = population.drop(0)
    population.reset_index(drop=True, inplace=True)
    population.rename(columns={"B01001_001E": "population"}, inplace=True)
    population = population[["GEO_ID", "population"]]

    # keep required rows and columns in employment file
    employment = employment.drop(0)
    employment.reset_index(drop=True, inplace=True)
    employment.rename(columns={"B08007_001E": "employment"}, inplace=True)
    employment = employment[["GEO_ID", "employment"]]

    # join population, employment, and mapping files
    census = pd.merge(population, employment, on="GEO_ID", how="outer")
    census_map = census.merge(
        map[["AFFGEOID", "geometry"]],
        left_on="GEO_ID",
        right_on="AFFGEOID",
        how="outer",
    )
    census_map = gpd.GeoDataFrame(census_map, geometry="geometry")

    # census file to EPSG:3857
    census_map = census_map.to_crs("EPSG:3857")

    # population and employment as float
    census_map["population"] = census_map["population"].astype(float)
    census_map["employment"] = census_map["employment"].astype(float)

    # population and employment density (per km2)
    census_map["pop-den"] = census_map["population"] / census_map.area * 1000000
    census_map["emp-den"] = census_map["employment"] / census_map.area * 1000000

    # keep required columns only
    census_map = census_map[["pop-den", "emp-den", "geometry"]]

    # export the joined census data as a shapefile
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    census_map.to_file(
        "%s/census_%s.shp" % (folder_path, state_abb),
        driver="ESRI Shapefile",
    )

    # print message
    print(f"Census data prepared for {city_name}!")


# function to query osm land use opportunities
def get_osm_opportunities(
    city_name,
    csv_directory,
    buffer_boundary=True,
    land_use_map_path="../data/land_use_mapping_osm.xlsx",
):
    """
    function to query OSM land use opportunites for a city
    """

    # print message
    print(f"Querying OSM land use opportunity data for {city_name}...")

    # get city boundary
    boundary = get_city_boundary(
        city_name, csv_directory, buffer_boundary
    )

    # MEP land use categoires and mapping
    mep_categories = {"specialty", "retail", "food", "health care", "recreation"}
    uniform_mep_map = {
        "leisure": "recreation",
        "shop": "retail",
        "healthcare": "health care",
        "education": "specialty",
        "religion": "specialty",
        "cuisine": "food",
        "sport": "recreation",
    }

    # function to clean dataframe
    def clean_dataframe(gdf, tags):
        for key in tags:
            if key not in gdf.columns:
                gdf[key] = np.nan

        relevant_columns = [key for key in tags] + ["geometry"]
        return gdf[relevant_columns]

    # funtion to add columns of MEP categories
    def add_MEP_columns(gdf, uniform_mep_map):
        gdf = gdf.copy()
        for key in tags:
            # assign a MEP category for each tag and place in a tag specific column.
            # NaNs stay NaN
            if key in gdf.columns:
                if key in uniform_mep_map:
                    # assign all nonempty rows with a tag of <key> to 1 MEP category
                    isna_series = gdf[key].notna()
                    mep_map = {True: uniform_mep_map[key], False: np.nan}
                    gdf[f"{key}_MEP"] = isna_series.map(mep_map)
                else:
                    # Assign a MEP category to each row based on the tag key's value. eg amenity= restaurant --> food
                    # NaNs get mapped to NaNs
                    mep_map = key_MEP_map[key]
                    gdf[f"{key}_MEP"] = gdf[key].map(mep_map)
        return gdf

    # function to select relevant columns
    def select_relevant_columns(gdf, tags):
        relevant_columns = [key for key in tags if key in gdf.columns] + ["geometry"]
        return gdf[relevant_columns]

    # functionsto check if MEP category exists in a row
    def contains_MEP_category(row, category):
        return category in set(row)

    # function to get the set of MEP categories in a row
    def get_MEP_set(row):
        return set(row).intersection(mep_categories)

    # read the excel file of tag keys, values, and MEP categories
    excel_tag_keys = ["amenity", "building", "tourism", "historic"]
    key_MEP_map = (
        {}
    )  # dictionary by tag key of dictionaries mapping tag values to MEP categories. {key: {tag value 1: MEP category 1, ...}}
    for key in excel_tag_keys:
        map = pd.read_excel(io=land_use_map_path, sheet_name=key)

        # represent all MEP categories as lowercase strings. The "FALSE" values get read differently if they are strings or booleans in excel.
        map["MEP category"] = map["MEP category"].apply(str).apply(str.lower)
        map = map[map["MEP category"] != "false"]

        # ensure that there are no mis-spelled MEP categories
        if not (set(map["MEP category"].unique()) <= mep_categories):
            error_message = f"Error: one of the MEP categories below is not allowed.\n{set(map['MEP category'].unique())}"
            sys.exit(error_message)

        key_MEP_map[key] = dict(zip(map["value"], map["MEP category"]))

    # dictionary of tags
    tags = {
        "amenity": list(key_MEP_map["amenity"].keys()),
        "building": list(key_MEP_map["building"].keys()),
        "leisure": True,
        "shop": True,  # using all shops as retail, could count as food instead.
        "healthcare": True,
        "education": True,
        "religion": True,
        "cuisine": True,
        "sport": True,
        "tourism": list(key_MEP_map["tourism"].keys()),
        "historic": list(key_MEP_map["historic"].keys()),
    }

    # query OSM POI features
    poi = ox.features_from_polygon(boundary, tags).to_crs("EPSG:4326")

    # keep points within the boundary
    boundary = gpd.GeoDataFrame(geometry=[boundary], crs="EPSG:4326")
    poi = gpd.sjoin(poi, boundary, predicate="within")

    # clean and filter POI features
    poi = clean_dataframe(poi, tags)
    poi = select_relevant_columns(poi, tags)

    # handle picnic_table tag
    if "leisure" in list(poi.columns):
        poi = poi[poi.leisure != "picnic_table"]

    # assign MEP categories for each tag value
    poi = add_MEP_columns(poi, uniform_mep_map)
    MEP_columns = [f"{key}_MEP" for key in tags if key in poi.columns]
    city_MEP_cols = poi[MEP_columns]

    # determine whether each row has a case of each MEP category
    for categ in mep_categories:
        poi[categ] = city_MEP_cols.apply(
            lambda row: contains_MEP_category(row, category=categ), axis=1
        )

    # add column with all the MEP categories for the row in one place
    poi["MEP set"] = city_MEP_cols.apply(lambda row: get_MEP_set(row), axis=1)

    # replace geometries with centroids
    poi["centroid"] = poi.to_crs(epsg=3857).centroid.to_crs(poi.crs)

    # poi as dataframe
    poi = pd.DataFrame(poi.drop(["geometry"], axis=1).reset_index())

    # keep required columns only
    poi = poi[["osmid", "amenity"] + list(mep_categories) + ["centroid"]]

    # ensure one row for a POI
    poi = pd.melt(
        poi,
        id_vars=["osmid", "amenity", "centroid"],
        value_vars=list(mep_categories),
        var_name="oppty",
        value_name="value",
    )
    poi = poi[poi["value"]].drop(columns=["value"])

    # rename opportunity categories
    poi["oppty"] = poi["oppty"].replace(
        {
            "specialty": "sch-day-re",
            "health care": "medical",
            "retail": "shop-err",
            "food": "meals",
            "recreation": "recreation",
        }
    )

    # rename centroid column
    poi = poi.rename(columns={"centroid": "geometry"})

    # poi as geodataframe
    poi = gpd.GeoDataFrame(poi, geometry="geometry", crs="EPSG:4326")
    poi = poi.to_crs("EPSG:3857")
    poi = poi[["osmid", "oppty", "geometry"]]

    # save the opportunity file
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    opportunity_path = f"{folder_path}/land_use_opportunities.shp"
    poi.to_file(
        opportunity_path,
        driver="ESRI Shapefile",
    )

    # print message
    print(f"OSM land use opportunity data queried for {city_name}!")


# function to query osm traffic signals and transit stops and upload them to database
def get_osm_traffic_features(
    city_name, csv_directory, buffer_boundary=True
):
    """
    function to query OSM land use opportunites for a city/region and upload them to database
    city boundary is obained from the database
    """

    # print message
    print(f"Querying OSM traffic features data for {city_name}...")

    # get city boundary
    boundary = get_city_boundary(
        city_name, csv_directory, buffer_boundary
    )

    # query traffic features
    traffic_features = ox.features_from_polygon(
        boundary,
        tags={
            "highway": ["traffic_signals", "crossing", "street_lamp", "stop"],
            "crossing": "traffic_signals",
            "turn": True,
        },
    )
    traffic_features = traffic_features.loc["node"]

    # query transit stops
    transit_stops = ox.features_from_polygon(
        boundary,
        tags={
            "highway": ["bus_stop", "platform"],
            "public_transport": ["platform", "stop_position","station"],
            "railway": ["station", "halt", "tram_stop"],
        },
    )
    #Done later: transit_stops = transit_stops.loc["node"]  

    def classify_transit_type(osm_stops_data):
        osm_stops_data = osm_stops_data.copy()

        # bus - check for highway labeled bus stop or public_transport == stop_position + bus == yes
        bus_stop_bool = (osm_stops_data.highway == 'bus_stop')
        if 'bus' in osm_stops_data.columns:
            bus_stop_position_bool = (osm_stops_data.public_transport == 'stop_position') & (osm_stops_data.bus == 'yes')
        is_bus_stop = bus_stop_position_bool | bus_stop_bool
        osm_stops_data['bus_transit_type'] = is_bus_stop.map({True: 'bus', False: np.nan})

        # train - check for railway labeled halt/stop or public_transport == stop_position + train == yes
        if 'railway' in osm_stops_data.columns:
            train_stop_bool = (osm_stops_data.railway == 'halt') & (osm_stops_data.public_transport.notna())
            if 'train' in osm_stops_data.columns:
                train_stop_position_bool = (osm_stops_data.public_transport == 'stop_position') & ((osm_stops_data.train == 'yes') | (osm_stops_data.railway == 'stop'))
                is_train_stop = train_stop_position_bool | train_stop_bool 
            else:
                is_train_stop = train_stop_bool
            osm_stops_data['train_transit_type'] = is_train_stop.map({True: 'train', False: np.nan})

        elif 'train' in osm_stops_data.columns:
            is_train_stop = (osm_stops_data.public_transport == 'stop_position') & (osm_stops_data.train == 'yes')
            osm_stops_data['train_transit_type'] = is_train_stop.map({True: 'train', False: np.nan})

        # tram - check for railway labeled tram_stop or public_transport == stop_position + tram == yes
        if 'railway' in osm_stops_data.columns:
            tram_stop_bool = (osm_stops_data.railway == 'tram_stop')
            if 'tram' in osm_stops_data.columns:
                tram_stop_position_bool = (osm_stops_data.public_transport == 'stop_position') & (osm_stops_data.tram == 'yes')
                is_tram_stop = tram_stop_bool | tram_stop_position_bool
            else:
                is_tram_stop = tram_stop_bool
            osm_stops_data['tram_transit_type'] = is_tram_stop.map({True: 'tram', False: np.nan})

        elif 'tram' in osm_stops_data.columns:
            is_tram_stop = (osm_stops_data.public_transport == 'stop_position') & (osm_stops_data.tram == 'yes')
            osm_stops_data['tram_transit_type'] = is_tram_stop.map({True: 'tram', False: np.nan})


        # if it passes the above tests or has public_transport = stop position or station, set transit_stop to True    
        if 'tram_transit_type' not in osm_stops_data.columns:
            osm_stops_data['tram_transit_type'] = np.nan
        if 'train_transit_type' not in osm_stops_data.columns:
            osm_stops_data['train_transit_type'] = np.nan
 
        transit_type_columns = osm_stops_data[['bus_transit_type','train_transit_type','tram_transit_type']].copy()

        # Create a set denoting which transit types are present in each row
        osm_stops_data['transit_set'] = transit_type_columns.apply(lambda row: set(row).intersection({'bus', 'train', 'tram'}), axis = 1)
        osm_stops_data = osm_stops_data.explode('transit_set')

        osm_stops_data = osm_stops_data.rename(columns={'transit_set':'transit_type'})
        return osm_stops_data
    
    transit_stops = classify_transit_type(transit_stops)
    print(f"Number of transit features from query: {len(transit_stops)}")

    # remove rows that are not public transportation
    transit_stops = transit_stops[(transit_stops.transit_type.notna()) | (transit_stops.public_transport.notna())].copy()

    # keep points within the boundary
    boundary = gpd.GeoDataFrame(geometry=[boundary], crs="EPSG:4326")
    traffic_features = gpd.sjoin(traffic_features, boundary, predicate="within")

    ####### edits: 
    ####### edits: 
    ####### edits: 
    transit_stops = transit_stops.reset_index()
    transit_stops.to_crs("EPSG:3857", inplace=True) 

    # Separate node stops from stations and platforms that are not classified as stops
    transit_stops_nodes = transit_stops[
        ~transit_stops.public_transport.isin(['platform','station']) | 
        transit_stops.transit_type.notna()].copy()

    transit_stops_nodes = transit_stops_nodes[transit_stops_nodes.element_type == 'node']

    no_stops = transit_stops.drop(transit_stops_nodes.index)
    transit_platforms = no_stops[no_stops.public_transport == 'platform'].copy()
    transit_stations = no_stops[no_stops.public_transport == 'station'].copy()

    transit_platforms['geometry'] = transit_platforms.geometry.buffer(2)
    transit_stations['geometry'] = transit_stations.geometry.buffer(2)

    platforms_with_stops = transit_platforms.sjoin(transit_stops, how = "inner", predicate='contains')
    stations_with_stops = transit_stations.sjoin(transit_stops, how = "inner", predicate='contains')
    stations_intersecting_platforms = transit_stations.sjoin(transit_platforms, how = "inner", predicate='intersects')

    # Remove any platforms and stations that are near stops or stations that are near platforms
    transit_stops = transit_stops[~transit_stops.osmid.isin(platforms_with_stops.osmid_left)]
    transit_stops = transit_stops[~transit_stops.osmid.isin(stations_with_stops.osmid_left)]
    transit_stops = transit_stops[~transit_stops.osmid.isin(stations_intersecting_platforms.osmid_left)]

    transit_stops = gpd.sjoin(transit_stops, boundary.to_crs(epsg=3857), predicate="within")
    print(f'Final number of transit stops: {len(transit_stops)}')

    # Create centroids of the kept stops/platforms/stations
    transit_stops["centroid"] = transit_stops.to_crs(epsg=3857).centroid.to_crs('EPSG:4326')

    # Clean up traffic_features
    # drop informal crossings
    traffic_features = traffic_features[traffic_features.crossing != "informal"]
    traffic_features = traffic_features[
        traffic_features.highway.isin(
            ["traffic_signals", "crossing", "street_lamp", "stop"]
        )
    ]

    # assign a final type to the traffic feature point.
    # points count twice only if there is a signal and a crossing represented with the same node.
    def highway_and_crossing(hwy, crossing):
        if crossing == "traffic_signals" and hwy == "crossing":
            output_set = set([hwy, crossing])
        else:
            output_set = set([hwy])
        return output_set

    traffic_features["object_type"] = traffic_features.apply(
        lambda row: highway_and_crossing(row["highway"], row["crossing"]), axis=1
    )

    # explode rows with multiple entries. Select the important columns
    traffic_features = traffic_features.explode("object_type")[
        ["highway", "traffic_signals", "object_type", "geometry"]
    ]

    # replace geometries with centroids
    traffic_features["centroid"] = traffic_features.to_crs(epsg=3857).centroid.to_crs(
        traffic_features.crs
    )
    

    # set these files as dataframe
    traffic_features = pd.DataFrame(
        traffic_features.drop(["geometry"], axis=1).reset_index()
    )
    transit_stops = pd.DataFrame(transit_stops.drop(["geometry"], axis=1).reset_index())

    # create a column for traffic features
    traffic_features["tr-feature"] = traffic_features["object_type"]

    # set all stops as transit stops
    transit_stops["tr-feature"] = "trnst-stop"

    # rename categories of traffic features
    traffic_features["tr-feature"] = traffic_features["tr-feature"].replace(
        {
            "stop": "tra-stop",
            "traffic_signals": "tra-signal",
            "street_lamp": "st-lamp",
            "crossing": "crossing",
        }
    )

    # keep required columns only
    traffic_features = traffic_features[["osmid", "tr-feature", "centroid"]]
    transit_stops = transit_stops[["osmid", "tr-feature", "centroid"]]

    # combine traffic features and transit stops together
    features = pd.concat([traffic_features, transit_stops], ignore_index=True)

    # rename centroid column
    features = features.rename(columns={"centroid": "geometry"})

    # features as geodataframe
    features = gpd.GeoDataFrame(features, geometry="geometry", crs="EPSG:4326")
    features = features.to_crs("EPSG:3857")
    features = features[["osmid", "tr-feature", "geometry"]]
    print(f'Total features: {len(features)}')

    # save the features file
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    features_path = f"{folder_path}/traffic_features.shp"
    features.to_file(
        features_path,
        driver="ESRI Shapefile",
    )

    # print message
    print(f"OSM traffic features data queried for {city_name}!")


def add_census_traffic_features_to_joined_network(
    city_name, csv_directory, buffer_boundary, state_abb, density_distance_mi
):

    # import census data
    census_path = f"data/{city_name}/census_{state_abb}.shp"
    if os.path.exists(census_path):
        print("Importing prepared census data...")
        census = gpd.read_file(census_path)
    else:
        print("Prepared census data missing.")

    # import OSM opportunites data
    opportunity_path = f"data/{city_name}/land_use_opportunities.shp"
    if os.path.exists(census_path):
        print("Importing prepared OSM land use opportunity data...")
        opportunity = gpd.read_file(opportunity_path)
    else:
        print("Prepared OSM land use opportunity data missing.")

    # import OSM traffic features data
    features_path = f"data/{city_name}/traffic_features.shp"
    if os.path.exists(features_path):
        print("Importing prepared OSM traffic features data...")
        features = gpd.read_file(features_path)
    else:
        print("Prepared OSM land use opportunity data missing.")

    # import joined network
    network_path = f"data/{city_name}/joined_network.shp"
    if os.path.exists(network_path):
        print("Importing joined network...")
        network = gpd.read_file(network_path)
    else:
        print("Joined network missing.")

    # print message
    print("Intersecting census data with joined network...")

    # buffer network links
    buffer_distance = density_distance_mi * 1609.34
    network_buffer = network.copy()
    network_buffer["geometry"] = network_buffer["geometry"].buffer(buffer_distance)
    network_buffer = network_buffer.set_geometry("geometry")

    # intersect network and census files
    data = gpd.overlay(network_buffer, census, how="intersection", keep_geom_type=False)

    # population and employment within buffer
    data["area"] = data["geometry"].area
    columns = {
        "pop-den",
        "emp-den",
    }
    for col in columns:
        data[f"wt-{col}"] = (
            data[f"{col}"] * data["area"] / data.groupby("id")["area"].transform("sum")
        )
        data[f"{col}"] = data.groupby("id")[f"wt-{col}"].transform("sum")
        data = data.drop(columns=[f"wt-{col}"])
    data = data.drop(columns=["geometry", "area"])
    data = data.sort_values(by="id")
    data = data.drop_duplicates(subset="id", keep="first")

    # assign geometry from unbuffered network
    data = data.merge(network[["id", "geometry"]], on="id", how="inner")
    data = data.set_geometry("geometry")

    # population and employment density in per 1000 square mile (from km2)
    data["pop-den"] = data["pop-den"] * 2.59 / 1000
    data["emp-den"] = data["emp-den"] * 2.59 / 1000

    # print message
    print(
        "Intersecting land use opportunities and traffic features data with joined network..."
    )

    # function to count the number of features that lie within a buffer of network links
    def count_features(
        feature, column_name, feature_data, network, density_distance_mi
    ):

        # buffer network links
        buffer_distance = density_distance_mi * 1609.34  # buffer distance in meters
        network_buffer = network.copy()
        network_buffer["geometry"] = network_buffer["geometry"].buffer(buffer_distance)
        network_buffer = network_buffer.set_geometry("geometry")

        # area of buffered network
        network_buffer["area"] = (
            network_buffer["geometry"].area * 3.86102e-7
        )  # per square mile

        # feature
        feature_data = feature_data[feature_data[column_name] == feature]

        # intersect network and poi file
        count = network_buffer.sjoin(feature_data, how="inner")

        # group by network id and count features
        count = count.groupby("id").size().reset_index(name=feature)

        # bring area column
        count = count.merge(network_buffer[["id", "area"]], on=["id"], how="right")
        count[feature] = count[feature].fillna(0)
        count[feature] = count[feature] / count["area"]
        count.drop("area", axis=1, inplace=True)

        # return
        return count

    # add columns of counts of land use opportunities
    for feature in ["sch-day-re", "medical", "shop-err", "recreation", "meals"]:
        # count each features
        count = count_features(feature, "oppty", opportunity, data, density_distance_mi)

        # join with network
        data = data.merge(count, on="id", how="left")

    # add columns of counts of traffic features
    for feature in ["tra-signal", "tra-stop", "crossing", "st-lamp", "trnst-stop"]:
        # count each features
        count = count_features(
            feature, "tr-feature", features, data, density_distance_mi
        )

        # join with network
        data = data.merge(count, on="id", how="left")

    # data as dataframe
    data = pd.DataFrame(data.drop(columns="geometry"))

    # boundary to EPSG:3857
    boundary = get_city_boundary(
        city_name, csv_directory, buffer_boundary
    )
    boundary = gpd.GeoDataFrame(geometry=[boundary], crs="EPSG:4326")
    boundary = boundary.to_crs("EPSG:3857")

    # population and employment by zone
    census_city = gpd.overlay(boundary, census, how="intersection", keep_geom_type=False)
    
    census_city["population"] = census_city["pop-den"] * census_city.area * 1e-6
    census_city["employment"] = census_city["emp-den"] * census_city.area * 1e-6

    # population and employment density (per square mile)
    data["pop-den-city"] = census_city["population"].sum() / (census_city.area.sum() * 3.86102e-7)
    data["emp-den-city"] = census_city["employment"].sum() / (census_city.area.sum() * 3.86102e-7)
    # opportunity density
    data["opp-den-city"] = opportunity.shape[0]/ (boundary.area[0] * 3.86102e-7) # per square mile
    # road length (mile)
    network["length"] = network["geometry"].length
    data["road-len-city"] = network["length"].sum() / 1609.34
    
    # save the data
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    data.to_csv(f"{folder_path}/prepared_data.csv", index=False)

    # print message
    print(
        f"Census, land use opportunites, and traffic features data joined with network for {city_name}!"
    )


# function to prepare calibration model-ready data
def prepare_data_for_model(city_name):

    # import prepared data (osm network links with speeds and features)
    data = pd.read_csv(f"data/{city_name}/prepared_data.csv")

    # limit observed speeds based on several assumptions to avoid erreneous values
    # for mean and medain speeds
    max_speed_limits = data.groupby("highway")["osm-sl-mph"].max()
    # mean_speed_cols = [
    #     "mean-wd-am",
    #     "mean-wd-af",
    #     "mean-wd-pm",
    #     # "mean-wd-nt",
    #     "mean-we-am",
    #     "mean-we-af",
    #     "mean-we-pm",
    #     # "mean-we-nt",
    # ]
    med_speed_cols = [
        "med-wd-am",
        "med-wd-af",
        "med-wd-pm",
        # "med-wd-nt",
        "med-we-am",
        "med-we-af",
        "med-we-pm",
        # "med-we-nt",
    ]
    # for column in mean_speed_cols + med_speed_cols:
    for column in med_speed_cols:
        for index, row in data.iterrows():
            if not pd.isna(row["osm-sl-mph"]):
                if row[column] - row["osm-sl-mph"] > 10:
                    data.at[index, column] = row["osm-sl-mph"] + 10
            else:
                highway_type = row["highway"]
                if row[column] > max_speed_limits[highway_type] + 5:
                    data.at[index, column] = max_speed_limits[highway_type] + 5

    # # for 85th percentile speeds
    # pr_speed_cols = [
    #     "85pr-wd-am",
    #     "85pr-wd-af",
    #     "85pr-wd-pm",
    #     # "85pr-wd-nt",
    #     "85pr-we-am",
    #     "85pr-we-af",
    #     "85pr-we-pm",
    #     # "85pr-we-nt",
    # ]
    # for column in pr_speed_cols:
    #     for index, row in data.iterrows():
    #         if not pd.isna(row["osm-sl-mph"]):
    #             if row[column] - row["osm-sl-mph"] > 20:
    #                 data.at[index, column] = row["osm-sl-mph"] + 20
    #         else:
    #             highway_type = row["highway"]
    #             if row[column] > max_speed_limits[highway_type] + 10:
    #                 data.at[index, column] = max_speed_limits[highway_type] + 10

    # missing osm speed limit column
    data["mis-osm-sl"] = data["osm-sl-mph"].isnull().replace({True: "yes", False: "no"})
    data["mis-osm-sl"] = pd.Categorical(data["mis-osm-sl"], categories=["yes", "no"])

    # roadway direction column
    data["one-way"] = pd.Categorical(
        data["one-way"], categories=[0, 1]
    ).rename_categories(["no", "yes"])
    data["dummy"] = data["one-way"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="one-way", prefix_sep=":")

    # lanes column
    data["lanes"] = pd.to_numeric(data["lanes"], errors="coerce")

    def map_to_string(value):
        if pd.isnull(value):
            return "missing"
        elif value == 1:
            return "one"
        elif value == 2:
            return "two"
        elif value == 3:
            return "three"
        elif value == 4:
            return "four"
        elif value == 5:
            return "five"
        elif value >= 6 and value <= 10:
            return "six plus"
        else:
            return "missing"

    data["lanes"] = data["lanes"].apply(map_to_string)
    data["dummy"] = data["lanes"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="lanes", prefix_sep=":")

    # missing lanes column
    data["mis-lanes"] = data["lanes"].apply(lambda x: "yes" if x == "missing" else "no")
    data["mis-lanes"] = pd.Categorical(data["mis-lanes"], categories=["yes", "no"])

    # highway class column
    data["highway"] = data["highway"].replace(
        to_replace=[
            value
            for value in data["highway"].unique()
            if value
            in [
                "motorway_link",
                "trunk_link",
            ]
        ],
        value="link",
    )
    data["highway"] = data["highway"].replace(
        to_replace=[
            value
            for value in data["highway"].unique()
            if value
            not in [
                "residential",
                "motorway",
                "trunk",
                "primary",
                "secondary",
                "tertiary",
                "service",
                "link",
            ]
        ],
        value="other",
    )
    data["highway"] = pd.Categorical(
        data["highway"],
        categories=[
            "residential",
            "motorway",
            "trunk",
            "primary",
            "secondary",
            "tertiary",
            "service",
            "link",
            "other",
        ],
    )
    data["dummy"] = data["highway"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="", prefix_sep="")

    # convert wide data to long format for different speed types
    speed_melted = data.melt(
        id_vars=[
            col
            for col in data.columns
            # if col not in mean_speed_cols + med_speed_cols + pr_speed_cols
            if col not in med_speed_cols

        ],
        var_name="speed-var",
        value_name="speed",
    )

    # convert wide data to long format for probe size in different times
    probe_size_cols = [
        "prsz-wd-am",
        "prsz-wd-af",
        "prsz-wd-pm",
        # "prsz-wd-nt",
        "prsz-we-am",
        "prsz-we-af",
        "prsz-we-pm",
        # "prsz-we-nt",
    ]
    
    probe_melted = data.melt(
        id_vars=["id"],
        value_vars=probe_size_cols,
        var_name="probe-size-var",
        value_name="probe-size"
        )
    
    probe_melted["probe-size-var"] = probe_melted["probe-size-var"].str.replace("prsz", "med")
    # Merge melted DataFrames
    data = speed_melted.merge(
        probe_melted, left_on=["id", "speed-var"], right_on=["id", "probe-size-var"]
    ).drop(columns=["probe-size-var"])
    # split var column into speed type, day, and hour
    data[["speed-type", "day", "hour"]] = data["speed-var"].str.split("-", expand=True)

    # day column
    data["day"] = pd.Categorical(
        data["day"], categories=["wd", "we"]
    ).rename_categories(["weekday", "weekend"])
    data["dummy"] = data["day"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="", prefix_sep="")

    # hour column
    data["hour"] = pd.Categorical(
        data["hour"], categories=["af", "am", "pm", "nt"]
    ).rename_categories(["afternoon", "am-peak", "pm-peak", "night"])
    data["dummy"] = data["hour"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="", prefix_sep="")

    # rename columns names and order them
    rename_dict = {
        "id": "ID",
        "residential": "Road Class: Residential",
        "motorway": "Road Class: Motorway",
        "trunk": "Road Class: Trunk",
        "primary": "Road Class: Primary",
        "secondary": "Road Class: Secondary",
        "tertiary": "Road Class: Tertiary",
        "service": "Road Class: Service",
        "link": "Road Class: Link",
        "other": "Road Class: Other",
        "one-way:no": "Bidirectional",
        "lanes:one": "Lanes = 1",
        "lanes:two": "Lanes = 2",
        "lanes:three": "Lanes = 3",
        "lanes:four": "Lanes = 4",
        "lanes:five": "Lanes = 5",
        "lanes:six plus": "Lanes = 6",
        "lanes:missing": "Lanes = Missing",
        "pop-den": "Population Density",
        "emp-den": "Employment Density",
        "sch-day-re": "School/Religious Opp Density",
        "medical": "Medical Opp Density",
        "shop-err": "Shopping/Errands Opp Density",
        "recreation": "Recreational Opp Density",
        "meals": "Meals Opp Density",
        "tra-signal": "Traffic Signal Density",
        "tra-stop": "Traffic Stop Density",
        "crossing": "Crossing Density",
        "st-lamp": "Streetlamp Density",
        "trnst-stop": "Transit Stop Density",
        "weekday": "Day: Weekday",
        "weekend": "Day: Weekend",
        "afternoon": "Hour: Afternoon",
        "am-peak": "Hour: AM Peak",
        "pm-peak": "Hour: PM Peak",
        "night": "Hour: Night",
        "speed-type": "Speed Type",
        "speed": "Speed",
        "probe-size": "Probe Size",
        "osm-sl-mph": "Speed Limit",
        "pop-den-city": "Population Density (City)",
        "emp-den-city": "Employment Density (City)",
        "opp-den-city": "Opp Density (City)",
        "road-len-city": "Road Length (City)",
    }
    data = data.rename(columns=rename_dict)
    data = data.reindex(columns=rename_dict.values())

    # save prepared and joined data as csv
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    model_input_path = f"{folder_path}/prepared_model_input.csv"
    data.to_csv(
        model_input_path,
        index=False,
    )

    # print message
    print(f"Data for model prepared for {city_name}!")
    ## ----------------------------------------------

### fuctions for imputation only

# function to prepare osm network only for imputation
def prepare_osm_network_only(
    city_name, csv_directory, buffer_boundary
):
    """
    function to prepare osm data
    """
    print(f"Preparing OSM network for {city_name}.")
    # import osm network
    folder_path = f"../OSM network/{city_name}/"
    network_path = f"{folder_path}/{city_name}_OSM_missing_speeds.csv"
    # folder_path = f"../OSM network/{city_name}_wd_am/" # d_h
    # network_path = f"{folder_path}/{city_name}_wd_am_OSM_missing_speeds.csv" # d_h
    osm_network = pd.read_csv(
        network_path,
        engine="python",
        encoding="latin1",
    )

    # osm network as gpd
    osm_network["geometry"] = osm_network["wkt"].apply(loads)
    osm_network = gpd.GeoDataFrame(osm_network, geometry="geometry")
    osm_network = osm_network.set_crs("EPSG:4326")

    # keep osm network within city boundary
    boundary = get_city_boundary(
        city_name, csv_directory, buffer_boundary
    )
    boundary = gpd.GeoDataFrame(geometry=[boundary], crs="EPSG:4326")
    osm_network = sjoin(osm_network, boundary, how="inner")

    # keep drive eligible links only
    osm_network = osm_network[osm_network["drive-mode"] == True]

    # keep the first value of drive eligible highway class
    drive_highway = [
        "motorway_link",
        "motorway",
        "trunk_link",
        "trunk",
        "primary",
        "primary_link",
        "secondary",
        "secondary_link",
        "tertiary",
        "tertiary_link",
        "residential",
        "service",
        "track",
        "road",
        "unclassified",
    ]
    osm_network["highway"] = osm_network.apply(
        lambda x: next((key for key in drive_highway if key in str(x)), None), axis=1
    )

    # update lanes column
    # keeping first values if multiple exists
    osm_network["lanes"] = osm_network["lanes"].replace(np.nan, "None")
    osm_network["lanes"] = osm_network["lanes"].apply(
        lambda x: int(x.split()[0]) if isinstance(x, str) and " " in x else x
    )
    osm_network["lanes"] = osm_network["lanes"].replace("None", np.nan)

    # keep required columns only
    osm_network = osm_network[
        ["id", "lanes", "highway", "one-way", "speed-kph", "geometry"]
    ]

    # set missing speeds as NA
    # missing speeds were coded -299 in osm query
    osm_network["speed-kph"] = np.where(osm_network["speed-kph"] == -299, np.nan, osm_network["speed-kph"])

    # speed in kph to mph
    osm_network["osm-sl-mph"] = osm_network["speed-kph"] / 1.609  # speed in mph

    # filter and rename column names from osm
    osm_network = osm_network[
        [
            "id",
            "lanes",
            "highway",
            "one-way",
            "osm-sl-mph",
            "geometry",
        ]
    ]

    # set geometris to EPSG:3857
    osm_network.to_crs("EPSG:3857", inplace=True)
    osm_network = osm_network.sort_values(by="id")
    osm_network = osm_network.drop_duplicates(subset="id", keep="first")

    # network as geodataframe
    osm_network = gpd.GeoDataFrame(osm_network, geometry="geometry")

    # joined network as a geodataframe
    # save the joined network file
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    network_path = f"{folder_path}/prepared_OSM_network.shp"
    osm_network.to_file(
        network_path,
        driver="ESRI Shapefile",
    )

    # print message
    print(f"OSM network prepared for {city_name}.")
    
def add_census_traffic_features_to_osm_network(
    city_name, csv_directory, buffer_boundary, state_abb, density_distance_mi
):

    # import census data
    census_path = f"data/{city_name}/census_{state_abb}.shp"
    if os.path.exists(census_path):
        print("Importing prepared census data...")
        census = gpd.read_file(census_path)
    else:
        print("Prepared census data missing.")

    # import OSM opportunites data
    opportunity_path = f"data/{city_name}/land_use_opportunities.shp"
    if os.path.exists(census_path):
        print("Importing prepared OSM land use opportunity data...")
        opportunity = gpd.read_file(opportunity_path)
    else:
        print("Prepared OSM land use opportunity data missing.")

    # import OSM traffic features data
    features_path = f"data/{city_name}/traffic_features.shp"
    if os.path.exists(features_path):
        print("Importing prepared OSM traffic features data...")
        features = gpd.read_file(features_path)
    else:
        print("Prepared OSM land use opportunity data missing.")

    # import prepared OSM network
    network_path = f"data/{city_name}/prepared_OSM_network.shp"
    if os.path.exists(network_path):
        print("Importing OSM network...")
        network = gpd.read_file(network_path)
    else:
        print("OSM network missing.")

    # print message
    print("Intersecting census data with OSM network...")

    # buffer network links
    buffer_distance = density_distance_mi * 1609.34
    network_buffer = network.copy()
    network_buffer["geometry"] = network_buffer["geometry"].buffer(buffer_distance)
    network_buffer = network_buffer.set_geometry("geometry")

    # intersect network and census files
    data = gpd.overlay(network_buffer, census, how="intersection", keep_geom_type=False)

    # population and employment within buffer
    data["area"] = data["geometry"].area
    columns = {
        "pop-den",
        "emp-den",
    }
    for col in columns:
        data[f"wt-{col}"] = (
            data[f"{col}"] * data["area"] / data.groupby("id")["area"].transform("sum")
        )
        data[f"{col}"] = data.groupby("id")[f"wt-{col}"].transform("sum")
        data = data.drop(columns=[f"wt-{col}"])
    data = data.drop(columns=["geometry", "area"])
    data = data.sort_values(by="id")
    data = data.drop_duplicates(subset="id", keep="first")

    # assign geometry from unbuffered network
    data = data.merge(network[["id", "geometry"]], on="id", how="inner")
    data = data.set_geometry("geometry")

    # population and employment density in per 1000 square mile (from km2)
    data["pop-den"] = data["pop-den"] * 2.59 / 1000
    data["emp-den"] = data["emp-den"] * 2.59 / 1000

    # print message
    print(
        "Intersecting land use opportunities and traffic features data with OSM network..."
    )

    # function to count the number of features that lie within a buffer of network links
    def count_features(
        feature, column_name, feature_data, network, density_distance_mi
    ):

        # buffer network links
        buffer_distance = density_distance_mi * 1609.34  # buffer distance in meters
        network_buffer = network.copy()
        network_buffer["geometry"] = network_buffer["geometry"].buffer(buffer_distance)
        network_buffer = network_buffer.set_geometry("geometry")

        # area of buffered network
        network_buffer["area"] = (
            network_buffer["geometry"].area * 3.86102e-7
        )  # per square mile

        # feature
        feature_data = feature_data[feature_data[column_name] == feature]

        # intersect network and poi file
        count = network_buffer.sjoin(feature_data, how="inner")

        # group by network id and count features
        count = count.groupby("id").size().reset_index(name=feature)

        # bring area column
        count = count.merge(network_buffer[["id", "area"]], on=["id"], how="right")
        count[feature] = count[feature].fillna(0)
        count[feature] = count[feature] / count["area"]
        count.drop("area", axis=1, inplace=True)

        # return
        return count

    # add columns of counts of land use opportunities
    for feature in ["sch-day-re", "medical", "shop-err", "recreation", "meals"]:
        # count each features
        count = count_features(feature, "oppty", opportunity, data, density_distance_mi)

        # join with network
        data = data.merge(count, on="id", how="left")

    # add columns of counts of traffic features
    for feature in ["tra-signal", "tra-stop", "crossing", "st-lamp", "trnst-stop"]:
        # count each features
        count = count_features(
            feature, "tr-feature", features, data, density_distance_mi
        )

        # join with network
        data = data.merge(count, on="id", how="left")

    # data as dataframe
    data = pd.DataFrame(data.drop(columns="geometry"))

    # boundary to EPSG:3857
    boundary = get_city_boundary(
        city_name, csv_directory, buffer_boundary
    )
    boundary = gpd.GeoDataFrame(geometry=[boundary], crs="EPSG:4326")
    boundary = boundary.to_crs("EPSG:3857")

    # population and employment by zone
    census_city = gpd.overlay(boundary, census, how="intersection", keep_geom_type=False)
    
    census_city["population"] = census_city["pop-den"] * census_city.area * 1e-6
    census_city["employment"] = census_city["emp-den"] * census_city.area * 1e-6

    # population and employment density (per square mile)
    data["pop-den-city"] = census_city["population"].sum() / (census_city.area.sum() * 3.86102e-7)
    data["emp-den-city"] = census_city["employment"].sum() / (census_city.area.sum() * 3.86102e-7)
    # opportunity density
    data["opp-den-city"] = opportunity.shape[0]/ (boundary.area[0] * 3.86102e-7) # per square mile
    # road length (mile)
    network["length"] = network["geometry"].length
    data["road-len-city"] = network["length"].sum() / 1609.34
    
    # save the data
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    data.to_csv(f"{folder_path}/prepared_data_for_imputation.csv", index=False)

    # print message
    print(
        f"Census, land use opportunites, and traffic features data joined with OSM network for {city_name}!"
    )


# function to prepare calibration model-ready data
def prepare_data_for_imputation(city_name):

    # import prepared data (osm network links with speeds and features)
    data = pd.read_csv(f"data/{city_name}/prepared_data_for_imputation.csv")

    # missing osm speed limit column
    data["mis-osm-sl"] = data["osm-sl-mph"].isnull().replace({True: "yes", False: "no"})
    data["mis-osm-sl"] = pd.Categorical(data["mis-osm-sl"], categories=["yes", "no"])

    # roadway direction column
    data["one-way"] = pd.Categorical(
        data["one-way"], categories=[0, 1]
    ).rename_categories(["no", "yes"])
    data["dummy"] = data["one-way"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="one-way", prefix_sep=":")

    # lanes column
    data["lanes"] = pd.to_numeric(data["lanes"], errors="coerce")

    def map_to_string(value):
        if pd.isnull(value):
            return "missing"
        elif value == 1:
            return "one"
        elif value == 2:
            return "two"
        elif value == 3:
            return "three"
        elif value == 4:
            return "four"
        elif value == 5:
            return "five"
        elif value >= 6 and value <= 10:
            return "six plus"
        else:
            return "missing"

    data["lanes"] = data["lanes"].apply(map_to_string)
    data["dummy"] = data["lanes"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="lanes", prefix_sep=":")

    # missing lanes column
    data["mis-lanes"] = data["lanes"].apply(lambda x: "yes" if x == "missing" else "no")
    data["mis-lanes"] = pd.Categorical(data["mis-lanes"], categories=["yes", "no"])

    # highway class column
    data["highway"] = data["highway"].replace(
        to_replace=[
            value
            for value in data["highway"].unique()
            if value
            in [
                "motorway_link",
                "trunk_link",
            ]
        ],
        value="link",
    )
    data["highway"] = data["highway"].replace(
        to_replace=[
            value
            for value in data["highway"].unique()
            if value
            not in [
                "residential",
                "motorway",
                "trunk",
                "primary",
                "secondary",
                "tertiary",
                "service",
                "link",
            ]
        ],
        value="other",
    )
    data["highway"] = pd.Categorical(
        data["highway"],
        categories=[
            "residential",
            "motorway",
            "trunk",
            "primary",
            "secondary",
            "tertiary",
            "service",
            "link",
            "other",
        ],
    )
    data["dummy"] = data["highway"].copy()
    data = pd.get_dummies(data, columns=["dummy"], prefix="", prefix_sep="")

    # rename columns names and order them
    rename_dict = {
        "id": "ID",
        "residential": "Road Class: Residential",
        "motorway": "Road Class: Motorway",
        "trunk": "Road Class: Trunk",
        "primary": "Road Class: Primary",
        "secondary": "Road Class: Secondary",
        "tertiary": "Road Class: Tertiary",
        "service": "Road Class: Service",
        "link": "Road Class: Link",
        "other": "Road Class: Other",
        "one-way:no": "Bidirectional",
        "lanes:one": "Lanes = 1",
        "lanes:two": "Lanes = 2",
        "lanes:three": "Lanes = 3",
        "lanes:four": "Lanes = 4",
        "lanes:five": "Lanes = 5",
        "lanes:six plus": "Lanes = 6",
        "lanes:missing": "Lanes = Missing",
        "pop-den": "Population Density",
        "emp-den": "Employment Density",
        "sch-day-re": "School/Religious Opp Density",
        "medical": "Medical Opp Density",
        "shop-err": "Shopping/Errands Opp Density",
        "recreation": "Recreational Opp Density",
        "meals": "Meals Opp Density",
        "tra-signal": "Traffic Signal Density",
        "tra-stop": "Traffic Stop Density",
        "crossing": "Crossing Density",
        "st-lamp": "Streetlamp Density",
        "trnst-stop": "Transit Stop Density",
        "osm-sl-mph": "Speed Limit",
        "pop-den-city": "Population Density (City)",
        "emp-den-city": "Employment Density (City)",
        "opp-den-city": "Opp Density (City)",
        "road-len-city": "Road Length (City)",
    }
    data = data.rename(columns=rename_dict)
    data = data.reindex(columns=rename_dict.values())

    # save prepared and joined data as csv
    folder_path = f"data/{city_name}/"
    if not os.path.exists(folder_path):
        os.makedirs(folder_path)
    model_input_path = f"{folder_path}/prepared_imputation_input.csv"
    data.to_csv(
        model_input_path,
        index=False,
    )

    # print message
    print(f"Data for imputation prepared for {city_name}!")
    ## ----------------------------------------------