# Continuation Prompt: Constant-Maturity Prediction (CMP) Indices for Risk Decomposition

## Your job

Build **constant-maturity prediction (CMP) indices** for risk decomposition and
analytics, inside the zed-kask platform. The user has a clear, specific design.
Your first job is to **listen to it and implement exactly that** — not to
substitute your own architecture. Read the user's requirements below as the
spec. When the user corrects you, change course immediately and completely.

## What the user actually wants (their words, paraphrased — treat as the spec)

1. **Constant-maturity prediction indices.** Take time out of the equation by
   building indices with a **fixed maturity** from rolling contracts, so the
   only thing that moves is the probability. Analogous to constant-maturity
   Treasury yields.

2. **Weighted portfolios, not curve read-offs.** Each index is a **weighted
   portfolio of real contracts** whose weighted-average maturity matches the
   target to within a tolerance (default 0.5 days). It must be a constructible,
   holdable basket — "so it could be turned into a financial product or ETF
   theoretically." Not an interpolated curve.

3. **Three orientation indices per base event**: **increase, decline, stable.**
   Stable can be built from no-change contracts directly, or by **balancing
   increase and decline contracts** against each other.

4. **Materiality = (type, level).** Type is **relative** (percent change) or
   **absolute** (absolute change in the base contract's own units — bp for
   rates, pp for inflation, $ for oil). The level is **volatility-based**,
   derived from the underlying's liquid futures. **Review the materiality
   setting on each base contract individually.**

5. **Base events** are contracts whose object is a **systematic factor in the
   global economy**, always available on Kalshi and Polymarket: interest rates,
   inflation, oil, natural gas, bitcoin, ethereum (to start).

6. **Abstract general contracts.** Compose abstract directional indices (e.g.
   "rate increase", "rate decrease") that abstract away specific contract
   details and are **never held to maturity** — so they behave like **synthetic
   swaps** (interest-rate swap, currency swap, commodity swap) and can be used
   for scenarios and as general risk measures.

7. **Semantic/ontological mapping — THIS IS THE CRITICAL, REPEATEDLY-FAILED
   REQUIREMENT.** Eligibility depends on **what the contract is about** (its
   object) at the right granularity, mapped through a real ontology — NOT
   keyword grep. "Fed decision", "FOMC meeting", "Fed funds rate", "rate cut",
   "how many cuts" are ALL about the same object: the central bank's short-term
   policy interest rate. A contract about a Federal Reserve meeting that sets
   short-term rates is an interest-rate contract — full stop. The mapping must
   not exclude obvious interest-rate contracts. Use the FIBO ontology (the
   companies server already anchors to it — see
   `kask/mcp-servers/hkask-mcp-companies/src/fibo.rs`) and/or Dublin Core.
   Orientation and magnitude follow from the object's relation to the current
   level of the factor.

8. **Contract maturity vs rate maturity.** The CMP target maturity is over
   **contract expirations** (3-month, 6-month to start), not the maturity of
   the underlying rate. "An increase in interest rates is an increase in the
   interest-rate curve across rate maturities, using prediction contracts of a
   constant maturity of 3 months or 6 months to start."

9. **No magic numbers.** Every threshold (materiality k, maturity tolerance,
   windows, roll hand-off, stable-balance tolerance, vol window) is a **passed
   variable** in a config, so the composition procedures can be fine-tuned
   without code changes.

10. **Equities are priced on fundamental forecast models (DCF/RIM, MAIA).**
    NO CAPM, no factor betas, no equity-return regressions. The
    arbitrage-pricing apparatus applies to the **contracts** (decompose and
    bridge their prices, analyze coherence), never to modeling stock returns.

## Errors the previous agent made (learn from these — do not repeat)

1. **Substituted its own design for the user's spec, repeatedly (~10 times).**
   When the user gave an explicit instruction, the agent implemented something
   adjacent but different, then defended it. **Listen first; implement what is
   asked.**

2. **Keyword grep instead of semantic/ontological mapping** (the most-rebuked
   failure). It matched the literal phrase "interest rate" and missed "Fed rate
   cuts", "FOMC", "Fed decision" — obvious interest-rate contracts. The user:
   "YOU CLEARLY AREN'T DOING ANY ONTOLOGICAL OR SEMANTIC MAPPING." Build a real
   object-resolution ontology (synonym closure → one referent), anchored to
   FIBO, not substring matching.

3. **Built on assumed contract availability without downloading the catalog.**
   It registered oil/gas as base events; the live catalog shows **zero oil/gas
   contracts** on either venue. The user had to insist: "we can't do anything
   until we can download a contract catalog and do the semantic mapping on the
   contracts from each provider." **Download and inspect the real catalogs
   FIRST** (Polymarket Gamma `/events`, Kalshi `/trade-api/v2/events`), then
   build the mapping on what's actually there.

4. **Equity-return beta regressions / CAPM drift.** It built factor loadings of
   stock returns and a Fama-French stage-1/stage-2 test. The user: "MAIA
   doesn't use betas for equity pricing... we price equities based on
   fundamental forecast models not arbitrage pricing theory or capital asset
   pricing models." This was "off the reservation." Do not build equity-return
   factor models.

5. **Magic numbers after being told to parameterize.** It proposed hard-coded
   thresholds (k=1.0, 0.5-day tolerance, tenor grids) as logic, then had to be
   told "compose the rules so that they pass variables... make sure you aren't
   embedding magic numbers anywhere."

6. **Confused contract maturity with rate maturity**, and proposed 1m/3m/6m
   targets without checking that the available macro contracts are long-dated
   annual levels. The user had to correct the maturity concept twice.

7. **Curve-interpolation instead of weighted portfolio.** It found an existing
   `cmp.rs` (log-odds curve interpolation) and nearly treated it as the CMP,
   until the user clarified the index must be a **weighted portfolio** (ETF-like
   basket).

8. **Over-built and over-planned.** It produced sprawling planning documents and
   kept "proceeding" to new build steps instead of stopping to listen. The user
   values direct, minimal, correct work over impressive-looking output.

## What exists in the codebase (current state — verify before trusting)

- `kask/mcp-servers/hkask-mcp-prediction-markets/src/`
  - `cmp.rs` — log-odds curve interpolation (T14). NOT the weighted-portfolio
    CMP; has embedded constants. Diagnostic only.
  - `cmp_portfolio.rs` — the previous agent's weighted-portfolio attempt:
    `CmpConfig`, `MaterialitySetting`, `classify_orientation`,
    `materiality_level`, `check_eligibility`, `solve_portfolio` (bracket-pair
    weight solver). Partially aligned with the spec but built on the flawed
    keyword `base_event.rs` classifier. Review critically; keep what's sound,
    fix the semantic layer.
  - `base_event.rs` — keyword-grep classifier (the rebuked approach). Replace
    or rebuild on real ontology.
  - `economic_object.rs` — a just-started FIBO-anchored object ontology with
    synonym closure (`EconomicObject`, `resolve_object`, `contracts_about`).
    This is the right direction but unfinished and unwired. **Start here.**
  - `types.rs` — `MarketRecord` (has `time_to_maturity`, reliability tiers,
    volatility flags, ontology block, `test_utils::market_record_fixture`).
  - `provider_kalshi.rs`, `provider_polymarket.rs` — `fetch_events`,
    `fetch_markets` (catalog endpoints, confirmed reachable).
- `kask/mcp-servers/hkask-mcp-companies/src/fibo.rs` — FIBO concept constants
  (the ontology anchor to reuse).
- Planning docs in `tasks/bayesian-apt/` (plan.md, cmp-foundation.md,
  hypothesis-dossier.md, etc.) — read critically; they contain the drift.

## Recommended first steps

1. **Download the real contract catalogs** from both providers and inventory
   what interest-rate / inflation / commodity / crypto contracts actually
   exist, with their expirations. Ground everything in this.
2. **Build the object-resolution ontology properly** (FIBO-anchored, synonym
   closure) so every interest-rate-object contract — including "Fed decision",
   "FOMC", "rate cut", "how many cuts" — resolves to the policy-rate object.
   Test that no obvious rate contract is excluded.
3. Only then layer orientation (increase/decline/stable), materiality
   (type, level, volatility-based, per-contract reviewed), and the
   weighted-portfolio maturity matching — all parameterized.
4. Keep the work minimal and correct. Listen to the user. When corrected,
   change course fully and immediately.
