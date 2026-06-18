---
title: Repository Management
description: Add, update, remove, and list Soar repositories from the command line without editing config.toml by hand.
---

# Repository Management

Soar manages repositories directly from the command line through `soar repo` (or `soar repository`). You can add, update, remove, and list repositories without manually editing `config.toml`.

## List Repositories

View all configured repositories:

```sh
soar repo list
```

This displays each repository's name, status (enabled or disabled), and URL.

## Add a Repository

```sh
soar repo add <name> <url> [options]
```

### Options

| Option | Description |
|--------|-------------|
| `--pubkey <KEY>` | Base64-encoded public key for signature verification |
| `--enabled <BOOL>` | Whether the repository is enabled (default: true) |
| `--desktop-integration <BOOL>` | Enable desktop integration for packages |
| `--signature-verification <BOOL>` | Enable signature verification |
| `--sync-interval <DURATION>` | Sync interval (for example `"1h"`, `"12h"`, `"1d"`) |

### Examples

Add a basic repository:

```sh
soar repo add myrepo https://example.com/metadata.sdb.zstd
```

Add a repository with signature verification:

```sh
soar repo add myrepo https://example.com/metadata.sdb.zstd \
  --pubkey "RWQ109gKujRqohsA7RERlXFfeJi23EcHN3Dz8TxyPAywa5mLw/fbcbU4" \
  --signature-verification true
```

Add a disabled repository:

```sh
soar repo add staging https://example.com/staging.sdb.zstd --enabled false
```

## Update a Repository

Modify settings on an existing repository. Only the fields you specify are changed:

```sh
soar repo update <name> [options]
```

### Options

| Option | Description |
|--------|-------------|
| `--url <URL>` | New metadata URL |
| `--pubkey <KEY>` | New public key |
| `--enabled <BOOL>` | Enable or disable the repository |
| `--desktop-integration <BOOL>` | Enable or disable desktop integration |
| `--signature-verification <BOOL>` | Enable or disable signature verification |
| `--sync-interval <DURATION>` | New sync interval |

### Examples

Disable a repository:

```sh
soar repo update myrepo --enabled false
```

Change the URL and sync interval:

```sh
soar repo update myrepo --url https://new.example.com/metadata.sdb.zstd --sync-interval 6h
```

## Remove a Repository

```sh
soar repo remove <name>
```

This removes the repository from the configuration and cleans up its local data directory.

### Example

```sh
soar repo remove myrepo
```

::: warning
Removing a repository deletes its cached metadata. Packages already installed from the repository remain installed but will not receive updates until the repository is re-added.
:::

## Related Topics

- [Configuration](./configuration.md) for manual repository configuration in `config.toml`
- [Maintenance](./maintenance.md) for repository sync and maintenance
