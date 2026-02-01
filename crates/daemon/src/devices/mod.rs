//! Device management module.
//!
//! This module provides functionality for managing trusted devices,
//! including persistence and trust level management.

pub mod trust_store;

pub use trust_store::{
    default_trust_store_path, PendingApproval, TrustLevel, TrustStore, TrustedDevice,
};
