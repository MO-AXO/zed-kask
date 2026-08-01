# Minimalist Refactor — Todo

## Slice 1 — `EnergyEstimator` trait deletion test
- [x] Done. Verdict: **remove**. 92+10 tests green.

## Slice 2 — `EscalationPort` trait deletion test
- [x] Done. Verdict: **remove**. 72+79 tests green. Zero dyn-consumers.

## Slice 3 — `LedgerStoragePort` trait deletion test
- [x] Done. Verdict: **remove**. 72+79 tests green. Zero dyn-consumers.

## Slice 4 — `EmbeddingPort` trait deletion test
- [x] Done. Verdict: **remove**. 72+79 tests green. Zero dyn-consumers.

## Slice 5 — `WalletBudgetPort` + `WalletBackedBudget` dead path deletion test
- [x] Done. Verdict: **remove**. 91 tests green (1 test removed with deleted production code).
  `register_wallet_budget` had zero call sites; entire `WalletBackedBudget` →
  `wallet_budgets` map → sensor fallback chain was dead.

## Slice 6 — `SkillReader` trait deletion test
- [x] Done. Verdict: **remove**. 130 tests green. Single impl, no test mock
  despite doc claim.

## Slice 7 — `RuntimePolicy` trait deletion test
- [x] Done. Verdict: **remove**. 91+130 tests green. Consumer already depended
  on impl crate directly.

## Final report
- [x] `tasks/final-report.md` written with before/after code graph, edge delta,
      deletion-test verdicts, and suggested .rules additions.

---

# Bridge Seam Simplification — Open

## Operator hypothesis (2026-07-31)

The complexity of the kask_bridge seam surface (D1–D12 in DIVERGENCE.md)
caused an error in one of the seams that we have missed. The best way to find
it and clean it up is by simplifying the seams — reducing the number of
interfaces the bridge must manage. The OpenRouter 402 / "can only afford
7326 tokens" failure is the presenting symptom; the root cause is a seam
that resolves the wrong key, sends the wrong max_tokens, or shadows the
upstream provider with a compatible-provider entry that diverges in key
resolution.

## Open questions

1. **Sovereignty keys (D5) essentialist review.** Do `hkask-keystore`,
   `HKASK_OCAP_SECRET`, `HKASK_A2A_SECRET`, and the `keyring`-direct
   keychain path pass the essentialist deletion test? Preliminary verdict:
   **no** — hKask runs in-process under zed-kask; the `McpRuntime`
   governance membrane is the real authority and does not depend on the
   OCAP token. The sovereignty-key seam exists for a standalone-hKask
   deployment topology that zed-kask does not use. It adds keychain reads,
   env-var resolution, and a `OnceLock`-injected `keyring` — all surfaces
   where a key can fail to resolve silently. Run the full G1/G2/G3 essentialist
   loop to confirm before deleting.

2. **Bridge seam inventory + essentialist audit.** Enumerate every interface
   in `kask/crates/kask_bridge/` and every D-seam in `DIVERGENCE.md`. For each,
   run the deep-module deletion test: if the seam were removed, where would
   the complexity reappear? Seams that exist only to support a deployment
   topology zed-kask doesn't use (standalone hKask, daemon transport, OCAP
   membrane in front of `McpRuntime`) are candidates for removal.

3. **OpenRouter key resolution divergence.** The key in `kask/.env`
   (`sk-or-v1-6afb63472df...`) queries as `limit: 500, usage_weekly: 499.97,
   limit_remaining: 0.032` via the OpenRouter API — a $500/week key that is
   maxed out, not the $3000/week key the operator reports in the dashboard.
   Either (a) the file has the wrong key, or (b) zed-kask is sending a
   different key than the one in the file. Discriminating test: add a
   one-line `log::info!` in `OpenRouterLanguageModelProvider::stream_completion`
   that logs the key prefix (first 12 chars) + the key ID hash, then compare
   against the file. This isolates whether the divergence is in key storage
   or in the bridge's key-resolution seam.

4. **`max_tokens: null` divergence.** Upstream zed sends `max_tokens: null`
   for OpenRouter (because `max_output_tokens()` returns `None`), and
   OpenRouter applies its 65,536 default. zed-kask has not changed this path.
   But the 402 is triggered by the 65,536 default exceeding the key's
   remaining weekly budget. If the key-resolution seam is fixed (correct
   $3000/week key), the 65,536 default is fine. If not, a `max_output_tokens`
   override setting may be needed. Do not add it until the key seam is
    resolved — otherwise we mask the real bug.

## Next action

Run the essentialist skill on D5 (sovereignty keys) as the first seam to
evaluate, since it has the clearest deletion-test failure and would remove
the most keychain-resolution surface area.