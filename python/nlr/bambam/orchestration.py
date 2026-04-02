import json
from pathlib import Path
from typing import Any, Dict, Optional
from consist import ExecutionOptions, Tracker
import pandas as pd


# ── Defaults from tasks.md ──────────────────────────────────────────────────
BUFFER_RADIUS_KM = 40
OMF_VERSION = "latest"
USE_SLURM = False
MODES_CONFIG = "configuration/bambam-omf/travel-mode-filter.json"
CATEGORY_MAPPING = None  # uses built-in hard-coded value
TIME_RANGE = "8-9am"
STARTING_EDGE_LIST_ID = 3  # 0=walk, 1=bike, 2=drive
ACS_TYPE = "five_year"
ACS_CATEGORIES = ["B0100101E"]
H3_RESOLUTION = 8  # ~1km²
BASE_CONFIG_VERSION = "v1"


def _ensure_and_write_json(path: Path, data: dict) -> Path:
    """Write *data* as JSON to *path*, creating parent dirs as needed."""
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2))
    return path


# ── Stub task functions ─────────────────────────────────────────────────────
# Each function represents a single pipeline step.  The body is a stub that
# writes placeholder output files so the pipeline can be wired end-to-end
# and executed once real implementations are supplied.


def import_network(
    region_name: str,
    extent: Dict[str, Any],
    output_dir: str = "output",
    buffer_radius: int = BUFFER_RADIUS_KM,
    modes_config: str = MODES_CONFIG,
    omf_version: str = OMF_VERSION,
    use_slurm: bool = USE_SLURM,
) -> Dict[str, str]:
    """bambam-omf network: download OvertureMaps network for region."""
    network_path = Path(output_dir) / region_name / "network"
    # TODO: call bambam-omf network CLI
    _ensure_and_write_json(network_path / "stub.json", {"stub": True})
    return {"network_path": str(network_path)}


def create_network_config(
    output_dir: str = "output",
) -> Dict[str, str]:
    """Create compass config fragment for the imported network."""
    fragment_path = Path(output_dir) / "config_fragments" / "network.json"
    # TODO: build real TOML fragment
    _ensure_and_write_json(fragment_path, {"network": {}})
    return {"network_config_fragment": str(fragment_path)}


def import_poi(
    extent: Dict[str, Any],
    output_dir: str = "output",
    buffer_radius: int = BUFFER_RADIUS_KM,
    category_mapping: Optional[str] = CATEGORY_MAPPING,
    use_slurm: bool = USE_SLURM,
) -> Dict[str, str]:
    """bambam-omf poi: download OvertureMaps points of interest."""
    poi_path = Path(output_dir) / "poi"
    # TODO: call bambam-omf poi CLI
    _ensure_and_write_json(poi_path / "stub.json", {"stub": True})
    return {"poi_path": str(poi_path)}


def create_poi_config(
    extent: Dict[str, Any],
    output_dir: str = "output",
    buffer_radius: int = BUFFER_RADIUS_KM,
    category_mapping: Optional[str] = CATEGORY_MAPPING,
) -> Dict[str, str]:
    """Create compass config fragment for opportunity data."""
    fragment_path = Path(output_dir) / "config_fragments" / "poi.json"
    # TODO: build real TOML fragment
    _ensure_and_write_json(fragment_path, {"poi": {}})
    return {"poi_config_fragment": str(fragment_path)}


def download_gtfs(
    extent: Dict[str, Any],
    output_dir: str = "output",
    use_slurm: bool = USE_SLURM,
) -> Dict[str, str]:
    """bambam-gtfs download: download transit agency archives."""
    gtfs_directory = Path(output_dir) / "gtfs" / "raw"
    # TODO: call bambam-gtfs download CLI
    _ensure_and_write_json(gtfs_directory / "stub.json", {"stub": True})
    return {"gtfs_directory": str(gtfs_directory)}


def preprocess_gtfs(
    date: str,
    output_dir: str = "output",
    time_range: str = TIME_RANGE,
    extent: Optional[Dict[str, Any]] = None,
    use_slurm: bool = USE_SLURM,
) -> Dict[str, str]:
    """bambam-gtfs preprocess-bundle: process archives into searchable edge lists."""
    processed_gtfs_directory = Path(output_dir) / "gtfs" / "processed"
    # TODO: call bambam-gtfs preprocess-bundle CLI
    _ensure_and_write_json(processed_gtfs_directory / "stub.json", {"stub": True})
    return {"processed_gtfs_directory": str(processed_gtfs_directory)}


def create_gtfs_config(
    output_dir: str = "output",
    starting_edge_list_id: int = STARTING_EDGE_LIST_ID,
) -> Dict[str, str]:
    """Create compass config fragment for GTFS edge lists."""
    fragment_path = Path(output_dir) / "config_fragments" / "gtfs.json"
    # TODO: build real TOML fragment
    _ensure_and_write_json(fragment_path, {"gtfs": {"starting_edge_list_id": starting_edge_list_id}})
    return {"gtfs_config_fragment": str(fragment_path)}


def create_population_config(
    acs_year: int,
    output_dir: str = "output",
    acs_type: str = ACS_TYPE,
    acs_categories: list = ACS_CATEGORIES,
    h3_resolution: int = H3_RESOLUTION,
) -> Dict[str, str]:
    """Create grid input plugin config fragment with ACS population."""
    fragment_path = Path(output_dir) / "config_fragments" / "population.json"
    # TODO: build real TOML fragment
    _ensure_and_write_json(fragment_path, {
        "grid_input_plugin": {
            "acs_year": acs_year,
            "acs_type": acs_type,
            "acs_categories": acs_categories,
            "h3_resolution": h3_resolution,
        }
    })
    return {"population_config_fragment": str(fragment_path)}


def create_base_compass_config(
    output_dir: str = "output",
    version: str = BASE_CONFIG_VERSION,
) -> Dict[str, str]:
    """Read and write the base compass config TOML."""
    compass_config_path = Path(output_dir) / "compass_config.toml"
    # TODO: copy base config from template
    compass_config_path.parent.mkdir(parents=True, exist_ok=True)
    compass_config_path.write_text(f"# base compass config {version}\n")
    return {"compass_config_path": str(compass_config_path)}


def append_mep_config(
    output_dir: str = "output",
) -> Dict[str, str]:
    """Append MEP computation via routee-compass eval plugin."""
    compass_config_path = Path(output_dir) / "compass_config.toml"
    # TODO: modify compass config in-place
    return {"compass_config_path": str(compass_config_path)}


def run_bambam(
    output_dir: str = "output",
) -> Dict[str, str]:
    """Execute the BAMBAM simulation."""
    results_path = Path(output_dir) / "results"
    # TODO: run bambam CLI
    _ensure_and_write_json(results_path / "stub.json", {"stub": True})
    return {"results_path": str(results_path)}


# ── Pipeline orchestration ──────────────────────────────────────────────────


def run_pipeline(
    region_name: str,
    extent: Dict[str, Any],
    acs_year: int,
    date: str = "2026-04-02",
    output_dir: str = "output",
    db_url: Path = Path("./provenance.duckdb"),
):
    """Run the full BAMBAM MVP pipeline via Consist.

    Parameters
    ----------
    region_name : str
        Name of the geographic region (e.g. "denver_co").
    extent : dict
        GeoJSON-like geometry defining the study area.
    acs_year : int
        American Community Survey year for population data.
    date : str
        Reference date for GTFS processing (YYYY-MM-DD).
    output_dir : str
        Root directory for pipeline outputs.
    db_url : str
        SQLAlchemy connection string for provenance tracking.
    """
    tracker = Tracker(db_url)

    shared_config = {
        "region_name": region_name,
        "extent": extent,
        "acs_year": acs_year,
        "date": date,
        "output_dir": output_dir,
    }

    with tracker.scenario("bambam-mvp", config=shared_config, tags=["mvp"]) as sc:

        # 1. Import network
        net_result = sc.run(
            import_network,
            name="import-network",
            config={
                "region_name": region_name,
                "extent": extent,
                "output_dir": output_dir,
                "buffer_radius": BUFFER_RADIUS_KM,
                "modes_config": MODES_CONFIG,
                "omf_version": OMF_VERSION,
                "use_slurm": USE_SLURM,
            },
            outputs=["network_path"],
            tags=["omf", "network"],
        )

        # 2. Create network config fragment
        net_cfg_result = sc.run(
            create_network_config,
            name="create-network-config",
            config={"output_dir": output_dir},
            inputs={"network": net_result},
            outputs=["network_config_fragment"],
            tags=["config"],
        )

        # 3. Import POI
        poi_result = sc.run(
            import_poi,
            name="import-poi",
            config={
                "extent": extent,
                "output_dir": output_dir,
                "buffer_radius": BUFFER_RADIUS_KM,
                "category_mapping": CATEGORY_MAPPING,
                "use_slurm": USE_SLURM,
            },
            outputs=["poi_path"],
            tags=["omf", "poi"],
        )

        # 4. Create POI config fragment
        poi_cfg_result = sc.run(
            create_poi_config,
            name="create-poi-config",
            config={
                "extent": extent,
                "output_dir": output_dir,
                "buffer_radius": BUFFER_RADIUS_KM,
                "category_mapping": CATEGORY_MAPPING,
            },
            inputs={"poi": poi_result},
            outputs=["poi_config_fragment"],
            tags=["config"],
        )

        # 5. Download GTFS
        gtfs_dl_result = sc.run(
            download_gtfs,
            name="download-gtfs",
            config={
                "extent": extent,
                "output_dir": output_dir,
                "use_slurm": USE_SLURM,
            },
            outputs=["gtfs_directory"],
            tags=["gtfs", "download"],
        )

        # 6. Preprocess GTFS
        gtfs_proc_result = sc.run(
            preprocess_gtfs,
            name="preprocess-gtfs",
            config={
                "date": date,
                "output_dir": output_dir,
                "time_range": TIME_RANGE,
                "extent": extent,
                "use_slurm": USE_SLURM,
            },
            inputs={"gtfs_raw": gtfs_dl_result},
            outputs=["processed_gtfs_directory"],
            tags=["gtfs", "preprocess"],
        )

        # 7. Create GTFS config fragment
        gtfs_cfg_result = sc.run(
            create_gtfs_config,
            name="create-gtfs-config",
            config={
                "output_dir": output_dir,
                "starting_edge_list_id": STARTING_EDGE_LIST_ID,
            },
            inputs={"gtfs_processed": gtfs_proc_result},
            outputs=["gtfs_config_fragment"],
            tags=["config"],
        )

        # 8. Create population config fragment
        pop_cfg_result = sc.run(
            create_population_config,
            name="create-population-config",
            config={
                "acs_year": acs_year,
                "output_dir": output_dir,
                "acs_type": ACS_TYPE,
                "acs_categories": ACS_CATEGORIES,
                "h3_resolution": H3_RESOLUTION,
            },
            outputs=["population_config_fragment"],
            tags=["config", "population"],
        )

        # 9. Create base compass config
        base_cfg_result = sc.run(
            create_base_compass_config,
            name="create-base-compass-config",
            config={"output_dir": output_dir, "version": BASE_CONFIG_VERSION},
            outputs=["compass_config_path"],
            tags=["config", "base"],
        )

        # 10. Append MEP computation
        mep_result = sc.run(
            append_mep_config,
            name="append-mep-config",
            config={"output_dir": output_dir},
            inputs={
                "base_config": base_cfg_result,
                "network_config": net_cfg_result,
                "poi_config": poi_cfg_result,
                "gtfs_config": gtfs_cfg_result,
                "population_config": pop_cfg_result,
            },
            outputs=["compass_config_path"],
            tags=["config", "mep"],
        )

        # 11. Run BAMBAM
        bambam_result = sc.run(
            run_bambam,
            name="run-bambam",
            config={"output_dir": output_dir},
            inputs={"compass_config": mep_result},
            outputs=["results_path"],
            tags=["bambam", "run"],
        )

        return bambam_result


if __name__ == "__main__":
    # Minimal invocation example with required inputs only
    result = run_pipeline(
        region_name="denver_co",
        extent={
            "type": "Polygon",
            "coordinates": [[
                [-105.05, 39.63],
                [-104.90, 39.63],
                [-104.90, 39.78],
                [-105.05, 39.78],
                [-105.05, 39.63],
            ]],
        },
        acs_year=2023,
    )