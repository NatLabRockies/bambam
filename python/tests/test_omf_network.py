from pathlib import Path

import pytest
from pydantic import ValidationError

from nlr.bambam.orchestrate.omf import OmfNetworkArgs, run_omf_network
from nlr.bambam.orchestrate.network import (
    GeographicExtentBbox,
    GeographicExtentWkt,
    NetworkImportRequest,
    OpenStreetMapDataSource,
    OvertureMapsDataSource,
    _coerce_extent,
    _coerce_source,
)


def test_schema_parity_with_rust() -> None:
    """Ensure the Python Pydantic schema matches the Rust schemars definition."""
    assert OmfNetworkArgs.check_schema_parity() is True

    rust_schema = OmfNetworkArgs.rust_json_schema()
    assert rust_schema["title"] == "OmfNetworkArgs"
    assert "configuration_file" in rust_schema["required"]
    assert "output_directory" in rust_schema["required"]

    py_schema = OmfNetworkArgs.model_json_schema()
    for prop in rust_schema["properties"]:
        assert prop in py_schema["properties"]


def test_omf_network_args_validation() -> None:
    """Validate required fields and type safety."""
    with pytest.raises(ValidationError):
        OmfNetworkArgs()  # type: ignore[call-arg]

    args = OmfNetworkArgs(
        configuration_file=Path("config.toml"),
        output_directory=Path("output"),
        infrastructure_match_distance_meters=15.0,
    )
    assert args.configuration_file == "config.toml"
    assert args.output_directory == "output"
    assert args.infrastructure_match_distance_meters == 15.0


def test_cli_args_conversion() -> None:
    """Validate translation from Pydantic model to CLI arguments."""
    args = OmfNetworkArgs(
        configuration_file="config.toml",
        output_directory="out",
        local_source="raw",
        extent_file="extent.json",
        infrastructure_match_distance_meters=15.0,
    )
    cli_args = args.to_cli_args()
    assert cli_args == [
        "network",
        "--configuration-file",
        "config.toml",
        "--output-directory",
        "out",
        "--local-source",
        "raw",
        "--extent-file",
        "extent.json",
        "--infrastructure-match-distance-meters",
        "15.0",
    ]


def test_run_omf_network_error_propagation() -> None:
    """Validate error propagation from the Rust runtime to Python."""
    with pytest.raises(RuntimeError, match="not found"):
        run_omf_network(
            configuration_file="nonexistent_config_file_for_test.toml",
            output_directory="out",
            local_source="raw",
        )


def test_unified_network_import_request_coercion() -> None:
    """Validate high-level extent and data source coercion on NetworkImportRequest."""
    bbox_ext = _coerce_extent((-105.3, 40.0, -105.2, 40.1))
    assert isinstance(bbox_ext, GeographicExtentBbox)
    assert bbox_ext.xmin == -105.3

    wkt_ext = _coerce_extent("POLYGON ((0 0, 1 0, 1 1, 0 1, 0 0))")
    assert isinstance(wkt_ext, GeographicExtentWkt)

    omf_src = _coerce_source("omf")
    assert isinstance(omf_src, OvertureMapsDataSource)

    osm_src = _coerce_source("osm", pbf_path="region.osm.pbf")
    assert isinstance(osm_src, OpenStreetMapDataSource)
    assert osm_src.pbf_path == "region.osm.pbf"

    req = NetworkImportRequest(
        extent=bbox_ext,
        source=omf_src,
        output_directory="out_dir",
        modes=["walk", "drive"],
    )
    dumped = req.to_rust_dict()
    assert dumped["extent"]["type"] == "bbox"
    assert dumped["source"]["type"] == "overture_maps"
    assert dumped["modes"] == ["walk", "drive"]
