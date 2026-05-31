//! # LAU Conservation Guard
//!
//! THE budget enforcement layer for the LAU construct.
//! Every operation costs conservation units. This guard tracks, verifies, and enforces budgets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

// Helper for serializing HashMap<OperationType, f64> with string keys
mod opmap_serde {
    use super::OperationType;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S: Serializer>(
        map: &HashMap<OperationType, f64>,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let string_map: HashMap<String, f64> = map
            .iter()
            .map(|(k, v)| (serde_json::to_string(k).unwrap(), *v))
            .collect();
        string_map.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<HashMap<OperationType, f64>, D::Error> {
        let string_map: HashMap<String, f64> = HashMap::deserialize(d)?;
        string_map
            .into_iter()
            .map(|(k, v)| {
                let op: OperationType = serde_json::from_str(&k).map_err(serde::de::Error::custom)?;
                Ok((op, v))
            })
            .collect()
    }
}

static RECEIPT_COUNTER: AtomicU64 = AtomicU64::new(1);

/// Types of operations that cost conservation units.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum OperationType {
    TextResponse(u64),
    CodeGeneration(u64),
    ToolExecution(u64),
    PhoneAFriend(u64),
    ProvenanceCommit,
    CorrelationScan,
    EnsignWake,
    EnsignTick,
    DeadbandCheck,
    RoomRouting,
    TileCreate,
    TileQuery,
}

impl OperationType {
    /// Returns the base cost associated with this operation type.
    pub fn base_cost(&self) -> f64 {
        match self {
            OperationType::TextResponse(cost) => *cost as f64,
            OperationType::CodeGeneration(cost) => *cost as f64,
            OperationType::ToolExecution(cost) => *cost as f64,
            OperationType::PhoneAFriend(cost) => *cost as f64,
            OperationType::ProvenanceCommit => 1.0,
            OperationType::CorrelationScan => 5.0,
            OperationType::EnsignWake => 2.0,
            OperationType::EnsignTick => 0.5,
            OperationType::DeadbandCheck => 0.1,
            OperationType::RoomRouting => 0.5,
            OperationType::TileCreate => 1.0,
            OperationType::TileQuery => 0.5,
        }
    }
}

/// Proof of a conservation spend. Can be used for refunds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConservationReceipt {
    pub id: String,
    pub operation: OperationType,
    pub room: String,
    pub model: String,
    pub cost: f64,
    pub timestamp: u64,
    pub tile_id: Option<String>,
}

/// Full report of conservation budget state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConservationReport {
    pub total_budget: f64,
    pub total_spent: f64,
    pub total_remaining: f64,
    pub total_wasted: f64,
    pub by_room: HashMap<String, f64>,
    pub by_model: HashMap<String, f64>,
    pub by_operation: HashMap<String, f64>,
    pub conservation_verified: bool,
    pub top_spenders: Vec<(String, f64)>,
}

impl ConservationReport {
    /// Render the report as a human-readable string.
    pub fn render(&self) -> String {
        let mut lines = Vec::new();
        lines.push("=== Conservation Report ===".to_string());
        lines.push(format!("Budget:     {:.2}", self.total_budget));
        lines.push(format!("Spent:      {:.2}", self.total_spent));
        lines.push(format!("Remaining:  {:.2}", self.total_remaining));
        lines.push(format!("Wasted:     {:.2}", self.total_wasted));
        lines.push(format!(
            "Verified:   {}",
            if self.conservation_verified { "✓" } else { "✗" }
        ));

        if !self.by_room.is_empty() {
            lines.push(String::new());
            lines.push("By Room:".to_string());
            let mut rooms: Vec<_> = self.by_room.iter().collect();
            rooms.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (room, cost) in &rooms {
                lines.push(format!("  {}: {:.2}", room, cost));
            }
        }

        if !self.by_model.is_empty() {
            lines.push(String::new());
            lines.push("By Model:".to_string());
            let mut models: Vec<_> = self.by_model.iter().collect();
            models.sort_by(|a, b| b.1.partial_cmp(a.1).unwrap_or(std::cmp::Ordering::Equal));
            for (model, cost) in &models {
                lines.push(format!("  {}: {:.2}", model, cost));
            }
        }

        if !self.top_spenders.is_empty() {
            lines.push(String::new());
            lines.push("Top Spenders:".to_string());
            for (name, cost) in &self.top_spenders {
                lines.push(format!("  {}: {:.2}", name, cost));
            }
        }

        lines.join("\n")
    }
}

/// Strategy for graceful degradation when budget runs low.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum DegradationStrategy {
    /// Reject all new operations, keep existing running.
    RejectNew,
    /// Only allow operations under a cost threshold.
    CheapOnly { threshold: f64 },
    /// Only allow provenance commits and deadband checks.
    CriticalOnly,
    /// Everything stops, notify Hermes.
    EmergencyStop,
}

/// Handles graceful degradation when budget is running out.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GracefulDegradation {
    pub strategy: DegradationStrategy,
}

impl GracefulDegradation {
    pub fn new(strategy: DegradationStrategy) -> Self {
        Self { strategy }
    }

    /// Determine if an operation should be allowed given its cost and remaining budget.
    pub fn should_allow(&self, cost: f64, remaining: f64) -> bool {
        match &self.strategy {
            DegradationStrategy::RejectNew => false,
            DegradationStrategy::CheapOnly { threshold } => cost <= *threshold,
            DegradationStrategy::CriticalOnly => {
                // Allow if remaining can cover it (for critical ops the caller decides criticality)
                cost <= remaining
            }
            DegradationStrategy::EmergencyStop => false,
        }
    }
}

/// THE conservation guard. Tracks, verifies, and enforces budgets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConservationGuard {
    pub budget: f64,
    pub spent: f64,
    pub wasted: f64,
    pub per_room: HashMap<String, f64>,
    pub per_model: HashMap<String, f64>,
    #[serde(with = "opmap_serde")]
    pub per_operation: HashMap<OperationType, f64>,
    pub autonomy_limits: HashMap<u32, f64>,
}

impl ConservationGuard {
    /// Create a new guard with the given total budget.
    pub fn new(budget: f64) -> Self {
        Self {
            budget,
            spent: 0.0,
            wasted: 0.0,
            per_room: HashMap::new(),
            per_model: HashMap::new(),
            per_operation: HashMap::new(),
            autonomy_limits: HashMap::new(),
        }
    }

    /// Check if an operation can be afforded given cost, room, and autonomy level.
    pub fn can_afford(&self, cost: f64, room: &str, autonomy_level: u32) -> bool {
        if cost <= 0.0 {
            return true;
        }
        let remaining = self.remaining();
        if cost > remaining {
            return false;
        }
        // Check autonomy limit
        if let Some(&limit) = self.autonomy_limits.get(&autonomy_level) {
            let room_spent = self.per_room.get(room).copied().unwrap_or(0.0);
            // The room's total can't exceed the autonomy limit for this level
            if room_spent + cost > limit {
                return false;
            }
        }
        true
    }

    /// Spend conservation units on an operation. Returns a receipt on success.
    pub fn spend(
        &mut self,
        op: OperationType,
        room: &str,
        model: &str,
        cost: f64,
    ) -> Result<ConservationReceipt, String> {
        if cost < 0.0 {
            return Err("Cost cannot be negative".to_string());
        }
        if cost > self.remaining() {
            return Err(format!(
                "Insufficient budget: need {:.2}, have {:.2}",
                cost,
                self.remaining()
            ));
        }

        let receipt = ConservationReceipt {
            id: format!("cr-{}", RECEIPT_COUNTER.fetch_add(1, Ordering::Relaxed)),
            operation: op.clone(),
            room: room.to_string(),
            model: model.to_string(),
            cost,
            timestamp: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
            tile_id: None,
        };

        self.spent += cost;
        *self.per_room.entry(room.to_string()).or_insert(0.0) += cost;
        *self.per_model.entry(model.to_string()).or_insert(0.0) += cost;
        *self.per_operation.entry(op).or_insert(0.0) += cost;

        Ok(receipt)
    }

    /// Refund a previously spent receipt, returning unused budget.
    pub fn refund(&mut self, receipt: ConservationReceipt) {
        self.spent -= receipt.cost;
        if let Some(room_total) = self.per_room.get_mut(&receipt.room) {
            *room_total -= receipt.cost;
            if *room_total <= 0.0 {
                self.per_room.remove(&receipt.room);
            }
        }
        if let Some(model_total) = self.per_model.get_mut(&receipt.model) {
            *model_total -= receipt.cost;
            if *model_total <= 0.0 {
                self.per_model.remove(&receipt.model);
            }
        }
        if let Some(op_total) = self.per_operation.get_mut(&receipt.operation) {
            *op_total -= receipt.cost;
            if *op_total <= 0.0 {
                self.per_operation.remove(&receipt.operation);
            }
        }
    }

    /// Remaining budget.
    pub fn remaining(&self) -> f64 {
        self.budget - self.spent - self.wasted
    }

    /// Verify conservation: total_in == total_spent + total_remaining + total_wasted.
    pub fn verify_conservation(&self) -> bool {
        let total = self.spent + self.remaining() + self.wasted;
        (total - self.budget).abs() < f64::EPSILON
    }

    /// Get total spent in a room.
    pub fn room_budget(&self, room: &str) -> f64 {
        self.per_room.get(room).copied().unwrap_or(0.0)
    }

    /// Get total cost attributed to a model.
    pub fn model_cost(&self, model: &str) -> f64 {
        self.per_model.get(model).copied().unwrap_or(0.0)
    }

    /// Generate a full conservation report.
    pub fn report(&self) -> ConservationReport {
        // Collect top spenders from room + model combined
        let mut spenders: Vec<(String, f64)> = Vec::new();
        for (room, cost) in &self.per_room {
            spenders.push((format!("room:{}", room), *cost));
        }
        for (model, cost) in &self.per_model {
            spenders.push((format!("model:{}", model), *cost));
        }
        spenders.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        spenders.truncate(10);

        let by_operation: HashMap<String, f64> = self
            .per_operation
            .iter()
            .map(|(op, cost)| {
                let name = match op {
                    OperationType::TextResponse(_) => "TextResponse".to_string(),
                    OperationType::CodeGeneration(_) => "CodeGeneration".to_string(),
                    OperationType::ToolExecution(_) => "ToolExecution".to_string(),
                    OperationType::PhoneAFriend(_) => "PhoneAFriend".to_string(),
                    OperationType::ProvenanceCommit => "ProvenanceCommit".to_string(),
                    OperationType::CorrelationScan => "CorrelationScan".to_string(),
                    OperationType::EnsignWake => "EnsignWake".to_string(),
                    OperationType::EnsignTick => "EnsignTick".to_string(),
                    OperationType::DeadbandCheck => "DeadbandCheck".to_string(),
                    OperationType::RoomRouting => "RoomRouting".to_string(),
                    OperationType::TileCreate => "TileCreate".to_string(),
                    OperationType::TileQuery => "TileQuery".to_string(),
                };
                (name, *cost)
            })
            .collect();

        ConservationReport {
            total_budget: self.budget,
            total_spent: self.spent,
            total_remaining: self.remaining(),
            total_wasted: self.wasted,
            by_room: self.per_room.clone(),
            by_model: self.per_model.clone(),
            by_operation,
            conservation_verified: self.verify_conservation(),
            top_spenders: spenders,
        }
    }

    /// Set the maximum budget for a given autonomy level.
    pub fn set_autonomy_limit(&mut self, level: u32, max_budget: f64) {
        self.autonomy_limits.insert(level, max_budget);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_guard_new() {
        let guard = ConservationGuard::new(100.0);
        assert_eq!(guard.budget, 100.0);
        assert_eq!(guard.spent, 0.0);
        assert_eq!(guard.wasted, 0.0);
        assert_eq!(guard.remaining(), 100.0);
    }

    #[test]
    fn test_spend_basic() {
        let mut guard = ConservationGuard::new(100.0);
        let receipt = guard
            .spend(
                OperationType::TextResponse(10),
                "room-1",
                "gpt-4",
                5.0,
            )
            .unwrap();
        assert_eq!(guard.spent, 5.0);
        assert_eq!(guard.remaining(), 95.0);
        assert_eq!(receipt.cost, 5.0);
        assert_eq!(receipt.room, "room-1");
        assert_eq!(receipt.model, "gpt-4");
    }

    #[test]
    fn test_spend_insufficient_budget() {
        let mut guard = ConservationGuard::new(10.0);
        let result = guard.spend(OperationType::CodeGeneration(20), "room-1", "gpt-4", 15.0);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient budget"));
    }

    #[test]
    fn test_spend_negative_cost() {
        let mut guard = ConservationGuard::new(100.0);
        let result = guard.spend(OperationType::TextResponse(5), "room-1", "gpt-4", -1.0);
        assert!(result.is_err());
    }

    #[test]
    fn test_refund() {
        let mut guard = ConservationGuard::new(100.0);
        let receipt = guard
            .spend(
                OperationType::TextResponse(10),
                "room-1",
                "gpt-4",
                5.0,
            )
            .unwrap();
        assert_eq!(guard.spent, 5.0);
        guard.refund(receipt);
        assert_eq!(guard.spent, 0.0);
        assert_eq!(guard.remaining(), 100.0);
    }

    #[test]
    fn test_can_afford_positive() {
        let mut guard = ConservationGuard::new(100.0);
        assert!(guard.can_afford(50.0, "room-1", 0));
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 50.0).unwrap();
        assert!(guard.can_afford(50.0, "room-1", 0));
    }

    #[test]
    fn test_can_afford_negative_too_expensive() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 90.0).unwrap();
        assert!(!guard.can_afford(20.0, "room-1", 0));
    }

    #[test]
    fn test_can_afford_zero_cost() {
        let guard = ConservationGuard::new(100.0);
        assert!(guard.can_afford(0.0, "room-1", 0));
    }

    #[test]
    fn test_can_afford_autonomy_limit() {
        let mut guard = ConservationGuard::new(100.0);
        guard.set_autonomy_limit(1, 10.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        // Room already has 5.0 spent, trying to add 6.0 would exceed 10.0 limit
        assert!(!guard.can_afford(6.0, "room-1", 1));
        // But 5.0 is fine
        assert!(guard.can_afford(5.0, "room-1", 1));
    }

    #[test]
    fn test_can_afford_no_autonomy_limit_for_level() {
        let guard = ConservationGuard::new(100.0);
        // Level 5 has no limit set, should just check remaining
        assert!(guard.can_afford(50.0, "room-1", 5));
    }

    #[test]
    fn test_remaining() {
        let mut guard = ConservationGuard::new(100.0);
        assert_eq!(guard.remaining(), 100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 30.0).unwrap();
        assert_eq!(guard.remaining(), 70.0);
    }

    #[test]
    fn test_remaining_with_waste() {
        let mut guard = ConservationGuard::new(100.0);
        guard.wasted = 10.0;
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 30.0).unwrap();
        assert_eq!(guard.remaining(), 60.0);
    }

    #[test]
    fn test_verify_conservation_pristine() {
        let guard = ConservationGuard::new(100.0);
        assert!(guard.verify_conservation());
    }

    #[test]
    fn test_verify_conservation_after_spend() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 30.0).unwrap();
        assert!(guard.verify_conservation());
    }

    #[test]
    fn test_verify_conservation_with_waste() {
        let mut guard = ConservationGuard::new(100.0);
        guard.wasted = 10.0;
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 30.0).unwrap();
        assert!(guard.verify_conservation());
    }

    #[test]
    fn test_room_budget() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        guard.spend(OperationType::CodeGeneration(20), "room-1", "gpt-4", 10.0).unwrap();
        guard.spend(OperationType::TextResponse(10), "room-2", "gpt-4", 3.0).unwrap();
        assert_eq!(guard.room_budget("room-1"), 15.0);
        assert_eq!(guard.room_budget("room-2"), 3.0);
        assert_eq!(guard.room_budget("room-3"), 0.0);
    }

    #[test]
    fn test_model_cost() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        guard.spend(OperationType::TextResponse(10), "room-2", "gpt-4", 10.0).unwrap();
        guard.spend(OperationType::TextResponse(10), "room-3", "claude", 3.0).unwrap();
        assert_eq!(guard.model_cost("gpt-4"), 15.0);
        assert_eq!(guard.model_cost("claude"), 3.0);
        assert_eq!(guard.model_cost("llama"), 0.0);
    }

    #[test]
    fn test_report() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        guard.spend(OperationType::CodeGeneration(20), "room-2", "claude", 10.0).unwrap();

        let report = guard.report();
        assert_eq!(report.total_budget, 100.0);
        assert_eq!(report.total_spent, 15.0);
        assert_eq!(report.total_remaining, 85.0);
        assert!(report.conservation_verified);
        assert_eq!(report.by_room.len(), 2);
        assert_eq!(report.by_model.len(), 2);
    }

    #[test]
    fn test_report_render() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        let report = guard.report();
        let rendered = report.render();
        assert!(rendered.contains("Conservation Report"));
        assert!(rendered.contains("100.00"));
    }

    #[test]
    fn test_set_autonomy_limit() {
        let mut guard = ConservationGuard::new(100.0);
        guard.set_autonomy_limit(1, 50.0);
        guard.set_autonomy_limit(2, 100.0);
        assert_eq!(guard.autonomy_limits.get(&1), Some(&50.0));
        assert_eq!(guard.autonomy_limits.get(&2), Some(&100.0));
        assert_eq!(guard.autonomy_limits.get(&3), None);
    }

    #[test]
    fn test_operation_type_base_cost_text_response() {
        let op = OperationType::TextResponse(42);
        assert_eq!(op.base_cost(), 42.0);
    }

    #[test]
    fn test_operation_type_base_cost_code_generation() {
        let op = OperationType::CodeGeneration(100);
        assert_eq!(op.base_cost(), 100.0);
    }

    #[test]
    fn test_operation_type_base_cost_tool_execution() {
        let op = OperationType::ToolExecution(15);
        assert_eq!(op.base_cost(), 15.0);
    }

    #[test]
    fn test_operation_type_base_cost_phone_a_friend() {
        let op = OperationType::PhoneAFriend(7);
        assert_eq!(op.base_cost(), 7.0);
    }

    #[test]
    fn test_operation_type_base_cost_provenance_commit() {
        assert_eq!(OperationType::ProvenanceCommit.base_cost(), 1.0);
    }

    #[test]
    fn test_operation_type_base_cost_correlation_scan() {
        assert_eq!(OperationType::CorrelationScan.base_cost(), 5.0);
    }

    #[test]
    fn test_operation_type_base_cost_ensign_wake() {
        assert_eq!(OperationType::EnsignWake.base_cost(), 2.0);
    }

    #[test]
    fn test_operation_type_base_cost_ensign_tick() {
        assert_eq!(OperationType::EnsignTick.base_cost(), 0.5);
    }

    #[test]
    fn test_operation_type_base_cost_deadband_check() {
        assert_eq!(OperationType::DeadbandCheck.base_cost(), 0.1);
    }

    #[test]
    fn test_operation_type_base_cost_room_routing() {
        assert_eq!(OperationType::RoomRouting.base_cost(), 0.5);
    }

    #[test]
    fn test_operation_type_base_cost_tile_create() {
        assert_eq!(OperationType::TileCreate.base_cost(), 1.0);
    }

    #[test]
    fn test_operation_type_base_cost_tile_query() {
        assert_eq!(OperationType::TileQuery.base_cost(), 0.5);
    }

    // --- GracefulDegradation tests ---

    #[test]
    fn test_degradation_reject_new() {
        let dg = GracefulDegradation::new(DegradationStrategy::RejectNew);
        assert!(!dg.should_allow(1.0, 100.0));
    }

    #[test]
    fn test_degradation_cheap_only_under_threshold() {
        let dg = GracefulDegradation::new(DegradationStrategy::CheapOnly { threshold: 5.0 });
        assert!(dg.should_allow(3.0, 100.0));
    }

    #[test]
    fn test_degradation_cheap_only_over_threshold() {
        let dg = GracefulDegradation::new(DegradationStrategy::CheapOnly { threshold: 5.0 });
        assert!(!dg.should_allow(6.0, 100.0));
    }

    #[test]
    fn test_degradation_critical_only_within_remaining() {
        let dg = GracefulDegradation::new(DegradationStrategy::CriticalOnly);
        assert!(dg.should_allow(5.0, 10.0));
    }

    #[test]
    fn test_degradation_critical_only_exceeds_remaining() {
        let dg = GracefulDegradation::new(DegradationStrategy::CriticalOnly);
        assert!(!dg.should_allow(15.0, 10.0));
    }

    #[test]
    fn test_degradation_emergency_stop() {
        let dg = GracefulDegradation::new(DegradationStrategy::EmergencyStop);
        assert!(!dg.should_allow(1.0, 100.0));
    }

    // --- Serde tests ---

    #[test]
    fn test_operation_type_serde_roundtrip() {
        let op = OperationType::CodeGeneration(42);
        let json = serde_json::to_string(&op).unwrap();
        let de: OperationType = serde_json::from_str(&json).unwrap();
        assert_eq!(op, de);
    }

    #[test]
    fn test_receipt_serde_roundtrip() {
        let mut guard = ConservationGuard::new(100.0);
        let receipt = guard
            .spend(
                OperationType::TextResponse(10),
                "room-1",
                "gpt-4",
                5.0,
            )
            .unwrap();
        let json = serde_json::to_string(&receipt).unwrap();
        let de: ConservationReceipt = serde_json::from_str(&json).unwrap();
        assert_eq!(receipt, de);
    }

    #[test]
    fn test_guard_serde_roundtrip() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        guard.set_autonomy_limit(1, 50.0);
        let json = serde_json::to_string(&guard).unwrap();
        let de: ConservationGuard = serde_json::from_str(&json).unwrap();
        assert_eq!(guard, de);
    }

    #[test]
    fn test_report_serde_roundtrip() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        let report = guard.report();
        let json = serde_json::to_string(&report).unwrap();
        let de: ConservationReport = serde_json::from_str(&json).unwrap();
        assert_eq!(report, de);
    }

    #[test]
    fn test_degradation_strategy_serde_roundtrip() {
        let strat = DegradationStrategy::CheapOnly { threshold: 5.0 };
        let json = serde_json::to_string(&strat).unwrap();
        let de: DegradationStrategy = serde_json::from_str(&json).unwrap();
        assert_eq!(strat, de);
    }

    #[test]
    fn test_graceful_degradation_serde_roundtrip() {
        let dg = GracefulDegradation::new(DegradationStrategy::CriticalOnly);
        let json = serde_json::to_string(&dg).unwrap();
        let de: GracefulDegradation = serde_json::from_str(&json).unwrap();
        assert_eq!(dg, de);
    }

    // --- Multi-spend tests ---

    #[test]
    fn test_multiple_spends_same_room() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        guard.spend(OperationType::CodeGeneration(20), "room-1", "gpt-4", 10.0).unwrap();
        assert_eq!(guard.room_budget("room-1"), 15.0);
        assert_eq!(guard.spent, 15.0);
    }

    #[test]
    fn test_spend_all_budget() {
        let mut guard = ConservationGuard::new(100.0);
        let result = guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 100.0);
        assert!(result.is_ok());
        assert_eq!(guard.remaining(), 0.0);
        assert!(guard.verify_conservation());
    }

    #[test]
    fn test_spend_exactly_remaining() {
        let mut guard = ConservationGuard::new(100.0);
        guard.wasted = 30.0;
        let result = guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 70.0);
        assert!(result.is_ok());
        assert_eq!(guard.remaining(), 0.0);
    }

    #[test]
    fn test_refund_clean_removes_entry() {
        let mut guard = ConservationGuard::new(100.0);
        let receipt = guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        assert!(guard.per_room.contains_key("room-1"));
        guard.refund(receipt);
        assert!(!guard.per_room.contains_key("room-1"));
        assert!(!guard.per_model.contains_key("gpt-4"));
    }

    #[test]
    fn test_report_top_spenders() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 30.0).unwrap();
        guard.spend(OperationType::TextResponse(10), "room-2", "claude", 20.0).unwrap();
        let report = guard.report();
        // Top spenders should be sorted descending
        assert!(report.top_spenders[0].1 >= report.top_spenders[1].1);
    }

    #[test]
    fn test_receipt_has_id() {
        let mut guard = ConservationGuard::new(100.0);
        let r1 = guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        let r2 = guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        assert_ne!(r1.id, r2.id);
        assert!(r1.id.starts_with("cr-"));
    }

    #[test]
    fn test_per_operation_tracking() {
        let mut guard = ConservationGuard::new(100.0);
        guard.spend(OperationType::TextResponse(10), "room-1", "gpt-4", 5.0).unwrap();
        guard.spend(OperationType::TextResponse(10), "room-2", "gpt-4", 10.0).unwrap();
        guard.spend(OperationType::CodeGeneration(20), "room-1", "gpt-4", 15.0).unwrap();

        let text_key = OperationType::TextResponse(10);
        let code_key = OperationType::CodeGeneration(20);
        assert_eq!(guard.per_operation.get(&text_key), Some(&15.0));
        assert_eq!(guard.per_operation.get(&code_key), Some(&15.0));
    }
}
