//! Vault and item models -- serde types matching 1Password's data model.
//!
//! Vaults contain items. Items have fields (username, password, notes, etc.)
//! All secret fields use zeroize for secure memory clearing.

use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

/// A 1Password vault.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vault {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub items: u32,
}

/// A 1Password item (login, note, identity, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub title: String,
    pub vault_id: String,
    pub category: ItemCategory,
    #[serde(default)]
    pub urls: Vec<ItemUrl>,
    #[serde(default)]
    pub fields: Vec<Field>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub favorite: bool,
    #[serde(default)]
    pub last_edited_by: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// Item category (1Password item types).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ItemCategory {
    Login,
    Password,
    SecureNote,
    CreditCard,
    Identity,
    Document,
    SshKey,
    ApiCredential,
    Database,
    #[serde(other)]
    Unknown,
}

impl std::fmt::Display for ItemCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Login => write!(f, "Login"),
            Self::Password => write!(f, "Password"),
            Self::SecureNote => write!(f, "Secure Note"),
            Self::CreditCard => write!(f, "Credit Card"),
            Self::Identity => write!(f, "Identity"),
            Self::Document => write!(f, "Document"),
            Self::SshKey => write!(f, "SSH Key"),
            Self::ApiCredential => write!(f, "API Credential"),
            Self::Database => write!(f, "Database"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

/// A URL associated with an item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemUrl {
    pub href: String,
    #[serde(default)]
    pub primary: bool,
}

/// An item field (username, password, TOTP, etc).
#[derive(Clone, Serialize, Deserialize)]
pub struct Field {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub value: SecretValue,
    pub purpose: Option<FieldPurpose>,
    #[serde(rename = "type", default)]
    pub field_type: FieldType,
}

impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("value", &"[REDACTED]")
            .field("purpose", &self.purpose)
            .field("field_type", &self.field_type)
            .finish()
    }
}

/// Field purpose (1Password standard fields).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FieldPurpose {
    Username,
    Password,
    Notes,
    #[serde(other)]
    Other,
}

/// Field type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FieldType {
    #[default]
    String,
    Concealed,
    Email,
    Url,
    Otp,
    Date,
    MonthYear,
    #[serde(other)]
    Unknown,
}

/// A secret value that is zeroized on drop.
#[derive(Clone, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
#[serde(transparent)]
pub struct SecretValue(std::string::String);

impl SecretValue {
    #[must_use]
    pub fn new(value: impl Into<std::string::String>) -> Self {
        Self(value.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Default for SecretValue {
    fn default() -> Self {
        Self(std::string::String::new())
    }
}

impl std::fmt::Debug for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl std::fmt::Display for SecretValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("[REDACTED]")
    }
}

impl Item {
    /// Find a field by purpose.
    #[must_use]
    pub fn field_by_purpose(&self, purpose: FieldPurpose) -> Option<&Field> {
        self.fields.iter().find(|f| f.purpose == Some(purpose))
    }

    /// Find a field by label (case-insensitive).
    #[must_use]
    pub fn field_by_label(&self, label: &str) -> Option<&Field> {
        let lower = label.to_lowercase();
        self.fields.iter().find(|f| f.label.to_lowercase() == lower)
    }

    /// Get the password field value.
    #[must_use]
    pub fn password(&self) -> Option<&str> {
        self.field_by_purpose(FieldPurpose::Password)
            .map(|f| f.value.as_str())
    }

    /// Get the username field value.
    #[must_use]
    pub fn username(&self) -> Option<&str> {
        self.field_by_purpose(FieldPurpose::Username)
            .map(|f| f.value.as_str())
    }

    /// Check if this item has an OTP/TOTP field.
    #[must_use]
    pub fn has_totp(&self) -> bool {
        self.fields.iter().any(|f| f.field_type == FieldType::Otp)
    }

    /// Get the primary URL.
    #[must_use]
    pub fn primary_url(&self) -> Option<&str> {
        self.urls
            .iter()
            .find(|u| u.primary)
            .or(self.urls.first())
            .map(|u| u.href.as_str())
    }

    /// Simple substring match against title, URLs, username, and tags.
    #[must_use]
    #[allow(dead_code)]
    pub fn matches(&self, query: &str) -> bool {
        let q = query.to_lowercase();
        self.title.to_lowercase().contains(&q)
            || self.urls.iter().any(|u| u.href.to_lowercase().contains(&q))
            || self.username().is_some_and(|u| u.to_lowercase().contains(&q))
            || self.tags.iter().any(|t| t.to_lowercase().contains(&q))
    }

    /// Compute a fuzzy match score (0 = no match, higher = better match).
    /// Scores title matches highest, then username, then URL, then tags.
    #[must_use]
    pub fn fuzzy_score(&self, query: &str) -> u32 {
        if query.is_empty() {
            return 1; // everything matches empty query
        }
        let q = query.to_lowercase();
        let mut score = 0u32;

        // Title scoring: exact > starts-with > contains > subsequence
        let title_lower = self.title.to_lowercase();
        if title_lower == q {
            score += 1000;
        } else if title_lower.starts_with(&q) {
            score += 500;
        } else if title_lower.contains(&q) {
            score += 200;
        } else if is_subsequence(&q, &title_lower) {
            score += 100;
        }

        // Username scoring
        if let Some(user) = self.username() {
            let user_lower = user.to_lowercase();
            if user_lower.contains(&q) {
                score += 150;
            } else if is_subsequence(&q, &user_lower) {
                score += 50;
            }
        }

        // URL scoring
        for url in &self.urls {
            let url_lower = url.href.to_lowercase();
            if url_lower.contains(&q) {
                score += 100;
            }
        }

        // Tag scoring
        for tag in &self.tags {
            if tag.to_lowercase().contains(&q) {
                score += 80;
            }
        }

        // Favorites get a small boost
        if self.favorite && score > 0 {
            score += 10;
        }

        score
    }
}

/// Check if `needle` is a subsequence of `haystack`.
fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut haystack_chars = haystack.chars();
    for nc in needle.chars() {
        loop {
            match haystack_chars.next() {
                Some(hc) if hc == nc => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// Summary view of an item (no secret fields).
#[derive(Debug, Clone, Serialize)]
pub struct ItemSummary {
    pub id: String,
    pub title: String,
    pub category: ItemCategory,
    pub vault_id: String,
    pub vault_name: String,
    pub url: Option<String>,
    pub username: Option<String>,
    pub favorite: bool,
    pub has_totp: bool,
}

impl ItemSummary {
    /// Create from an item with a vault name for display.
    #[must_use]
    pub fn from_item_with_vault(item: &Item, vault_name: &str) -> Self {
        Self {
            id: item.id.clone(),
            title: item.title.clone(),
            category: item.category,
            vault_id: item.vault_id.clone(),
            vault_name: vault_name.to_string(),
            url: item.primary_url().map(std::string::String::from),
            username: item.username().map(std::string::String::from),
            favorite: item.favorite,
            has_totp: item.has_totp(),
        }
    }
}

impl From<&Item> for ItemSummary {
    fn from(item: &Item) -> Self {
        Self {
            id: item.id.clone(),
            title: item.title.clone(),
            category: item.category,
            vault_id: item.vault_id.clone(),
            vault_name: String::new(),
            url: item.primary_url().map(std::string::String::from),
            username: item.username().map(std::string::String::from),
            favorite: item.favorite,
            has_totp: item.has_totp(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_item() -> Item {
        Item {
            id: "abc123".into(),
            title: "GitHub".into(),
            vault_id: "vault1".into(),
            category: ItemCategory::Login,
            urls: vec![ItemUrl {
                href: "https://github.com".into(),
                primary: true,
            }],
            fields: vec![
                Field {
                    id: "f1".into(),
                    label: "username".into(),
                    value: SecretValue::new("user@example.com"),
                    purpose: Some(FieldPurpose::Username),
                    field_type: FieldType::String,
                },
                Field {
                    id: "f2".into(),
                    label: "password".into(),
                    value: SecretValue::new("super-secret-pw"),
                    purpose: Some(FieldPurpose::Password),
                    field_type: FieldType::Concealed,
                },
                Field {
                    id: "f3".into(),
                    label: "one-time password".into(),
                    value: SecretValue::new("otpauth://totp/..."),
                    purpose: None,
                    field_type: FieldType::Otp,
                },
            ],
            tags: vec!["dev".into()],
            favorite: true,
            last_edited_by: None,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn field_by_purpose() {
        let item = test_item();
        assert_eq!(item.username(), Some("user@example.com"));
        assert_eq!(item.password(), Some("super-secret-pw"));
    }

    #[test]
    fn field_by_label() {
        let item = test_item();
        let f = item.field_by_label("Password").unwrap();
        assert_eq!(f.value.as_str(), "super-secret-pw");
    }

    #[test]
    fn primary_url() {
        let item = test_item();
        assert_eq!(item.primary_url(), Some("https://github.com"));
    }

    #[test]
    fn has_totp_field() {
        let item = test_item();
        assert!(item.has_totp());
    }

    #[test]
    fn matches_title() {
        let item = test_item();
        assert!(item.matches("github"));
        assert!(item.matches("Git"));
        assert!(!item.matches("gitlab"));
    }

    #[test]
    fn matches_url() {
        let item = test_item();
        assert!(item.matches("github.com"));
    }

    #[test]
    fn matches_username() {
        let item = test_item();
        assert!(item.matches("user@example"));
    }

    #[test]
    fn matches_tag() {
        let item = test_item();
        assert!(item.matches("dev"));
    }

    #[test]
    fn fuzzy_score_exact_title() {
        let item = test_item();
        let score = item.fuzzy_score("GitHub");
        assert!(score >= 1000, "exact title match should score >= 1000, got {score}");
    }

    #[test]
    fn fuzzy_score_starts_with() {
        let item = test_item();
        let score = item.fuzzy_score("Git");
        assert!(score >= 500, "starts-with match should score >= 500, got {score}");
    }

    #[test]
    fn fuzzy_score_contains() {
        let item = test_item();
        let score = item.fuzzy_score("itHu");
        assert!(score >= 200, "contains match should score >= 200, got {score}");
    }

    #[test]
    fn fuzzy_score_subsequence() {
        let item = test_item();
        let score = item.fuzzy_score("ghb");
        assert!(score >= 100, "subsequence match should score >= 100, got {score}");
    }

    #[test]
    fn fuzzy_score_no_match() {
        let item = test_item();
        let score = item.fuzzy_score("zzzzz");
        assert_eq!(score, 0, "no match should score 0");
    }

    #[test]
    fn fuzzy_score_empty_query() {
        let item = test_item();
        let score = item.fuzzy_score("");
        assert_eq!(score, 1, "empty query matches everything with score 1");
    }

    #[test]
    fn is_subsequence_basic() {
        assert!(is_subsequence("ghb", "github"));
        assert!(is_subsequence("abc", "aXbXc"));
        assert!(!is_subsequence("abc", "acb"));
        assert!(is_subsequence("", "anything"));
    }

    #[test]
    fn secret_value_debug_redacted() {
        let sv = SecretValue::new("secret");
        assert_eq!(format!("{sv:?}"), "[REDACTED]");
        assert_eq!(format!("{sv}"), "[REDACTED]");
        assert_eq!(sv.as_str(), "secret");
    }

    #[test]
    fn item_summary_from_item() {
        let item = test_item();
        let summary = ItemSummary::from(&item);
        assert_eq!(summary.title, "GitHub");
        assert_eq!(summary.username.as_deref(), Some("user@example.com"));
        assert!(summary.favorite);
        assert!(summary.has_totp);
    }

    #[test]
    fn item_summary_with_vault_name() {
        let item = test_item();
        let summary = ItemSummary::from_item_with_vault(&item, "Personal");
        assert_eq!(summary.vault_name, "Personal");
        assert_eq!(summary.title, "GitHub");
    }
}
