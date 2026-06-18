---
title: Search Packages
description: Find packages with soar search and inspect them in detail with soar query, including aliases, filters, and query syntax.
---

# Search Packages

Soar helps you find packages across all of your configured repositories and inspect any of them in detail. This guide covers the `search` command, its filters, and the `query` command for detailed package information.

## Basic Search

Search for packages with the `soar search` command:

```sh
soar search <query>
```

The `search` command is also available through shorter aliases:

```sh
# Short alias
soar s <query>

# Find alias
soar find <query>
```

For example, search for packages containing `bat`:

```sh
soar search bat
```

A search checks for a partial match in `pkg_id`, `pkg_name`, `pkg`, and the target from `provides`.

## Search Filters

### Case-Sensitive Search

Match the query with exact case:

```sh
soar search <query> --case-sensitive
```

For example:

```sh
soar search Bat --case-sensitive
```

### Exact Match Search

Match the full name only, with no partial matches:

```sh
soar search <query> --exact
```

For example:

```sh
soar search bat --exact
```

### Result Limit

Limit the number of results returned:

```sh
soar search <query> --limit <number>
```

For example, return only the top 10 results:

```sh
soar search editor --limit 10
```

## Cross-Repository Search

By default, Soar searches across all configured repositories, and results show which repository each package comes from:

```sh
soar search bat
```

Results may include packages from multiple repositories:

```
bat#official:soarpkgs
bat#official:official
```

To search within a specific repository, use the repository syntax:

```sh
soar search bat:official
```

## Reading Search Results

Each result is prefixed with a status indicator. The Unicode form is used when `display.icons` is enabled, and the ASCII form otherwise.

| Indicator | Meaning |
|-----------|---------|
| `[✓]` or `[+]` | Package is installed |
| `[○]` or `[-]` | Package is not installed |

Example output:

```
[✓] bat#official:official (0.24.0)
[○] bat#official:soarpkgs (0.23.0)
[○] code#official:flathub (latest)
```

## Search Patterns

### Partial Matching

A query matches any package that contains the query string:

```sh
# Matches any package containing "fire"
soar search fire

# Matches any package containing "code"
soar search code
```

Example results for `soar search fire`:

```
[-] firefox#mozilla:official (122.0)
[-] firewall#system:official (latest)
[+] firefoxpwa#third-party:flathub (1.0)
```

### Searching by pkg_id

Search by package ID family:

```sh
soar search git.busybox.net
```

### Searching by Provides

Search by alternative binary names:

```sh
soar search batcat
```

This finds `bat` because it provides `batcat` as an alternative name.

## Query Command

The `query` command provides detailed information about a package:

```sh
soar query <package>

# Short alias
soar Q <package>
```

For example:

```sh
soar query bat
```

### Query Syntax

The `query` command supports a detailed syntax for specific lookups:

```sh
soar query <name>#<pkg_id>@<version>:<repo>
```

The components are:

- `<name>` is the package name (required)
- `#<pkg_id>` is the package ID, used for disambiguation (optional)
- `@<version>` is a version constraint (optional)
- `:<repo>` is the repository name (optional)

### Query Output

The query command returns the following fields:

| Field | Description |
|-------|-------------|
| Name | Package name |
| Version | Current or latest version |
| Repository | Source repository |
| pkg_id | Package ID or family |
| Size | Package size on disk |
| Install Date | When the package was installed |
| Last Updated | Last update timestamp |
| Provides | Alternative binary names |
| Description | Package description |

Example output:

```sh
soar query bat

Output:
Name:        bat
Version:     0.24.0
Repository:  official
pkg_id:      catlike.tools.bat.official
Size:        2.3 MiB
Install Date: 2025-01-15
Last Updated: 2025-01-20
Provides:    batcat
Description: A cat clone with syntax highlighting and Git integration
```

## Tips for Effective Searching

Begin with simple queries before adding filters:

```sh
soar search editor
```

Use case sensitivity to disambiguate between similar names:

```sh
soar search Bat --case-sensitive
```

Follow a search with `query` for detailed information:

```sh
soar search bat
soar query bat
```

Limit results for popular terms to keep output readable:

```sh
soar search tool --limit 10
```

Search for known aliases that a package provides:

```sh
soar search batcat
```

Scope the search to a repository when you know it:

```sh
soar search bat:official
```

## Configuration

Search behavior can be configured in Soar's configuration file. See [Configuration](./configuration.md) for details on default search repositories, search result ordering, case sensitivity defaults, and result limit defaults.

## Related Commands

- [Installing Packages](./install.md)
- [Removing Packages](./remove.md)
- [Updating Packages](./update.md)
