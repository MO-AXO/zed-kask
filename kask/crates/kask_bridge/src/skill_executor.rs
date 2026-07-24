//! `SkillManifestExecutor` adapter — bridges zed's `SkillTool` to hKask's `ManifestExecutor`.
//!
//! This is the D1 seam. When zed's `SkillTool` is constructed with a manifest executor,
//! skill activation runs the hKask cascade (KnowAct/FlowDef/RenderAct + PDCA + gas/rjoule
//! + OCAP) instead of injecting the `SKILL.md` body.
//!
//! The adapter implements zed's `agent::SkillManifestExecutor` trait by delegating to
//! hKask's `ManifestExecutor`. It holds an `Arc<dyn InferencePort>` (the bridge's
//! `LanguageModelInferencePort`) and constructs a `ManifestExecutor` per skill activation
//! (or reuses one if the manifest is the same).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use hkask_templates::ManifestExecutor;
use hkask_types::InferencePort;
use serde_json::Value;

/// Bridge between zed's `SkillManifestExecutor` trait and hKask's `ManifestExecutor`.
///
/// Holds an `InferencePort` (the bridge's `LanguageModelInferencePort` over zed's
/// `LanguageModel`) and an optional `ToolPort` (for FlowDef execution — D3, not yet wired).
/// KnowAct skills (which only need inference) work now; FlowDef skills gate on D3.
pub struct BridgeManifestExecutor {
    inference: Arc<dyn InferencePort>,
    a2a_secret: Vec<u8>,
}

impl BridgeManifestExecutor {
    pub fn new(inference: Arc<dyn InferencePort>, a2a_secret: Vec<u8>) -> Self {
        Self {
            inference,
            a2a_secret,
        }
    }
}

#[async_trait]
impl agent::SkillManifestExecutor for BridgeManifestExecutor {
    async fn execute_knowact(
        &self,
        skill_directory: &Path,
        template_ref: &str,
        context: HashMap<String, Value>,
    ) -> Result<String, String> {
        // Construct a ManifestExecutor with the bridge's InferencePort.
        // ToolPort is not yet wired (D3) — KnowAct skills don't need it.
        // Use a no-op ToolPort placeholder for now.
        let executor = ManifestExecutor::new(
            self.inference.clone(),
            Arc::new(NoOpToolPort),
            hkask_types::template::LLMParameters::default(),
            self.a2a_secret.clone(),
        )
        .with_template_base_path(skill_directory.to_path_buf());

        let result = executor
            .execute_knowact(template_ref, &context)
            .await
            .map_err(|e| e.to_string())?;

        Ok(result.to_string())
    }
}

/// Placeholder ToolPort for KnowAct-only execution (D3 not yet wired).
/// KnowAct skills only call `inference.generate()` — they never invoke tools.
/// If a FlowDef skill tries to execute a tool, it will get a "not found" error.
struct NoOpToolPort;

#[async_trait::async_trait]
impl hkask_capability::ToolPort for NoOpToolPort {
    fn invoke<'a>(
        &'a self,
        _server: &'a str,
        tool: &'a str,
        _args: Value,
        _token: &'a hkask_capability::DelegationToken,
    ) -> hkask_capability::ToolFuture<'a, Result<Value, hkask_capability::ToolPortError>> {
        Box::pin(async move {
            Err(hkask_capability::ToolPortError::NotFound(
                hkask_types::NotFound {
                    entity_type: "tool".to_string(),
                    id: format!(
                        "ToolPort not wired (D3 pending) — tool '{}' cannot be invoked",
                        tool
                    ),
                },
            ))
        })
    }

    fn discover_tools<'a>(&'a self) -> hkask_capability::ToolFuture<'a, Vec<String>> {
        Box::pin(async move { Vec::new() })
    }

    fn get_tool_info<'a>(
        &'a self,
        _tool_name: &'a str,
    ) -> hkask_capability::ToolFuture<'a, Option<hkask_capability::ToolInfo>> {
        Box::pin(async move { None })
    }
}
