# bambam-py

## Python API

### bambam-osm 

---

### 1. Basic In-Process Execution with Pydantic Validation

You can construct an `OmfNetworkArgs` model directly. Pydantic validates inputs before passing them across the PyO3 boundary into Rust:

```python
from pathlib import Path
from nlr.bambam.orchestrate.omf import OmfNetworkArgs

# Accepts strings or pathlib.Path objects
args = OmfNetworkArgs(
    name="minnesota-flex",
    configuration_file=Path("configuration/bambam-omf/travel-mode-filter.json"),
    output_directory=Path("output/minnesota"),
    store_raw=True,
    omf_ids=True,
    extent_file=Path("query/boulder_extent.json"),  # optional
)

# Execute the import: releases the Python GIL during network download & processing
args.run()
```

---

### 2. Functional Shorthand

If you prefer a direct function call without instantiating the model manually:

```python
from nlr.bambam.orchestrate.omf import run_omf_network

run_omf_network(
    name="denver_region",
    configuration_file="configuration/bambam-omf/travel-mode-filter.json",
    output_directory="output/denver",
    store_raw=False,
)
```

You can also pass a standard dictionary (e.g., loaded from a YAML or JSON workflow recipe):

```python
recipe = {
    "name": "denver_region",
    "configuration_file": "configuration/bambam-omf/travel-mode-filter.json",
    "output_directory": "output/denver",
}

run_omf_network(recipe)
```

---

### 3. CLI Subprocess Fallback (`to_cli_args`)

If you want to invoke `bambam-omf network` in a separate OS process (for example, in a Celery worker, Airflow bash operator, or separate shell environment), use `.to_cli_args()` to generate the exact CLI command:

```python
import subprocess
from nlr.bambam.orchestrate.omf import OmfNetworkArgs

args = OmfNetworkArgs(
    name="minnesota-flex",
    configuration_file="configuration/bambam-omf/travel-mode-filter.json",
    output_directory="output/minnesota",
    store_raw=True,
)

# Generates: ['network', '--name', 'minnesota-flex', '--configuration-file', ...]
cmd = ["bambam-omf"] + args.to_cli_args()
subprocess.run(cmd, check=True)
```

---

### 4. Schema Parity & Contract Verification

To ensure your Python Pydantic definitions and the Rust `schemars` definitions have not drifted out of sync (useful in unit tests or CI pipelines):

```python
from nlr.bambam.orchestrate.omf import OmfNetworkArgs

# 1. Verify that field names and required constraints match exactly
assert OmfNetworkArgs.check_schema_parity() is True

# 2. Inspect the raw JSON Schema exported directly from the compiled Rust library
schema = OmfNetworkArgs.rust_json_schema()
print("Rust Title:", schema["title"])
print("Required fields according to Rust:", schema["required"])
```
