use std::{fs, path::Path};

use serde::Deserialize;

use crate::{error::CongestionError, model::SpeedPredictor};

/// A single node in a flattened XGBoost regression tree.
#[derive(Debug, Clone, Copy)]
pub struct FlatTreeNode {
    /// `< 0` (`-1`) if this node is a leaf; otherwise the `0`-based index of the left child in `nodes`.
    pub left_child: i32,
    /// `< 0` (`-1`) if this node is a leaf; otherwise the `0`-based index of the right child in `nodes`.
    pub right_child: i32,
    /// Feature index to split on (ignored for leaves).
    pub split_index: u32,
    /// For internal nodes: split threshold (`feature_val < split_cond` goes left).
    /// For leaf nodes (`left_child < 0`): leaf weight added to the prediction sum.
    pub split_cond_or_weight: f32,
    /// Whether `NaN` values route to `left_child` (`true`) or `right_child` (`false`).
    pub default_left: bool,
}

impl FlatTreeNode {
    #[inline]
    pub const fn is_leaf(&self) -> bool {
        self.left_child < 0
    }
}

/// Pure-Rust evaluator for an XGBoost `gbtree` regression model serialized via
/// `model.save_model("model.json")` (or `model.get_booster().save_model("model.json")`).
#[derive(Debug, Clone)]
pub struct XgboostJsonModel {
    pub num_features: usize,
    pub base_score: f32,
    /// Contiguous arena of all nodes across all trees in the ensemble.
    pub nodes: Vec<FlatTreeNode>,
    /// Starting index in `self.nodes` for the root of each tree.
    pub tree_roots: Vec<u32>,
}

#[derive(Deserialize)]
struct RawXgbRoot {
    learner: RawXgbLearner,
}

#[derive(Deserialize)]
struct RawXgbLearner {
    gradient_booster: RawXgbGradientBooster,
    learner_model_param: RawXgbLearnerModelParam,
}

#[derive(Deserialize)]
struct RawXgbLearnerModelParam {
    base_score: String,
    #[serde(default)]
    num_feature: Option<String>,
}

#[derive(Deserialize)]
struct RawXgbGradientBooster {
    model: RawXgbGbtreeModel,
}

#[derive(Deserialize)]
struct RawXgbGbtreeModel {
    trees: Vec<RawXgbTree>,
}

#[derive(Deserialize)]
struct RawXgbTree {
    left_children: Vec<i32>,
    right_children: Vec<i32>,
    split_indices: Vec<u32>,
    split_conditions: Vec<f32>,
    #[serde(default)]
    default_left: Vec<u8>,
}

impl XgboostJsonModel {
    /// Loads an `XgboostJsonModel` from a `.json` file on disk.
    pub fn from_json_file(path: &Path) -> Result<Self, CongestionError> {
        let raw = fs::read_to_string(path).map_err(|source| CongestionError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        Self::from_json_str(&raw).map_err(|e| {
            CongestionError::Model(format!("failed loading '{}': {e}", path.display()))
        })
    }

    /// Parses an `XgboostJsonModel` from an XGBoost JSON string.
    pub fn from_json_str(json_str: &str) -> Result<Self, CongestionError> {
        let root: RawXgbRoot = serde_json::from_str(json_str).map_err(|e| {
            CongestionError::Model(format!("invalid XGBoost JSON model structure: {e}"))
        })?;

        let base_score = parse_xgb_float_str(&root.learner.learner_model_param.base_score)?;
        let num_features = root
            .learner
            .learner_model_param
            .num_feature
            .as_deref()
            .and_then(|s| s.trim().parse::<usize>().ok())
            .unwrap_or(0);

        let raw_trees = root.learner.gradient_booster.model.trees;
        let total_nodes: usize = raw_trees.iter().map(|t| t.left_children.len()).sum();

        let mut nodes = Vec::with_capacity(total_nodes);
        let mut tree_roots = Vec::with_capacity(raw_trees.len());

        for (t_idx, tree) in raw_trees.into_iter().enumerate() {
            let n = tree.left_children.len();
            if n == 0 {
                continue;
            }
            if tree.right_children.len() != n
                || tree.split_indices.len() != n
                || tree.split_conditions.len() != n
            {
                return Err(CongestionError::Model(format!(
                    "tree {t_idx} has mismatched array lengths in XGBoost JSON"
                )));
            }

            let base_offset = nodes.len() as i32;
            tree_roots.push(base_offset as u32);

            for i in 0..n {
                let left = tree.left_children[i];
                let right = tree.right_children[i];
                let default_left = tree
                    .default_left
                    .get(i)
                    .map(|&v| v != 0)
                    .unwrap_or(true);
                nodes.push(FlatTreeNode {
                    left_child: if left < 0 { -1 } else { base_offset + left },
                    right_child: if right < 0 { -1 } else { base_offset + right },
                    split_index: tree.split_indices[i],
                    split_cond_or_weight: tree.split_conditions[i],
                    default_left,
                });
            }
        }

        Ok(Self {
            num_features,
            base_score,
            nodes,
            tree_roots,
        })
    }

    #[inline]
    pub fn evaluate_row(&self, features: &[f32]) -> f32 {
        let mut sum = self.base_score;
        for &root_idx in &self.tree_roots {
            let mut idx = root_idx as usize;
            loop {
                let node = &self.nodes[idx];
                if node.is_leaf() {
                    sum += node.split_cond_or_weight;
                    break;
                }
                let f_val = features
                    .get(node.split_index as usize)
                    .copied()
                    .unwrap_or(f32::NAN);
                let go_left = if f_val.is_nan() {
                    node.default_left
                } else {
                    f_val < node.split_cond_or_weight
                };
                idx = if go_left {
                    node.left_child as usize
                } else {
                    node.right_child as usize
                };
            }
        }
        sum
    }
}

impl SpeedPredictor for XgboostJsonModel {
    #[inline]
    fn predict(&self, features: &[f32]) -> f32 {
        self.evaluate_row(features)
    }

    fn num_features(&self) -> usize {
        self.num_features
    }
}

/// Parses XGBoost stringified floats, handling both `"0.5"` (XGBoost 1.x) and `"[3.5E1]"` (XGBoost 2.x+).
fn parse_xgb_float_str(raw: &str) -> Result<f32, CongestionError> {
    let trimmed = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    trimmed.parse::<f32>().map_err(|e| {
        CongestionError::Model(format!("failed to parse XGBoost float '{raw}': {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xgboost_json_model_evaluation() {
        // 2-tree toy model with base_score = "[25.0]" (XGBoost 2.x bracket format)
        let sample_json = r#"{
            "learner": {
                "learner_model_param": {
                    "base_score": "[25.0]",
                    "num_feature": "2"
                },
                "gradient_booster": {
                    "model": {
                        "trees": [
                            {
                                "left_children": [1, -1, -1],
                                "right_children": [2, -1, -1],
                                "split_indices": [0, 0, 0],
                                "split_conditions": [50.0, -5.0, 10.0],
                                "default_left": [1, 0, 0]
                            },
                            {
                                "left_children": [1, -1, -1],
                                "right_children": [2, -1, -1],
                                "split_indices": [1, 0, 0],
                                "split_conditions": [1.0, 2.5, -2.5],
                                "default_left": [0, 0, 0]
                            }
                        ]
                    }
                }
            }
        }"#;

        let model = XgboostJsonModel::from_json_str(sample_json).unwrap();
        assert_eq!(model.num_features(), 2);
        assert!((model.base_score - 25.0).abs() < 1e-6);

        // Feature 0 = 60.0 (>= 50.0 -> right child = +10.0)
        // Feature 1 = 0.0 (< 1.0 -> left child = +2.5)
        // Total = 25.0 + 10.0 + 2.5 = 37.5
        let pred = model.predict(&[60.0, 0.0]);
        assert!((pred - 37.5).abs() < 1e-6);

        // Feature 0 = 30.0 (< 50.0 -> left child = -5.0)
        // Feature 1 = 1.0 (>= 1.0 -> right child = -2.5)
        // Total = 25.0 - 5.0 - 2.5 = 17.5
        let pred2 = model.predict(&[30.0, 1.0]);
        assert!((pred2 - 17.5).abs() < 1e-6);
    }
}
