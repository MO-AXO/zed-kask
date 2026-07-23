# kask/ — hKask integration into zed-kask

This directory holds everything hKask that is **merged into the zed-kask fork**.
It is additive to upstream zed — `git merge upstream/main` never touches here.

## Layout
- `crates/` — hKask keep-crates (`hkask-*`) + the bridge (`kask_bridge`, D8) + the panel (`kask_panel`, D10)
- `mcp-servers/` — the 15 hKask MCP server crates (12 loaded by default)
- `skills/` — the skills registry (`manifest.yaml` + `*.j2` templates; Pattern A source of truth)
- `scripts/` — hKask admin/build/CI scripts (including `check-hkask-no-zed-deps.sh`)
- `docs/` — architecture, specs, plans (the documentation home)

## References
- `DIVERGENCE.md` (repo root) — the fork's divergence manifest + upstream-sync procedure
- `kask/docs/architecture/zed-host-architecture-plan.md` — the full architecture + migration plan
- `kask/docs/specs/` — D1–D10 seam specifications
