"""
pulls trees out of a result.json file and flattens the properties.
"""
import json
import argparse
from pathlib import Path

parser = argparse.ArgumentParser("tree de-mungificator™")
parser.add_argument("file")

if __name__ == "__main__":
    args = parser.parse_args()
    with open(args.file) as f:
        result = json.loads(f.read())
    if not isinstance(result, list):
        result = [result]

    in_file = Path(args.file)
    out_base = in_file.parent / in_file.name.split(".")[0]

    for idx, blob in enumerate(result):
        for feature in blob["tree"]["features"]:
            leg = feature["properties"]["state"]["active_leg"]
            mode = feature["properties"]["state"][f"leg_{leg}_mode"]
            feature["properties"]["edge_mode"] = mode
            nested = ["state", "cost"]
            for nest in nested:
                for k, v in feature["properties"][nest].items():
                    key = k if not k in feature["properties"] else f"{nest}.{k}"
                    feature["properties"][key] = v  
                del feature["properties"][nest]
        with open(f"{out_base}-tree-{idx}.json", "w") as f:
            f.write(json.dumps(blob["tree"], indent=2))
        