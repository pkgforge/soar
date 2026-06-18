---
title: Configuration
description: Reference for soar's config.toml options, repositories, environment variables, and common fixes.
---

# Configuration

Soar stores configuration at `~/.config/soar/config.toml`. If the file doesn't exist, sensible defaults are used.

::: tip Quick Start
Run `soar defconfig` to create a default configuration file.
:::

## Configuration Reference

The options below are grouped by category. Every value reflects soar's defaults.

### Paths

Control where soar stores its data.

| Configuration Option | Type | Default | Description |
|---------------------|------|---------|-------------|
| `cache_path` | String | `~/.local/share/soar/cache` | Directory for cached package files |
| `db_path` | String | `~/.local/share/soar/db` | Path to package database |
| `bin_path` | String | `~/.local/share/soar/bin` | Directory for binary symlinks |
| `repositories_path` | String | `~/.local/share/soar/repos` | Local repository clones |
| `portable_dirs` | String | `~/.local/share/soar/portable-dirs` | Base path for portable app data (AppImage/FlatImage/RunImage/Wrappe only) |

### Performance

| Configuration Option | Type | Default | Description |
|---------------------|------|---------|-------------|
| `parallel` | Boolean | `true` | Enable parallel downloads |
| `parallel_limit` | Integer | `4` | Max parallel downloads (1-16) |
| `ghcr_concurrency` | Integer | `8` | Max GHCR concurrent requests (1-32) |
| `search_limit` | Integer | `20` | Max search results (5-100) |
| `cross_repo_updates` | Boolean | `false` | Allow cross-repo updates (not implemented) |

### Package Installation

| Configuration Option | Type | Default | Description |
|---------------------|------|---------|-------------|
| `install_patterns` | Array | `["!*.log", "!SBUILD", "!*.json", "!*.version"]` | Files to exclude during installation |

### Security

| Configuration Option | Type | Default | Description |
|---------------------|------|---------|-------------|
| `signature_verification` | Boolean | `null` (auto) | Enable package signature verification |

### Desktop Integration

| Configuration Option | Type | Default | Description |
|---------------------|------|---------|-------------|
| `desktop_integration` | Boolean | `null` (repo-specific) | Enable desktop menu entries |

### Repository Sync

| Configuration Option | Type | Default | Description |
|---------------------|------|---------|-------------|
| `sync_interval` | String | `"3h"` | How often to sync repositories |

Special `sync_interval` values: `"always"`, `"never"`, `"auto"` (3h), or a duration like `"30m"`, `"6h"`, `"1d"`.

### Display

| Configuration Option | Type | Default | Description |
|---------------------|------|---------|-------------|
| `display.progress_style` | String | `"modern"` | Progress bar style: `classic`, `modern`, `minimal` |
| `display.icons` | Boolean | `true` | Show Unicode icons |
| `display.spinners` | Boolean | `true` | Show animated spinners |

## Key Options

### Path Settings

Control where soar stores data. Add `bin_path` to your PATH:

```sh
export PATH="$HOME/.local/share/soar/bin:$PATH"
```

### Performance

- **`parallel` / `parallel_limit`**: Increase for faster downloads on stable connections, decrease for slow or unstable connections.
- **`ghcr_concurrency`**: Adjust if experiencing GHCR rate limiting.

### Install Patterns

Glob patterns for files to exclude during installation. Patterns starting with `!` are exclusions:

```toml
install_patterns = [
    "!*.log",      # Exclude log files
    "!SBUILD",     # Exclude build scripts
    "!*.debug",    # Exclude debug symbols
]
```

### Security

**`signature_verification`**: Set to `true` for maximum security or `false` for trusted local repos. This setting can be overridden per-repository.

### Desktop Integration

**`desktop_integration`**: Enable this for GUI applications to appear in application menus. The setting can be configured globally or per-repository.

### Display Settings

```toml
[display]
progress_style = "modern"  # classic, modern, minimal
icons = true
spinners = true
```

## Repositories

::: tip
You can manage repositories from the command line with `soar repo add/update/remove/list`. See [Repository Management](./repo.md).
:::

Repositories are defined as arrays of tables in your configuration:

```toml
[[repositories]]
name = "soarpkgs"
url = "https://github.com/pkgforge/soarpkgs/releases/latest/download/metadata-x86_64-linux.sdb.zstd"
pubkey = "RWQ109gKujRqohsA7RERlXFfeJi23EcHN3Dz8TxyPAywa5mLw/fbcbU4"
desktop_integration = true
enabled = true
signature_verification = true
sync_interval = "3h"
```

### Repository Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | String | (required) | Unique repository name. **Note:** `"local"` is reserved |
| `url` | String | (required) | URL to repository metadata |
| `pubkey` | String | `null` | Repository's public key (inline string) |
| `enabled` | Boolean | `true` | Enable/disable this repository |
| `desktop_integration` | Boolean | `true` | Enable desktop integration for packages |
| `signature_verification` | Boolean | auto | Enable signature verification (auto-enabled if `pubkey` exists) |
| `sync_interval` | String | `"3h"` | Sync interval: `"always"`, `"never"`, `"auto"`, or duration |

### Default Repositories

Soar includes one default repository for Linux platforms (aarch64, x86_64):

- **soarpkgs**: The unified package repository
  - URL: `https://github.com/pkgforge/soarpkgs/releases/latest/download/metadata-{platform}.sdb.zstd`
  - Signature verification enabled
  - Desktop integration enabled

## Managing Configuration

::: code-group

```sh [View]
soar config
```

```sh [Edit]
soar config -e
```

```sh [Custom path]
soar -c /path/to/config.toml [subcommand]
```

:::

## Environment Variables

| Variable | Description |
|----------|-------------|
| `SOAR_CONFIG` | Custom config file path |
| `SOAR_ROOT` | Root directory override (affects all profiles) |
| `SOAR_CACHE` | Cache directory override |
| `SOAR_BIN` | Binary directory override |
| `SOAR_DB` | Database path override |
| `SOAR_PACKAGES` | Packages directory override |
| `SOAR_REPOSITORIES` | Repositories directory override |
| `SOAR_PORTABLE_DIRS` | Portable directories path override |
| `RUST_LOG` | Debug logging level (`debug`, `info`, `trace`) |

::: info
Environment variables take precedence over configuration file settings and profile paths.
:::

## Common Issues

### Invalid TOML Syntax

Check for unclosed brackets, missing quotes, or duplicate names. Use `soar config` to validate.

### Command Not Found

Add `bin_path` to your PATH in `~/.bashrc` or `~/.zshrc`.

### Repository Not Syncing

Run `soar sync` manually. Check network connectivity and repository URLs.

### Signature Verification Failed

Verify the `pubkey` value is correct. Run `soar sync` to update repository data.

### Garbled Output

Switch to classic display mode:

```toml
[display]
progress_style = "classic"
icons = false
```
