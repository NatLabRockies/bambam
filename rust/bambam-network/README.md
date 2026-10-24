# `bambam-network`

Shared split-based road network ETL, island detection, spatial infrastructure attachment, and RouteE Compass serialization core.

## Overview

`bambam-network` provides a source-agnostic pipeline for importing continental-scale road networks into [RouteE Compass](https://github.com/NatLabRockies/routee-compass) format. Both `bambam-omf` (Overture Maps Foundation) and `bambam-osm` (OpenStreetMap) implement its traits:

- **`ImportConnector`**: Represents intersection/control vertices.
- **`ImportInfrastructure`**: Represents traffic control and comfort infrastructure (`traffic_signals`, `stop`, `give_way`, `crossing`) spatially snapped to vertices via an `rstar::RTree` within a configurable Haversine tolerance (default `15.0`m).
- **`ImportSegment<C>`**: Represents road segments split at topological connectors via linear referencing (`SegmentSplit`), eliminating destructive in-place graph simplification.

## Type Alignment Across Crates

| Concept | `bambam-network` (Shared Core) | `bambam-omf` (Overture Maps) | `bambam-osm` (OpenStreetMap) |
|---|---|---|---|
| **Raw Collection** | — | `OmfCollection` (`TransportationCollection`) | `OsmCollection` |
| **Connector Input** | `ImportConnector` | `OmfConnectorRecord` (`TransportationConnectorRecord`) | `OsmConnectorRecord` |
| **Segment Input** | `ImportSegment<C>` | `OmfSegmentRecord` (`TransportationSegmentRecord`) | `OsmSegmentRecord` |
| **Infrastructure Input** | `ImportInfrastructure` | `OmfInfrastructureRecord` (`InfrastructureRecord`) | `OsmInfrastructureRecord` |
| **Split Primitives** | `ConnectorInSegment`, `SegmentSplit`, `SegmentHeading` | Re-exported from `bambam-network` | Re-exported from `bambam-network` |
| **Vectorized Edge List** | `VectorizedEdgeList<C>` | `OmfEdgeList` (`VectorizedEdgeList<SegmentFullType>`) | `OsmEdgeList` (`VectorizedEdgeList<Highway>`) |
| **Vectorized Graph** | — | `OmfGraphVectorized` | `OsmGraphVectorized` |
| **Output Records** | `VertexCompassRecord`, `VertexCompleteRecord`, `VertexMappingRecord`, `EdgeMappingRecord` | Re-exported from `bambam-network` | Re-exported from `bambam-network` + `OsmNodeDataSerializable`, `OsmWayDataSerializable` |

## Output Guarantees

Every network import produces a consistent directory structure:
- `raw/`: Always-on Snappy-compressed Parquet snapshots of the source records (`segments.parquet`, `connectors.parquet`, `infrastructure.parquet` for OMF; `nodes.parquet`, `ways.parquet` for OSM).
- `vertices-compass.csv.gz`: RouteE Compass vertex coordinates (`vertex_id,x,y`).
- `vertices-complete.csv.gz`: Enriched vertex records with `traffic_signals`, `stop`, `give_way`, `crossing`, and matched `infrastructure_ids`.
- `vertices-mapping.csv.gz`: Bidirectional mapping between `vertex_id` and source Connector/Node ID (`vertex_id,source_id`).
- `vertices-{traffic-signals,stop,give-way,crossing}-enumerated.txt.gz`: Enumerated boolean vertex control tables.
- `<mode>/edges-compass.csv.gz`: RouteE Compass directed edges (`edge_id,src_vertex_id,dst_vertex_id,distance`).
- `<mode>/edges-mapping.csv.gz`: Bidirectional mapping between `edge_id` and source Segment/Way ID with linear reference bounds (`edge_id,source_id,start_lr,end_lr,heading`).
- `<mode>/edges-{geometries,speeds-mph,classes,bearings}-enumerated.txt.gz` & `<mode>/edges-classes-speed-mapping.csv.gz`.
