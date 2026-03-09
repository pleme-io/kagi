# Kagibako (鍵箱) — GPU 1Password Client

Crate: `kagibako` | Binary: `kagi` | Config app name: `kagi`

GPU-rendered 1Password client. Uses 1Password's service (Connect API + `op` CLI) for
all vault operations. No local vault storage. No Electron.

## Build & Test

```bash
cargo build                       # compile
cargo test --lib                  # unit tests
cargo run                         # launch GUI (or fall back to CLI hint)
cargo run -- list                 # list vaults
cargo run -- list "Personal"      # list items in a vault
cargo run -- get "GitHub"         # copy password to clipboard
cargo run -- get "GitHub" -f totp # copy TOTP
cargo run -- search "aws"         # fuzzy search across all vaults
```

## Competitive Position

| Competitor | Stack | Our advantage |
|-----------|-------|---------------|
| **1Password 8** | Electron | GPU-rendered, vim-modal, MCP-drivable, Rhai scriptable |
| **Bitwarden** | Electron | 1Password's proven backend, GPU native, no self-hosting |
| **KeePassXC** | C++/Qt | Not Qt-dependent, MCP automation, Nix-configured |
| **pass/gopass** | Bash/Go+GPG | Full GUI, fuzzy search, 1Password security model |

Unique value: GPU-native 1Password client with MCP automation for AI workflows,
vim-modal navigation, auto-clearing secure clipboard, and Rhai scripting.

## Architecture

### Module Map

```
src/
  main.rs          ← CLI entry point (clap: open, list, get, search, daemon)
  config.rs        ← KagiConfig via shikumi (api, clipboard, appearance sections)
  api.rs           ← VaultBackend trait + ConnectBackend + OpCliBackend
  vault.rs         ← Data models: Vault, Item, Field, SecretValue (zeroize)
  clipboard.rs     ← SecureClip: arboard + auto-clear timer + zeroize
  render.rs        ← GPU vault browser (TODO: madori integration)

  search/          ← (planned) Fuzzy search engine
    mod.rs         ← FuzzyMatcher trait, scoring algorithm
    index.rs       ← In-memory item index for instant search

  biometric/       ← (planned) Biometric unlock
    mod.rs         ← BiometricAuth trait
    macos.rs       ← Touch ID via `op` CLI --biometric flag
    linux.rs       ← fingerprint via `op` CLI

  mcp/             ← (planned) MCP server via kaname
    mod.rs         ← KagiMcp server struct
    tools.rs       ← Tool implementations

  scripting/       ← (planned) Rhai scripting via soushi
    mod.rs         ← Engine setup, kagi.* API registration

module/
  default.nix      ← HM module (blackmatter.components.kagi)
```

### Data Flow

```
1Password Service (Connect API or `op` CLI)
          |
    VaultBackend trait
          |
    Vault → Item → Field (SecretValue with zeroize)
          |
    ┌─────┴─────┐
    │ FuzzySearch │ ← user query
    └─────┬─────┘
          |
    ItemSummary[] (no secrets in list views)
          |
    ┌─────┴──────┐
    │ GPU Render  │ ← garasu/madori/egaku
    └─────┬──────┘
          |
    SecureClip ← copy secret → auto-clear after N seconds
```

### API Backends

Two backends implement the `VaultBackend` trait:

1. **ConnectBackend** — 1Password Connect REST API (`/v1/vaults`, `/v1/vaults/{id}/items`)
   - Preferred for automation and server-side use
   - Requires `connect_url` + `connect_token` in config
   - Uses reqwest with Bearer auth

2. **OpCliBackend** — `op` CLI subprocess (`op vault list`, `op item get`)
   - Preferred for local interactive use
   - Supports biometric unlock (Touch ID)
   - Falls back when Connect config is absent
   - Optional `service_account_token` for CI/automation

Backend selection: if `connect_url` AND `connect_token` are set, use Connect.
Otherwise, use `op` CLI.

### Security Model

- **zeroize**: All `SecretValue` fields implement `ZeroizeOnDrop` — memory cleared when dropped
- **No local storage**: Items are always fetched from 1Password service, never cached to disk
- **Auto-clear clipboard**: Secrets copied to clipboard are cleared after configurable timeout (default 30s)
- **Field redaction**: `Debug` and `Display` for `SecretValue` print `[REDACTED]`
- **No secret logging**: Secret values never appear in tracing output

### Current Implementation Status

**Done:**
- `vault.rs` — Complete data models with zeroize, tests for field lookup/matching
- `api.rs` — Both backends (Connect + `op` CLI) with full CRUD operations
- `config.rs` — shikumi integration with all config sections
- `clipboard.rs` — SecureClip with auto-clear timer, tests
- `main.rs` — CLI with list/get/search subcommands, working end-to-end

**Not started:**
- GUI rendering via madori/garasu/egaku
- MCP server via kaname
- Rhai scripting via soushi
- Fuzzy search engine (current search is substring match)
- Biometric unlock module
- Daemon mode via tsunagu
- HM module (module/default.nix exists in flake but not yet created)

## Configuration

Uses **shikumi** for config discovery and hot-reload:
- Config file: `~/.config/kagi/kagi.yaml`
- Env override: `$KAGI_CONFIG`
- Env prefix: `KAGI_` (e.g., `KAGI_CLIPBOARD__CLEAR_TIMEOUT_SECS=60`)
- Hot-reload on file change (nix-darwin symlink aware)

### Config Schema

```yaml
api:
  connect_url: "https://connect.example.com"    # 1Password Connect URL (optional)
  connect_token: "eyJ..."                        # Connect bearer token (optional)
  op_path: "op"                                  # Path to `op` CLI binary
  service_account_token: null                    # OP_SERVICE_ACCOUNT_TOKEN

clipboard:
  clear_timeout_secs: 30                         # Auto-clear delay
  auto_clear: true                               # Enable auto-clear

appearance:
  background: "#2e3440"                          # Nord polar night
  foreground: "#eceff4"                          # Nord snow storm
  accent: "#88c0d0"                              # Nord frost
```

## Shared Library Integration

| Library | Usage |
|---------|-------|
| **shikumi** | Config discovery + hot-reload (`KagiConfig`) |
| **garasu** | GPU rendering for vault browser UI |
| **madori** | App framework (event loop, render loop) |
| **egaku** | Widgets (text input for search, list view for items, modal for detail) |
| **irodzuki** | Theme: base16 to GPU uniforms |
| **hasami** | Clipboard management (replaces current raw arboard usage) |
| **todoku** | HTTP client (replaces current raw reqwest for Connect API) |
| **tsunagu** | Daemon mode for background sync |
| **kaname** | MCP server framework |
| **soushi** | Rhai scripting engine |
| **awase** | Hotkey system for vim-modal navigation |
| **tsuuchi** | Notifications (watchtower alerts) |

**Note:** Cargo.toml currently references `hikidashi` and `kotoba` — these are the
old names for `hasami` and `kaname` respectively. Update when those crates are
renamed on the registry.

## MCP Server (kaname)

Standard tools: `status`, `config_get`, `config_set`, `version`

App-specific tools:
- `search_items(query)` — fuzzy search across all vaults
- `get_item(vault, item)` — get item with all fields
- `get_field(vault, item, field)` — get a specific field value
- `copy_password(vault, item)` — copy password to clipboard
- `copy_totp(vault, item)` — copy current TOTP code
- `list_vaults()` — list all vaults with item counts
- `create_item(vault, title, category, fields)` — create new item
- `update_item(vault, item, fields)` — update existing item
- `get_watchtower()` — password health report (weak, reused, compromised)
- `generate_password(length, options)` — generate a secure password

## Rhai Scripting (soushi)

Scripts from `~/.config/kagi/scripts/*.rhai`

```rhai
// Available API:
kagi.search("github")           // -> [{id, title, category, vault}]
kagi.get("vault-id", "item-id") // -> {title, fields: [{label, value}]}
kagi.copy_password("v", "i")    // copy password to clipboard
kagi.copy_totp("v", "i")        // copy TOTP to clipboard
kagi.generate(32, #{            // generate password
  uppercase: true,
  lowercase: true,
  digits: true,
  symbols: true,
})
kagi.vaults()                   // -> [{id, name, items}]
kagi.recent()                   // -> recently accessed items
kagi.favorites()                // -> favorite items
```

Event hooks: `on_startup`, `on_shutdown`, `on_copy(item_id)`, `on_search(query)`

## Hotkey System (awase)

### Modes

**Normal** (default — vault/item list navigation):
| Key | Action |
|-----|--------|
| `j/k` | Navigate items up/down |
| `Enter` | Open item detail |
| `p` | Copy password |
| `u` | Copy username |
| `t` | Copy TOTP |
| `/` | Focus search input |
| `f` | Toggle favorites filter |
| `Tab` | Switch vault |
| `q` | Quit |
| `:` | Enter command mode |

**Detail** (viewing a single item):
| Key | Action |
|-----|--------|
| `j/k` | Navigate fields |
| `Enter` | Copy field value |
| `e` | Edit item (future) |
| `H` | Toggle hidden field visibility |
| `q` / `Esc` | Back to list |

**Command** (`:` prefix):
- `:search <query>` — search across all vaults
- `:vault <name>` — switch to vault
- `:generate [length]` — generate password
- `:watchtower` — password health check
- `:clear` — clear clipboard now

## Nix Integration

### Flake Exports
- `packages.aarch64-darwin.{kagi, default}` — the binary
- `overlays.default` — `pkgs.kagi`
- `homeManagerModules.default` — `blackmatter.components.kagi`
- `devShells.aarch64-darwin.default` — dev environment with rustc, cargo, op CLI

### HM Module (planned)

Namespace: `blackmatter.components.kagi`

Typed options:
- `enable` — install package + generate config
- `package` — override package
- `api.{connect_url, connect_token, op_path}` — API backend config
- `clipboard.{clear_timeout_secs, auto_clear}` — clipboard behavior
- `appearance.{background, foreground, accent}` — colors
- `daemon.enable` — background sync via tsunagu (launchd/systemd)
- `mcp.enable` — register kagi MCP server for Claude Code
- `extraSettings` — raw attrset escape hatch

YAML generated via `lib.generators.toYAML` -> `xdg.configFile."kagi/kagi.yaml"`

## Design Constraints

- **Never store vault data locally** — always fetch from 1Password service
- **zeroize all secrets** — `SecretValue` with `ZeroizeOnDrop`, never log secret content
- **Clipboard auto-clear** — default 30s, configurable, cancel on new copy
- **VaultBackend trait** — all vault operations go through the trait, never call API directly
- **ItemSummary for lists** — list views use `ItemSummary` (no secret fields), detail view uses `Item`
- **GPU rendering** — all UI via garasu/madori/egaku, no TUI fallback in GUI mode
- **Platform-agnostic** — biometric unlock behind trait boundary, CLI subcommands work everywhere
