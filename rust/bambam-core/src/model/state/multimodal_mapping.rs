use routee_compass_core::{
    model::state::StateModelError,
    util::fs::{read_decoders, read_utils},
};
use std::{collections::HashMap, fmt::Debug, path::Path};

/// stores the bijection from categorical name to an enumeration label compatible
/// with Compass LabelModels and Custom StateModels.
///
/// ## Types
/// T: the type `T` is some hashable, categorical type. consider String as a starting point.
///
/// U: the type `U` is some hashable, numeric typw which can be built From<usize> so that it
/// can be used to index a Vec<T>.
///
#[derive(Clone, Debug)]
pub struct MultimodalMapping<T: Clone + Debug, U: Clone + Debug> {
    cat_to_label: HashMap<T, U>,
    label_to_cat: Vec<T>,
}

/// a common type of multimodal mapping which maps strings to i64 values.
/// categories begin from zero. negative values denote an empty class label (None case).
pub type MultimodalStateMapping = MultimodalMapping<String, i64>;

/// A trait for types that can be used as categorical identifiers
#[allow(dead_code)]
trait Categorical: Eq + std::hash::Hash + Clone + Debug {}

/// A trait for types that can be used as indices in the multimodal mapping
#[allow(dead_code)]
trait IndexType:
    Eq + std::hash::Hash + Clone + Copy + TryFrom<usize> + TryInto<usize> + PartialOrd + Debug
{
}

// Blanket implementations for common types
impl<T> Categorical for T where T: Eq + std::hash::Hash + Clone + Debug {}
impl<U> IndexType for U where
    U: Eq + std::hash::Hash + Clone + Copy + TryFrom<usize> + TryInto<usize> + PartialOrd + Debug
{
}

impl MultimodalStateMapping {
    pub fn from_enumerated_category_file(filepath: &Path) -> Result<Self, StateModelError> {
        let contents = read_utils::read_raw_file(filepath, read_decoders::string, None, None)
            .map_err(|e| {
                StateModelError::BuildError(format!(
                    "failure reading enumerated category mapping from {}: {e}",
                    filepath.to_string_lossy()
                ))
            })?;
        MultimodalMapping::new(&contents)
    }
}

impl<T, U> MultimodalMapping<T, U>
where
    T: Eq + std::hash::Hash + Clone + Debug,
    U: Eq + std::hash::Hash + Clone + Copy + TryFrom<usize> + TryInto<usize> + PartialOrd + Debug,
{
    /// create an empty mapping
    pub fn empty() -> Self {
        MultimodalMapping {
            cat_to_label: HashMap::new(),
            label_to_cat: Vec::new(),
        }
    }

    /// create a new mapping from a list of categoricals. for each categorical value in categoricals,
    /// it will be assigned a label integer using the categorical's row index.
    pub fn new(categoricals: &[T]) -> Result<Self, StateModelError> {
        let label_to_cat = categoricals.to_vec();
        let cat_to_label = label_to_cat
            .iter()
            .enumerate()
            .map(|(idx, t)| {
                let u = try_into_u(idx).map_err(|e| {
                    StateModelError::BuildError(format!("for mapping value {t:?}, {e}"))
                })?;
                Ok((t.clone(), u))
            })
            .collect::<Result<HashMap<T, U>, StateModelError>>()?;
        Ok(Self {
            cat_to_label,
            label_to_cat,
        })
    }

    /// get the list (in enumeration order) of categories
    pub fn get_categories(&self) -> &[T] {
        &self.label_to_cat
    }

    /// count the number of mapped categories
    pub fn n_categories(&self) -> usize {
        self.label_to_cat.len()
    }

    /// append another categorical to the mapping, returning the new categorical's label id.
    /// if the categorical is already stored in the mapping, we return the existing label and
    /// no insert occurs.
    pub fn insert(&mut self, categorical: T) -> Result<U, StateModelError> {
        if let Some(&label) = self.cat_to_label.get(&categorical) {
            return Ok(label);
        }

        let next_label = try_into_u(self.label_to_cat.len())?;
        self.cat_to_label.insert(categorical.clone(), next_label);
        self.label_to_cat.push(categorical);
        Ok(next_label)
    }

    /// perform a categorical->label lookup.
    pub fn get_label<Q>(&self, categorical: &Q) -> Option<&U>
    where
        T: std::borrow::Borrow<Q>,
        Q: std::hash::Hash + Eq + ?Sized,
    {
        self.cat_to_label.get(categorical)
    }

    /// perform a label->categorical lookup.
    pub fn get_categorical(&self, label: U) -> Result<Option<&T>, StateModelError> {
        if is_empty(label)? {
            Ok(None)
        } else {
            let idx: usize = try_into_usize(label)?;
            let result = self.label_to_cat.get(idx);
            Ok(result)
        }
    }
}

fn is_empty<U>(u: U) -> Result<bool, StateModelError>
where
    U: Eq + std::hash::Hash + Clone + Copy + TryFrom<usize> + TryInto<usize> + PartialOrd + Debug,
{
    let zero = U::try_from(0).map_err(|_| {
        StateModelError::BuildError("could not create zero value for type".to_string())
    })?;
    Ok(u < zero)
}

/// helper function to convert a U into a usize with error message result.
/// handles the case where u is negative and treats that as an empty value.
fn try_into_usize<U>(u: U) -> Result<usize, StateModelError>
where
    U: Eq + std::hash::Hash + Clone + Copy + TryFrom<usize> + TryInto<usize> + Debug,
{
    let as_usize = u.try_into().map_err(|_e| {
        StateModelError::BuildError(format!(
            "could not convert Index {u:?} to a usize type, should implement TryInto<usize>"
        ))
    })?;
    Ok(as_usize)
}

/// helper function to convert a usize into a U with error message result
fn try_into_u<U>(idx: usize) -> Result<U, StateModelError>
where
    U: Eq + std::hash::Hash + Clone + Copy + TryFrom<usize> + TryInto<usize> + Debug,
{
    idx.try_into().map_err(|_e| {
        StateModelError::BuildError(format!(
            "could not convert index {idx} to a Index type, should implement TryFrom<usize>"
        ))
    })
}
