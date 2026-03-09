//! Kagi (鍵) — GPU-rendered 1Password client.
//!
//! Replaces the 1Password GUI while using the 1Password service and API:
//! - GPU-accelerated UI via garasu (wgpu/winit)
//! - 1Password Connect API or `op` CLI for vault operations
//! - Secure clipboard management with auto-clear
//! - Fuzzy search across vaults, items, and fields
//! - Hot-reloadable configuration via shikumi

mod api;
mod clipboard;
mod config;
mod render;
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
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    let config = config::load(&cli.config)?;
    let clip = clipboard::SecureClip::from_config(&config.clipboard);

    match cli.command {
        None | Some(Commands::Open) => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .with_writer(std::io::stderr)
                .init();

            tracing::info!("launching kagi GUI");
            // TODO: madori::App::builder(KagiRenderer::new(...)).run()
            eprintln!("GUI mode not yet implemented — use `kagi list` or `kagi get <item>`");
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
                let vaults = backend.list_vaults().await?;

                // Search all vaults for the item
                for vault in &vaults {
                    let items = backend.list_items(&vault.id).await?;
                    if let Some(found) = items.iter().find(|i| {
                        i.title.eq_ignore_ascii_case(&item) || i.id == item
                    }) {
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
                                clip.copy_secret(v)?;
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
        Some(Commands::Search { query }) => {
            tracing_subscriber::fmt()
                .with_env_filter(EnvFilter::from_default_env())
                .init();

            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                let backend = api::create_backend(&config.api)?;
                let vaults = backend.list_vaults().await?;
                let mut results = Vec::new();

                for vault in &vaults {
                    let items = backend.list_items(&vault.id).await?;
                    for item in items {
                        if item.matches(&query) {
                            results.push((vault.name.clone(), item));
                        }
                    }
                }

                if results.is_empty() {
                    println!("No results for \"{query}\"");
                } else {
                    println!("Results for \"{query}\" ({} found):", results.len());
                    for (vault_name, item) in &results {
                        let summary = ItemSummary::from(item);
                        let url = summary.url.as_deref().unwrap_or("");
                        println!("  [{}] {} — {} {url}", vault_name, summary.title, summary.category);
                    }
                }

                Ok::<(), anyhow::Error>(())
            })?;
        }
    }

    Ok(())
}
