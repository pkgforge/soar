---
title: Health
description: Diagnose your Soar installation, fix broken packages and symlinks, and run cache and sync maintenance.
---

# Health

Soar's health check quickly identifies potential problems with your
installation, including missing binaries, broken packages, and broken symlinks.

## Checking Health

To check Soar's health, run:

```sh
soar health
```

### What It Checks

When executed, the command:

- Checks whether Soar's binary path is included in the `PATH` environment variable.
- Lists **broken packages**, which are incomplete package installations.
- Lists **broken symlinks**, which are dangling symlinks created by Soar that no longer point to valid files.
  - **bin directory**: detects all broken symlinks.
  - **desktop and icons directories**: detects only broken symlinks whose filenames end with the `-soar` suffix.

### Reading the Output

The health check displays results in a table with a status indicator for each
category.

**Status icons:**

- checkmark (green): the item is healthy.
- cross (red): issues were found.
- arrow: marks individual items in a list.

**Table categories:**

- **PATH**: checks whether Soar's binary directory is in your `PATH`.
- **Broken Packages**: lists incomplete package installations.
- **Broken Symlinks**: lists dangling symlinks created by Soar.

When issues are detected, suggested commands to fix them are printed below the
table.

### Example Output

When everything is healthy:

```
╭────────────────────────────────────────╮
│          System Health Check           │
├──────────────────┬─────────────────────┤
│ PATH             │ ✓ Configured        │
│ Broken Packages  │ ✓ None              │
│ Broken Symlinks  │ ✓ None              │
╰──────────────────┴─────────────────────╯
```

When issues are found:

```
╭────────────────────────────────────────╮
│          System Health Check           │
├──────────────────┬─────────────────────┤
│ PATH             │ ⚠ /path/to/bin not │
│                  │   in PATH           │
│ Broken Packages  │ ✗ 2 found          │
│ Broken Symlinks  │ ✗ 1 found          │
╰──────────────────┴─────────────────────╯

Broken packages:
  → cat#test: /home/user/.local/share/soar/packages/cat-test-q1235
  → ls#test: /home/user/.local/share/soar/packages/ls-test-q2345
Run soar clean --broken to remove

Broken symlinks:
  → /home/user/.local/bin/ls
Run soar clean --broken-symlinks to remove
```

### Fixing Issues

| Issue | Command |
|-------|---------|
| Broken packages | `soar clean --broken` |
| Broken symlinks | `soar clean --broken-symlinks` |
| Stale cache | `soar clean --cache` |

See [Clean Command](#clean-command) for details on each operation.

## Environment Variables

To view all Soar-related environment variables and their current values, run:

```sh
soar env
```

This command displays:

- `SOAR_CONFIG`: config file path.
- `SOAR_BIN`: binary directory path.
- `SOAR_DB`: database directory path.
- `SOAR_CACHE`: cache directory path.
- `SOAR_PACKAGES`: packages directory path.
- `SOAR_REPOSITORIES`: repository directory path.

These environment variables can be set to override Soar's default paths and
behavior. For example:

```sh
# Use a custom cache directory
export SOAR_CACHE="/tmp/soar-cache"
soar install neovim

# Switch to nightly builds
SOAR_NIGHTLY=1 soar self update
```

## Clean Command

The `clean` command performs maintenance operations that keep your Soar
installation tidy and efficient.

### Usage

```sh
soar clean [OPTIONS]
```

### Options

| Option | Description |
|--------|-------------|
| `--cache` | Deletes the entire cache directory (all cached package files). |
| `--broken` | Removes packages marked as broken in the database (incomplete installations). |
| `--broken-symlinks` | Removes broken symlinks from the bin, desktop, and icons directories. |

::: warning Cleaning is destructive
`--cache` deletes the entire cache directory, and the broken-package and
broken-symlink operations permanently remove the affected entries. With no
flags, `soar clean` runs every operation at once.
:::

### Examples

```sh
# Run all clean operations (no flags = clean everything)
soar clean

# Clean only the cache
soar clean --cache

# Remove only broken packages
soar clean --broken

# Remove only broken symlinks
soar clean --broken-symlinks

# Run specific clean operations together
soar clean --cache --broken
```

### When to Clean

- **After installation failures**: when `soar health` reports broken packages.
- **To free disk space**: clear the cache, which deletes the entire cache directory.
- **After manual file removal**: clean up broken symlinks pointing to deleted files.
- **Before major updates**: clear the cache to ensure fresh downloads.
- **General maintenance**: run `soar clean` with no flags to perform all operations.

## Sync Command

The `sync` command updates repository metadata from remote sources so you always
have the latest package information.

### Usage

```sh
soar sync
```

The command has two aliases:

```sh
soar S      # Short alias
soar fetch  # Alternative name
```

### What It Does

When executed, the command:

- Fetches the latest package metadata from all enabled repositories.
- Updates the local database with new package versions.
- Respects each repository's `sync_interval` setting.

### Sync Intervals

Repositories can be configured with different sync intervals:

```toml
# In config.toml
[[repositories]]
name = "main"
url = "https://example.com/repo"
sync_interval = "3h"  # Sync every 3 hours (default)
```

**Special `sync_interval` values:**

- `"always"`: sync every time `soar sync` is run.
- `"never"`: never sync automatically.
- `"auto"`: use the default 3-hour interval.
- `"3h"`, `"12h"`, `"1d"`: custom duration strings.

### When to Sync

- **Before searching for new packages**: ensures you see the latest versions.
- **After adding a new repository**: fetches the initial metadata.
- **Before updating packages**: gets the latest version information.
- **Anytime**: run to refresh repository metadata.

### Example

```sh
# Sync all repositories
soar sync
```

```
Syncing repository 'main'...
Fetching metadata from https://example.com/repo...
Updated: 1427 packages available
```
