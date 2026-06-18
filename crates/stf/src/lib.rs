//! The rollup State Transition Function.

pub mod authentication;
mod delegation;
#[cfg(feature = "native")]
pub mod price_source;
pub mod runtime;

pub use runtime::*;
use sov_modules_stf_blueprint::StfBlueprint;
use sov_rollup_interface::stf::StateTransitionVerifier;

pub extern crate sov_modules_api;

/// Alias for StateTransitionVerifier.
pub type StfVerifier<DA, ZkSpec, RT> = StateTransitionVerifier<StfBlueprint<ZkSpec, RT>, DA>;
