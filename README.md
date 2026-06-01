# lau-conservation-guard

> Budget enforcement layer for the LAU construct — tracks, verifies, and enforces conservation units

## What This Does

Budget enforcement layer for the LAU construct — tracks, verifies, and enforces conservation units. Part of the PLATO/LAU ecosystem — a mathematically rigorous framework for building educational agents that learn, teach, and evolve.

## The Key Idea

This crate implements the core abstractions needed for its domain, with a focus on correctness, composability, and conservation guarantees. Every public type is serializable (serde), every algorithm is tested, and every invariant is verified.

## Install

```bash
cargo add lau-conservation-guard
```

## Quick Start

See the API Reference below for complete usage. Key entry points:

```rust
use lau_conservation_guard::*;
// See types and methods below for complete usage
```

## API Reference

```rust
    pub fn serialize<S: Serializer>(
    pub fn deserialize<'de, D: Deserializer<'de>>(
pub enum OperationType 
    pub fn base_cost(&self) -> f64 
pub struct ConservationReceipt 
pub struct ConservationReport 
    pub fn render(&self) -> String 
pub enum DegradationStrategy 
pub struct GracefulDegradation 
    pub fn new(strategy: DegradationStrategy) -> Self 
    pub fn should_allow(&self, cost: f64, remaining: f64) -> bool 
pub struct ConservationGuard 
    pub fn new(budget: f64) -> Self 
    pub fn can_afford(&self, cost: f64, room: &str, autonomy_level: u32) -> bool 
    pub fn spend(
    pub fn refund(&mut self, receipt: ConservationReceipt) 
    pub fn remaining(&self) -> f64 
    pub fn verify_conservation(&self) -> bool 
    pub fn room_budget(&self, room: &str) -> f64 
    pub fn model_cost(&self, model: &str) -> f64 
    pub fn report(&self) -> ConservationReport 
    pub fn set_autonomy_limit(&mut self, level: u32, max_budget: f64) 
```

## How It Works

Read the source in `src/` for full implementation details. All algorithms are documented with inline comments explaining the mathematical foundations.

## The Math

This crate implements formal mathematical constructs. See the source documentation for theorem statements and proofs of correctness.

## Testing

**51 tests** covering construction, serialization, correctness properties, edge cases, and composability with other lau-* crates.

## License

MIT
