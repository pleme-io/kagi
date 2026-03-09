//! MCP server for Kagi (Kagibako) 1Password client via kaname.
//!
//! Exposes vault and item lookup tools over the Model Context Protocol
//! (stdio transport), allowing AI assistants to search items and retrieve
//! non-secret metadata. Secrets (passwords, TOTP) are copied to clipboard
//! with auto-clear rather than returned in MCP responses.

use kaname::rmcp;
use kaname::ToolResponse;
use rmcp::{
    handler::server::router::tool::ToolRouter,
    handler::server::wrapper::Parameters,
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_handler, tool_router, ErrorData as McpError, ServerHandler,
};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::api::{self, VaultBackend};
use crate::clipboard::SecureClip;
use crate::config::KagiConfig;
use crate::vault::ItemSummary;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, JsonSchema)]
struct SearchItemsRequest {
    /// Search query (fuzzy matches against titles and URLs).
    query: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetPasswordRequest {
    /// Item name or ID to copy the password for.
    item: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GetTotpRequest {
    /// Item name or ID to copy the TOTP code for.
    item: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ListItemsRequest {
    /// Vault name or ID. Omit to list items from all vaults.
    vault: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ConfigGetRequest {
    /// Config key (dot-separated path).
    key: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct ConfigSetRequest {
    /// Config key.
    key: String,
    /// Value to set (as string).
    value: String,
}

// ---------------------------------------------------------------------------
// MCP Service
// ---------------------------------------------------------------------------

/// Kagi MCP server.
pub struct KagiMcpServer {
    tool_router: ToolRouter<Self>,
    config: KagiConfig,
}

impl std::fmt::Debug for KagiMcpServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KagiMcpServer").finish()
    }
}

#[tool_router]
impl KagiMcpServer {
    pub fn new(config: KagiConfig) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
        }
    }

    // -- Standard tools --

    #[tool(description = "Get Kagi server status.")]
    async fn status(&self) -> Result<CallToolResult, McpError> {
        let backend_type = if self.config.api.connect_url.is_some()
            && self.config.api.connect_token.is_some()
        {
            "connect"
        } else {
            "op-cli"
        };
        Ok(ToolResponse::success(&serde_json::json!({
            "status": "running",
            "backend": backend_type,
        })))
    }

    #[tool(description = "Get the Kagi version.")]
    async fn version(&self) -> Result<CallToolResult, McpError> {
        Ok(ToolResponse::success(&serde_json::json!({
            "name": "kagi",
            "crate": "kagibako",
            "version": env!("CARGO_PKG_VERSION"),
        })))
    }

    #[tool(description = "Get a configuration value by key.")]
    async fn config_get(
        &self,
        Parameters(req): Parameters<ConfigGetRequest>,
    ) -> Result<CallToolResult, McpError> {
        // Redact secret fields
        let mut json = serde_json::to_value(&self.config).unwrap_or_default();
        if let Some(api) = json.get_mut("api") {
            if let Some(obj) = api.as_object_mut() {
                if obj.contains_key("connect_token") {
                    obj.insert(
                        "connect_token".into(),
                        serde_json::Value::String("[REDACTED]".into()),
                    );
                }
                if obj.contains_key("service_account_token") {
                    obj.insert(
                        "service_account_token".into(),
                        serde_json::Value::String("[REDACTED]".into()),
                    );
                }
            }
        }
        let value = req
            .key
            .split('.')
            .fold(Some(&json as &serde_json::Value), |v, k| {
                v.and_then(|v| v.get(k))
            });
        match value {
            Some(v) => Ok(ToolResponse::success(v)),
            None => Ok(ToolResponse::error(&format!("Key '{}' not found", req.key))),
        }
    }

    #[tool(description = "Set a configuration value (runtime only, not persisted).")]
    async fn config_set(
        &self,
        Parameters(req): Parameters<ConfigSetRequest>,
    ) -> Result<CallToolResult, McpError> {
        Ok(ToolResponse::text(&format!(
            "Config key '{}' would be set to '{}'. Runtime config mutation not yet supported; \
             edit ~/.config/kagi/kagi.yaml instead.",
            req.key, req.value
        )))
    }

    // -- App-specific tools --

    #[tool(description = "List all 1Password vaults with item counts.")]
    async fn list_vaults(&self) -> Result<CallToolResult, McpError> {
        match api::create_backend(&self.config.api) {
            Ok(backend) => match backend.list_vaults().await {
                Ok(vaults) => {
                    let items: Vec<serde_json::Value> = vaults
                        .iter()
                        .map(|v| {
                            serde_json::json!({
                                "id": v.id,
                                "name": v.name,
                                "items": v.items,
                            })
                        })
                        .collect();
                    Ok(ToolResponse::success(&serde_json::json!({
                        "count": items.len(),
                        "vaults": items,
                    })))
                }
                Err(e) => Ok(ToolResponse::error(&format!("Failed to list vaults: {e}"))),
            },
            Err(e) => Ok(ToolResponse::error(&format!("Backend error: {e}"))),
        }
    }

    #[tool(description = "Search items across all vaults by name/URL.")]
    async fn search_items(
        &self,
        Parameters(req): Parameters<SearchItemsRequest>,
    ) -> Result<CallToolResult, McpError> {
        match api::create_backend(&self.config.api) {
            Ok(backend) => match backend.list_vaults().await {
                Ok(vaults) => {
                    let mut results = Vec::new();
                    for vault in &vaults {
                        if let Ok(items) = backend.list_items(&vault.id).await {
                            for item in &items {
                                let score = item.fuzzy_score(&req.query);
                                if score > 0 {
                                    let summary = ItemSummary::from(item);
                                    results.push(serde_json::json!({
                                        "vault": vault.name,
                                        "title": summary.title,
                                        "category": summary.category,
                                        "username": summary.username,
                                        "url": summary.url,
                                        "score": score,
                                    }));
                                }
                            }
                        }
                    }
                    results.sort_by(|a, b| {
                        b.get("score")
                            .and_then(|v| v.as_u64())
                            .unwrap_or(0)
                            .cmp(&a.get("score").and_then(|v| v.as_u64()).unwrap_or(0))
                    });
                    Ok(ToolResponse::success(&serde_json::json!({
                        "query": req.query,
                        "count": results.len(),
                        "items": results,
                    })))
                }
                Err(e) => Ok(ToolResponse::error(&format!("Failed to search: {e}"))),
            },
            Err(e) => Ok(ToolResponse::error(&format!("Backend error: {e}"))),
        }
    }

    #[tool(
        description = "Copy a password to the clipboard (auto-clears). Returns confirmation, never the password itself."
    )]
    async fn get_password(
        &self,
        Parameters(req): Parameters<GetPasswordRequest>,
    ) -> Result<CallToolResult, McpError> {
        match api::create_backend(&self.config.api) {
            Ok(backend) => {
                let vaults = backend.list_vaults().await.map_err(|e| {
                    McpError::internal_error(format!("Failed to list vaults: {e}"), None)
                })?;
                for vault in &vaults {
                    if let Ok(items) = backend.list_items(&vault.id).await {
                        if let Some(found) = items.iter().find(|i| {
                            i.title.eq_ignore_ascii_case(&req.item) || i.id == req.item
                        }) {
                            let full = backend
                                .get_item(&vault.id, &found.id)
                                .await
                                .map_err(|e| {
                                    McpError::internal_error(format!("Item fetch error: {e}"), None)
                                })?;
                            if let Some(pw) = full.password() {
                                match SecureClip::from_config(&self.config.clipboard) {
                                    Ok(clip) => {
                                        let _ = clip.copy_secret(pw);
                                        let timeout = self.config.clipboard.clear_timeout_secs;
                                        return Ok(ToolResponse::success(&serde_json::json!({
                                            "copied": true,
                                            "item": found.title,
                                            "field": "password",
                                            "auto_clear_secs": timeout,
                                        })));
                                    }
                                    Err(e) => {
                                        return Ok(ToolResponse::error(&format!(
                                            "Clipboard error: {e}"
                                        )))
                                    }
                                }
                            } else {
                                return Ok(ToolResponse::error(&format!(
                                    "No password field in '{}'",
                                    found.title
                                )));
                            }
                        }
                    }
                }
                Ok(ToolResponse::error(&format!(
                    "Item '{}' not found in any vault",
                    req.item
                )))
            }
            Err(e) => Ok(ToolResponse::error(&format!("Backend error: {e}"))),
        }
    }

    #[tool(
        description = "Copy the current TOTP code to the clipboard (auto-clears). Returns confirmation, never the code itself."
    )]
    async fn get_totp(
        &self,
        Parameters(req): Parameters<GetTotpRequest>,
    ) -> Result<CallToolResult, McpError> {
        match api::create_backend(&self.config.api) {
            Ok(backend) => {
                let vaults = backend.list_vaults().await.map_err(|e| {
                    McpError::internal_error(format!("Failed to list vaults: {e}"), None)
                })?;
                for vault in &vaults {
                    if let Ok(items) = backend.list_items(&vault.id).await {
                        if let Some(found) = items.iter().find(|i| {
                            i.title.eq_ignore_ascii_case(&req.item) || i.id == req.item
                        }) {
                            match backend.get_totp(&vault.id, &found.id).await {
                                Ok(code) => {
                                    match SecureClip::from_config(&self.config.clipboard) {
                                        Ok(clip) => {
                                            let _ = clip.copy_secret(&code);
                                            let timeout = self.config.clipboard.clear_timeout_secs;
                                            return Ok(ToolResponse::success(&serde_json::json!({
                                                "copied": true,
                                                "item": found.title,
                                                "field": "totp",
                                                "auto_clear_secs": timeout,
                                            })));
                                        }
                                        Err(e) => {
                                            return Ok(ToolResponse::error(&format!(
                                                "Clipboard error: {e}"
                                            )))
                                        }
                                    }
                                }
                                Err(e) => {
                                    return Ok(ToolResponse::error(&format!(
                                        "TOTP error for '{}': {e}",
                                        found.title
                                    )))
                                }
                            }
                        }
                    }
                }
                Ok(ToolResponse::error(&format!(
                    "Item '{}' not found in any vault",
                    req.item
                )))
            }
            Err(e) => Ok(ToolResponse::error(&format!("Backend error: {e}"))),
        }
    }

    #[tool(description = "List items in a vault (or all vaults). Returns summaries without secrets.")]
    async fn list_items(
        &self,
        Parameters(req): Parameters<ListItemsRequest>,
    ) -> Result<CallToolResult, McpError> {
        match api::create_backend(&self.config.api) {
            Ok(backend) => match backend.list_vaults().await {
                Ok(vaults) => {
                    let target_vaults: Vec<_> = if let Some(ref name) = req.vault {
                        vaults
                            .iter()
                            .filter(|v| v.name.eq_ignore_ascii_case(name) || v.id == *name)
                            .collect()
                    } else {
                        vaults.iter().collect()
                    };

                    if target_vaults.is_empty() && req.vault.is_some() {
                        return Ok(ToolResponse::error(&format!(
                            "Vault '{}' not found",
                            req.vault.as_deref().unwrap_or("")
                        )));
                    }

                    let mut all_items = Vec::new();
                    for vault in &target_vaults {
                        if let Ok(items) = backend.list_items(&vault.id).await {
                            for item in &items {
                                let summary = ItemSummary::from(item);
                                all_items.push(serde_json::json!({
                                    "vault": vault.name,
                                    "title": summary.title,
                                    "category": summary.category,
                                    "username": summary.username,
                                    "url": summary.url,
                                    "favorite": item.favorite,
                                }));
                            }
                        }
                    }
                    Ok(ToolResponse::success(&serde_json::json!({
                        "count": all_items.len(),
                        "items": all_items,
                    })))
                }
                Err(e) => Ok(ToolResponse::error(&format!("Failed to list: {e}"))),
            },
            Err(e) => Ok(ToolResponse::error(&format!("Backend error: {e}"))),
        }
    }
}

// ---------------------------------------------------------------------------
// ServerHandler
// ---------------------------------------------------------------------------

#[tool_handler]
impl ServerHandler for KagiMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: rmcp::model::Implementation {
                name: "kagi".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: None,
                description: None,
                icons: None,
                website_url: None,
            },
            instructions: Some(
                "Kagi 1Password client MCP server. Search vaults, list items, \
                 and copy passwords/TOTP to clipboard securely. Secrets are never \
                 returned in responses -- they are copied to the clipboard with auto-clear."
                    .to_string(),
            ),
            ..Default::default()
        }
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run the MCP server on stdio.
pub async fn run(config: KagiConfig) -> Result<(), Box<dyn std::error::Error>> {
    use rmcp::{transport::stdio, ServiceExt};

    let service = KagiMcpServer::new(config);
    let server = service.serve(stdio()).await?;
    server.waiting().await?;
    Ok(())
}
