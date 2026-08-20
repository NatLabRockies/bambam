# `bambam-modal-metrics`

## A library crate containing:

Extensible traits for road network data that can facilitate modal metric computations

The logic for computing modal metrics such as the Walking Comfort Index (WCI) or the Level of Traffic Stress (LTS)

This library is wired into `bambam` through the set of commands defined in `bamam_util`. You can run the `bambam_util modal_metric` command from the `bambam` crate to compute modal metrics such as WCI and LTS for a given road network. As of now, `bambam-modal-metrics` only supports OpenStreetMaps way/node data as input, but extension to OvertureMaps is planned.

## What is a modal metric?

A modal metric is a value that qualitatively describes links (or edges) in a transportation network for a specific modality. Two examples of this type of metric are:

### 1.  **Walking Comfort Index**: 
How comfortable are links in a network in terms of walkability?

#### Considerations:
- Link and neighboring link traffic speed
- Link type and characteristics (sidewalk? footway? cycleway?)
- Does the link contain either a stop sign or a traffic signal to allow for crossings and speed limiting?

### 2. **Level of Traffic Stress**: 
How stressful are links in a network for cyclists?

#### Considerations:
- Link and neighboring link traffic speed
- Link type
- Cycleway infrastructure
- Single lane vs. multi-lane