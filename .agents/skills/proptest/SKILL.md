# Proptest

Property-based testing skill. Identifies testable properties from a target function's contract, designs input strategies using the shared `hkask-test-harness` crate, generates proptest code, executes it, analyzes shrunk counterexamples, and reports verified properties. Complements TDD (which writes the first test per behavior) by writing the universal test that covers the full input space.

## When to Use

- After implementing a function with a clear invariant (e.g., compression never expands)
- When retrofitting tests for an untested pure function (e.g., `compute_budget`, `unwrap_tool_envelope`)
- When TDD's `tdd-gap-check` flags a `prob:` field gap (a probabilistic contract with no property test)
- When bug-hunt finds a class of bugs that a property test would catch (e.g., integer overflow in budget calculation)

## How It Works

Five-phase convergent PDCA, with the shape emerging from PBT's domain (QuickCheck/proptest):

1. **Identify** — Read the target's contract (`/// expect:`, `/// post:`, `/// inv:`), classify testable properties by oracle type (panic-freedom, invariant, round-trip, reference, idempotency)
2. **Strategize** — For each property, design the input strategy. Check `hkask-test-harness` first (`arb_json_value`, `test_token_for_tool`, `NoopToolPort`), then custom strategies (`select`, `prop_recursive!`, `any::<T>()`, `prop_filter`, tuple composition)
3. **Write** — Generate the complete `proptest!` block with principle grounding comments, descriptive failure messages, and oracle-appropriate assertions
4. **Analyze** — Execute `cargo test`, parse results. If the test fails, analyze the shrunk counterexample: is it a real bug (flag for `diagnose`) or a test bug (fix the property)?
5. **Report** — Structured report: properties verified, failures found, coverage gained, harness usage

## Oracle Taxonomy

| Oracle type | What it asserts | Test structure |
|-------------|----------------|----------------|
| Panic-freedom | Never panics on any input | `catch_unwind` + `prop_assert!(result.is_ok())`, no `prop_assume!` |
| Invariant | A property holds for all inputs | `prop_assert!` / `prop_assert_eq!` with the property |
| Round-trip | `deserialize(serialize(x)) == x` | Generate valid values, serialize, deserialize, compare field-by-field |
| Reference | Output matches independent implementation | Run both, assert equality |
| Idempotency | `f(f(x)) == f(x)` | Apply twice, assert equality |

## Relationship to Other Skills

- **TDD**: writes the first test per behavior (one input, one assertion). Proptest writes the universal test (all inputs, one invariant). TDD's `tdd-gap-check` flags `prob:` field gaps; proptest fills them.
- **bug-hunt**: explores for unknown bugs via charter-driven probing. Proptest systematically verifies known properties. Bug-hunt's `pattern_signatures` feed into proptest's Identify phase.
- **diagnose**: when proptest finds a real bug, the shrunk counterexample is a pre-minimized reproducer — exactly what diagnose's Phase 2 needs.

## Harness Integration

The skill checks `hkask-test-harness` first for shared generators:
- `arb_json_value()` for JSON/YAML deserialization surfaces
- `test_token_for_tool(name)` + `test_agent_webid()` for governance gate tests
- `NoopToolPort` for ToolPort stub fixtures

When the harness doesn't provide what's needed, the skill designs custom strategies using `select`, `prop_recursive!`, `any::<T>()`, `prop_filter`, and tuple composition.