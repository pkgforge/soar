---
title: Inspect Packages
description: Investigate packages with the query, inspect, and log commands before and after installation.
---

# Inspect Packages

Soar provides inspection commands that help you understand packages before installation and debug issues afterward. This guide covers the `query`, `inspect`, and `log` commands.

## Overview of Inspection Commands

Soar offers three complementary inspection commands.

| Command | Purpose | Use When |
|---------|---------|----------|
| **`soar query`** | View detailed package metadata | You want comprehensive package information |
| **`soar inspect`** | View build scripts | You need to understand how a package is built |
| **`soar log`** | View build logs | You are debugging installation failures |

---

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
- Package ID (pkg_id)
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
| **Name** | Package name in format `name#pkg_id:repo` |
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
| **Build Log** | Link to build log output |
| **Build Script** | Link to build script used |
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

## Inspect Command

The `inspect` command displays the build script (SBUILD) used to compile or prepare a package. This helps you understand:

- Build dependencies
- Compilation commands
- Installation steps
- Custom build logic

### Basic Usage

```sh
# View build script for a package
soar inspect <package>
```

### How Inspect Works

The `inspect` command:

1. Searches for matching packages by name, pkg_id, or version.
2. Prompts for interactive selection if multiple matches are found.
3. Checks if the package is installed locally and reads from `$INSTALL_DIR/SBUILD`.
4. Fetches from the repository URL if the package is not installed locally.
5. Displays the complete build script.

### Examples

```sh
$ soar inspect ffmpeg

Reading build script from /home/user/.local/share/soar/packages/ffmpeg-7.1 [15.2 MB]

# SBUILD file for ffmpeg
pkg_name="ffmpeg"
pkg_version="7.1"
pkg_source="https://ffmpeg.org/releases/ffmpeg-7.1.tar.xz"

build() {
    ./configure --prefix="$INSTALL_DIR" --enable-gpl
    make -j$(nproc)
    make install
}

dependencies=["nasm", "pkg-config", "libx264-dev"]
```

### Inspect Output Format

Build scripts follow the SBUILD format with these common sections.

| Section | Description |
|---------|-------------|
| **pkg_name** | Package name |
| **pkg_version** | Version string |
| **pkg_source** | Download URL or source location |
| **build()** | Build commands (compilation, installation) |
| **dependencies** | Build dependencies required |

### Interpreting Build Scripts

#### Understanding build commands

```sh
build() {
    # Configure step - sets up build configuration
    ./configure --prefix="$INSTALL_DIR" --enable-feature

    # Compile step - builds the software
    make -j$(nproc)

    # Install step - copies files to install directory
    make install
}
```

#### Available environment variables

Build scripts have access to these variables.

| Variable | Description |
|----------|-------------|
| `$INSTALL_DIR` | Target installation directory |
| `$PKG_NAME` | Package name |
| `$PKG_VERSION` | Package version |
| `$NPROC` | Number of CPU cores (for parallel builds) |

### Use Cases

- **Security audit:** review what commands run during installation.
- **Build debugging:** understand why a package fails to build.
- **Customization:** see whether you can modify build options.
- **Dependency planning:** check build dependencies before installing.
- **Learning:** understand how packages are assembled.

---

## Log Command

The `log` command displays the build log from the last installation attempt. This is invaluable for debugging failed installations.

### Basic Usage

```sh
# View build log for a package
soar log <package>
```

### How Log Works

The `log` command:

1. Searches for matching packages.
2. Prompts for selection if multiple matches are found.
3. Checks if the package is installed locally and reads from `$INSTALL_DIR/<package>.log`.
4. Fetches from the repository URL if the package is not installed locally.
5. Displays the complete build log.

### Examples

```sh
$ soar log bat

[2024-01-15 10:23:45] Starting installation of bat-0.24.0
[2024-01-15 10:23:47] Download complete: 1.8 MB
[2024-01-15 10:23:48] Installation completed successfully
```

```sh
$ soar log python@3.12

[2024-01-15 11:30:12] Starting installation of python@3.12
[2024-01-15 11:31:20] ERROR: Build failed
[2024-01-15 11:31:20] ERROR: openssl/ssl.h: No such file or directory
```

#### View failed installation log

```sh
$ soar log python@3.12

Reading build log from /home/user/.local/share/soar/packages/python@3.12 [25.4 MB]

[2024-01-15 11:30:12] Starting installation of python@3.12
[2024-01-15 11:30:12] Downloading from https://www.python.org/ftp/python/3.12.1/Python-3.12.1.tar.xz
[2024-01-15 11:30:45] Download complete: 25.4 MB
[2024-01-15 11:30:45] Extracting to /tmp/python@3.12
[2024-01-15 11:30:47] Running build commands from SBUILD
[2024-01-15 11:30:48] Executing: ./configure --prefix="$INSTALL_DIR"
...
[2024-01-15 11:31:20] ERROR: Build failed
[2024-01-15 11:31:20] ERROR: openssl/ssl.h: No such file or directory
[2024-01-15 11:31:20] ERROR: Build dependency 'openssl-dev' not found
[2024-01-15 11:31:20] Installation failed
```

#### View log for a specific version

```sh
# Check log for a specific version
soar log "ripgrep@13.0"
```

### Log Output Format

Build logs contain timestamped entries for each installation step.

| Entry Type | Example |
|------------------|---------|
| Start/End markers | `[2024-01-15 10:23:45] Starting installation...` |
| Progress updates | `[2024-01-15 10:23:47] Download complete: 1.8 MB` |
| Success messages | `[2024-01-15 10:23:48] Installation completed successfully` |
| Error messages | `[2024-01-15 11:31:20] ERROR: Build failed` |

### Interpreting Build Logs

#### Common success patterns

```sh
[timestamp] Starting installation of <package>-<version>
[timestamp] Downloading from <url>
[timestamp] Download complete: <size>
[timestamp] Verifying checksum... OK
[timestamp] Extracting to <temp_dir>
[timestamp] Running build commands from SBUILD
[timestamp] Installation completed successfully
```

#### Common failure patterns

```sh
# Missing build dependency
[timestamp] ERROR: <header_file>: No such file or directory
[timestamp] ERROR: Build dependency '<dep>' not found

# Download failure
[timestamp] ERROR: Failed to download from <url>
[timestamp] ERROR: HTTP 404 Not Found

# Checksum mismatch
[timestamp] ERROR: Checksum verification failed
[timestamp] ERROR: Expected <hash1>, got <hash2>

# Build command failure
[timestamp] ERROR: Build failed with exit code 1
[timestamp] ERROR: make: *** No rule to make target 'install'
```

### Use Cases

- **Debug failures:** understand why an installation failed.
- **Verify installation:** confirm all steps completed successfully.
- **Performance analysis:** check how long installation took.
- **Audit trail:** review what happened during installation.
- **Bug reports:** include logs when reporting issues.

---

## Package Query Syntax

All three inspection commands support a flexible package query syntax.

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
soar inspect bat
soar info | grep bat
```

### Debugging failed installations

```sh
soar log python@3.12
soar inspect python@3.12
soar install python@3.12 -vv
```

### Comparing package versions

```sh
soar query "ripgrep@13.0"
soar query "ripgrep@14.0"
soar inspect "ripgrep@13.0"
```

## Tips

- Use `soar query` before installing to verify package details.
- Check build scripts before installing to understand dependencies.
- Always check the build log first when troubleshooting.
- Search for errors with `grep ERROR` on log files.

## Troubleshooting

### No build script or log found

Binary packages do not have build scripts. Build logs only exist after an installation attempt.

### Query returns multiple matches

Use a more specific query such as `soar query python@3.12`, or specify a repository with `soar query python:soarpkgs`.

### Large files prompt for confirmation

Files over 1 MB require confirmation. Use `less` for pagination or save to a file.

For more help, see [Health Check](./health.md).

## See Also

- [Search Packages](./search.md)
- [Install Packages](./install.md)
- [Remove Packages](./remove.md)
