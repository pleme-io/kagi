//! Kagi (鍵) -- GPU-rendered 1Password client.
//!
//! Replaces the 1Password GUI while using the 1Password service and API:
//! - GPU-accelerated UI via garasu (wgpu/winit)
//! - 1Password Connect API or `op` CLI for vault operations
//! - Secure clipboard management with auto-clear via hasami
//! - Fuzzy search across vaults, items, and fields
//! - Hot-reloadable configuration via shikumi

mod api;
mod clipboard;
mod config;
mod input;
mod mcp;
mod render;
mod scripting;
mod vault;

use api::VaultBackend;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;
use vault::ItemSummary;

#[derive(Parser)]
#[command(name = "kagi", version, about = "GPU-rendered 1Password client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Configuration file override
    #[arg(long, env = "KAGI_CONFIG")]
    config: Option<std::path::PathBuf>,
}

#[derive(Subcommand)]
enum Commands {
    /// Launch the GUI
    Open,
    /// List items in a vault
    List {
        /// Vault name or UUID
        vault: Option<String>,
    },
    /// Get a specific item and copy password to clipboard
    Get {
        /// Item name or UUID
        item: String,
        /// Field to copy (default: password)
        #[arg(short, long, default_value = "password")]
        field: String,
    },
    /// Search across all vaults
    Search {
        /// Search query
        query: String,
    },
    /// Start the MCP server (stdio transport).
    Mcp,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let config = config::load(&cli.config)?;

    match cli.command {
        None | Some(Commands::Open) => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .with_writer(std::io::stderr)
                .init();

            tracing::info!("launching kagi GUI");
            run_gui(config)?;
        }
        Some(Commands::List { vault }) => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .init();

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let backend = api::create_backend(&config.api)?;
                let vaults = backend.list_vaults().await?;

                let target_vault = match &vault {
                    Some(name) => vaults
                        .iter()
                        .find(|v| v.name.eq_ignore_ascii_case(name) || v.id == *name),
                    None => None,
                };

                if let Some(v) = target_vault {
                    let items = backend.list_items(&v.id).await?;
                    println!("Vault: {} ({} items)", v.name, items.len());
                    for item in &items {
                        let summary = ItemSummary::from(item);
                        let user = summary.username.as_deref().unwrap_or("");
                        println!("  {} [{}] {user}", summary.title, summary.category);
                    }
                } else {
                    println!("Vaults:");
                    for v in &vaults {
                        println!("  {} ({} items)", v.name, v.items);
                    }
                    if vault.is_some() {
                        eprintln!("\nVault not found. Use one of the names above.");
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?;
        }
        Some(Commands::Get { item, field }) => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .init();

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let backend = api::create_backend(&config.api)?;
                let clip = clipboard::SecureClip::from_config(&config.clipboard)
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                let vaults = backend.list_vaults().await?;

                // Search all vaults for the item
                for vault in &vaults {
                    let items = backend.list_items(&vault.id).await?;
                    if let Some(found) = items.iter().find(|i| {
                        i.title.eq_ignore_ascii_case(&item) || i.id == item
                    }) {
                        // Handle TOTP specially
                        if field == "totp" || field == "otp" {
                            match backend.get_totp(&vault.id, &found.id).await {
                                Ok(code) => {
                                    clip.copy_secret(&code)
                                        .map_err(|e| anyhow::anyhow!("{e}"))?;
                                    let timeout = config.clipboard.clear_timeout_secs;
                                    println!(
                                        "Copied TOTP for \"{}\" to clipboard (clears in {timeout}s)",
                                        found.title
                                    );
                                    if config.clipboard.auto_clear {
                                        tokio::time::sleep(std::time::Duration::from_secs(
                                            u64::from(timeout) + 1,
                                        ))
                                        .await;
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Failed to get TOTP for \"{}\": {e}", found.title);
                                }
                            }
                            return Ok(());
                        }

                        let full = backend.get_item(&vault.id, &found.id).await?;

                        let value = if field == "password" {
                            full.password()
                        } else if field == "username" {
                            full.username()
                        } else {
                            full.field_by_label(&field).map(|f| f.value.as_str())
                        };

                        match value {
                            Some(v) => {
                                clip.copy_secret(v)
                                    .map_err(|e| anyhow::anyhow!("{e}"))?;
                                let timeout = config.clipboard.clear_timeout_secs;
                                println!(
                                    "Copied {field} for \"{}\" to clipboard (clears in {timeout}s)",
                                    full.title
                                );
                                // Keep runtime alive for auto-clear timer
                                if config.clipboard.auto_clear {
                                    tokio::time::sleep(std::time::Duration::from_secs(
                                        u64::from(timeout) + 1,
                                    ))
                                    .await;
                                }
                            }
                            None => {
                                eprintln!("Field \"{field}\" not found in \"{}\"", full.title);
                                let labels: Vec<&str> =
                                    full.fields.iter().map(|f| f.label.as_str()).collect();
                                eprintln!("Available fields: {}", labels.join(", "));
                            }
                        }
                        return Ok(());
                    }
                }
                eprintln!("Item \"{item}\" not found in any vault");
                Ok::<(), anyhow::Error>(())
            })?;
        }
        Some(Commands::Mcp) => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .with_writer(std::io::stderr)
                .init();

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                if let Err(e) = mcp::run(config).await {
                    eprintln!("MCP server error: {e}");
                    std::process::exit(1);
                }
            });
        }
        Some(Commands::Search { query }) => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .init();

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let backend = api::create_backend(&config.api)?;
                let vaults = backend.list_vaults().await?;
                let mut scored_results = Vec::new();

                for vault in &vaults {
                    let items = backend.list_items(&vault.id).await?;
                    for item in items {
                        let score = item.fuzzy_score(&query);
                        if score > 0 {
                            scored_results.push((score, vault.name.clone(), item));
                        }
                    }
                }

                // Sort by score descending
                scored_results.sort_by(|a, b| b.0.cmp(&a.0));

                if scored_results.is_empty() {
                    println!("No results for \"{query}\"");
                } else {
                    println!("Results for \"{query}\" ({} found):", scored_results.len());
                    for (score, vault_name, item) in &scored_results {
                        let summary = ItemSummary::from(item);
                        let url = summary.url.as_deref().unwrap_or("");
                        let star = if item.favorite { "*" } else { " " };
                        println!("  {star}[{vault_name}] {} \u{2014} {} {url} (score: {score})", summary.title, summary.category);
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Messages passed between the event loop and background runtime
// ---------------------------------------------------------------------------

/// Requests sent from the event loop to the background runtime.
enum BackendRequest {
    LoadVaults,
    LoadItems { vault_id: String },
    GetItem { vault_id: String, item_id: String },
    GetTotp { vault_id: String, item_id: String },
    CopyToClipboard { value: String, label: String },
}

/// Responses sent from the background runtime to the event loop.
enum BackendResponse {
    Vaults(Result<Vec<vault::Vault>, String>),
    Items {
        items: Result<Vec<vault::Item>, String>,
    },
    ItemDetail(Result<vault::Item, String>),
    Totp(Result<String, String>),
    Copied {
        label: String,
        result: Result<(), String>,
    },
    AllItemsLoaded {
        items: Vec<vault::Item>,
        summaries: Vec<ItemSummary>,
    },
}

/// Launch the GPU-rendered vault browser.
fn run_gui(config: config::KagiConfig) -> anyhow::Result<()> {
    use input::Action;
    use madori::{App, AppConfig, AppEvent};
    use render::KagiRenderer;
    use std::sync::mpsc;

    let mut renderer = KagiRenderer::new(&config.appearance);
    renderer.state.loading = true;

    // Channels for async communication
    let (req_tx, req_rx) = mpsc::channel::<BackendRequest>();
    let (resp_tx, resp_rx) = mpsc::channel::<BackendResponse>();

    // Spawn background tokio runtime for backend operations
    let api_config = config.api.clone();
    let clip_config = config.clipboard.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
        rt.block_on(async {
            let backend = match api::create_backend(&api_config) {
                Ok(b) => b,
                Err(e) => {
                    let _ = resp_tx.send(BackendResponse::Vaults(Err(format!("{e}"))));
                    return;
                }
            };

            let clip = clipboard::SecureClip::from_config(&clip_config).ok();

            while let Ok(req) = req_rx.recv() {
                match req {
                    BackendRequest::LoadVaults => {
                        let result = backend.list_vaults().await.map_err(|e| format!("{e}"));
                        // If vaults loaded, also load all items for global search
                        if let Ok(ref vaults) = result {
                            let mut all_items = Vec::new();
                            let mut all_summaries = Vec::new();
                            for v in vaults {
                                if let Ok(items) = backend.list_items(&v.id).await {
                                    for item in &items {
                                        all_summaries.push(ItemSummary::from_item_with_vault(
                                            item, &v.name,
                                        ));
                                    }
                                    all_items.extend(items);
                                }
                            }
                            let _ = resp_tx.send(BackendResponse::AllItemsLoaded {
                                items: all_items,
                                summaries: all_summaries,
                            });
                        }
                        let _ = resp_tx.send(BackendResponse::Vaults(result));
                    }
                    BackendRequest::LoadItems { vault_id } => {
                        let result = backend
                            .list_items(&vault_id)
                            .await
                            .map_err(|e| format!("{e}"));
                        let _ = resp_tx.send(BackendResponse::Items {
                            items: result,
                        });
                    }
                    BackendRequest::GetItem {
                        vault_id,
                        item_id,
                    } => {
                        let result = backend
                            .get_item(&vault_id, &item_id)
                            .await
                            .map_err(|e| format!("{e}"));
                        let _ = resp_tx.send(BackendResponse::ItemDetail(result));
                    }
                    BackendRequest::GetTotp {
                        vault_id,
                        item_id,
                    } => {
                        let result = backend
                            .get_totp(&vault_id, &item_id)
                            .await
                            .map_err(|e| format!("{e}"));
                        let _ = resp_tx.send(BackendResponse::Totp(result));
                    }
                    BackendRequest::CopyToClipboard { value, label } => {
                        let result = match &clip {
                            Some(c) => c
                                .copy_secret(&value)
                                .map_err(|e| format!("{e}")),
                            None => Err("clipboard not available".into()),
                        };
                        let _ = resp_tx.send(BackendResponse::Copied { label, result });
                    }
                }
            }
        });
    });

    // Request initial vault load
    req_tx.send(BackendRequest::LoadVaults)?;

    let app_config = AppConfig {
        title: String::from("Kagi \u{2014} 1Password Client"),
        width: 1280,
        height: 720,
        resizable: true,
        vsync: true,
        transparent: false,
    };

    let clear_timeout = config.clipboard.clear_timeout_secs;

    App::builder(renderer)
        .config(app_config)
        .on_event(move |event, renderer: &mut KagiRenderer| {
            // Poll for backend responses (non-blocking)
            while let Ok(resp) = resp_rx.try_recv() {
                match resp {
                    BackendResponse::Vaults(Ok(vaults)) => {
                        renderer.state.set_vaults(vaults);
                        renderer.state.loading = false;
                        renderer.state.set_status("Vaults loaded");
                    }
                    BackendResponse::Vaults(Err(e)) => {
                        renderer.state.loading = false;
                        renderer.state.set_status(format!("Error loading vaults: {e}"));
                    }
                    BackendResponse::Items { items: Ok(items), .. } => {
                        renderer.state.set_items(items);
                        renderer.state.loading = false;
                    }
                    BackendResponse::Items { items: Err(e), .. } => {
                        renderer.state.loading = false;
                        renderer.state.set_status(format!("Error loading items: {e}"));
                    }
                    BackendResponse::ItemDetail(Ok(item)) => {
                        renderer.state.detail_item = Some(item);
                        renderer.state.detail_field_index = 0;
                        renderer.state.show_hidden = false;
                        renderer.state.prev_mode = Some(renderer.state.mode.clone());
                        renderer.state.mode = render::ViewMode::ItemDetail;
                        renderer.state.loading = false;
                    }
                    BackendResponse::ItemDetail(Err(e)) => {
                        renderer.state.loading = false;
                        renderer.state.set_status(format!("Error loading item: {e}"));
                    }
                    BackendResponse::Totp(Ok(code)) => {
                        // Copy TOTP to clipboard
                        let _ = req_tx.send(BackendRequest::CopyToClipboard {
                            value: code,
                            label: "TOTP".into(),
                        });
                    }
                    BackendResponse::Totp(Err(e)) => {
                        renderer.state.set_status(format!("TOTP error: {e}"));
                    }
                    BackendResponse::Copied { label, result } => match result {
                        Ok(()) => {
                            renderer.state.set_status(format!(
                                "Copied {label} to clipboard (clears in {clear_timeout}s)"
                            ));
                        }
                        Err(e) => {
                            renderer.state.set_status(format!("Clipboard error: {e}"));
                        }
                    },
                    BackendResponse::AllItemsLoaded { items, summaries } => {
                        renderer.state.set_all_items(items, summaries);
                    }
                }
            }

            match event {
                AppEvent::Key(key_event) => {
                    let action = input::map_key(
                        &key_event.key,
                        key_event.pressed,
                        &key_event.modifiers,
                        &key_event.text,
                        &renderer.state.mode,
                    );

                    match action {
                        Action::Down => renderer.state.move_down(),
                        Action::Up => renderer.state.move_up(),
                        Action::Select => {
                            match renderer.state.mode {
                                render::ViewMode::VaultList => {
                                    // Select vault -> load items and switch to item list
                                    let vault_idx = renderer.state.vault_list.selected_index();
                                    if let Some(vault) = renderer.state.vaults.get(vault_idx) {
                                        renderer.state.selected_vault = vault_idx;
                                        renderer.state.loading = true;
                                        let _ = req_tx.send(BackendRequest::LoadItems {
                                            vault_id: vault.id.clone(),
                                        });
                                        renderer.state.mode = render::ViewMode::ItemList;
                                    }
                                }
                                render::ViewMode::ItemList | render::ViewMode::Search => {
                                    // Get full item details from backend
                                    if let Some((vault_id, item_id)) = renderer.state.selected_ids() {
                                        renderer.state.loading = true;
                                        let _ = req_tx.send(BackendRequest::GetItem {
                                            vault_id,
                                            item_id,
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        Action::CopyPassword => {
                            let ids = if renderer.state.mode == render::ViewMode::ItemDetail {
                                renderer.state.detail_item.as_ref().map(|i| (i.vault_id.clone(), i.id.clone()))
                            } else {
                                renderer.state.selected_ids()
                            };
                            if let Some((vault_id, item_id)) = ids {
                                // Need to fetch item to get password
                                let _ = req_tx.send(BackendRequest::GetItem {
                                    vault_id: vault_id.clone(),
                                    item_id: item_id.clone(),
                                });
                                // We handle the copy when we get the response, but we need
                                // a way to know it's for a copy. Use a simpler approach:
                                // look for password in already-loaded items
                                let password = if let Some(ref item) = renderer.state.detail_item {
                                    item.password().map(String::from)
                                } else {
                                    renderer.state.selected_item().and_then(|i| i.password().map(String::from))
                                };
                                if let Some(pw) = password {
                                    let _ = req_tx.send(BackendRequest::CopyToClipboard {
                                        value: pw,
                                        label: "password".into(),
                                    });
                                } else {
                                    renderer.state.set_status("No password field found");
                                }
                            }
                        }
                        Action::CopyUsername => {
                            let username = if let Some(ref item) = renderer.state.detail_item {
                                item.username().map(String::from)
                            } else {
                                renderer.state.selected_item().and_then(|i| i.username().map(String::from))
                            };
                            if let Some(user) = username {
                                let _ = req_tx.send(BackendRequest::CopyToClipboard {
                                    value: user,
                                    label: "username".into(),
                                });
                            } else {
                                renderer.state.set_status("No username field found");
                            }
                        }
                        Action::CopyTotp => {
                            let ids = if let Some(ref item) = renderer.state.detail_item {
                                Some((item.vault_id.clone(), item.id.clone()))
                            } else {
                                renderer.state.selected_ids()
                            };
                            if let Some((vault_id, item_id)) = ids {
                                let _ = req_tx.send(BackendRequest::GetTotp {
                                    vault_id,
                                    item_id,
                                });
                            } else {
                                renderer.state.set_status("No item selected");
                            }
                        }
                        Action::EnterSearch => {
                            renderer.state.enter_search();
                        }
                        Action::ToggleFavorites => {
                            renderer.state.favorites_only = !renderer.state.favorites_only;
                            let msg = if renderer.state.favorites_only {
                                "Showing favorites only"
                            } else {
                                "Showing all items"
                            };
                            renderer.state.set_status(msg);
                            // Refresh display
                            if renderer.state.mode == render::ViewMode::Search {
                                renderer.state.apply_search();
                            }
                        }
                        Action::NextVault => {
                            if !renderer.state.vaults.is_empty() {
                                renderer.state.selected_vault =
                                    (renderer.state.selected_vault + 1) % renderer.state.vaults.len();
                                renderer.state.vault_list = egaku::ListView::new(
                                    renderer.state.vaults.iter()
                                        .map(|v| format!("{} ({})", v.name, v.items))
                                        .collect(),
                                    20,
                                );
                                // Load items for the new vault
                                if let Some(vault) = renderer.state.vaults.get(renderer.state.selected_vault) {
                                    renderer.state.loading = true;
                                    let _ = req_tx.send(BackendRequest::LoadItems {
                                        vault_id: vault.id.clone(),
                                    });
                                }
                            }
                        }
                        Action::Back => {
                            renderer.state.go_back();
                        }
                        Action::Quit => {
                            return madori::EventResponse {
                                consumed: true,
                                exit: true,
                                set_title: None,
                            };
                        }
                        Action::ToggleHidden => {
                            renderer.state.show_hidden = !renderer.state.show_hidden;
                        }
                        Action::CopyField => {
                            if let Some(value) = renderer.state.selected_field_value().map(String::from) {
                                let label = renderer.state.detail_item.as_ref()
                                    .and_then(|item| {
                                        item.fields.get(renderer.state.detail_field_index)
                                            .map(|f| f.label.clone())
                                    })
                                    .unwrap_or_else(|| "field".into());
                                let _ = req_tx.send(BackendRequest::CopyToClipboard {
                                    value,
                                    label,
                                });
                            }
                        }
                        Action::SearchInput(c) => {
                            renderer.state.search_input.insert_char(c);
                            renderer.state.apply_search();
                        }
                        Action::SearchBackspace => {
                            renderer.state.search_input.delete_back();
                            renderer.state.apply_search();
                        }
                        Action::SearchSubmit => {
                            // Select the first search result and enter detail
                            if !renderer.state.search_results.is_empty() {
                                if let Some((vault_id, item_id)) = renderer.state.selected_ids() {
                                    renderer.state.loading = true;
                                    let _ = req_tx.send(BackendRequest::GetItem {
                                        vault_id,
                                        item_id,
                                    });
                                }
                            }
                        }
                        Action::None => {}
                    }
                }
                AppEvent::CloseRequested => {
                    return madori::EventResponse {
                        consumed: false,
                        exit: true,
                        set_title: None,
                    };
                }
                _ => {}
            }
            madori::EventResponse::default()
        })
        .run()
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}
