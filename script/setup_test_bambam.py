print("importing osmnx, compass")
import osmnx as ox
from nlr.routee.compass.io import generate_compass_dataset
from nlr.routee.compass.io.generate_dataset import GeneratePipelinePhase

if __name__ == "__main__":

    phases = [
        GeneratePipelinePhase.CONFIG,
        GeneratePipelinePhase.GRAPH
    ]

    print("downloading graph")
    g = ox.graph_from_place("Denver, Colorado, USA", network_type="drive")
    print("processing graph into compass dataset")
    generate_compass_dataset(g, output_directory="denver_co", phases=phases)

    # Boulder graph for GTFS
    g = ox.graph_from_place("Boulder, Colorado, USA", network_type="drive")
    print("processing graph into compass dataset")
    generate_compass_dataset(g, output_directory="boulder_co", phases=phases)