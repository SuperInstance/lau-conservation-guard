# lau-conservation-guard

**Budget enforcement layer for the LAU construct — tracks, verifies, and enforces conservation units across rooms, models, and operation types.**

A lightweight Rust library that treats every AI operation (text responses, code generation, tool calls, etc.) as a metered expense against a global budget. It provides receipt-based accounting, per-room and per-model cost breakdowns, autonomy-level limits, and graceful degradation strategies for when budgets run low.

---

## What This Does

In a multi-agent system where every action costs energy (compute, API tokens, latency budget), you need an answer to three questions at all times:

1. **Can I afford this operation?**
2. **Where did the budget go?**
3. **What do I do when I'm running out?**

`lau-conservation-guard` answers all three. It's a ledger that:

- **Tracks** every spend with a receipt (room, model, operation type, cost, timestamp)
- **Verifies** that `budget == spent + remaining + wasted` at all times
- **Enforces** per-room and per-autonomy-level spending caps
- **Degrades gracefully** when budget runs low (reject new, cheap-only, critical-only, or emergency stop)

---

## Key Idea

The guard maintains an invariant:

```
budget = spent + remaining + wasted
```

Every spend decrements `remaining` and increments `spent`. Every refund reverses it. Waste is a separate bucket for overhead that can't be reclaimed. At any point, calling `verify_conservation()` confirms the invariant holds.

On top of this, **autonomy levels** let you set spending caps per privilege tier — e.g., a level-1 agent can only spend 10 units in a given room, while a level-5 agent has no cap.

---

## Install

```toml
[dependencies]
lau-conservation-guard = "0.1"
```

Or:

```sh
cargo add lau-conservation-guard
```

### Requirements

- Rust 2021 edition or later
- `serde` + `serde_json` (transitive)

---

## Quick Start

```rust
use lau_conservation_engine::{
    ConservationGuard, OperationType, GracefulDegradation, DegradationStrategy,
};

fn main() {
    // 1. Create a guard with a budget of 1000 conservation units
    let mut guard = ConservationGuard::new(1000.0);

    // 2. Set autonomy limits
    guard.set_autonomy_limit(1, 50.0);   // level-1: max 50 per room
    guard.set_autonomy_limit(2, 200.0);  // level-2: max 200 per room

    // 3. Spend on an operation
    let receipt = guard
        .spend(
            OperationType::TextResponse(42),
            "chat-room",
            "gpt-4",
            2.5,
        )
        .unwrap();

    println!("Receipt: {} cost {:.2}", receipt.id, receipt.cost);

    // 4. Check affordability before spending
    if guard.can_afford(5.0, "chat-room", 1) {
        guard.spend(OperationType::CodeGeneration(100), "chat-room", "gpt-4", 5.0).unwrap();
    }

    // 5. Refund if needed
    guard.refund(receipt);

    // 6. Generate a report
    let report = guard.report();
    println!("{}", report.render());

    // 7. Verify conservation invariant
    assert!(guard.verify_conservation());
}
```

---

## API Reference

### `ConservationGuard`

The central budget tracker.

```rust
pub struct ConservationGuard {
    pub budget: f64,
    pub spent: f64,
    pub wasted: f64,
    pub per_room: HashMap<String, f64>,
    pub per_model: HashMap<String, f64>,
    pub per_operation: HashMap<OperationType, f64>,
    pub autonomy_limits: HashMap<u32, f64>,
}
```

| Method | Returns | Description |
|--------|---------|-------------|
| `new(budget)` | `Self` | Create a guard with the given total budget |
| `remaining()` | `f64` | `budget − spent − wasted` |
| `can_afford(cost, room, autonomy_level)` | `bool` | Check if an operation fits within remaining budget AND room-level autonomy cap |
| `spend(op, room, model, cost)` | `Result<ConservationReceipt, String>` | Deduct budget, generate a receipt, track by room/model/operation |
| `refund(receipt)` | `()` | Reverse a spend using its receipt |
| `verify_conservation()` | `bool` | Confirm `budget == spent + remaining + wasted` |
| `room_budget(room)` | `f64` | Total spent in a specific room |
| `model_cost(model)` | `f64` | Total cost attributed to a model |
| `report()` | `ConservationReport` | Full breakdown (by room, model, operation, top spenders) |
| `set_autonomy_limit(level, max_budget)` | `()` | Set per-room spending cap for an autonomy level |

### `OperationType`

All operation categories the guard recognizes:

| Variant | Base Cost | Description |
|---------|-----------|-------------|
| `TextResponse(tokens)` | `tokens as f64` | LLM text generation |
| `CodeGeneration(tokens)` | `tokens as f64` | Code output |
| `ToolExecution(cost)` | `cost as f64` | External tool/API call |
| `PhoneAFriend(cost)` | `cost as f64` | Delegation to another agent |
| `ProvenanceCommit` | 1.0 | Provenance record commit |
| `CorrelationScan` | 5.0 | Correlation analysis |
| `EnsignWake` | 2.0 | Ensign agent wake-up |
| `EnsignTick` | 0.5 | Ensign periodic tick |
| `DeadbandCheck` | 0.1 | Deadband threshold check |
| `RoomRouting` | 0.5 | Room message routing |
| `TileCreate` | 1.0 | Tile creation |
| `TileQuery` | 0.5 | Tile lookup/query |

Each variant implements `base_cost()`, and the token/cost-bearing variants pass through their payload as the cost.

### `ConservationReceipt`

Proof of spend, used for refunds.

```rust
pub struct ConservationReceipt {
    pub id: String,           // e.g., "cr-42"
    pub operation: OperationType,
    pub room: String,
    pub model: String,
    pub cost: f64,
    pub timestamp: u64,       // Unix epoch seconds
    pub tile_id: Option<String>,
}
```

Receipts are auto-incremented via a global atomic counter (`cr-1`, `cr-2`, ...).

### `ConservationReport`

Full budget snapshot.

```rust
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
```

| Method | Description |
|--------|-------------|
| `render() → String` | Human-readable report with sections for budget, room, model, and top spenders |

### `GracefulDegradation`

Controls what happens when budget runs low.

```rust
pub enum DegradationStrategy {
    RejectNew,                         // Block all new operations
    CheapOnly { threshold: f64 },      // Only allow ops under threshold
    CriticalOnly,                      // Only provenance + deadband
    EmergencyStop,                     // Everything stops
}
```

```rust
let dg = GracefulDegradation::new(DegradationStrategy::CheapOnly { threshold: 5.0 });
if dg.should_allow(3.0, remaining_budget) {
    // proceed
}
```

| Method | Description |
|--------|-------------|
| `new(strategy)` | Create with a strategy |
| `should_allow(cost, remaining) → bool` | Check if an operation should be permitted |

---

## How It Works

### Spend Flow

1. **Check**: `can_afford()` verifies the cost fits within both the remaining budget and any autonomy-level cap for the room.
2. **Deduct**: `spend()` creates a receipt, increments `spent`, and updates `per_room`, `per_model`, and `per_operation` maps.
3. **Audit**: Every receipt has a unique ID, timestamp, and full provenance (room, model, operation).

### Refund Flow

Passing a receipt to `refund()` reverses the spend: decrements `spent`, adjusts all tracking maps, and removes entries that hit zero. This is how you handle retries, cancellations, or credited operations.

### Conservation Invariant

```
budget = spent + remaining + wasted
```

`remaining` is always `budget − spent − wasted`. `verify_conservation()` checks that these sum correctly (within floating-point epsilon).

### Autonomy Levels

Autonomy limits are per-room caps keyed by level number:

```rust
guard.set_autonomy_limit(1, 50.0);  // Level 1: max 50 units per room
```

When `can_afford()` is called with an autonomy level, it checks whether `room_spent + cost > limit`. This prevents a low-privilege agent from burning the entire budget in one room.

### Receipt IDs

Receipts use a global atomic counter (`AtomicU64`) for unique IDs in the format `cr-N`. This is thread-safe and monotonically increasing.

---

## The Math

### Budget Conservation

The core invariant is linear:

```
budget = spent + remaining + wasted
```

This is a tautology by construction — `remaining` is defined as `budget − spent − wasted`. The `verify_conservation()` method exists as a sanity check (e.g., catching floating-point drift or logic errors).

### Cost Attribution

Total spend is decomposed along three axes:

```
total_spent = Σ (per_room)  = Σ (per_model)  = Σ (per_operation)
```

Each axis sums to the same total — they're different views of the same data. This means:

- You can see *which room* is expensive
- You can see *which model* is expensive
- You can see *which operation type* is expensive

All three views are always consistent because they're updated atomically in each `spend()` call.

### Autonomy Caps

For autonomy level `L` with cap `C_L`, a room `R` can accumulate at most `C_L` total spend:

```
room_spent(R, L) ≤ C_L
```

This is a linear constraint that prevents any single agent tier from monopolizing the budget.

---

## Testing

The library includes **51 unit tests** covering:

- Basic spend, refund, and remaining calculations
- Insufficient budget and negative cost rejection
- Conservation invariant verification (pristine, after spend, with waste)
- Per-room and per-model cost tracking
- Autonomy level enforcement
- All 12 `OperationType` base costs
- All 4 degradation strategies
- Receipt ID uniqueness and format
- Serde roundtrip serialization for all types
- Full report generation and rendering
- Edge cases: zero cost, exact-remaining spend, clean refund removal

Run them:

```sh
cargo test
```

---

## License

MIT
