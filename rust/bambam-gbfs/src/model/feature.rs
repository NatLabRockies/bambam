pub mod fieldname {
    /// the name of the agency providing the GBFS vehicle.
    /// if this value is set, the trip has boarded the service.
    pub const GBFS_SERVICE_ID: &str = "gbfs_system_id";

    /// true if the trip has a [GBFS_AGENCY_ID] and if the current
    /// edge has a GBFS zone where `ride_end_allowed` is true.
    pub const GBFS_DESTINATION: &str = "gbfs_destination";
}

pub mod variable {
    //! the configuration for state variables in GTFS-Flex routing

    use routee_compass_core::model::state::{CustomVariableConfig, StateVariableConfig};

    /// stores a zone id in a state variable
    pub fn gbfs_system_id() -> StateVariableConfig {
        StateVariableConfig::Custom {
            custom_type: "Option<GbfsAgencyId>".to_string(),
            value: empty(),
            accumulator: true,
        }
    }

    pub fn gbfs_destination() -> StateVariableConfig {
        StateVariableConfig::Custom {
            custom_type: "Bool".to_string(),
            value: CustomVariableConfig::Boolean { initial: false },
            accumulator: false,
        }
    }

    /// empty value is "-1" for categoricals mapped to real numbers
    pub fn empty() -> CustomVariableConfig {
        CustomVariableConfig::SignedInteger { initial: -1 }
    }
}

pub mod state {
    use super::fieldname;
    use bambam_core::model::state::CategoricalStateMapping;
    use routee_compass_core::model::state::{StateModel, StateModelError, StateVariable};

    /// label value representing un-assigned agency_ids
    const NO_AGENCY_ID: i64 = -1;

    /// assigns the given agency_id to the state vector.
    pub fn set_system_id(
        state: &mut [StateVariable],
        state_model: &StateModel,
        agency_id: &str,
        mapping: &CategoricalStateMapping,
    ) -> Result<(), StateModelError> {
        let value = mapping.get_label(agency_id).ok_or_else(|| {
            StateModelError::RuntimeError(format!("agency_id {agency_id} missing from mapping"))
        })?;
        state_model.set_custom_i64(state, fieldname::GBFS_SERVICE_ID, value)
    }

    /// gets the stored agency_id from the state variable, if it exists.
    pub fn get_system_id<'a, 'b>(
        state: &'a [StateVariable],
        state_model: &'a StateModel,
        mapping: &'b CategoricalStateMapping,
    ) -> Result<Option<&'b String>, StateModelError> {
        let agency_label = state_model.get_custom_i64(state, fieldname::GBFS_SERVICE_ID)?;
        let agency_id = mapping.get_categorical(agency_label)?;
        Ok(agency_id)
    }

    /// confirms that there is a stored agency_id and that it matches the provided one.
    pub fn verify_system_id(
        agency_id: &str,
        state: &[StateVariable],
        state_model: &StateModel,
        mapping: &CategoricalStateMapping,
    ) -> Result<bool, StateModelError> {
        let stored_agency = get_system_id(state, state_model, mapping)?;
        match stored_agency {
            Some(a) if a == agency_id => Ok(true),
            _ => Ok(false),
        }
    }

    /// confirms that the search has boarded a GBFS agency (that GBFS_AGENCY_ID is set).
    pub fn is_boarded(
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, StateModelError> {
        let agency_id = state_model.get_custom_i64(state, fieldname::GBFS_SERVICE_ID)?;
        Ok(agency_id != NO_AGENCY_ID)
    }

    /// affirms that the trip location associated with this state vector is a valid trip destination.
    pub fn set_valid_destination(
        state: &mut [StateVariable],
        state_model: &StateModel,
    ) -> Result<(), StateModelError> {
        state_model.set_custom_bool(state, fieldname::GBFS_DESTINATION, &true)
    }

    /// is the trip location associated with this state vector is a valid trip destination?
    pub fn get_valid_destination(
        state: &[StateVariable],
        state_model: &StateModel,
    ) -> Result<bool, StateModelError> {
        state_model.get_custom_bool(state, fieldname::GBFS_DESTINATION)
    }
}
