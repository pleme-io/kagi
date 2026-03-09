//! Rhai scripting plugin system.
//!
//! Loads user scripts from `~/.config/kagi/scripts/*.rhai` and registers
//! app-specific functions for password manager automation.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use soushi::ScriptEngine;

/// Event hooks that scripts can define.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScriptEvent {
    /// Fired when the app starts.
    OnStart,
    /// Fired when the app is quitting.
    OnQuit,
    /// Fired on key press with the key name.
    OnKey(String),
}

/// Manages the Rhai scripting engine with kagi-specific functions.
pub struct KagiScriptEngine {
    engine: ScriptEngine,
    /// Shared state for script-triggered actions.
    pub pending_actions: Arc<Mutex<Vec<ScriptAction>>>,
}

/// Actions that scripts can trigger.
#[derive(Debug, Clone)]
pub enum ScriptAction {
    /// Search for items matching a query.
    Search(String),
    /// Copy the password for an item.
    CopyPassword(String),
    /// Copy the TOTP for an item.
    CopyTotp(String),
}

impl KagiScriptEngine {
    /// Create a new scripting engine with kagi-specific functions registered.
    #[must_use]
    pub fn new() -> Self {
        let mut engine = ScriptEngine::new();
        engine.register_builtin_log();
        engine.register_builtin_env();
        engine.register_builtin_string();

        let pending = Arc::new(Mutex::new(Vec::<ScriptAction>::new()));

        // Register kagi.search(query)
        let p = Arc::clone(&pending);
        engine.register_fn("kagi_search", move |query: &str| {
            if let Ok(mut actions) = p.lock() {
                actions.push(ScriptAction::Search(query.to_string()));
            }
        });

        // Register kagi.copy_password(item)
        let p = Arc::clone(&pending);
        engine.register_fn("kagi_copy_password", move |item: &str| {
            if let Ok(mut actions) = p.lock() {
                actions.push(ScriptAction::CopyPassword(item.to_string()));
            }
        });

        // Register kagi.copy_totp(item)
        let p = Arc::clone(&pending);
        engine.register_fn("kagi_copy_totp", move |item: &str| {
            if let Ok(mut actions) = p.lock() {
                actions.push(ScriptAction::CopyTotp(item.to_string()));
            }
        });

        // Register kagi.list_vaults() — returns empty array (placeholder for live state)
        engine.register_fn("kagi_list_vaults", || -> soushi::rhai::Array {
            soushi::rhai::Array::new()
        });

        Self {
            engine,
            pending_actions: pending,
        }
    }

    /// Load scripts from the default config directory.
    pub fn load_user_scripts(&mut self) {
        let scripts_dir = scripts_dir();
        if scripts_dir.is_dir() {
            match self.engine.load_scripts_dir(&scripts_dir) {
                Ok(names) => {
                    if !names.is_empty() {
                        tracing::info!(count = names.len(), "loaded kagi scripts: {names:?}");
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, "failed to load kagi scripts");
                }
            }
        }
    }

    /// Fire an event hook.
    pub fn fire_event(&self, event: &ScriptEvent) {
        let hook_name = match event {
            ScriptEvent::OnStart => "on_start",
            ScriptEvent::OnQuit => "on_quit",
            ScriptEvent::OnKey(_) => "on_key",
        };

        let script = match event {
            ScriptEvent::OnKey(key) => format!("if is_def_fn(\"{hook_name}\", 1) {{ {hook_name}(\"{key}\"); }}"),
            _ => format!("if is_def_fn(\"{hook_name}\", 0) {{ {hook_name}(); }}"),
        };

        if let Err(e) = self.engine.eval(&script) {
            tracing::debug!(hook = hook_name, error = %e, "script hook not defined or failed");
        }
    }

    /// Drain any pending actions triggered by scripts.
    pub fn drain_actions(&self) -> Vec<ScriptAction> {
        if let Ok(mut actions) = self.pending_actions.lock() {
            actions.drain(..).collect()
        } else {
            Vec::new()
        }
    }
}

impl Default for KagiScriptEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Default scripts directory: `~/.config/kagi/scripts/`.
fn scripts_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("kagi")
        .join("scripts")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn engine_creation() {
        let _engine = KagiScriptEngine::new();
    }

    #[test]
    fn search_action() {
        let engine = KagiScriptEngine::new();
        engine
            .engine
            .eval(r#"kagi_search("github")"#)
            .unwrap();
        let actions = engine.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ScriptAction::Search(q) if q == "github"));
    }

    #[test]
    fn copy_password_action() {
        let engine = KagiScriptEngine::new();
        engine
            .engine
            .eval(r#"kagi_copy_password("GitHub")"#)
            .unwrap();
        let actions = engine.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ScriptAction::CopyPassword(item) if item == "GitHub"));
    }

    #[test]
    fn copy_totp_action() {
        let engine = KagiScriptEngine::new();
        engine
            .engine
            .eval(r#"kagi_copy_totp("AWS")"#)
            .unwrap();
        let actions = engine.drain_actions();
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0], ScriptAction::CopyTotp(item) if item == "AWS"));
    }

    #[test]
    fn list_vaults_returns_array() {
        let engine = KagiScriptEngine::new();
        let result = engine.engine.eval("kagi_list_vaults()").unwrap();
        assert!(result.is_array());
    }

    #[test]
    fn fire_event_does_not_panic() {
        let engine = KagiScriptEngine::new();
        engine.fire_event(&ScriptEvent::OnStart);
        engine.fire_event(&ScriptEvent::OnQuit);
        engine.fire_event(&ScriptEvent::OnKey("p".to_string()));
    }

    #[test]
    fn drain_actions_clears() {
        let engine = KagiScriptEngine::new();
        engine
            .engine
            .eval(r#"kagi_search("test")"#)
            .unwrap();
        assert_eq!(engine.drain_actions().len(), 1);
        assert!(engine.drain_actions().is_empty());
    }
}
