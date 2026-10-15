---
title: Inspect Packages
description: Investigate packages with soar query before and after installation.
---

# Inspect Packages

The `query` command shows everything a repository says about a package, so you can check it before installing and confirm what you got afterward.

## Query Command

The `query` command provides detailed metadata about packages, including versions, sizes, dependencies, and repository information.

### Basic Usage

```sh
# Query a package by name
soar query <package>

# Using the short alias
soar Q <package>
```

### How Query Works

The `query` command searches for packages and displays detailed information in a formatted table. It checks:

- Package name
- Family and package id, where the repository publishes them
- Version numbers
- Repository sources

### Examples

```sh
$ soar query bat

✓ Name            bat#cat:soarpkgs
✓ Description     A cat(1) clone with wings
✓ Version         0.24.0
✓ Size            1.8 MB
✓ Checksum        abc123... (blake3)
✓ Type            binary
```

### Query Output Format

The query command displays the following information.

| Field | Description |
|-------|-------------|
| **Name** | Package name in format `family/name:repo`, with the family omitted when the repository publishes none |
| **Description** | Human-readable package description |
| **Version** | Current package version |
| **Size** | Download size (formatted for readability) |
| **Checksum** | BLAKE3 checksum for download verification |
| **Homepages** | Official project websites |
| **Licenses** | Package license information |
| **Maintainers** | Package maintainer contact information |
| **Notes** | Additional installation or usage notes |
| **Type** | Package type (appimage, flatimage, archive, etc.) |
| **Build CI** | Build action and build ID |
| **Build Date** | When the package was built |
| **GHCR Blob** | GitHub Container Registry blob URL (if available) |
| **Download URL** | Direct download URL (shown if GHCR Blob not available) |
| **GHCR Package** | Full GHCR package URL |
| **Index** | Package index page URL |

::: info Optional fields
Some fields are optional and may not appear if they are not available for the package. The download information shows either GHCR Blob or Download URL depending on the package source.
:::

### Use Cases

- **Before installation:** verify package details before installing.
- **Version comparison:** check what versions are available.
- **Repository verification:** confirm which repository provides a package.
- **Size planning:** check package size for disk space planning.

---

## Package Query Syntax

The `query` command accepts a flexible package query syntax.

### Query Formats

```sh
# By name only
soar query bat

# By package ID (includes version variants)
soar query python@3.12

# By specific version
soar query "ripgrep@14.0"

# By repository (using colon syntax)
soar query bat:soarpkgs
```

### Query Components

| Component | Format | Example | Description |
|-----------|--------|---------|-------------|
| **Name** | `package` | `bat` | Package name |
| **Package ID** | `package@version` | `python@3.12` | Specific variant |
| **Version** | `package@version` | `ripgrep@14.0` | Exact version |
| **Repository** | `package:repo` | `bat:soarpkgs` | Source repository |

### Interactive Selection

When multiple packages match your query, Soar prompts for selection.

```sh
$ soar query python

Multiple packages found. Select one:
  1) python@3.12 (soarpkgs) - Python 3.12.1
  2) python@3.11 (soarpkgs) - Python 3.11.8
  3) python@3.10 (soarpkgs) - Python 3.10.13

Enter selection [1-3]: 1
```

---

## Common Workflows

### Pre-installation investigation

```sh
soar query bat
soar info | grep bat
```

### Debugging failed installations

```sh
soar query python@3.12
soar install python@3.12 -vv
```

### Comparing package versions

```sh
soar query "ripgrep@13.0"
soar query "ripgrep@14.0"
```

## Tips

- Use `soar query` before installing to verify package details.
- Run an install with `-vv` to see each step when something fails.

## Troubleshooting

### Query returns multiple matches

Use a more specific query such as `soar query python@3.12`, or specify a repository with `soar query python:soarpkgs`.

For more help, see [Health Check](./health.md).

## See Also

- [Search Packages](./search.md)
- [Install Packages](./install.md)
- [Remove Packages](./remove.md)
