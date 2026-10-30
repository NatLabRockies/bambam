from __future__ import annotations

import json
import logging
from pathlib import Path
from typing import Any, Literal, Optional, Sequence, Union

from pydantic import BaseModel, ConfigDict, Field, field_validator

import nlr.bambam.bambam_py_api as _py_api

logger = logging.getLogger(__name__)


class GeographicExtentBbox(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["bbox"] = "bbox"
    xmin: float
    ymin: float
    xmax: float
    ymax: float


class GeographicExtentWkt(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["wkt"] = "wkt"
    wkt: str


class GeographicExtentWktFile(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["wkt_file"] = "wkt_file"
    path: str

    @field_validator("path", mode="before")
    @classmethod
    def _stringify_path(cls, v: Any) -> Any:
        return str(v) if isinstance(v, Path) else v


GeographicExtent = Union[
    GeographicExtentBbox,
    GeographicExtentWkt,
    GeographicExtentWktFile,
]


class OvertureMapsDataSource(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["overture_maps"] = "overture_maps"
    local_source: Optional[str] = None

    @field_validator("local_source", mode="before")
    @classmethod
    def _stringify_path(cls, v: Any) -> Any:
        return str(v) if isinstance(v, Path) else v


class OpenStreetMapDataSource(BaseModel):
    model_config = ConfigDict(extra="forbid")

    type: Literal["open_street_map"] = "open_street_map"
    pbf_path: str
    local_source: Optional[str] = None
    simplify: Optional[bool] = True
    truncate_by_edge: Optional[bool] = True

    @field_validator("pbf_path", "local_source", mode="before")
    @classmethod
    def _stringify_path(cls, v: Any) -> Any:
        return str(v) if isinstance(v, Path) else v


NetworkDataSource = Union[OvertureMapsDataSource, OpenStreetMapDataSource]


class NetworkImportRequest(BaseModel):
    """Unified high-level request for importing a RouteE Compass road network
    from either Overture Maps or OpenStreetMap over a geographic extent.

    Mirrors the Rust struct `bambam::app::network_import::NetworkImportRequest`.
    """

    model_config = ConfigDict(
        extra="forbid",
        validate_assignment=True,
        populate_by_name=True,
    )

    extent: GeographicExtent = Field(
        ...,
        description="Geographic extent (bounding box, WKT string, or WKT file).",
    )
    source: NetworkDataSource = Field(
        ...,
        description="Network data source ('overture_maps' or 'open_street_map').",
    )
    output_directory: str = Field(
        ...,
        description="Output directory where Compass network files, mappings, and raw/ Parquet files are written.",
    )
    configuration_file: Optional[str] = Field(
        default=None,
        description="Optional path to a network import configuration file (`bambam-network-config.json`).",
    )
    modes: Optional[list[str]] = Field(
        default=None,
        description="Optional travel modes (e.g., ['walk', 'bike', 'drive']).",
    )
    infrastructure_match_distance_meters: Optional[float] = Field(
        default=None,
        description="Optional maximum distance in meters for snapping infrastructure to vertices (defaults to 15.0).",
    )

    @field_validator("output_directory", "configuration_file", mode="before")
    @classmethod
    def _stringify_path(cls, v: Any) -> Any:
        return str(v) if isinstance(v, Path) else v

    def to_rust_dict(self) -> dict[str, Any]:
        return self.model_dump(mode="json")

    @classmethod
    def rust_json_schema(cls) -> dict[str, Any]:
        schema_fn = getattr(_py_api, "network_import_schema")
        return json.loads(schema_fn())  # type: ignore[no-any-return]

    def run(self) -> None:
        run_fn = getattr(_py_api, "run_network_import")
        run_fn(self.to_rust_dict())


def _coerce_extent(
    extent: Union[
        GeographicExtent,
        Sequence[float],
        str,
        Path,
        dict[str, Any],
    ],
) -> GeographicExtent:
    if isinstance(
        extent,
        (GeographicExtentBbox, GeographicExtentWkt, GeographicExtentWktFile),
    ):
        return extent
    if isinstance(extent, dict):
        t = extent.get("type")
        if t == "bbox":
            return GeographicExtentBbox(**extent)
        if t == "wkt":
            return GeographicExtentWkt(**extent)
        if t == "wkt_file":
            return GeographicExtentWktFile(**extent)
        if {"xmin", "ymin", "xmax", "ymax"}.issubset(extent.keys()):
            return GeographicExtentBbox(type="bbox", **extent)
    if isinstance(extent, Path):
        return GeographicExtentWktFile(path=str(extent))
    if isinstance(extent, str):
        stripped = extent.strip()
        if stripped.upper().startswith(("POLYGON", "MULTIPOLYGON")) or stripped.startswith("{"):
            return GeographicExtentWkt(wkt=stripped)
        return GeographicExtentWktFile(path=stripped)
    if isinstance(extent, (tuple, list)) and len(extent) == 4:
        xmin, ymin, xmax, ymax = extent
        return GeographicExtentBbox(
            xmin=float(xmin),
            ymin=float(ymin),
            xmax=float(xmax),
            ymax=float(ymax),
        )
    raise ValueError(
        f"Unsupported extent value: {extent!r}. Provide a (xmin, ymin, xmax, ymax) tuple, "
        "a WKT POLYGON/MULTIPOLYGON string, a WKT file path, or a GeographicExtent model."
    )


def _coerce_source(
    source: Union[NetworkDataSource, str, Path, dict[str, Any]],
    *,
    pbf_path: Optional[Union[str, Path]] = None,
    local_source: Optional[Union[str, Path]] = None,
    simplify: bool = True,
    truncate_by_edge: bool = True,
) -> NetworkDataSource:
    if isinstance(source, (OvertureMapsDataSource, OpenStreetMapDataSource)):
        return source
    if isinstance(source, dict):
        t = source.get("type")
        if t == "overture_maps":
            return OvertureMapsDataSource(**source)
        if t == "open_street_map":
            return OpenStreetMapDataSource(**source)
    if isinstance(source, Path):
        if str(source).endswith(".pbf"):
            return OpenStreetMapDataSource(
                pbf_path=str(source),
                simplify=simplify,
                truncate_by_edge=truncate_by_edge,
            )
        return OvertureMapsDataSource(local_source=str(source))
    if isinstance(source, str):
        norm = source.strip().lower()
        if norm in ("omf", "overture", "overture_maps"):
            return OvertureMapsDataSource(
                local_source=str(local_source) if local_source is not None else None
            )
        if norm in ("osm", "openstreetmap", "open_street_map"):
            if pbf_path is None:
                raise ValueError("Must provide `pbf_path` when `source='osm'`.")
            return OpenStreetMapDataSource(
                pbf_path=str(pbf_path),
                simplify=simplify,
                truncate_by_edge=truncate_by_edge,
            )
        if source.endswith(".pbf"):
            return OpenStreetMapDataSource(
                pbf_path=source,
                simplify=simplify,
                truncate_by_edge=truncate_by_edge,
            )
    raise ValueError(
        f"Unsupported source value: {source!r}. Expected 'omf', 'osm', a .osm.pbf path, or a NetworkDataSource model."
    )


def import_network(
    extent: Optional[
        Union[GeographicExtent, Sequence[float], str, Path, dict[str, Any]]
    ] = None,
    source: Optional[Union[NetworkDataSource, str, Path, dict[str, Any]]] = None,
    output_directory: Optional[Union[str, Path]] = None,
    *,
    request: Optional[NetworkImportRequest] = None,
    configuration_file: Optional[Union[str, Path]] = None,
    modes: Optional[Sequence[str]] = None,
    pbf_path: Optional[Union[str, Path]] = None,
    local_source: Optional[Union[str, Path]] = None,
    simplify: bool = True,
    truncate_by_edge: bool = True,
    infrastructure_match_distance_meters: Optional[float] = None,
) -> NetworkImportRequest:
    """High-level entry point to import a road network from Overture Maps or OpenStreetMap.

    Always retains raw data under `<output_directory>/raw/*.parquet` and writes
    standardized ID mappings (`vertices-mapping.csv.gz` and `edges-mapping.csv.gz`).
    """
    if request is not None:
        req = request
    else:
        if extent is None or source is None or output_directory is None:
            raise ValueError(
                "Must provide `extent`, `source`, and `output_directory` (or `request`)."
            )
        coerced_extent = _coerce_extent(extent)
        coerced_source = _coerce_source(
            source,
            pbf_path=pbf_path,
            local_source=local_source,
            simplify=simplify,
            truncate_by_edge=truncate_by_edge,
        )
        req = NetworkImportRequest(
            extent=coerced_extent,
            source=coerced_source,
            output_directory=str(output_directory),
            configuration_file=str(configuration_file) if configuration_file is not None else None,
            modes=list(modes) if modes is not None else None,
            infrastructure_match_distance_meters=infrastructure_match_distance_meters,
        )

    logger.info("Executing network import -> '%s'...", req.output_directory)
    req.run()
    return req
