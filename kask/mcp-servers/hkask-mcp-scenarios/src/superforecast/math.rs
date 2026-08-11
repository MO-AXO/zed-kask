//! Forecast math — pure deterministic functions: Fermi decomposition, event-tree
//! conditional probability propagation, Brier scoring, sensitivity ranking, and
//! framing-document construction. No I/O, no `ForecastStore` state — the stateful
//! orchestration (assessment, composition, conversion, persistence) remains in
//! `superforecast.rs`.
//!
//! Extracted from `superforecast.rs` (deep-module split).

use std::collections::{HashMap, HashSet};

use hkask_forecast as forecast;

use crate::types::{
    EventTree, EventTreeNode, ForecastOutcome, FramingDocument, ScenarioError, ScenarioEvent,
    ScenarioType, StakeholderConfig, SubQuestion, TimeHorizon, UseCase,
};
