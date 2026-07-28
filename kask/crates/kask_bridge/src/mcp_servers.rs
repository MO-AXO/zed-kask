//! Canonical registry of built-in kask MCP servers.
//!
//! Single source of truth for the server ID → binary name → description mapping.
//! Previously duplicated in three places (`zed/src/main.rs`, `settings_ui/src/pages/kask_page.rs`,
//! `kask_panel/src/kask_panel.rs`) with drift between them. This module consolidates
//! the list so all consumers reference the same data.
//!
//! The server IDs here match the keys used in `KaskMcpSettingsContent::overrides`
//! and the `context_servers` entries registered with zed's `ContextServerStore`.

/// A built-in kask MCP server descriptor.
#[derive(Debug, Clone, Copy)]
pub struct BuiltinMcpServer {
    /// The server ID used in settings (`kask.mcp.overrides`) and as the
    /// `ContextServerId` when registering with zed's `ContextServerStore`.
    pub id: &'static str,
    /// The binary name (without path) of the MCP server executable.
    /// Resolved via `HKASK_MCP_{ID}_BIN` env var or PATH lookup at launch time.
    pub binary: &'static str,
    /// Human-readable description shown in the settings UI and kask panel.
    pub description: &'static str,
    /// Credential env vars this server is allowed to receive (allowlist).
    /// Only credentials in this list are injected into the server's child
    /// process env. This limits the blast radius of a compromised MCP server —
    /// a server that only needs `DEEPINFRA_API_KEY` won't receive
    /// `HKASK_SMTP_PASSWORD` or `HKASK_EODHD_API_KEY`.
    ///
    /// `None` means "no credential filtering" (receives all credentials).
    /// Used for servers that haven't been audited yet — prefer `Some(&[])`
    /// (receives no credentials) for new servers.
    pub credentials: Option<&'static [&'static str]>,
}

/// The canonical list of built-in kask MCP servers.
///
/// Order is stable and meaningful — the kask panel uses index-based selection.
pub const BUILT_IN_MCP_SERVERS: &[BuiltinMcpServer] = &[
    BuiltinMcpServer {
        id: "codegraph",
        binary: "hkask-mcp-codegraph",
        description: "Codegraph — code structure query and traversal",
        credentials: Some(&["DEEPINFRA_API_KEY", "OPENROUTER_API_KEY"]),
    },
    BuiltinMcpServer {
        id: "companies",
        binary: "hkask-mcp-companies",
        description: "Companies — company research and filings",
        credentials: Some(&[
            "HKASK_EODHD_API_KEY",
            "HKASK_FMP_API_KEY",
            "HKASK_EXA_API_KEY",
            "HKASK_TAVILY_API_KEY",
            "HKASK_BRAVE_API_KEY",
            "HKASK_SERPAPI_API_KEY",
        ]),
    },
    BuiltinMcpServer {
        id: "condenser",
        binary: "hkask-mcp-condenser",
        description: "Condenser — context condensation and summarization",
        credentials: Some(&[]),
    },
    BuiltinMcpServer {
        id: "corpus",
        binary: "hkask-mcp-corpus",
        description: "Corpus — document corpus and QA generation",
        credentials: Some(&["FALAI_API_KEY", "RUNPOD_API_KEY"]),
    },
    BuiltinMcpServer {
        id: "curator",
        binary: "hkask-mcp-curator",
        description: "Curator — regulation cascade and algedonic signals",
        credentials: Some(&["HKASK_SMTP_PASSWORD"]),
    },
    BuiltinMcpServer {
        id: "kata-kanban",
        binary: "hkask-mcp-kata-kanban",
        description: "Kata Kanban — improvement kata board",
        credentials: Some(&[]),
    },
    BuiltinMcpServer {
        id: "media",
        binary: "hkask-mcp-media",
        description: "Media — image generation and media workflows",
        credentials: Some(&["FALAI_API_KEY"]),
    },
    BuiltinMcpServer {
        id: "research",
        binary: "hkask-mcp-research",
        description: "Research — web research and paper search",
        credentials: Some(&[
            "HKASK_EXA_API_KEY",
            "HKASK_TAVILY_API_KEY",
            "HKASK_BRAVE_API_KEY",
            "HKASK_SERPAPI_API_KEY",
            "HKASK_FIRECRAWL_API_KEY",
            "HKASK_BROWSERBASE_API_KEY",
        ]),
    },
    BuiltinMcpServer {
        id: "scenarios",
        binary: "hkask-mcp-scenarios",
        description: "Scenarios — scenario planning and forecasting",
        credentials: Some(&[]),
    },
    BuiltinMcpServer {
        id: "training",
        binary: "hkask-mcp-training",
        description: "Training — LoRA training configuration and audit",
        credentials: Some(&[
            "DEEPINFRA_API_KEY",
            "RUNPOD_API_KEY",
            "RUNPOD_TEMPLATE_ID",
            "NEBIUS_PROJECT_ID",
            "NEBIUS_SUBNET_ID",
            "HF_TOKEN",
        ]),
    },
];

/// Just the server IDs, as a static slice of `&str`.
/// Convenience for consumers that only need the ID list (e.g. `kask_panel`).
pub const BUILT_IN_MCP_SERVERS_IDS: &[&str] = &[
    "codegraph",
    "companies",
    "condenser",
    "corpus",
    "curator",
    "kata-kanban",
    "media",
    "research",
    "scenarios",
    "training",
];

/// The server list as `(id, description)` pairs.
/// Convenience for the settings UI which renders `(id, description)` rows.
pub const BUILT_IN_MCP_SERVERS_PAIRS: &[(&str, &str)] = &[
    (
        "codegraph",
        "Codegraph — code structure query and traversal",
    ),
    ("companies", "Companies — company research and filings"),
    (
        "condenser",
        "Condenser — context condensation and summarization",
    ),
    ("corpus", "Corpus — document corpus and QA generation"),
    (
        "curator",
        "Curator — regulation cascade and algedonic signals",
    ),
    ("kata-kanban", "Kata Kanban — improvement kata board"),
    ("media", "Media — image generation and media workflows"),
    ("research", "Research — web research and paper search"),
    ("scenarios", "Scenarios — scenario planning and forecasting"),
    (
        "training",
        "Training — LoRA training configuration and audit",
    ),
];

/// Look up a server by ID.
#[must_use]
pub fn find_server(id: &str) -> Option<&'static BuiltinMcpServer> {
    BUILT_IN_MCP_SERVERS.iter().find(|s| s.id == id)
}

/// Filter a list of `(env_var, credential_url)` pairs to only those the
/// specified server is allowed to receive.
///
/// When the server's `credentials` field is `Some(allowlist)`, only env vars
/// in the allowlist are kept. When it's `None`, all credentials are kept
/// (backward-compatible behavior for unaudited servers).
///
/// This limits the blast radius of a compromised MCP server — a server that
/// only needs `DEEPINFRA_API_KEY` won't receive `HKASK_SMTP_PASSWORD`.
#[must_use]
pub fn filter_credentials_for_server(
    server_id: &str,
    credentials: &[(String, String)],
) -> Vec<(String, String)> {
    let Some(server) = find_server(server_id) else {
        return credentials.to_vec();
    };
    match server.credentials {
        Some(allowlist) => credentials
            .iter()
            .filter(|(env_var, _)| allowlist.contains(&env_var.as_str()))
            .cloned()
            .collect(),
        None => credentials.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_servers_have_unique_ids() {
        let mut ids: Vec<&str> = BUILT_IN_MCP_SERVERS.iter().map(|s| s.id).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "duplicate server IDs found");
    }

    #[test]
    fn all_binaries_follow_naming_convention() {
        for s in BUILT_IN_MCP_SERVERS {
            assert!(
                s.binary.starts_with("hkask-mcp-"),
                "binary '{}' does not follow 'hkask-mcp-*' convention",
                s.binary
            );
        }
    }

    #[test]
    fn find_server_returns_known_ids() {
        assert!(find_server("codegraph").is_some());
        assert!(find_server("kata-kanban").is_some());
        assert!(find_server("nonexistent").is_none());
    }

    // The derived arrays below are hand-maintained convenience views over
    // BUILT_IN_MCP_SERVERS. Without these tests they can silently drift the
    // moment a server is added to BUILT_IN_MCP_SERVERS without updating the
    // derived slices (the settings UI / kask panel would then drop the new
    // server while the runtime registry served it).
    #[test]
    fn ids_slice_matches_main_registry() {
        let expected: Vec<&str> = BUILT_IN_MCP_SERVERS.iter().map(|s| s.id).collect();
        let actual: Vec<&str> = BUILT_IN_MCP_SERVERS_IDS.to_vec();
        assert_eq!(
            actual, expected,
            "BUILT_IN_MCP_SERVERS_IDS is out of sync with BUILT_IN_MCP_SERVERS"
        );
    }

    #[test]
    fn pairs_slice_matches_main_registry() {
        let expected: Vec<(&str, &str)> = BUILT_IN_MCP_SERVERS
            .iter()
            .map(|s| (s.id, s.description))
            .collect();
        let actual: Vec<(&str, &str)> = BUILT_IN_MCP_SERVERS_PAIRS.to_vec();
        assert_eq!(
            actual, expected,
            "BUILT_IN_MCP_SERVERS_PAIRS is out of sync with BUILT_IN_MCP_SERVERS"
        );
    }

    // Every server must have a credential allowlist (not `None`).
    // `None` means "receive all credentials" — the unsafe default we're
    // moving away from. New servers should use `Some(&[])` (no credentials)
    // and add specific env vars as needed.
    #[test]
    fn all_servers_have_credential_allowlist() {
        for s in BUILT_IN_MCP_SERVERS {
            assert!(
                s.credentials.is_some(),
                "server '{}' has no credential allowlist (credentials is None) — \
                 use Some(&[]) for servers that need no credentials",
                s.id
            );
        }
    }

    // The curator server should only receive the SMTP password, not data
    // service API keys. This pins the blast-radius reduction.
    #[test]
    fn curator_credentials_do_not_include_data_service_keys() {
        let all_credentials: Vec<(String, String)> = [
            "HKASK_EODHD_API_KEY",
            "HKASK_FMP_API_KEY",
            "HKASK_SMTP_PASSWORD",
            "DEEPINFRA_API_KEY",
        ]
        .iter()
        .map(|env| (env.to_string(), "url".to_string()))
        .collect();
        let filtered = filter_credentials_for_server("curator", &all_credentials);
        let env_vars: Vec<&str> = filtered.iter().map(|(k, _)| k.as_str()).collect();
        assert!(
            env_vars.contains(&"HKASK_SMTP_PASSWORD"),
            "curator should receive HKASK_SMTP_PASSWORD"
        );
        assert!(
            !env_vars.contains(&"HKASK_EODHD_API_KEY"),
            "curator should NOT receive HKASK_EODHD_API_KEY"
        );
        assert!(
            !env_vars.contains(&"DEEPINFRA_API_KEY"),
            "curator should NOT receive DEEPINFRA_API_KEY"
        );
    }

    // The codegraph server should only receive inference keys, not SMTP.
    #[test]
    fn codegraph_credentials_do_not_include_smtp_password() {
        let all_credentials: Vec<(String, String)> = [
            "DEEPINFRA_API_KEY",
            "OPENROUTER_API_KEY",
            "HKASK_SMTP_PASSWORD",
            "HKASK_EODHD_API_KEY",
        ]
        .iter()
        .map(|env| (env.to_string(), "url".to_string()))
        .collect();
        let filtered = filter_credentials_for_server("codegraph", &all_credentials);
        let env_vars: Vec<&str> = filtered.iter().map(|(k, _)| k.as_str()).collect();
        assert!(env_vars.contains(&"DEEPINFRA_API_KEY"));
        assert!(env_vars.contains(&"OPENROUTER_API_KEY"));
        assert!(
            !env_vars.contains(&"HKASK_SMTP_PASSWORD"),
            "codegraph should NOT receive HKASK_SMTP_PASSWORD"
        );
    }

    // Unknown server IDs get all credentials (backward-compatible).
    #[test]
    fn unknown_server_gets_all_credentials() {
        let credentials = vec![
            ("KEY_A".to_string(), "url_a".to_string()),
            ("KEY_B".to_string(), "url_b".to_string()),
        ];
        let filtered = filter_credentials_for_server("nonexistent", &credentials);
        assert_eq!(filtered.len(), 2);
    }
}
