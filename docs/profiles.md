---
title: Profiles
description: Isolated package environments in soar, with path resolution, usage, and system mode.
---

# Profiles

Profiles are isolated package environments that allow you to maintain separate package sets for different purposes. Each profile has its own root directory and package storage.

## Profile Configuration

Each profile is defined in your Soar configuration file (`~/.config/soar/config.toml` for user, `/etc/soar/config.toml` for system) under a `[profile.<name>]` section.

### Profile Structure

```toml
[profile.<name>]
root_path = "/path/to/profile/root"
packages_path = "/path/to/packages"  # Optional
```

- **`root_path`** (required): Root directory for the profile. Defaults to `~/.local/share/soar` or `$SOAR_ROOT/soar` if not in system mode.
- **`packages_path`** (optional): Custom location for package storage. If not set, defaults to `<root_path>/packages`.

### Path Resolution Priority

Soar resolves paths in this order (highest priority first):

1. **Environment variables** (`SOAR_BIN`, `SOAR_DB`, `SOAR_CACHE`, `SOAR_PACKAGES`, `SOAR_REPOSITORIES`, `SOAR_PORTABLE_DIRS`)
2. **Global configuration overrides** (`bin_path`, `db_path`, `cache_path`, etc. in the root of `config.toml`)
3. **Profile-specific paths** (computed from `root_path`)

::: warning Precedence
Global configuration paths and environment variables take precedence over profile-computed paths. If you set `bin_path` in the global config or the `SOAR_BIN` environment variable, it will be used instead of the profile's `<root_path>/bin`.
:::

### Computed Paths

From the `root_path`, Soar automatically derives these paths (unless overridden):

| Path | Computed As | Can Override With |
|------|-------------|-------------------|
| Packages | `<root_path>/packages` (or `packages_path` if set) | `SOAR_PACKAGES` env var |
| Binaries | `<root_path>/bin` | `bin_path` config or `SOAR_BIN` env var |
| Database | `<root_path>/db` | `db_path` config or `SOAR_DB` env var |
| Cache | `<root_path>/cache` | `cache_path` config or `SOAR_CACHE` env var |
| Repositories | `<root_path>/repos` | `repositories_path` config or `SOAR_REPOSITORIES` env var |
| Portable Dirs | `<root_path>/portable-dirs` | `portable_dirs` config or `SOAR_PORTABLE_DIRS` env var |

### Minimal Configuration

```toml
[profile.default]
root_path = "~/.local/share/soar"
```

This creates:

- `~/.local/share/soar/packages/` - Installed packages
- `~/.local/share/soar/bin/` - Binary symlinks (unless overridden globally)
- `~/.local/share/soar/cache/` - Download cache (unless overridden globally)
- `~/.local/share/soar/db/` - Package database (unless overridden globally)

### Custom Packages Path

```toml
[profile.production]
root_path = "/opt/soar/production"
packages_path = "/opt/soar/production/packages"
```

## Using Profiles

### Specifying a Profile

Use the `--profile <name>` flag with any Soar command:

```bash
# Install to specific profile
soar --profile dev install neovim
soar --profile testing install ripgrep

# List packages in a profile
soar --profile dev list

# Remove from a profile
soar --profile testing remove ripgrep

# Update packages in a profile
soar --profile dev update
```

### Environment Variable Override

The `SOAR_ROOT` environment variable overrides the `root_path` setting for any profile:

```bash
SOAR_ROOT=/tmp/test-soar soar install neovim
```

### System Mode

Use the `--system` flag (`-S`) to operate in system-wide mode. This changes the config location to `/etc/soar/config.toml` and typically requires root privileges:

```bash
sudo soar --system install git
sudo soar --system --profile global add node
```

## Profile Examples

### Development vs Production

```toml
# ~/.config/soar/config.toml
[profile.dev]
root_path = "~/dev-soar"

[profile.production]
root_path = "~/prod-soar"
```

```bash
# Install development tools
soar --profile dev install rustc cargo node

# Install production-grade tools
soar --profile production install nginx postgresql
```

### System-Wide Profile

```toml
# /etc/soar/config.toml
[profile.global]
root_path = "/opt/soar"
```

```bash
sudo soar --system --profile global install git
```

## Directory Structure

Profile with `root_path = "~/.local/share/soar"`:

```
~/.local/share/soar/
├── bin/              # Symlinks to package binaries
├── cache/            # Download cache
├── db/               # Package database
├── packages/         # Installed packages
├── repos/            # Repository metadata
└── portable-dirs/    # Portable app data
```

## Summary

Profiles provide simple, file-based environment isolation:

- **Configuration**: Define profiles in `config.toml` with `root_path` and optional `packages_path`
- **Usage**: Specify profiles with `--profile <name>` flag
- **Path Priority**: Environment variables > global config overrides > profile-computed paths
- **Computed Paths**: bin, db, cache, repos, portable-dirs are automatically derived from `root_path` unless overridden
- **System Mode**: Use `--system` flag for system-wide installations
- **Environment Override**: `SOAR_ROOT` overrides profile `root_path` at runtime
