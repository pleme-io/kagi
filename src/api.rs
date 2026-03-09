//! 1Password API integration.
//!
//! Two backends:
//! - 1Password Connect API (REST, for server deployments)
//! - `op` CLI wrapper (for local use, biometric unlock)
//!
//! Both implement `VaultBackend` for vault/item operations.

use crate::config::ApiConfig;
use crate::vault::{Field, FieldPurpose, FieldType, Item, ItemCategory, ItemUrl, SecretValue, Vault};
use serde::Deserialize;
use std::process::Command;

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] reqwest::Error),

    #[error("1Password API error ({status}): {body}")]
    Api { status: u16, body: String },

    #[error("`op` CLI error: {0}")]
    Cli(String),

    #[error("not configured: {0}")]
    NotConfigured(String),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, ApiError>;

/// Common interface for 1Password backends.
pub trait VaultBackend: Send + Sync {
    /// List all vaults.
    fn list_vaults(&self) -> impl std::future::Future<Output = Result<Vec<Vault>>> + Send;

    /// List items in a vault (summaries only, no secret fields).
    fn list_items(&self, vault_id: &str) -> impl std::future::Future<Output = Result<Vec<Item>>> + Send;

    /// Get a full item with all fields (including secrets).
    fn get_item(&self, vault_id: &str, item_id: &str) -> impl std::future::Future<Output = Result<Item>> + Send;
}

/// Backend enum that dispatches to either Connect API or `op` CLI.
pub enum Backend {
    Connect(ConnectBackend),
    Cli(OpCliBackend),
}

impl VaultBackend for Backend {
    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        match self {
            Self::Connect(b) => b.list_vaults().await,
            Self::Cli(b) => b.list_vaults().await,
        }
    }

    async fn list_items(&self, vault_id: &str) -> Result<Vec<Item>> {
        match self {
            Self::Connect(b) => b.list_items(vault_id).await,
            Self::Cli(b) => b.list_items(vault_id).await,
        }
    }

    async fn get_item(&self, vault_id: &str, item_id: &str) -> Result<Item> {
        match self {
            Self::Connect(b) => b.get_item(vault_id, item_id).await,
            Self::Cli(b) => b.get_item(vault_id, item_id).await,
        }
    }
}

/// Create the appropriate backend from config.
pub fn create_backend(config: &ApiConfig) -> Result<Backend> {
    if let (Some(url), Some(token)) = (&config.connect_url, &config.connect_token) {
        tracing::info!("using 1Password Connect API at {url}");
        Ok(Backend::Connect(ConnectBackend::new(url, token)?))
    } else {
        tracing::info!("using `op` CLI at {}", config.op_path);
        Ok(Backend::Cli(OpCliBackend::new(&config.op_path, config.service_account_token.as_deref())))
    }
}

// ---------------------------------------------------------------------------
// 1Password Connect API backend
// ---------------------------------------------------------------------------

pub struct ConnectBackend {
    client: reqwest::Client,
    base_url: String,
}

impl ConnectBackend {
    pub fn new(base_url: &str, token: &str) -> Result<Self> {
        let mut headers = reqwest::header::HeaderMap::new();
        let auth_value = reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|e| ApiError::NotConfigured(format!("invalid token: {e}")))?;
        headers.insert(reqwest::header::AUTHORIZATION, auth_value);
        headers.insert(
            reqwest::header::CONTENT_TYPE,
            reqwest::header::HeaderValue::from_static("application/json"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(std::time::Duration::from_secs(30))
            .build()?;

        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
        })
    }

    async fn get_json<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{path}", self.base_url);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            return Err(ApiError::Api { status, body });
        }

        Ok(resp.json().await?)
    }
}

/// Connect API vault response.
#[derive(Deserialize)]
struct ConnectVault {
    id: String,
    name: String,
    description: Option<String>,
    items: Option<u32>,
}

impl From<ConnectVault> for Vault {
    fn from(v: ConnectVault) -> Self {
        Self {
            id: v.id,
            name: v.name,
            description: v.description,
            items: v.items.unwrap_or(0),
        }
    }
}

impl VaultBackend for ConnectBackend {
    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        let vaults: Vec<ConnectVault> = self.get_json("/v1/vaults").await?;
        Ok(vaults.into_iter().map(Vault::from).collect())
    }

    async fn list_items(&self, vault_id: &str) -> Result<Vec<Item>> {
        self.get_json(&format!("/v1/vaults/{vault_id}/items"))
            .await
    }

    async fn get_item(&self, vault_id: &str, item_id: &str) -> Result<Item> {
        self.get_json(&format!("/v1/vaults/{vault_id}/items/{item_id}"))
            .await
    }
}

// ---------------------------------------------------------------------------
// `op` CLI backend
// ---------------------------------------------------------------------------

pub struct OpCliBackend {
    op_path: String,
    service_account_token: Option<String>,
}

impl OpCliBackend {
    #[must_use]
    pub fn new(op_path: &str, service_account_token: Option<&str>) -> Self {
        Self {
            op_path: op_path.to_string(),
            service_account_token: service_account_token.map(String::from),
        }
    }

    fn run(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new(&self.op_path);
        cmd.args(args).arg("--format=json");

        if let Some(token) = &self.service_account_token {
            cmd.env("OP_SERVICE_ACCOUNT_TOKEN", token);
        }

        let output = cmd
            .output()
            .map_err(|e| ApiError::Cli(format!("failed to run `op`: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ApiError::Cli(stderr.into_owned()));
        }

        String::from_utf8(output.stdout)
            .map_err(|e| ApiError::Cli(format!("invalid UTF-8 output: {e}")))
    }
}

/// `op` CLI vault JSON.
#[derive(Deserialize)]
struct OpVault {
    id: String,
    name: String,
    description: Option<String>,
    items: Option<u32>,
}

/// `op` CLI item JSON.
#[derive(Deserialize)]
struct OpItem {
    id: String,
    title: String,
    #[serde(default)]
    vault: OpItemVault,
    #[serde(default)]
    category: String,
    #[serde(default)]
    urls: Vec<OpUrl>,
    #[serde(default)]
    fields: Vec<OpField>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    favorite: bool,
}

#[derive(Deserialize, Default)]
struct OpItemVault {
    #[serde(default)]
    id: String,
}

#[derive(Deserialize)]
struct OpUrl {
    href: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct OpField {
    id: String,
    #[serde(default)]
    label: String,
    #[serde(default)]
    value: Option<String>,
    #[serde(default)]
    purpose: Option<String>,
    #[serde(rename = "type", default)]
    field_type: Option<String>,
}

impl From<OpItem> for Item {
    fn from(op: OpItem) -> Self {
        let category = match op.category.to_uppercase().as_str() {
            "LOGIN" => ItemCategory::Login,
            "PASSWORD" => ItemCategory::Password,
            "SECURE_NOTE" => ItemCategory::SecureNote,
            "CREDIT_CARD" => ItemCategory::CreditCard,
            "IDENTITY" => ItemCategory::Identity,
            "DOCUMENT" => ItemCategory::Document,
            "SSH_KEY" => ItemCategory::SshKey,
            "API_CREDENTIAL" => ItemCategory::ApiCredential,
            "DATABASE" => ItemCategory::Database,
            _ => ItemCategory::Unknown,
        };

        let urls = op
            .urls
            .into_iter()
            .map(|u| ItemUrl {
                href: u.href,
                primary: u.primary,
            })
            .collect();

        let fields = op
            .fields
            .into_iter()
            .map(|f| {
                let purpose = f.purpose.as_deref().map(|p| match p.to_uppercase().as_str() {
                    "USERNAME" => FieldPurpose::Username,
                    "PASSWORD" => FieldPurpose::Password,
                    "NOTES" => FieldPurpose::Notes,
                    _ => FieldPurpose::Other,
                });
                let field_type = match f.field_type.as_deref() {
                    Some("CONCEALED") => FieldType::Concealed,
                    Some("EMAIL") => FieldType::Email,
                    Some("URL") => FieldType::Url,
                    Some("OTP") => FieldType::Otp,
                    _ => FieldType::String,
                };
                Field {
                    id: f.id,
                    label: f.label,
                    value: SecretValue::new(f.value.unwrap_or_default()),
                    purpose,
                    field_type,
                }
            })
            .collect();

        Self {
            id: op.id,
            title: op.title,
            vault_id: op.vault.id,
            category,
            urls,
            fields,
            tags: op.tags,
            favorite: op.favorite,
            last_edited_by: None,
            created_at: None,
            updated_at: None,
        }
    }
}

impl VaultBackend for OpCliBackend {
    async fn list_vaults(&self) -> Result<Vec<Vault>> {
        let output = self.run(&["vault", "list"])?;
        let vaults: Vec<OpVault> = serde_json::from_str(&output)?;
        Ok(vaults
            .into_iter()
            .map(|v| Vault {
                id: v.id,
                name: v.name,
                description: v.description,
                items: v.items.unwrap_or(0),
            })
            .collect())
    }

    async fn list_items(&self, vault_id: &str) -> Result<Vec<Item>> {
        let output = self.run(&["item", "list", "--vault", vault_id])?;
        let items: Vec<OpItem> = serde_json::from_str(&output)?;
        Ok(items.into_iter().map(Item::from).collect())
    }

    async fn get_item(&self, vault_id: &str, item_id: &str) -> Result<Item> {
        let output = self.run(&["item", "get", item_id, "--vault", vault_id])?;
        let item: OpItem = serde_json::from_str(&output)?;
        Ok(Item::from(item))
    }
}
