//! Multi-step repair plans for AI agent error resolution.
//!
//! When a single [`FixPatch`] is not enough to fix an error, a [`RepairPlan`]
//! provides a sequence of dependent repair steps that agents can apply
//! in order.

use kodo_ast::FixPatch;

/// A multi-step repair plan that agents can apply sequentially.
///
/// Each step may depend on a previous step being applied first.
/// The `confidence` field indicates how likely the plan is to
/// fully resolve the error.
#[derive(Debug, Clone)]
pub struct RepairPlan {
    /// Sequential repair steps.
    pub steps: Vec<RepairStep>,
    /// Confidence that this plan will resolve the error (0.0 to 1.0).
    pub confidence: f64,
}

/// A single step in a repair plan.
#[derive(Debug, Clone)]
pub struct RepairStep {
    /// Unique identifier for this step within the plan.
    pub id: usize,
    /// Human-readable description of what this step does.
    pub description: String,
    /// The patches to apply in this step.
    pub patches: Vec<FixPatch>,
    /// Optional dependency on a previous step (by id).
    pub depends_on: Option<usize>,
}
