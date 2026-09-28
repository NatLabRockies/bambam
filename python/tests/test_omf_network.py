from pathlib import Path

import pytest
from pydantic import ValidationError

from nlr.bambam.orchestrate.omf import OmfNetworkArgs, run_omf_network


def test_schema_parity_with_rust() -> None:
    """Ensure the Python Pydantic schema matches the Rust schemars definition."""
    assert OmfNetworkArgs.check_schema_parity() is True

    rust_schema = OmfNetworkArgs.rust_json_schema()
    assert rust_schema["title"] == "OmfNetworkArgs"
    assert "name" in rust_schema["required"]
    assert "configuration_file" in rust_schema["required"]

    py_schema = OmfNetworkArgs.model_json_schema()
    for prop in rust_schema["properties"]:
        assert prop in py_schema["properties"]


def test_omf_network_args_validation() -> None:
    """Validate required fields and type safety."""
    with pytest.raises(ValidationError):
        OmfNetworkArgs()  # type: ignore[call-arg]

    args = OmfNetworkArgs(
        name="denver",
        configuration_file=Path("config.toml"),
        output_directory=Path("output"),
        store_raw=True,
    )
    assert args.name == "denver"
    assert args.configuration_file == "config.toml"
    assert args.output_directory == "output"
    assert args.store_raw is True
    assert args.omf_ids is False


def test_cli_args_conversion() -> None:
    """Validate translation from Pydantic model to CLI arguments."""
    args = OmfNetworkArgs(
        name="denver",
        configuration_file="config.toml",
        output_directory="out",
        local_source="cache.json",
        store_raw=True,
        omf_ids=True,
        extent_file="extent.json",
    )
    cli_args = args.to_cli_args()
    assert cli_args == [
        "network",
        "--name",
        "denver",
        "--configuration-file",
        "config.toml",
        "--output-directory",
        "out",
        "--local-source",
        "cache.json",
        "--store-raw",
        "--omf-ids",
        "--extent-file",
        "extent.json",
    ]


def test_run_omf_network_error_propagation() -> None:
    """Validate error propagation from the Rust runtime to Python."""
    with pytest.raises(RuntimeError, match="not found"):
        run_omf_network(
            name="test",
            configuration_file="nonexistent_config_file_for_test.toml",
        )
