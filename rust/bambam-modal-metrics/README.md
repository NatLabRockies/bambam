# `bambam-modal-metrics`

## A library crate containing:

Extensible traits for road network data that can facilitate modal metric computations

The logic for computing modal metrics such as the Walking Comfort Index (WCI) or the Level of Traffic Stress (LTS)

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