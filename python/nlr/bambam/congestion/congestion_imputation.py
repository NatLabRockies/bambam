"""
This script consists prepares data for calibrating operating speed prediciton models.

Author: Sailesh Acharya, Guangchen Zhao.

Last update: 2025-3-26
"""

# import modules
import pandas as pd
import os
import sys

# setup
# csv.field_size_limit(sys.maxsize)
os.chdir(os.getcwd())
from .calibration_utils import *

# city name
cities = pd.read_csv("../data/cities_congestion_2025.csv")

csv_directory = ""

buffer_boundary = False
# hostname = (
#     "batch-support-cluster-postgres-1.cluster-ccklrxkcenui.us-west-2.rds.amazonaws.com"
# )
# portname = 5432
# dbname = "mep"
# username = "yhou"
# password = "rmywis5Ntahunha"
density_distance_mi = 0.5


# join_osm_tomtom_networks(
#     city_name, csv_directory, buffer_boundary
# )
# prepare_census_data(city_name, state_abb)
# get_osm_opportunities(
#     city_name,
#     hostname,
#     portname,
#     dbname,
#     username,
#     password,
#     buffer_boundary=True,
#     land_use_map_path="../data/land_use_mapping_osm.xlsx",
# )
# get_osm_traffic_features(
#     city_name, csv_directory, buffer_boundary=True
# )
# add_census_traffic_features_to_joined_network(city_name, csv_directory, buffer_boundary, state_abb, density_distance_mi)
# prepare_data_for_model(city_name)
# Main function to handle command-line input
if __name__ == "__main__":
    if len(sys.argv) < 4:
        print(
            "Usage: python congestion_calibration.py <function_name> <from_city_index> <to_city_index>"
        )
        sys.exit(1)

    function_name = sys.argv[1]
    from_city = int(sys.argv[2])
    to_city = int(sys.argv[3])

    # Map function names to actual functions
    if to_city > len(cities):
        print("index is out of range.")
        sys.exit(1)
    for i in range(from_city, to_city + 1):
        city_name = cities[cities["num"] == i]["name"].values[0]
        state_abb = cities[cities["num"] == i]["state"].values[0]

        if function_name == "join_osm_tomtom_networks":
            join_osm_tomtom_networks(
                city_name,
                csv_directory,
                buffer_boundary,
            )

        elif function_name == "prepare_census_data":
            prepare_census_data(city_name, state_abb)

        elif function_name == "get_osm_opportunities":
            get_osm_opportunities(
                city_name,
                csv_directory,
                buffer_boundary=True,
                land_use_map_path="../data/land_use_mapping_osm.xlsx",
            )

        elif function_name == "get_osm_traffic_features":
            get_osm_traffic_features(
                city_name,
                csv_directory,
                buffer_boundary=True,
            )
            
        elif function_name == "add_census_traffic_features_to_joined_network":
            add_census_traffic_features_to_joined_network(city_name, csv_directory, buffer_boundary, 
                                                          state_abb, density_distance_mi)
            
        elif function_name == "prepare_data_for_model":
            prepare_data_for_model(city_name)
        
        elif function_name == "prepare_osm_network_only":
            prepare_osm_network_only(
                city_name, csv_directory, buffer_boundary
            )
        
        elif function_name == "add_census_traffic_features_to_osm_network":
            add_census_traffic_features_to_osm_network(city_name, csv_directory, buffer_boundary, 
                                                          state_abb, density_distance_mi)
            
        elif function_name == "prepare_data_for_imputation":
            prepare_data_for_imputation(city_name)
            
        elif function_name == "run_all_for_calibration":
            join_osm_tomtom_networks(
                city_name,
                csv_directory,
                buffer_boundary,
            )
            
            prepare_census_data(city_name, state_abb)
            
            get_osm_opportunities(
                city_name,
                csv_directory,
                buffer_boundary=True,
                land_use_map_path="../data/land_use_mapping_osm.xlsx",
            )
            
            get_osm_traffic_features(
                city_name,
                csv_directory,
                buffer_boundary=True,
            )
            
            add_census_traffic_features_to_joined_network(city_name, csv_directory, buffer_boundary, 
                                                          state_abb, density_distance_mi)
            
            prepare_data_for_model(city_name)

        elif function_name == "run_all_for_imputation":
            prepare_osm_network_only(
                city_name, csv_directory, buffer_boundary
            )
            
            prepare_census_data(city_name, state_abb)
            
            get_osm_opportunities(
                city_name,
                csv_directory,
                buffer_boundary=True,
                land_use_map_path="../data/land_use_mapping_osm.xlsx",
            )
            
            get_osm_traffic_features(
                city_name,
                csv_directory,
                buffer_boundary=True,
            )
            
            add_census_traffic_features_to_osm_network(city_name, csv_directory, buffer_boundary, 
                                                          state_abb, density_distance_mi)
            
            prepare_data_for_imputation(city_name)

        else:
            print(f"Function '{function_name}' not found.")