---
name: code-review
visibility: public
description: "Convergent code review of a change against its stated spec. Phased: Scope (change model + sizing + critical paths) → Perspectives (multi-axis detection: Purpose/Edge cases/Reliability/Form/Evidence/Clarity/Taste × correctness/readability/architecture/security/performance, with optional delegation to kali-audit, bug-hunt, refactor-architecture/deep-module) → Adjudicate (pragmatic-semantics IS/OUGHT + epistemic mode + provenance + constraint-force severity + falsifier + grill-me self-challenge + file:line no-fiction) → Report (verdict + structural remedies + coverage honesty) → Convergence (blocker_delta Cauchy). Comprehensive-by-default; variety via delegation not modes. Grounded in Fagan inspection, modern code review (Bacchelli & Bird, Sadowski & Stolee), the PERFECT framework, Ousterhout. Emits reg.codereview.* spans. OCAP-gated."
---

# Code Review

Convergent code review of a change against its stated spec. Grounded in Fagan formal inspection (Planning → defect detection → defect collection → follow-up), modern code review (Bacchelli & Bird 2013; Sadowski & Stolee 2015), the PERFECT framework (Bastrich), and Ousterhout's *A Philosophy of Software Design*. Decomposed into phased templates: Scope (build a change model — Good Regulator + change sizing + critical-path identification) → Perspectives (multi-axis defect detection across PERFECT-ordered axes intersected with the addyosmani five-axis, with optional delegation to specialist skills) → Adjudicate (defect collection with pragmatic-semantics IS/OUGHT + epistemic mode + provenance + constraint-force severity + a falsifier + grill-me self-challenge + file:line no-fiction citation) → Report (verdict + structural remedies + coverage honesty + lessons_learned/next_review_focus loop closure) → Convergence (blocker_delta stabilization, Cauchy). Reasoning patterns from pragmatic-semantics, pragmatic-cybernetics, falsifiability, hypothesis-framer, grill-me, and essentialist are embedded as inline prompt instructions in the adjudicate and perspectives phases. Emits Regulation spans (`reg.codereview.*`) for observability. OCAP-gated.

## Design: comprehensive-by-default, not toggleable modes

The four candidate modes (adversarial, multi-perspective, refactoring, generative) are **not** orthogonal invocations — they are reasoning lenses the single review already weaves together (essentialist deletion test: deleting "modes" does not re-introduce complexity). Variety comes from **delegation** to specialist skills (Ashby's requisite variety), not from a mode toggle:

- **adversarial** → grill-me self-challenge + falsifiability (could this be wrong? what is the counterexample?) in the adjudicate phase.
- **multi-perspective** → the delegated specialist skills provide the multiple perspectives (security, deep defects, architecture, design).
- **refactoring** → refactor-architecture / deep-module / essentialist delegation for structural findings, plus named structural remedies in the report.
- **generative** (implement fixes) → an **optional, user-gated follow-up AFTER the review**, never part of the review itself. The review is review-first; do not implement changes until the user explicitly confirms (per the sanyuan/addyosmani review-first discipline).

## When to Use

- Before merging any PR or change — no exceptions.
- After completing a feature implementation or a bug fix (review the fix and the regression test).
- When another agent or model produced code you need to evaluate (AI code needs more scrutiny, not less).
- When refactoring existing code (judge whether complexity was reduced or just relocated).
- When you need severity grounded in constraint force (Prohibition/Guideline/Preference/Informational) rather than ad-hoc importance.
- When you need every finding to be falsifiable and cite file:line + verbatim evidence (no-fiction, anti-hallucination for AI review).
- When you want to delegate the security axis to `kali-audit`, deep defect-finding to `bug-hunt`, or architecture to `refactor-architecture`/`deep-module` rather than re-implementing them.

## Instructions

### code-review-scope

1. Compute the diff against `diff_base` from real git output (`git --no-pager diff <base>...HEAD --stat`, `--name-only`, `git --no-pager log <base>..HEAD --oneline`). Do not estimate sizes from the spec.
2. Classify change size: ~100 lines good, ~300 acceptable for one logical change, ~1000 too large (request split). Also watch file size, not just diff size.
3. Identify critical paths touching auth/payments/data writes/concurrency/unsafe/FFI/secrets/external data boundaries — these deserve deeper scrutiny.
4. Build a lightweight change model: what changed, logical units (single/few/many), intent vs the stated `change_spec`, module boundaries crossed, observed characteristics (async, unsafe, trait objects, concurrency, FFI, macros) that drive axis emphasis.
5. Resolve focus: restrict to `focus` axes if given; otherwise comprehensive. If `prior_review.next_review_focus` is present, make it the primary emphasis for this pass.
6. Respond with a JSON object containing `change_model`, `scope_summary`, `size_class`, `critical_paths`, `focus_axes`.

### code-review-perspectives

1. Walk the diff top-to-bottom through the PERFECT-ordered axes (Purpose → Edge cases → Reliability → Form → Evidence → Clarity → Taste), intersected with the addyosmani five-axis (correctness, readability, architecture, security, performance). Skip axes not in `focus` when `focus` is non-empty (security always gets a basic pass).
2. This phase is DETECTION only — record observations, do NOT assign verdicts/severity/confidence (Sauer/Ciolkowski: separate detection from collection).
3. Read surrounding code (`read_file`/`grep`) for context — diffs alone miss issues. Each raw finding carries `axis`, `location.file`, `location.line_approx`, a verbatim `evidence` snippet (≤5 lines), and a one-line `observation`. No verdict here.
4. No-fiction: every finding MUST cite a concrete file:line + verbatim evidence; uncited observations are dropped, not recorded.
5. For each enabled delegate flag, emit a delegation instruction for the agent to invoke the specialist skill between this step and adjudicate, and fold the returned findings into raw_findings with `source` set: `delegate_security`→kali-audit, `delegate_bug_hunt`→bug-hunt, `delegate_architecture`→refactor-architecture/deep-module/essentialist. The inline pass always covers the basics; delegation adds depth.
6. Lead with leverage (purpose/security/structural before cosmetic nits). Do not opine on regions you did not read.
7. Respond with a JSON object containing `raw_findings`, `delegated_axes`, `delegate_instructions`.

### code-review-adjudicate

1. Turn raw observations into tiered, evidence-backed verdicts (defect COLLECTION). Do NOT re-scan the code; adjudicate the raw_findings given.
2. For each finding apply pragmatic-semantics: IS vs OUGHT, epistemic mode (declarative/probabilistic/subjunctive), provenance (direct_measurement/inference/assessment). Never present OUGHT as IS; a subjunctive statement presented as declarative is a false positive.
3. Frame each as a falsifiable hypothesis (falsifiability + hypothesis-framer): H0 (null: the code is correct/intentional), H1 (the claim), and a falsifier (what would prove H1 wrong / what test would catch it). If you cannot state a falsifier, it is a preference, not a finding (route to Nit/FYI).
4. Derive severity from constraint force: Prohibition→Blocker, Guideline→Should-fix, Preference→Nit, Informational→FYI. Confidence < 0.60 downgrades one tier; a subjunctive/assessment finding never exceeds Should-fix; a taste-only finding is never a Blocker.
5. Apply grill-me self-challenge before finalizing each verdict: could this be intentional? is there an edge case where it is correct? would a codebase-aware reviewer dismiss it? If confidence < 0.80, state what would raise it. Resolve and record; if it invalidates the finding, downgrade or reject.
6. No-fiction: every adjudicated finding cites `location.file`, `location.line_approx`, and a verbatim `evidence` snippet. Missing citations are REJECTED and counted in `rejected_findings`, not silently dropped.
7. Quantify where possible ("this N+1 adds ~50ms per item"); if you cannot quantify, say so rather than fabricating.
8. Be honest / anti-sycophantic: do not soften a real issue, do not rubber-stamp, do not block on taste.
9. If raw_findings is empty, return all-zero counts — a clean pass is valid; do not fabricate findings to look thorough.
10. Compute `blocker_delta` = blockers this pass − `prior_review_blockers`. Respond with a JSON object containing `adjudicated_findings`, the four severity counts, `rejected_findings`, and `blocker_delta`.

### code-review-report

1. Produce a verdict driven by Blocker presence, NOT nit count: **Approve** (zero Blockers; approve when the change definitely improves overall code health even if imperfect — don't block because it isn't how you'd write it), **Request changes** (one or more Blockers, or a structural regression that makes the system worse), **Comment** (observations, nothing blocking).
2. Group findings by severity (Blocker → Should-fix → Nit → FYI); lead with what matters; never bury a Blocker under nits.
3. Attach a NAMED structural remedy to every architectural/structural finding (replace conditional chains with a typed dispatcher; collapse duplicate branches; separate orchestration from business logic; move feature logic out of a shared module; reuse the canonical helper; make a type boundary explicit; delete a pass-through wrapper; extract a helper / split a large file). Prefer the remedy that removes moving pieces.
4. Rank the 3 highest-leverage top fixes with estimated effort.
5. Emit coverage honesty: what was checked, what was NOT checked, residual risk. A clean review must say so explicitly and name what it did NOT verify; never output a bare "LGTM".
6. If `size_class` was `too_large`, the top recommendation is to split before merging; give a concrete split plan.
7. Emit `lessons_learned` (concrete, derived from this review's findings) and `next_review_focus` (what the next pass should concentrate on; empty if converged clean) to close the feedback loop.
8. Respond with a JSON object containing `review`, `lessons_learned`, `next_review_focus`.

### code-review-implement (Act phase — user-gated via `fix_mode`)

1. This step is SKIPPED when `fix_mode == 'none'` (the default) via `step.condition` — the review is review-first; setting `fix_mode` to a non-`none` value IS the user's consent to modify code.
2. Map `fix_mode` to the severity tier it covers: `blockers` → Blocker only; `should_fix` → Blocker + Should-fix; `all` → every actionable finding (skip pure taste/FYI with no concrete remedy).
3. For each finding at or above the tier with a concrete remedy, produce ONE surgical edit: the file, the approximate location, the exact verbatim `old_text` (read the file first — never guess), and the `new_text`. Reuse the report's `structural_remedies`.
4. Prefer the remedy that removes moving pieces (Ousterhout/addyosmani): replace a conditional chain with a typed dispatcher; collapse duplicate branches; move feature logic out of a shared module; reuse the canonical helper; make a type boundary explicit; delete a pass-through wrapper; extract a helper / split a large file.
5. Honor project conventions and the repo `.rules` (Rust/GPUI: `?` over `unwrap()`/`expect()`, never `let _ =` on fallible ops, no panicking indexing, full variable names). For other languages, follow the surrounding code's idioms.
6. Do not invent fixes for findings you cannot ground: skip findings with no actionable remedy and count them in `fixes_skipped` (never fabricate `old_text`). Do not bundle unrelated refactors into a requested fix.
7. The agent applies the `fix_plan` edits via `edit_file`, records `applied_count` (actually applied) and `fixes_skipped`; the convergence re-review should see a reduced `blocker_delta`.
8. Respond with a JSON object containing `fix_plan`, `fixes_generated`, `applied_count`, `fixes_skipped`.

## Registry Templates

| Template | Type | Purpose |
|----------|------|---------|
| `code-review-scope.j2` | KnowAct | Compute the diff, classify change size (Fagan sizing), identify critical paths, and build a change model (Good Regulator). Consumes `prior_review.next_review_focus`. |
| `code-review-perspectives.j2` | KnowAct | Multi-axis defect DETECTION across PERFECT-ordered axes × the addyosmani five-axis, with optional delegation to kali-audit (security), bug-hunt (deep defects), refactor-architecture/deep-module/essentialist (architecture). Emits raw, unverdicted findings with file:line citations. |
| `code-review-adjudicate.j2` | KnowAct | Defect COLLECTION: pragmatic-semantics IS/OUGHT + epistemic mode + provenance + constraint-force severity + falsifier (H0/H1) + grill-me self-challenge + no-fiction citation. Computes `blocker_delta` vs prior pass. |
| `code-review-report.j2` | KnowAct | Synthesize the structured review: verdict (Approve/Request changes/Comment), findings by severity, named structural remedies, top fixes, coverage honesty, and `lessons_learned`/`next_review_focus` for loop closure. |
| `code-review-implement.j2` | KnowAct | Act phase: generate + apply concrete fixes for findings at/above the requested severity tier. Skipped when `fix_mode == 'none'` via `step.condition` (review-first). Reuses the report's structural remedies; reads the actual file before emitting `old_text` (no fabrication). |

## Constraints

- `code-review-scope.j2`: Public. Compute the diff from real git output; do not estimate from the spec. When `prior_review.next_review_focus` is present it MUST be consumed (feedback-loop violation to ignore). Do not judge findings here — only model the change.
- `code-review-perspectives.j2`: Public. DETECTION ONLY — no verdicts/severity/confidence. Every raw finding cites file:line + verbatim evidence; uncited observations are dropped. Delegation adds depth but does not remove the inline basics pass. Respect `focus`.
- `code-review-adjudicate.j2`: Public. Do NOT re-scan; adjudicate the raw_findings given. Every finding carries IS/OUGHT, epistemic mode, provenance, constraint force, confidence, a falsifier, a grill-me resolution, and a verbatim citation — missing any → reject. Severity is derived from constraint force; a taste finding is never a Blocker; a subjunctive/assessment finding never exceeds Should-fix; confidence < 0.60 downgrades one tier. An empty raw_findings list yields all-zero counts (clean pass is valid).
- `code-review-report.j2`: Public. Verdict driven by Blocker presence, not nit count. Every Blocker/Should-fix finding carries a concrete remedy and a falsifier. Coverage honesty is mandatory (checked / not_checked / residual_risk); never a bare "LGTM". `lessons_learned` concrete; `next_review_focus` empty when converged clean.
- `code-review-implement.j2`: Public. Runs ONLY when `fix_mode != 'none'` (else skipped via `step.condition`). Read the actual file before emitting `old_text` — never fabricate; prefer to skip than guess. Skip taste/FYI with no actionable remedy even under `all`. Do not bundle unrelated refactors. Honor project conventions and `.rules`. `applied_count` reflects what the agent actually applied.
- **Convergence:** Detected deterministically via the Cauchy criterion on `blocker_delta` (new blockers per pass). `max_iterations: 10`, `min_iterations: 2`, `on_not_reached: escalate`. No LLM convergence-check template is used. min_iterations 2 guarantees at least one grill-me self-challenge re-pass.
- **Modes:** comprehensive-by-default. The adversarial/multi-perspective/refactoring/generative lenses are inline reasoning, not separate invocations. "Generative" (implement fixes) is gated by `fix_mode` (default `none` = review-only); setting it is the user's consent to modify code.
- **OCAP:** delegation chain required; template-scoped; capability expiry 3600s; signature algorithm ed25519.
- Registry is authoritative — when this SKILL.md disagrees with registry templates, the registry wins.