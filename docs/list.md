---
title: List Packages
description: List available packages with soar list and installed packages with soar info, including filters, counts, and output formats.
---

# List Packages

Soar provides two complementary commands for listing packages: one for what is available to install and one for what is already installed. This guide covers both, along with their filters and output formats.

::: info List vs Info
- `soar list` lists **available** packages from repositories, meaning what you *can* install.
- `soar info` lists **installed** packages on your system, meaning what you *have* installed.
:::

## List Available Packages

The `list` command shows all packages available across your configured repositories.

### Basic Usage

```sh
# List all available packages
soar list

# Using the short alias
soar ls
```

### Filter by Repository

To list packages from a specific repository only, pass its name:

```sh
# List packages from the 'soarpkgs' repository
soar list soarpkgs

# List packages from the 'myrepo' repository
soar ls myrepo
```

### Example Output

```sh
$ soar list soarpkgs

[-] 7z#e4d8:soarpkgs | 24.09 | archive
[+] bat#7a3c:soarpkgs | 0.24.0 | cli
[+] curl#9f2d:soarpkgs | 8.11.1 | web
[-] ffmpeg#1b5e:soarpkgs | 7.1 | multimedia
...

┏━━━━━━━━━━━┳━━━━━━━━━━━━━━━┓
┃ Total     ┃ 4            ┃
┃ ━━━━━━━━━━━━━━━━━━━━━━━┃
┃ ✓ Installed ┃ 2            ┃
┃ ○ Available ┃ 2            ┃
┗━━━━━━━━━━━┻━━━━━━━━━━━━━━━┛
```

The output format is `[icon] name#pkg_id:repo | version | type`:

- **icon**: `✓` (or `+`) means installed, `○` (or `-`) means available, depending on whether `display.icons` is enabled
- **name**: Package name (blue)
- **pkg_id**: Package identifier (cyan)
- **repo**: Repository name (cyan)
- **version**: Package version (light red)
- **type**: Package type (magenta, optional)

## List Installed Packages

The `info` command, aliased as `list-installed`, shows all packages currently installed on your system, including their size and installation status. It also reports partially installed packages as `Broken`.

### Basic Usage

```sh
# List all installed packages
soar info

# Using the list-installed alias
soar list-installed
```

Each entry includes the total size used by the package.

### Info Command Options

| Option | Short | Description |
|--------|-------|-------------|
| `--repo-name` | `-r` | Filter installed packages by repository name |
| `--count` | - | Only show the total count of unique installed packages |

### Filter Installed Packages by Repository

To see only packages installed from a specific repository:

```sh
# Show packages installed from 'soarpkgs'
soar info --repo-name soarpkgs

# Using the short option
soar info -r soarpkgs
```

### Count Installed Packages

To get a quick count of installed packages:

```sh
# Show total count of unique packages
soar info --count

# Count packages from a specific repository
soar info --repo-name soarpkgs --count
```

### Example Output

```sh
$ soar info

bat-0.24.0:soarpkgs (2025-01-15) (1.8 MB)
curl-8.11.1:soarpkgs (2025-01-15) (2.4 MB)
ffmpeg-7.1:soarpkgs (2025-01-14) (15.2 MB)
jq-1.7.1:soarpkgs (2025-01-10) (1.5 MB) ✗ Broken

┏━━━━━━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓
┃ ✓ Installed ┃ 3, 3 distinct (20.0 MB)     ┃
┃ ✗ Broken    ┃ 1 (1.5 MB)                  ┃
┃ ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
┃ Total       ┃ 4 (21.5 MB)                 ┃
┗━━━━━━━━━━━━┻━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛
```

The output format is `name-version:repo (date) (size) [status]`:

- **name**: Package name (blue)
- **version**: Package version (magenta)
- **repo**: Repository name (cyan)
- **date**: Installation date (green)
- **size**: Package size on disk (human-readable)
- **status**: Empty for installed packages, `✗ Broken` or `[Broken]` (red) for packages with missing or corrupted files

```sh
$ soar info --count
4
```

## Common Use Cases

### Check Package Status

Use `info` to verify installation status before performing operations:

```sh
# Check if a package is installed
soar info | grep ripgrep

# Check packages from a specific repository
soar info --repo-name soarpkgs | grep ffmpeg
```

### Compare Available vs Installed

```sh
# See all available ffmpeg packages
soar list | grep ffmpeg

# Check if ffmpeg is installed
soar info | grep ffmpeg
```

### Manage Repositories

```sh
# List all packages in a repository before adding it
soar list new-repo

# After installation, verify what was installed
soar info --repo-name new-repo

# Get a quick count of packages from the repository
soar info --repo-name new-repo --count
```

### Clean Up the System

```sh
# Check the total number of packages installed
soar info --count

# Identify broken installations
soar info | grep "Broken"

# Remove broken packages (see remove.md)
soar remove broken-package
```

## See Also

- [Search Packages](./search.md) for searching available packages by query
- [Install Packages](./install.md) for installing packages from repositories
- [Remove Packages](./remove.md) for removing installed packages
- [Configuration](./configuration.md) for configuring repositories
