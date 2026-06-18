---
title: Download Files
description: Download files with Soar from direct URLs, GitHub and GitLab releases, GHCR, or configured repositories, with filtering and automatic extraction.
---

# Download Files

Soar downloads files from direct URLs, GitHub releases, GitLab releases, and GitHub Container Registry (GHCR). The download command supports filtering to narrow down options, interactive asset selection when several matches remain, and automatic archive extraction.

## Basic Usage

To download a file, use the `download` command or its `dl` alias:

```sh
soar download <url>
```

Download Soar nightly:

```sh
soar download https://github.com/pkgforge/soar/releases/download/nightly/soar-nightly-x86_64-linux
```

Set the output filename or directory with the `--output` or `-o` flag:

```sh
soar download https://github.com/pkgforge/soar/releases/download/nightly/soar-nightly-x86_64-linux -o soar-nightly
```

Download multiple files and save them to the `downloads` directory:

```sh
soar download https://github.com/pkgforge/soar/releases/download/nightly/soar-nightly-x86_64-linux https://github.com/pkgforge/soar/releases/download/nightly/soar-nightly-aarch64-linux -o downloads/
```

::: warning
Multiple custom output filenames are not supported. If you specify an output filename, it is used for all downloads, so only the last download is saved when you pass multiple URLs.
:::

## Package Downloads

Download packages directly from configured Soar repositories:

::: code-group

```sh [By name]
soar download neovim
```

```sh [Specific version]
soar download neovim@0.9.5
```

```sh [From a specific repo]
soar download repo/neovim
```

:::

## Download Command Options

### Confirmation Options

| Option | Short | Description |
|--------|-------|-------------|
| `--yes` | `-y` | Skip confirmation prompts (useful for scripts) |

### Output Options

| Option | Short | Description |
|--------|-------|-------------|
| `--output` | `-o` | Set output filename or directory |

### Filtering and Selection Options

| Option | Short | Description |
|--------|-------|-------------|
| `--regex` | `-r` | Filter assets using a regular expression pattern (works with all sources) |
| `--glob` | `-g` | Filter assets using a glob pattern (works with all sources) |
| `--match` | - | Filter assets by keyword substring matching (works with all sources) |
| `--exclude` | - | Exclude assets matching this pattern (works with all sources) |
| `--exact-case` | - | Enable case-sensitive matching (default is case-insensitive) |

::: info Filter behavior
Filters narrow down the available assets. If multiple assets remain after filtering, Soar shows an interactive selection prompt for you to choose manually. Use `--yes` to skip the prompt and automatically download the first matching asset.
:::

### Source Options

| Option | Description |
|--------|-------------|
| `--github` | Download from GitHub releases using format `owner/repo[@tag]` |
| `--gitlab` | Download from GitLab releases using format `owner/project[@tag]` |
| `--ghcr` | Download from GitHub Container Registry using format `owner/image[:tag]` |

### Extraction Options

| Option | Description |
|--------|-------------|
| `--extract` | Automatically extract downloaded archives (`.tar`, `.tar.gz`, `.tgz`, `.tar.xz`, `.txz`, `.tar.bz2`, `.tbz2`, `.tar.zst`, `.zip`, `.7z`) |
| `--extract-dir` | Custom extraction directory (default: current directory or output path) |

### File Handling Options

| Option | Description |
|--------|-------------|
| `--skip-existing` | Skip download if the file already exists |
| `--force-overwrite` | Overwrite existing files without prompting |

## Download Sources

### Direct URLs

Download any file from the internet using a direct URL:

```sh
# Single file
soar download https://example.com/file.tar.gz

# Save to a specific location
soar download https://example.com/file.tar.gz -o /path/to/download/

# With automatic extraction
soar download https://example.com/file.tar.gz --extract
```

::: info URL auto-detection
Soar automatically detects GitHub, GitLab, and GHCR URLs. You do not need the `--github`, `--gitlab`, or `--ghcr` flags when using full URLs.
:::

### GitHub Releases

Download assets from GitHub releases using the `--github` flag:

```sh
# Basic format: owner/repo
soar download --github pkgforge/soar

# Specific tag/release
soar download --github pkgforge/soar@v1.0.0

# Filter by architecture using regex
# Shows interactive selection if multiple assets match 'x86_64-linux'
soar download --github pkgforge/soar --regex 'x86_64-linux'

# Filter by multiple patterns
# Shows interactive selection if both x86_64 and aarch64 matches exist
soar download --github pkgforge/soar --regex 'x86_64-linux|aarch64-linux'

# Use --yes to skip interactive selection and download first match
soar download --github pkgforge/soar --regex 'x86_64-linux' --yes

# Download and extract automatically
soar download --github pkgforge/soar@nightly --regex 'x86_64-linux' --extract

# Exclude certain assets
soar download --github pkgforge/soar --exclude 'musl' --exclude 'debug'
```

::: info Latest release behavior
Without a tag or with `@latest`, Soar downloads the latest stable release and skips prereleases. Use `@tag-name` for specific versions including prereleases.
:::

#### GitHub Asset Selection Examples

```sh
# Download only musl builds
# Interactive: shows all musl builds if multiple match
soar download --github cli/cli --glob '*-musl*'

# Download only .tar.gz files
# Interactive: shows all .tar.gz files if multiple match
soar download --github pkgforge/soar --glob '*.tar.gz'

# Match specific keywords
# Interactive: shows all assets containing 'appimage' if multiple match
soar download --github neovim/neovim --match 'appimage'

# Auto-download first match (skip interactive selection)
soar download --github cli/cli --glob '*-musl*' --yes

# Case-sensitive matching
soar download --github pkgforge/soar --exact-case --regex 'Linux'
```

### GitLab Releases

Download assets from GitLab releases using the `--gitlab` flag:

```sh
# Basic format: owner/project
soar download --gitlab gitlab-org/gitlab

# Specific tag/release
soar download --gitlab gitlab-org/gitlab@v16.0.0

# Filter by platform
soar download --gitlab gitlab-org/gitlab --regex 'linux-amd64'

# Download and extract
soar download --gitlab gitlab-org/gitlab@v16.0.0 --regex 'linux-amd64' --extract
```

::: info Latest release behavior
Without a tag or with `@latest`, Soar downloads the latest stable release and skips prereleases. Use `@tag-name` for specific versions including prereleases.
:::

#### GitLab Asset Selection Examples

```sh
# Download only ARM64 builds
# Interactive: shows all ARM64 builds if multiple match
soar download --gitlab gitlab-org/gitlab --glob '*-arm64*'

# Download specific file types
# Interactive: shows all .tar.xz files if multiple match
soar download --gitlab gitlab-org/gitlab --glob '*.tar.xz'

# Auto-download first match (skip interactive selection)
soar download --gitlab gitlab-org/gitlab --glob '*-arm64*' --yes

# Exclude debug builds
soar download --gitlab gitlab-org/gitlab --exclude 'debug'
```

### GitHub Container Registry (GHCR)

Download container images from GitHub Container Registry:

```sh
# Basic format: owner/image
soar download --ghcr pkgforge/soar

# Specific tag
soar download --ghcr pkgforge/soar:latest

# Specific version
soar download --ghcr pkgforge/soar:v1.0.0

# Multiple tags
soar download --ghcr pkgforge/soar:alpine --ghcr pkgforge/soar:slim
```

::: info Automatic retry
GHCR downloads automatically retry on rate limits (HTTP 429) or network errors, up to 5 attempts with 5-second delays.
:::

## Asset Selection Patterns

### How Asset Selection Works

Soar's filtering works in two stages:

1. **Filtering stage**: Filters (`--regex`, `--glob`, `--match`, `--exclude`) narrow down the list of available assets.
2. **Selection stage**:
   - If exactly one asset matches, it is automatically downloaded.
   - If multiple assets match and `--yes` is not set, an interactive selection prompt appears.
   - If multiple assets match and `--yes` is set, the first matching asset is automatically downloaded.

::: info Key point
Filters reduce the options but do not automatically select which file to download when multiple matches remain. You must choose interactively unless `--yes` is specified.
:::

### Pattern Matching Options

Soar provides three ways to filter assets:

1. **Regex (`--regex` / `-r`)**: Full regular expression support
2. **Glob (`--glob` / `-g`)**: Shell-style glob patterns
3. **Match (`--match`)**: Simple substring matching

### Regex Examples

```sh
# Match specific architecture
# Auto-downloads if exactly one match, otherwise shows selection
soar download --github pkgforge/soar --regex 'x86_64-unknown-linux-gnu'

# Match multiple architectures
# Interactive: shows both x86_64 and aarch64 if both exist
soar download --github pkgforge/soar --regex '(x86_64|aarch64)-linux'

# Match version patterns
soar download --github pkgforge/soar --regex 'v[0-9]+\.[0-9]+\.[0-9]+'

# Match file extensions
# Interactive: shows all .tar.gz and .zip files if multiple match
soar download --github pkgforge/soar --regex '\.(tar\.gz|zip)$'
```

### Glob Examples

```sh
# Match all Linux builds
# Interactive: shows all Linux builds if multiple match
soar download --github pkgforge/soar --glob '*linux*'

# Match specific file type
soar download --github pkgforge/soar --glob '*.tar.gz'

# Match architecture pattern
soar download --github pkgforge/soar --glob '*-x86_64-*'

# Match AppImages
# Auto-downloads if only one AppImage exists
soar download --github neovim/neovim --glob '*.AppImage'
```

### Keyword Match Examples

```sh
# Find assets containing "linux"
# Interactive: shows all assets with "linux" if multiple match
soar download --github pkgforge/soar --match linux

# Find assets containing "musl"
soar download --github pkgforge/soar --match musl

# Multiple keywords (AND logic, all must match)
# Narrows down to assets containing both "linux" AND "x86_64"
soar download --github pkgforge/soar --match linux --match x86_64
```

### Exclusion Patterns

Use `--exclude` to filter out unwanted assets. Combined with other filters, this narrows down the selection pool. If multiple assets remain after exclusions, interactive selection is shown:

```sh
# Exclude musl builds
# Shows selection of non-musl builds if multiple remain
soar download --github pkgforge/soar --exclude 'musl'

# Exclude debug builds
soar download --github pkgforge/soar --exclude 'debug'

# Multiple exclusions
soar download --github pkgforge/soar --exclude 'musl' --exclude 'debug' --exclude 'test'

# Combine with inclusion patterns
# Shows Linux builds excluding musl (interactive if multiple match)
soar download --github pkgforge/soar --regex 'linux' --exclude 'musl'
```

### Case Sensitivity

By default, pattern matching is case-insensitive. Use `--exact-case` for case-sensitive matching:

```sh
# Case-insensitive (default) matches Linux, LINUX, linux
soar download --github pkgforge/soar --match linux

# Case-sensitive only matches lowercase "linux"
soar download --github pkgforge/soar --match linux --exact-case

# Case-sensitive regex
soar download --github pkgforge/soar --regex '[A-Z]{2}' --exact-case
```

## Archive Extraction

Soar can automatically extract downloaded archives.

### Supported Formats

- `.tar`
- `.tar.gz` / `.tgz`
- `.tar.xz` / `.txz`
- `.tar.bz2` / `.tbz2`
- `.tar.zst`
- `.zip`
- `.7z`

Soar detects the format from the file's magic bytes first and falls back to the file extension, so a correctly formatted archive is extracted even if its name uses a different extension.

### Extraction Examples

```sh
# Extract to current directory
soar download https://example.com/file.tar.gz --extract

# Extract to specific directory
soar download https://example.com/file.tar.gz --extract --extract-dir /opt/app

# Download GitHub release and extract
soar download --github pkgforge/soar@v1.0.0 --regex 'x86_64-linux' --extract

# Extract with custom output
soar download https://example.com/file.tar.gz -o myfile.tar.gz --extract --extract-dir ./mydir
```

## File Handling

### Skip Existing Files

When a target file already exists, soar prompts you to choose by default. Pass `--skip-existing` to skip it instead, which is useful when re-running download scripts. An existing file is only skipped when its checksum still matches; a partial download is resumed rather than skipped.

```sh
# Skip files that already exist
soar download https://example.com/file.tar.gz --skip-existing

# Useful for batch downloads
soar download --github pkgforge/soar --regex 'linux' --skip-existing
```

### Force Overwrite

Overwrite existing files without prompting:

```sh
# Force overwrite
soar download https://example.com/file.tar.gz --force-overwrite

# Combine with extraction
soar download https://example.com/file.tar.gz --extract --force-overwrite
```

### Non-Interactive Mode

Skip all confirmation prompts with `--yes`. When multiple assets match your filters, `--yes` automatically downloads the first matching asset instead of showing an interactive selection:

```sh
# Download without confirmation
soar download https://example.com/file.tar.gz --yes

# Batch download with all options
# Skips interactive selection and downloads first match
soar download --github pkgforge/soar --regex 'linux' --extract --yes --skip-existing

# When multiple assets match, --yes picks the first one automatically
soar download --github cli/cli --regex 'linux' --yes
```

### Interactive Selection

When multiple assets match your filters and `--yes` is not set, Soar displays an interactive selection prompt:

```sh
soar download --github cli/cli --regex 'linux'
# > Select an asset (1-4):
#   1. cli-linux-amd64.rpm (95.2 MB)
#   2. cli-linux-arm64.deb (89.1 MB)
#   3. cli-linux-386.tar.gz (91.5 MB)
#   4. cli-linux-amd64.tar.gz (95.2 MB)
```

The selection works as follows:

- **Exactly 1 match**: Automatically downloads without prompting
- **Multiple matches without `--yes`**: Shows the interactive prompt above
- **Multiple matches with `--yes`**: Automatically downloads the first matching asset

::: tip Automatic versus interactive
Use filters to narrow down options. If only one asset remains after filtering, it downloads automatically. If multiple remain, you are prompted to choose unless you use `--yes` to accept the first match.
:::

## Advanced Examples

### Multi-Architecture Download

```sh
# Download multiple architectures
# Shows interactive selection if both x86_64 and aarch64 builds exist
soar download --github pkgforge/soar --regex '(x86_64|aarch64)-linux' -o ./binaries/

# Auto-download first architecture match
soar download --github pkgforge/soar --regex '(x86_64|aarch64)-linux' -o ./binaries/ --yes
```

### Latest Release from Multiple Sources

::: code-group

```sh [GitHub]
soar download --github pkgforge/soar@latest --regex 'linux' --extract
```

```sh [GitLab]
soar download --gitlab gitlab-org/gitlab@latest --regex 'amd64' --extract
```

:::

### Filter and Extract Workflow

```sh
# Download, filter, and extract in one command
soar download --github neovim/neovim \
  --glob 'nvim-linux64.tar.gz' \
  --extract \
  --extract-dir ./nvim \
  --output nvim.tar.gz
```

### Batch Downloads with Exclusions

```sh
# Download all Linux builds except musl
# Shows interactive selection if multiple non-musl Linux builds remain
soar download --github pkgforge/soar \
  --regex 'linux' \
  --exclude 'musl' \
  --exclude 'debug' \
  -o ./builds/

# Auto-download first match
soar download --github pkgforge/soar \
  --regex 'linux' \
  --exclude 'musl' \
  --exclude 'debug' \
  -o ./builds/ \
  --yes
```

### Container Image Management

```sh
# Pull multiple image variants
soar download --ghcr pkgforge/soar:alpine
soar download --ghcr pkgforge/soar:slim
soar download --ghcr pkgforge/soar:latest
```

## Tips and Best Practices

1. **Use `--glob` for simple patterns**: Easier to read than regex for common cases
2. **Combine filters**: Use both inclusion (`--regex` / `--glob`) and exclusion (`--exclude`) for precise control
3. **Understand selection behavior**: Filters narrow down options, but if multiple assets remain you are prompted to choose, or use `--yes` for automatic first-match download
4. **Test patterns first**: Use `--yes` only after verifying your filters work correctly
5. **Extraction paths**: Use `--extract-dir` to keep downloads organized
6. **Batch operations**: `--skip-existing` is essential for re-running download scripts
7. **Case sensitivity**: Most patterns are case-insensitive by default, so use `--exact-case` when needed
8. **Asset names**: GitHub and GitLab asset names typically include architecture, OS, and file type
9. **Latest releases**: Use `@latest` or omit the tag to get the newest stable release, since prereleases are skipped
10. **Filter universality**: `--regex`, `--glob`, `--match`, and `--exclude` work with all sources (GitHub, GitLab, GHCR)
11. **URL auto-detection**: Full URLs are automatically detected, so the `--github`, `--gitlab`, and `--ghcr` flags are not needed
12. **Package downloads**: Download packages directly by name from configured repositories
