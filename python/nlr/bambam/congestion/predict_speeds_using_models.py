# this script applies the calibrated prediction models to fill observed speeds in the OSM network

# import modules
import pandas as pd
import geopandas as gpd
import numpy as np
from joblib import load
import os
from shapely.wkt import loads
    
def predict_speeds(city_name):
    print(f"Predicting speed for {city_name}...")
    # import joined data
    data_path = f"./data/{city_name}/prepared_imputation_input.csv"
    network_path = f"./data/{city_name}/prepared_OSM_network.shp"
    
    data = pd.read_csv(data_path)
    network = gpd.read_file(network_path)
    network["length-met"] = network.geometry.length
    
    data = data.merge(network[['id', 'highway','length-met','geometry']], left_on="ID", right_on="id")
    
    # impute speed limits with median values
    print(
        "Links with missing speed limits:",
        data["Speed Limit"].isna().sum(),
    )
    median_speeds = data.groupby("highway")["Speed Limit"].median().to_dict()
    data["sl-med-imp"] = data.apply(
        lambda row: (
            row["Speed Limit"]
            if not np.isnan(row["Speed Limit"])
            else median_speeds.get(row["highway"], np.nan)
        ),
        axis=1,
    )
    
    # ensure speed limits are at least 15 mph
    data["sl-med-imp"] = np.where((data["sl-med-imp"] < 15) | (data["sl-med-imp"].isna()), 15, data["sl-med-imp"])
    
    # impute road calss link using the model without speed limit
    data[['Speed Limit']] = np.where(data[['Road Class: Link']], 
                                     np.nan, 
                                     data[['Speed Limit']])
    
    road_classes = [
        'Road Class: Motorway',
        'Road Class: Trunk',
        'Road Class: Link',
        'Road Class: Primary',
        'Road Class: Secondary',
        'Road Class: Tertiary'
    ]
    data_input = data[data[road_classes].any(axis=1)]
    data_other = data[~data[road_classes].any(axis=1)]
    
    opp_columns = ["School/Religious Opp Density", "Medical Opp Density", "Shopping/Errands Opp Density", "Recreational Opp Density", "Meals Opp Density"]
    data_input["Opp Density"]= data_input[opp_columns].sum(axis=1)
    
    Signal_Crossing_Stop_columns = ["Traffic Signal Density", "Crossing Density", "Traffic Stop Density"]
    data_input["Traffic Signal/Crossing/Traffic Stop Density"]= data_input[Signal_Crossing_Stop_columns].sum(axis=1)
    
    # features used in prediction for links having speed limits
    predictors_speedlimit = [
        "Speed Limit",
        "Population Density",
        "Employment Density",
        "Day: Weekday",
        "Hour: AM Peak",
        "Hour: PM Peak",
        "Road Class: Motorway",
        "Road Class: Trunk",
        "Road Class: Primary",
        "Road Class: Secondary",
        "Road Class: Tertiary",
        "Road Class: Service",
        "Road Class: Link",
        "Road Class: Other",
        "Opp Density",
        "Traffic Signal/Crossing/Traffic Stop Density",
    ]
    
    # features used in prediction for links not having speed limits
    predictors_no_speedlimit = [
        "Population Density",
        "Employment Density",
        "Day: Weekday",
        "Hour: AM Peak",
        "Hour: PM Peak",
        "Road Class: Motorway",
        "Road Class: Trunk",
        "Road Class: Primary",
        "Road Class: Secondary",
        "Road Class: Tertiary",
        "Road Class: Service",
        "Road Class: Link",
        "Road Class: Other",
        "Opp Density",
        "Traffic Signal/Crossing/Traffic Stop Density",
    ]
    
    # load fitted models
    model_with_speedlimit = load(
        "./Outputs/Models with speed limit/xgboost_model_with_speedlimit.joblib"
    )
    model_without_speedlimit = load(
        "./Outputs/Models without speed limit/xgboost_model_without_speedlimit.joblib"
    )
    
    # add empty columns for predicted speeds
    data_input.loc[:, "wd-am-prd"] = np.nan
    data_input.loc[:, "wd-af-prd"] = np.nan
    data_input.loc[:, "wd-pm-prd"] = np.nan
    data_input.loc[:, "we-am-prd"] = np.nan
    data_input.loc[:, "we-af-prd"] = np.nan
    data_input.loc[:, "we-pm-prd"] = np.nan
    
    # predict speeds for weekday morning
    data_input.loc[:, "Day: Weekday"] = 1
    data_input.loc[:, "Hour: AM Peak"] = 1
    data_input.loc[:, "Hour: PM Peak"] = 0
    data_input.loc[~data_input["Speed Limit"].isna(), "wd-am-prd"] = model_with_speedlimit.predict(
        data_input[predictors_speedlimit][~data_input["Speed Limit"].isna()]
    )
    data_input.loc[data_input["Speed Limit"].isna(), "wd-am-prd"] = model_without_speedlimit.predict(
        data_input[predictors_no_speedlimit][data_input["Speed Limit"].isna()]
    )
    
    # predict speeds for weekday afternoon
    data_input.loc[:, "Day: Weekday"] = 1
    data_input.loc[:, "Hour: AM Peak"] = 0
    data_input.loc[:, "Hour: PM Peak"] = 0
    data_input.loc[~data_input["Speed Limit"].isna(), "wd-af-prd"] = model_with_speedlimit.predict(
        data_input[predictors_speedlimit][~data_input["Speed Limit"].isna()]
    )
    data_input.loc[data_input["Speed Limit"].isna(), "wd-af-prd"] = model_without_speedlimit.predict(
        data_input[predictors_no_speedlimit][data_input["Speed Limit"].isna()]
    )
    
    # predict speeds for weekday evening
    data_input.loc[:, "Day: Weekday"] = 1
    data_input.loc[:, "Hour: AM Peak"] = 0
    data_input.loc[:, "Hour: PM Peak"] = 1
    data_input.loc[~data_input["Speed Limit"].isna(), "wd-pm-prd"] = model_with_speedlimit.predict(
        data_input[predictors_speedlimit][~data_input["Speed Limit"].isna()]
    )
    data_input.loc[data_input["Speed Limit"].isna(), "wd-pm-prd"] = model_without_speedlimit.predict(
        data_input[predictors_no_speedlimit][data_input["Speed Limit"].isna()]
    )
    
    # predict speeds for weekend morning
    data_input.loc[:, "Day: Weekday"] = 0
    data_input.loc[:, "Hour: AM Peak"] = 1
    data_input.loc[:, "Hour: PM Peak"] = 0
    data_input.loc[~data_input["Speed Limit"].isna(), "we-am-prd"] = model_with_speedlimit.predict(
        data_input[predictors_speedlimit][~data_input["Speed Limit"].isna()]
    )
    data_input.loc[data_input["Speed Limit"].isna(), "we-am-prd"] = model_without_speedlimit.predict(
        data_input[predictors_no_speedlimit][data_input["Speed Limit"].isna()]
    )
    
    # predict speeds for weekend afternoon
    data_input.loc[:, "Day: Weekday"] = 0
    data_input.loc[:, "Hour: AM Peak"] = 0
    data_input.loc[:, "Hour: PM Peak"] = 0
    data_input.loc[~data_input["Speed Limit"].isna(), "we-af-prd"] = model_with_speedlimit.predict(
        data_input[predictors_speedlimit][~data_input["Speed Limit"].isna()]
    )
    data_input.loc[data_input["Speed Limit"].isna(), "we-af-prd"] = model_without_speedlimit.predict(
        data_input[predictors_no_speedlimit][data_input["Speed Limit"].isna()]
    )
    
    # predict speeds for weekend evening
    data_input.loc[:, "Day: Weekday"] = 0
    data_input.loc[:, "Hour: AM Peak"] = 0
    data_input.loc[:, "Hour: PM Peak"] = 1
    data_input.loc[~data_input["Speed Limit"].isna(), "we-pm-prd"] = model_with_speedlimit.predict(
        data_input[predictors_speedlimit][~data_input["Speed Limit"].isna()]
    )
    data_input.loc[data_input["Speed Limit"].isna(), "we-pm-prd"] = model_without_speedlimit.predict(
        data_input[predictors_no_speedlimit][data_input["Speed Limit"].isna()]
    )
    
    # impute tomtom speeds with median values
    for column in [
        "wd-am-prd",
        "wd-af-prd",
        "wd-pm-prd",
        "we-am-prd",
        "we-af-prd",
        "we-pm-prd",
    ]:
        # median_speeds = data.groupby("highway")[column].median()
        data_other.loc[:,column] = data_other.loc[:,"sl-med-imp"]
    
    
    data_input = data_input.drop(columns=['Opp Density', 'Traffic Signal/Crossing/Traffic Stop Density', 'Day: Weekday', 'Hour: AM Peak', 'Hour: PM Peak'])
    
    data_output = pd.concat([data_input, data_other], ignore_index=True)
    data_output = data_output.sort_values(by="ID")    
    
    # Fill NAs in "osm-sl-mph" by median values by road class
    data_output["Speed Limit"] = data_output["sl-med-imp"]
    
    # Add "drive-mode" columns with all values TRUE
    data_output.loc[:, "drive-mode"] = True
    
    # save network with predicted speeds
    data_output = data_output.rename(
        columns={
            "ID": "osm-id",
            "Speed Limit": "osm-sl-mph",
            "geometry": "wkt",
        }
    )
    
    data_output = data_output[
        [
            "osm-id",
            "highway",
            "osm-sl-mph",
            "wd-am-prd",
            "wd-af-prd",
            "wd-pm-prd",
            "we-am-prd",
            "we-af-prd",
            "we-pm-prd",
            "sl-med-imp",
            "length-met",
            "drive-mode",
            "wkt",
        ]
    ]
    
    # data_output = gpd.GeoDataFrame(data_output, geometry="geometry")
    # output_path = f"./data/{city_name}/network_with_predicted_speeds.shp"
    output_path = f"./data/{city_name}/network_with_predicted_speeds.csv"

    data_output.to_csv(
        output_path,
        index=False,
    )
    # print message
    print(f"Speed predicted for {city_name}!")
    ## ----------------------------------------------
    
cities = pd.read_csv("./cities_congestion_2025.csv")

for i in range(82,86):
    city_name = cities[cities["num"] == i]["name"].values[0]
    predict_speeds(city_name)
# data = gpd.read_file("./Data/new_haven_con25/network_with_predicted_speeds.shp")

data = gpd.read_file("./Data/new_haven_con25/network_with_predicted_speeds.shp")
# data = pd.read_csv(
#     "./data/new_haven_con25/network_with_predicted_speeds.csv",
#     engine="python",
#     encoding="latin1",
# )
# data["geometry"] = data["wkt"].apply(loads)
# data = gpd.GeoDataFrame(data, geometry="geometry")
# data.set_crs("EPSG:3857", inplace=True)
# data.to_crs("EPSG:4326", inplace=True)
# some stats
data["wd-am-prd"].describe()
# data["med-wd-am"].describe()
data["we-am-prd"].describe()
# data["med-we-am"].describe()
data["wd-af-prd"].describe()
# data["med-wd-af"].describe()
data["we-af-prd"].describe()
# data["med-we-af"].describe()
data["wd-pm-prd"].describe()
# data["med-wd-pm"].describe()
data["we-pm-prd"].describe()
# data["med-we-pm"].describe()
data["sl-med-imp"].describe()
# data["sl-85-imp"].describe()
# data[data["highway"] == "motorway"]["we-pm-prd"].describe()
# data[data["highway"] == "motorway"]["med-we-pm"].describe()
# data[data["highway"] == "trunk"]["we-pm-prd"].describe()
# data[data["highway"] == "trunk"]["med-we-pm"].describe()
# data[data["highway"] == "primary"]["we-pm-prd"].describe()
# data[data["highway"] == "primary"]["med-we-pm"].describe()
# data[data["highway"] == "secondary"]["we-pm-prd"].describe()
# data[data["highway"] == "secondary"]["med-we-pm"].describe()
# data[data["highway"] == "tertiary"]["we-pm-prd"].describe()
# data[data["highway"] == "tertiary"]["med-we-pm"].describe()
# data[data["highway"] == "residential"]["we-pm-prd"].describe()
# data[data["highway"] == "residential"]["med-we-pm"].describe()
# data[data["highway"] == "service"]["we-pm-prd"].describe()
# data[data["highway"] == "service"]["med-we-pm"].describe()
# data[data["highway"] == "other"]["we-pm-prd"].describe()
# data[data["highway"] == "other"]["med-we-pm"].describe()
# data["highway"].value_counts()
# data[data["highway"] == "road"]

import pandas as pd
import matplotlib.pyplot as plt
import seaborn as sns

data = gpd.read_file("./Data/portland_or_wa_con25/network_with_predicted_speeds.shp")

# Step 1: Rename columns for clarity
data = data.rename(columns={
    "sl-med-imp": "Speed Limit",
    "wd-am-prd": "Weekday Morning",
    "wd-af-prd": "Weekday Afternoon",
    "wd-pm-prd": "Weekday Evening",
    "we-am-prd": "Weekend Morning",
    "we-af-prd": "Weekend Afternoon",
    "we-pm-prd": "Weekend Evening"
})
data['highway'] = data['highway'].replace("unclassified", "Other")
data['highway'] = data['highway'].str.capitalize()
# Step 2: Melt to long format
melted = data.melt(id_vars='highway',
                 value_vars=[
                     'Speed Limit', 'Weekday Morning', 'Weekday Afternoon', 'Weekday Evening',
                     'Weekend Morning', 'Weekend Afternoon', 'Weekend Evening'
                 ],
                 var_name='Period',
                 value_name='Speed')

# Step 3: Group by and take median
grouped = melted.groupby(['Period', 'highway'])['Speed'].median().reset_index()

# Optional: Order road classes and time periods
road_order = ["Motorway", "Trunk", "Primary", "Secondary", "Tertiary", "Residential", "Service", "Other"]
period_order = [
    'Speed Limit',
    'Weekday Morning', 'Weekday Afternoon', 'Weekday Evening',
    'Weekend Morning', 'Weekend Afternoon', 'Weekend Evening'
]
grouped['highway'] = pd.Categorical(grouped['highway'], categories=road_order, ordered=True)
grouped['Period'] = pd.Categorical(grouped['Period'], categories=period_order, ordered=True)

# Step 4: Plot
plt.figure(figsize=(12, 6))
sns.barplot(data=grouped, x='Period', y='Speed', hue='highway')
plt.ylabel("Median Speed (mph)")
plt.xticks(rotation=30)
plt.legend(title="Road Class", bbox_to_anchor=(1.05, 1), loc='upper left', fontsize=12, title_fontsize=13)
plt.tight_layout()
plt.show()