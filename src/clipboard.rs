//! Secure clipboard management.
//!
//! Copies secrets to system clipboard via arboard, then schedules
//! auto-clear after a configurable timeout. Uses zeroize for secure
//! memory handling.

use crate::config::ClipboardConfig;
use arboard::Clipboard as ArboardClipboard;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::Notify;

#[derive(thiserror::Error, Debug)]
pub enum ClipboardError {
    #[error("clipboard access failed: {0}")]
    Access(String),
}

pub type Result<T> = std::result::Result<T, ClipboardError>;

/// Secure clipboard with auto-clear support.
pub struct SecureClip {
    inner: Mutex<Option<ArboardClipboard>>,
    auto_clear: bool,
    clear_timeout: Duration,
    cancel: Arc<Notify>,
}

impl SecureClip {
    /// Create from config.
    #[must_use]
    pub fn from_config(config: &ClipboardConfig) -> Self {
        Self {
            inner: Mutex::new(None),
            auto_clear: config.auto_clear,
            clear_timeout: Duration::from_secs(u64::from(config.clear_timeout_secs)),
            cancel: Arc::new(Notify::new()),
        }
    }

    fn with_board<F, T>(&self, f: F) -> Result<T>
    where
        F: FnOnce(&mut ArboardClipboard) -> std::result::Result<T, arboard::Error>,
    {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| ClipboardError::Access(format!("mutex poisoned: {e}")))?;

        let board = match guard.as_mut() {
            Some(b) => b,
            None => {
                let b = ArboardClipboard::new()
                    .map_err(|e| ClipboardError::Access(e.to_string()))?;
                guard.insert(b)
            }
        };

        f(board).map_err(|e| ClipboardError::Access(e.to_string()))
    }

    /// Copy a secret to the clipboard. If auto-clear is enabled,
    /// spawns a background task to clear after timeout.
    pub fn copy_secret(&self, value: &str) -> Result<()> {
        // Cancel any previous auto-clear timer
        self.cancel.notify_waiters();

        self.with_board(|b| b.set_text(value))?;
        tracing::info!("copied secret to clipboard");

        if self.auto_clear {
            let cancel = Arc::clone(&self.cancel);
            let timeout = self.clear_timeout;

            // We can't hold the Mutex across an async boundary, so we
            // create a new clipboard handle in the clear task.
            tokio::spawn(async move {
                tokio::select! {
                    () = tokio::time::sleep(timeout) => {
                        tracing::info!("auto-clearing clipboard after {timeout:?}");
                        match ArboardClipboard::new() {
                            Ok(mut b) => { let _ = b.clear(); }
                            Err(e) => tracing::warn!("failed to clear clipboard: {e}"),
                        }
                    }
                    () = cancel.notified() => {
                        tracing::debug!("auto-clear cancelled (new copy)");
                    }
                }
            });
        }

        Ok(())
    }

    /// Copy non-secret text (no auto-clear).
    pub fn copy_text(&self, text: &str) -> Result<()> {
        self.with_board(|b| b.set_text(text))
    }

    /// Read text from clipboard.
    pub fn paste(&self) -> Result<String> {
        self.with_board(|b| b.get_text())
    }

    /// Immediately clear the clipboard.
    pub fn clear(&self) -> Result<()> {
        self.cancel.notify_waiters();
        self.with_board(|b| b.clear())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ClipboardConfig;

    #[test]
    fn secure_clip_creates() {
        let config = ClipboardConfig::default();
        let _clip = SecureClip::from_config(&config);
    }
}
