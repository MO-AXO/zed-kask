//! kask_panel — native GPUI Panel for per-MCP-server one-on-one interaction (D10).
//!
//! Reuses zed's visual language: ui::prelude::* components + theme tokens.
//! Copy-template: agent_ui/src/agent_panel.rs (impl Panel for AgentPanel).
//!
//! Structure:
//! - server catalog (the 12 default-load MCP servers, from the in-process registry)
//! - per-server sub-view: direct :tool args invocation (OCAP-gated) + scoped inference
//!
//! TODO (T-s2): KaskPanel struct + impl Panel boilerplate.
//! TODO (T-s3): per-server sub-view (direct invoke + scoped inference).
//! TODO (T-s4): wire to in-process tool registry (T3.0) + guarded inference (D8).

// Stub — implementation begins when the bridge (D8) is wired.
