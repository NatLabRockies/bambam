# import modules
import pandas as pd
import numpy as np
from scipy.stats import chi2_contingency
from sklearn.model_selection import train_test_split
from sklearn.linear_model import LinearRegression
from sklearn.ensemble import RandomForestRegressor
from sklearn.tree import DecisionTreeRegressor
from sklearn.neural_network import MLPRegressor
import xgboost as xgb
from sklearn.metrics import mean_squared_error, mean_absolute_error
import shap
import matplotlib.pyplot as plt
import matplotlib.colors as mcolors
from joblib import dump
from sklearn.model_selection import GridSearchCV
from sklearn.model_selection import RandomizedSearchCV
from scipy.stats import uniform, randint
import time

## prepare data for ML models-----------------------------------------
# import prepared data
cities = pd.read_csv("./cities_congestion_2025.csv")

data_train = pd.DataFrame()

#for i in [1] + list(range(3,5)) + list(range(6,12)):
for i in range(71,81):
    city_name = cities[cities["num"] == i]["name"].values[0]
    path = f"./data/{city_name}/prepared_model_input.csv"
    data_i = pd.read_csv(path)
    data_train = pd.concat([data_train, data_i], ignore_index=True)
            
# for calibration, keep observations for median speeds only
data_train = data_train[data_train["Speed Type"] == "med"]
# exclude night time observations
data_train = data_train[data_train["Hour: Night"] == False]
print(data_train.shape)

# exclude links without speed values (for model development)
data_train = data_train.dropna(subset=["Speed"])
print(data_train.shape)

# keep high functional class roads
# Define the high functional class
data_train[['Speed Limit']] = np.where(data_train[['Road Class: Link']], 
                                       np.nan, 
                                       data_train[['Speed Limit']])

road_classes = [
    'Road Class: Motorway',
    'Road Class: Trunk',
    'Road Class: Link',
    'Road Class: Primary',
    'Road Class: Secondary',
    'Road Class: Tertiary'
]
data_train = data_train[data_train[road_classes].any(axis=1)]
print(data_train.shape)

########
data_train_0 = data_train
data_train = data_train_0

# dataframe for ml models (for links having speed limit feature)
data_train = data_train.dropna(subset=["Speed Limit"])
print(data_train.shape)
# keep links with probe size >= 200

data_train = data_train[data_train["Probe Size"] >= 200].reset_index()
print(data_train.shape)

# merge attributes
opp_columns = ["School/Religious Opp Density", "Medical Opp Density", "Shopping/Errands Opp Density", "Recreational Opp Density", "Meals Opp Density"]
data_train["Opp Density"]= data_train[opp_columns].sum(axis=1)

Signal_Crossing_Stop_columns = ["Traffic Signal Density", "Crossing Density", "Traffic Stop Density"]
data_train["Traffic Signal/Crossing/Traffic Stop Density"]= data_train[Signal_Crossing_Stop_columns].sum(axis=1)

# train and test sets with panel consideration
unique_id = data_train["ID"].unique()
train_id, _ = train_test_split(unique_id, train_size=0.8, random_state=42)
train_data = data_train[data_train["ID"].isin(train_id)]
test_data = data_train[~data_train["ID"].isin(train_id)]
print("Training set dimensions:", train_data.shape)
print("Testing set dimensions:", test_data.shape)

# split the data into features (X) and target (y)
predictors = [
    "Speed Limit",
    "Population Density",
    "Employment Density",
    #######
    # "School/Religious Opp Density",
    # "Medical Opp Density",
    # "Shopping/Errands Opp Density",
    # "Recreational Opp Density",
    # "Meals Opp Density",
    # ########
    # "Traffic Signal Density",
    # "Traffic Stop Density",
    # "Crossing Density",
    # "Streetlamp Density",
    ########
    "Day: Weekday",
    # # "Day: Weekend",
    # # "Hour: Afternoon",
    "Hour: AM Peak",
    "Hour: PM Peak",
    # "Hour: Night",
    # "Road Class: Residential",
    "Road Class: Motorway",
    "Road Class: Trunk",
    "Road Class: Primary",
    "Road Class: Secondary",
    "Road Class: Tertiary",
    "Road Class: Service",
    "Road Class: Link",
    "Road Class: Other",
    # "Bidirectional",
    # "Lanes",
    # "Lanes = 1",
    # "Lanes = 2",
    # "Lanes = 3",
    # "Lanes = 4",
    # "Lanes = 5",
    # "Lanes = 6",
    # "Lanes = Missing",
    # "TTI",
    # "Population Density (City)",
    # "Employment Density (City)",
    # "Opp Density (City)",
    # "Road Length (City)",
    # "USI_den",
    # "USI_mix",
    # "USI_cen",
    # "USI_str",
    "Opp Density",
    "Traffic Signal/Crossing/Traffic Stop Density",
]

X_train = train_data[predictors]
y_train = train_data["Speed"]
X_test = test_data[predictors]
y_test = test_data["Speed"]
## -----------------------------------------

## XGBoost model-----------------------------------------

# define the parameter grid
start_time = time.time()
param_grid = {
    "n_estimators": randint(100, 401),
    "max_depth": randint(4, 15),
    "learning_rate": uniform(0.01, 0.29),
    "subsample": uniform(0.7, 0.3),
    "colsample_bytree": uniform(0.7, 0.3),
    "colsample_bylevel": uniform(0.7, 0.3),
    "gamma": uniform(0, 5),
    "reg_alpha": uniform(0, 1),
    "reg_lambda": uniform(0, 1),
    "min_child_weight": randint(1, 10),
    "scale_pos_weight": uniform(1, 10),
}

# initialize grid search
grid_search = RandomizedSearchCV(
    estimator=xgb.XGBRegressor(random_state=42),
    param_distributions=param_grid,
    n_iter=100,
    cv=5,
    n_jobs=-1,
    verbose=2,
    scoring="neg_mean_squared_error",
    random_state=42,
)
grid_search.fit(X_train, y_train)

# retrieve the best parameters and model
best_params = grid_search.best_params_
best_model = grid_search.best_estimator_
print("Training time: %s seconds" % (time.time() - start_time))

# print("Best parameters for XgBoost: ", best_params)
with open(
    "./Outputs/Models with speed limit/xgboost_parameters.txt",
    "w",
) as f:
    f.write("Grid search parameter grid:\n")
    for key, value in param_grid.items():
        f.write("{}: {}\n".format(key, value))
    f.write("\nBest parameters found by grid search:\n")
    for key, value in grid_search.best_params_.items():
        f.write("{}: {}\n".format(key, value))

# save this best model
dump(
    best_model,
    "./Outputs/Models with speed limit/xgboost_model_with_speedlimit.joblib",
)

# make predictions
start_time = time.time()

test_data["predicted_speed"] = best_model.predict(X_test)
print("Testing time: %s seconds" % (time.time() - start_time))

# evaluate the model
with open("./Outputs/Models with speed limit/xgboost.txt", "w") as f:
    rmse = np.sqrt(mean_squared_error(test_data["Speed"], test_data["predicted_speed"]))
    f.write("Root Mean Squared Error (RMSE) for XGBoost: {}\n".format(rmse))
    mae = mean_absolute_error(test_data["Speed"], test_data["predicted_speed"])
    f.write("Mean Absolute Error (MAE) for XGBoost: {}\n".format(mae))
    smape = np.mean(
        2
        * np.abs(y_test - test_data["predicted_speed"])
        / (np.abs(y_test) + np.abs(test_data["predicted_speed"]))
        * 100
    )
    f.write(
        "Symmetric Mean Absolute Percentage Error (SMAPE) for XGBoost: {}\n".format(
            smape
        )
    )

# plot of Actual vs predicted speed
norm = mcolors.BoundaryNorm(
    boundaries=[1, 10, 25, 50, 100, 200, 300, 400, 500, 1200], ncolors=256
)
cmap = plt.cm.YlOrRd
cmap.set_under("white")
plt.figure(figsize=(6, 7.2))
plt.hexbin(
    x=test_data["Speed"],
    y=test_data["predicted_speed"],
    gridsize=50,
    cmap=cmap,
    norm=norm,
)
plt.plot([0, 100], [0, 100], color="gray", linestyle="--")
plt.xlabel("Actual Speed (mph)")
plt.ylabel("Predicted Speed (mph)")
cb = plt.colorbar(label="Observations", orientation="horizontal")
n = format(len(test_data), ",")
plt.text(0.95, 0.05, f"n = {n}", ha="right", va="bottom", transform=plt.gca().transAxes)
plt.savefig(
    "./Outputs/Models with speed limit/xgboost_prediction.png",
    bbox_inches="tight",
)
plt.close()
# calculate SHAP values

from sklearn.preprocessing import LabelEncoder
for col in X_train.select_dtypes(include=['bool']).columns:
    X_train[col] = LabelEncoder().fit_transform(X_train[col])
    
explainer = shap.Explainer(best_model, X_train)
shap_values = explainer(X_test)

# plot SHAP values
shap.summary_plot(shap_values, X_test, plot_type="bar", show=False)
plt.xlabel("Mean |SHAP value| (average impact)")
plt.savefig("./Outputs/Models with speed limit/xgboost_shap_summary.png")
plt.close()
shap.summary_plot(shap_values, X_test, show=False)
plt.savefig("./Outputs/Models with speed limit/xgboost_shap_values.png")
plt.close()
## -----------------------------------------



### model without speed limit
data_train = data_train_0

# dataframe for ml models (for links not having speed limit feature)
data_train = data_train[pd.isna(data_train["Speed Limit"])]
print(data_train.shape)

# keep links with probe size >= 200
data_train = data_train[data_train["Probe Size"] >= 200].reset_index()
print(data_train.shape)

# merge attributes
opp_columns = ["School/Religious Opp Density", "Medical Opp Density", "Shopping/Errands Opp Density", "Recreational Opp Density", "Meals Opp Density"]
data_train["Opp Density"]= data_train[opp_columns].sum(axis=1)

Signal_Crossing_Stop_columns = ["Traffic Signal Density", "Crossing Density", "Traffic Stop Density"]
data_train["Traffic Signal/Crossing/Traffic Stop Density"]= data_train[Signal_Crossing_Stop_columns].sum(axis=1)

# train and test sets with panel consideration
unique_id = data_train["ID"].unique()
train_id, _ = train_test_split(unique_id, train_size=0.8, random_state=42)
train_data = data_train[data_train["ID"].isin(train_id)]
test_data = data_train[~data_train["ID"].isin(train_id)]
print("Training set dimensions:", train_data.shape)
print("Testing set dimensions:", test_data.shape)

# split the data into features (X) and target (y)
predictors = [
    # "Speed Limit",
    "Population Density",
    "Employment Density",
    #######
    # "School/Religious Opp Density",
    # "Medical Opp Density",
    # "Shopping/Errands Opp Density",
    # "Recreational Opp Density",
    # "Meals Opp Density",
    # ########
    # "Traffic Signal Density",
    # "Traffic Stop Density",
    # "Crossing Density",
    # "Streetlamp Density",
    ########
    "Day: Weekday",
    # # "Day: Weekend",
    # # "Hour: Afternoon",
    "Hour: AM Peak",
    "Hour: PM Peak",
    # "Hour: Night",
    # "Road Class: Residential",
    "Road Class: Motorway",
    "Road Class: Trunk",
    "Road Class: Primary",
    "Road Class: Secondary",
    "Road Class: Tertiary",
    "Road Class: Service",
    "Road Class: Link",
    "Road Class: Other",
    # "Bidirectional",
    # "Lanes",
    # "Lanes = 1",
    # "Lanes = 2",
    # "Lanes = 3",
    # "Lanes = 4",
    # "Lanes = 5",
    # "Lanes = 6",
    # "Lanes = Missing",
    # "TTI",
    # "Population Density (City)",
    # "Employment Density (City)",
    # "Opp Density (City)",
    # "Road Length (City)",
    # "USI_den",
    # "USI_mix",
    # "USI_cen",
    # "USI_str",
    "Opp Density",
    "Traffic Signal/Crossing/Traffic Stop Density",
]

X_train = train_data[predictors]
y_train = train_data["Speed"]
X_test = test_data[predictors]
y_test = test_data["Speed"]
## -----------------------------------------

## XGBoost model-----------------------------------------

# define the parameter grid
start_time = time.time()
param_grid = {
    "n_estimators": randint(100, 401),
    "max_depth": randint(4, 15),
    "learning_rate": uniform(0.01, 0.29),
    "subsample": uniform(0.7, 0.3),
    "colsample_bytree": uniform(0.7, 0.3),
    "colsample_bylevel": uniform(0.7, 0.3),
    "gamma": uniform(0, 5),
    "reg_alpha": uniform(0, 1),
    "reg_lambda": uniform(0, 1),
    "min_child_weight": randint(1, 10),
    "scale_pos_weight": uniform(1, 10),
}

# initialize grid search
grid_search = RandomizedSearchCV(
    estimator=xgb.XGBRegressor(random_state=42),
    param_distributions=param_grid,
    n_iter=100,
    cv=5,
    n_jobs=-1,
    verbose=2,
    scoring="neg_mean_squared_error",
    random_state=42,
)
grid_search.fit(X_train, y_train)

# retrieve the best parameters and model
best_params = grid_search.best_params_
best_model = grid_search.best_estimator_
print("Training time: %s seconds" % (time.time() - start_time))

# print("Best parameters for XgBoost: ", best_params)
with open(
    "./Outputs/Models without speed limit/xgboost_parameters.txt",
    "w",
) as f:
    f.write("Grid search parameter grid:\n")
    for key, value in param_grid.items():
        f.write("{}: {}\n".format(key, value))
    f.write("\nBest parameters found by grid search:\n")
    for key, value in grid_search.best_params_.items():
        f.write("{}: {}\n".format(key, value))

# save this best model
dump(
    best_model,
    "./Outputs/Models without speed limit/xgboost_model_without_speedlimit.joblib",
)

# make predictions
start_time = time.time()

test_data["predicted_speed"] = best_model.predict(X_test)
print("Testing time: %s seconds" % (time.time() - start_time))

# evaluate the model
with open("./Outputs/Models without speed limit/xgboost.txt", "w") as f:
    rmse = np.sqrt(mean_squared_error(test_data["Speed"], test_data["predicted_speed"]))
    f.write("Root Mean Squared Error (RMSE) for XGBoost: {}\n".format(rmse))
    mae = mean_absolute_error(test_data["Speed"], test_data["predicted_speed"])
    f.write("Mean Absolute Error (MAE) for XGBoost: {}\n".format(mae))
    smape = np.mean(
        2
        * np.abs(y_test - test_data["predicted_speed"])
        / (np.abs(y_test) + np.abs(test_data["predicted_speed"]))
        * 100
    )
    f.write(
        "Symmetric Mean Absolute Percentage Error (SMAPE) for XGBoost: {}\n".format(
            smape
        )
    )

# plot of Actual vs predicted speed
norm = mcolors.BoundaryNorm(
    boundaries=[1, 10, 25, 50, 100, 200, 300, 400, 500, 1200], ncolors=256
)
cmap = plt.cm.YlOrRd
cmap.set_under("white")
plt.figure(figsize=(6, 7.2))
plt.hexbin(
    x=test_data["Speed"],
    y=test_data["predicted_speed"],
    gridsize=50,
    cmap=cmap,
    norm=norm,
)
plt.plot([0, 100], [0, 100], color="gray", linestyle="--")
plt.xlabel("Actual Speed (mph)")
plt.ylabel("Predicted Speed (mph)")
cb = plt.colorbar(label="Observations", orientation="horizontal")
n = format(len(test_data), ",")
plt.text(0.95, 0.05, f"n = {n}", ha="right", va="bottom", transform=plt.gca().transAxes)
plt.savefig(
    "./Outputs/Models without speed limit/xgboost_prediction.png",
    bbox_inches="tight",
)
plt.close()
# calculate SHAP values

from sklearn.preprocessing import LabelEncoder
for col in X_train.select_dtypes(include=['bool']).columns:
    X_train[col] = LabelEncoder().fit_transform(X_train[col])
    
explainer = shap.Explainer(best_model, X_train)
shap_values = explainer(X_test)

# plot SHAP values
shap.summary_plot(shap_values, X_test, plot_type="bar", show=False)
plt.xlabel("Mean |SHAP value| (average impact)")
plt.savefig("./Outputs/Models without speed limit/xgboost_shap_summary.png")
plt.close()
shap.summary_plot(shap_values, X_test, show=False)
plt.savefig("./Outputs/Models without speed limit/xgboost_shap_values.png")
plt.close()
## -----------------------------------------
