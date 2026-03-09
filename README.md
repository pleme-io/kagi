# Kagi (鍵)

GPU-rendered 1Password client. Replaces the 1Password GUI while using the 1Password service and API for all vault operations.

## Features

- GPU-accelerated vault browser via garasu (wgpu Metal/Vulkan)
- 1Password Connect API or `op` CLI backend
- Fuzzy search across all vaults and items
- Secure clipboard with auto-clear (configurable timeout)
- Biometric unlock (via `op` CLI integration)
- Hot-reloadable configuration via shikumi

## Architecture

| Module | Purpose |
|--------|---------|
| `api` | 1Password Connect API + `op` CLI backends |
| `vault` | Vault/item data models (zeroize for secrets) |
| `clipboard` | Secure clipboard with auto-clear |
| `render` | GPU vault browser UI via garasu |
| `config` | shikumi-based configuration |

## Dependencies

- **garasu** — GPU rendering engine
- **tsunagu** — daemon IPC (background sync)
- **shikumi** — config discovery + hot-reload

## Build

```bash
cargo build
cargo run
cargo run -- list
cargo run -- get "My Login" --field password
cargo run -- search "github"
```

## Configuration

`~/.config/kagi/kagi.yaml`

```yaml
api:
  op_path: op
clipboard:
  clear_timeout_secs: 30
  auto_clear: true
```
