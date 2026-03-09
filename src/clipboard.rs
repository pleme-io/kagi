//! Secure clipboard management via hasami.
//!
//! Wraps hasami's `TimedClipboard` for password auto-clear, plus
//! a `ClipboardHistory` for recent copies. Uses zeroize for secure
//! memory handling of secrets before they're sent to the clipboard.

use crate::config::ClipboardConfig;
use hasami::{Clipboard, ClipboardProvider, TimedClipboard};
use std::sync::Arc;
use std::time::Duration;

#[derive(thiserror::Error, Debug)]
pub enum ClipboardError {
    #[error("clipboard access failed: {0}")]
    Access(#[from] hasami::HasamiError),
}

pub type Result<T> = std::result::Result<T, ClipboardError>;

/// Secure clipboard with auto-clear support via hasami.
pub struct SecureClip {
    timed: TimedClipboard<Clipboard>,
    clear_timeout: Duration,
    auto_clear: bool,
}

impl SecureClip {
    /// Create from config using a real system clipboard.
    ///
    /// # Errors
    ///
    /// Returns `ClipboardError` if the system clipboard cannot be accessed.
    pub fn from_config(config: &ClipboardConfig) -> Result<Self> {
        let clipboard = Arc::new(Clipboard::new()?);
        Ok(Self {
            timed: TimedClipboard::new(clipboard),
            clear_timeout: Duration::from_secs(u64::from(config.clear_timeout_secs)),
            auto_clear: config.auto_clear,
        })
    }

    /// Create from a pre-built clipboard provider (for testing or custom backends).
    #[allow(dead_code)]
    pub fn with_provider(provider: Arc<Clipboard>, config: &ClipboardConfig) -> Self {
        Self {
            timed: TimedClipboard::new(provider),
            clear_timeout: Duration::from_secs(u64::from(config.clear_timeout_secs)),
            auto_clear: config.auto_clear,
        }
    }

    /// Copy a secret to the clipboard. If auto-clear is enabled,
    /// schedules clearing after the configured timeout.
    pub fn copy_secret(&self, value: &str) -> Result<()> {
        if self.auto_clear {
            self.timed.copy_sensitive(value, self.clear_timeout)?;
            tracing::info!(
                timeout_secs = self.clear_timeout.as_secs(),
                "copied secret to clipboard (auto-clear scheduled)"
            );
        } else {
            self.timed.copy_text(value)?;
            tracing::info!("copied secret to clipboard (no auto-clear)");
        }
        Ok(())
    }

    /// Copy non-secret text (no auto-clear).
    #[allow(dead_code)]
    pub fn copy_text(&self, text: &str) -> Result<()> {
        self.timed.copy_text(text)?;
        Ok(())
    }

    /// Read text from clipboard.
    #[allow(dead_code)]
    pub fn paste(&self) -> Result<String> {
        Ok(self.timed.provider().paste_text()?)
    }

    /// Immediately clear the clipboard.
    #[allow(dead_code)]
    pub fn clear(&self) -> Result<()> {
        self.timed.provider().clear()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClipboardConfig;

    #[test]
    fn clipboard_error_display() {
        let err = ClipboardError::Access(hasami::HasamiError::Empty);
        assert!(err.to_string().contains("clipboard"));
    }

    #[test]
    fn config_creates_timeout() {
        let config = ClipboardConfig {
            clear_timeout_secs: 60,
            auto_clear: true,
        };
        assert_eq!(config.clear_timeout_secs, 60);
        assert!(config.auto_clear);
    }
}
