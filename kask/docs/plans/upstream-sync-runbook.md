# Upstream-Sync Runbook

## When
Run after every `git fetch upstream` or before a release.

## Steps
1. `git fetch upstream && git merge upstream/main`.
2. Resolve conflicts ONLY in:
   - The D1–D9 seam files (listed in `DIVERGENCE.md` — all in zed's tree, NOT under `kask/`).
   - The root `Cargo.toml` `[workspace.members]` + `[workspace.dependencies]` arrays.
3. If a conflict appears **under `kask/`** — STOP. `kask/` is additive (ours); upstream never touches it. A conflict there means something is wrong with the merge base.
4. `cargo build -p kask_bridge -p kask_panel` — verify the stubs still compile.
5. `bash kask/scripts/check-hkask-no-zed-deps.sh` — the §13.1 invariant must hold.
6. Update `DIVERGENCE.md` if a divergence moved or a new Dn was added.

## What NEVER conflicts
- Everything under `kask/` (additive — upstream has no such paths).
- The hKask keep-crates, skills, scripts, docs (all under `kask/`).

## What MIGHT conflict
- D-seam files in zed's tree (`crates/agent_skills`, `crates/agent/src/tools/skill_tool.rs`, `crates/context_server/src/client.rs`, `crates/language_model*`, `crates/credentials_provider`, `crates/agent/src/thread.rs`, `crates/paths/src/paths.rs`, `crates/release_channel/src/lib.rs`, etc.).
- `Cargo.toml` workspace arrays (our `kask/*` entries + upstream's crate additions).
- `crates/settings_ui/src/page_data.rs` (the kask page registration, D9).
