---
title: Declarative Package Management
description: Define packages in packages.toml and apply them in one command to keep consistent environments across machines.
---

# Declarative Package Management

Soar supports declarative package management through `packages.toml`. Define your desired packages and apply them in one command to maintain consistent environments across machines.

This page covers the file structure, every package field, advanced features such as hooks, builds, sandboxing, and portable mode, and the commands used to apply your configuration.

## Configuration File

Create `packages.toml` at `~/.config/soar/packages.toml`:

```sh
soar defpackages
```

## File Structure

The `packages.toml` file supports two top-level sections:

- **`[defaults]`** - Default settings applied to all packages
- **`[packages]`** - Package specifications

```toml
[defaults]
profile = "default"
binary_only = false
install_patterns = ["!*.log", "!SBUILD"]

[packages]
# Package specifications go here
```

## Defaults Section

The `[defaults]` section sets default values that apply to all packages unless overridden.

| Field | Type | Description |
|-------|------|-------------|
| `profile` | String | Default profile to install packages to |
| `binary_only` | Boolean | Only extract binaries, skip other files |
| `install_patterns` | Array of Strings | File patterns to include/exclude during installation |
| `sandbox` | Object | Default sandbox configuration applied to all packages. Per-package `sandbox` blocks override individual fields; `fs_read`/`fs_write` lists are concatenated with these defaults |

## Packages Section

The `[packages]` section contains package specifications in multiple formats.

### Simple String Format

```toml
[packages]
# Latest version (never pinned)
bat = "*"

# Specific version (auto-pinned for repo packages)
ripgrep = "14.1.0"

# Using table format (auto-pinned for repo packages)
fd = { version = "9.0.0" }

# Remote packages are not auto-pinned
remote-tool = { url = "https://example.com/tool.tar.gz" }
```

### Version Resolution and Pinning

| Spec Format | Version | Pinned | Notes |
|-------------|---------|--------|-------|
| `pkg = "*"` | Latest | No | Always installs latest version |
| `pkg = "1.2.3"` | 1.2.3 | Yes (repo only) | Pinned for repository packages |
| `pkg = { version = "1.2.3" }` | 1.2.3 | Yes (repo only) | Pinned for repository packages |
| `pkg = { url = "..." }` | Detected | No | Remote packages not auto-pinned |
| `pkg = { version = "1.2.3", pinned = false }` | 1.2.3 | Yes | `pinned = false` does **not** unpin a versioned repo package |
| `pkg = { url = "...", pinned = true }` | Detected | Yes | Explicitly pin remote package |

**Key points:**
- Repository packages with a specific version are always pinned. Setting `pinned = false` does not override this. A versioned non-remote package stays pinned.
- Remote packages (url/github/gitlab) are never auto-pinned unless you explicitly set `pinned = true`
- Pinned packages are skipped during auto-update operations
- Version `*` always resolves to latest and is never pinned
- After installing a package with version `"*"`, soar updates your `packages.toml` with the specific version installed

### Detailed Format

```toml
[packages.my_package]
pkg_id = "pkg-bin"
repo = "soarpkgs"
version = "1.0.0"
pinned = true
portable = { home = "~/.pkg", config = "~/.pkg/config" }
```

## Package Options Reference

| Field | Type | Description |
|-------|------|-------------|
| `version` | String | Package version to install (`"*"` for latest) |
| `pkg_id` | String | Package variant/family identifier |
| `repo` | String | Install from a specific repository |
| `url` | String | Install directly from a URL |
| `bsum` | String | Expected BLAKE3 checksum (hex) for `url`/`github`/`gitlab` downloads; install aborts on mismatch |
| `pinned` | Boolean | Prevent automatic updates (default: `false`) |
| `profile` | String | Install to a specific profile |
| `github` | String | GitHub repo in `owner/repo` format |
| `gitlab` | String | GitLab repo in `owner/repo` format |
| `asset_pattern` | String | Glob pattern to match release assets |
| `tag_pattern` | String | Glob pattern to match release tags |
| `include_prerelease` | Boolean | Include pre-release versions |
| `version_command` | String | Custom command to fetch latest version and download URL |
| `binary_only` | Boolean | Only extract binaries, skip other files |
| `binaries` | Array | Map multiple binaries to custom names (see [Binary Mappings](#binary-mappings)) |
| `install_patterns` | Array | File patterns to include/exclude |
| `portable` | Object | Configure portable directories (see [Portable](#portable)) |
| `hooks` | Object | Lifecycle hooks (see [Hooks](#hooks)) |
| `build` | Object | Build from source (see [Build From Source](#build-from-source)) |
| `sandbox` | Object | Security sandbox (see [Sandbox](#sandbox)) |
| `pkg_type` | String | Override package type detection |
| `entrypoint` | String | Entry point executable name |
| `nested_extract` | String | Path to nested archive to extract |
| `extract_root` | String | Subdirectory to treat as root |
| `arch_map` | Object | Map standard architecture names to custom values used by the package source (see [Architecture Mapping](#architecture-mapping)) |

### Binary Mappings

Map multiple executables within a package to custom symlink names. The `source` field supports glob patterns to match multiple files at once.

| Field | Type | Description |
|-------|------|-------------|
| `source` | String | Path or glob pattern to match executables within package |
| `link_as` | String | Custom symlink name (optional, defaults to the source filename) |

When a glob matches multiple files, each is symlinked using its original filename. `link_as` is only used when a single file matches.

### Hooks

Execute commands at various stages of the package lifecycle:

| Hook | Description |
|------|-------------|
| `post_download` | Run after download, before extraction |
| `post_extract` | Run after extraction |
| `post_install` | Run after symlinks created |
| `pre_remove` | Run before package removal |

**Available environment variables:** `$INSTALL_DIR`, `$BIN_DIR`, `$PKG_NAME`, `$PKG_ID`, `$PKG_VERSION`

```toml
[packages.myapp]
url = "https://example.com/myapp-1.0.0.tar.gz"
hooks = { post_install = "myapp --init" }
```

### Build From Source

| Field | Type | Description |
|-------|------|-------------|
| `commands` | Array | Shell commands to run sequentially |
| `dependencies` | Array | Required tools (checked via `which`) |

**Available environment variables:** `$INSTALL_DIR`, `$BIN_DIR`, `$PKG_NAME`, `$PKG_ID`, `$PKG_VERSION`, `$NPROC`

```toml
[packages.custom-tool]
url = "https://example.com/tool-1.0.0.tar.gz"

[packages.custom-tool.build]
commands = ["make -j$NPROC", "make install PREFIX=$INSTALL_DIR"]
dependencies = ["gcc", "make"]
```

### Sandbox

Restrict filesystem and network access for hooks and build commands using Linux's Landlock LSM (kernel 5.13+).

| Field | Type | Description |
|-------|------|-------------|
| `enabled` | Boolean | Whether sandboxing is enabled (default: `true` when Landlock is available). Set to `false` to skip sandboxing entirely (overrides `require`) |
| `require` | Boolean | Fail if Landlock is unavailable instead of falling back to unsandboxed execution (default: `false`) |
| `fs_read` | Array | Additional readable paths (beyond defaults like `/usr`, `/lib`) |
| `fs_write` | Array | Additional writable paths (beyond install dir and `/tmp`) |
| `network` | Boolean | Allow network access (requires Landlock V4+, kernel 6.7+) |

```toml
[packages.untrusted-tool]
url = "https://example.com/tool-1.0.0.tar.gz"
sandbox = { require = true, network = false }
```

### Portable

Configure portable mode for AppImage, FlatImage, RunImage, and Wrappe packages. Creates symlinks from expected data directories to custom locations.

| Field | Type | Description |
|-------|------|-------------|
| `path` | String | Sets both `home` and `config` to the same path |
| `home` | String | Portable home directory |
| `config` | String | Portable config directory |
| `share` | String | Portable share directory |
| `cache` | String | Portable cache directory |

**Format support:** AppImage/RunImage (all fields), FlatImage (only `config`), Wrappe (only `path`)

```toml
[packages.obsidian]
url = "https://example.com/obsidian.AppImage"
portable = { path = "~/.obsidian-data" }
```

### Architecture Mapping

`arch_map` is a per-package field that maps standard architecture names to the
custom values used by a package source. The keys are the bare architecture
names reported by the running system (`x86_64` or `aarch64`), and the values
are whatever the source uses (e.g. `amd64`, `arm64`, `x64`). The mapped value
is substituted for the `{arch}` placeholder in `url` and `asset_pattern`.

```toml
[packages.mytool]
url = "https://example.com/mytool-{version}-{arch}.tar.gz"

[packages.mytool.arch_map]
x86_64 = "amd64"
aarch64 = "arm64"
```

On an `x86_64` machine this resolves `{arch}` to `amd64`. If no mapping is
provided for the current architecture, the bare name (`x86_64`/`aarch64`) is
used as-is. The `{os}` placeholder resolves to the operating system (e.g.
`linux`), and `{version}` resolves to the package version with any leading
`v` stripped.

## GitHub/GitLab Integration

Install packages directly from GitHub or GitLab releases:

```toml
[packages]
# From GitHub releases
gh-release = { github = "cli/cli" }

# With asset pattern
gh-release = {
  github = "cli/cli",
  asset_pattern = "gh_*_linux_amd64.tar.gz"
}

# With tag pattern
gh-beta = {
  github = "owner/repo",
  tag_pattern = "v2*",
  include_prerelease = true
}

# From GitLab
gl-release = { gitlab = "gitlab-org/gitlab" }
```

::: warning Glob patterns, not regex
`asset_pattern` and `tag_pattern` use **glob patterns**, not regex. Supported patterns include:
- `*` matches any sequence of characters
- `?` matches any single character
- `[abc]` matches any character in the set
- `[!abc]` matches any character not in the set
:::

## Version Command

The `version_command` field is used for custom URL packages to detect what version is available from a remote source BEFORE installation. This tells soar what version it would download if it were to install the package.

### When It Runs

The `version_command` runs BEFORE installation for custom URL packages to:

1. **Detect remote version**: Query the remote source to find what version is available
2. **Generate download URL**: Optionally provide the exact download URL for that version
3. **Report size**: Optionally provide the download size for progress display

This is primarily useful for custom URL packages where the download URL needs the version number substituted, but the version isn't embedded in the URL you configure.

### Output Format

The `version_command` must output exactly 3 lines:

```
<remote_version>
<download_url_for_that_version>
<size_in_bytes>
```

- **Line 1** (required): The version available at the remote source (e.g., `1.2.3` or `v1.2.3`)
- **Line 2** (optional): Download URL for that version. If omitted or empty, soar uses the `url` field with `{version}` placeholder substituted
- **Line 3** (optional): Size of download in bytes for progress display

### When to Use version_command

Use `version_command` for custom URL packages when:

1. **Your URL uses `{version}` placeholder** but you need to dynamically discover what version is available
2. **The remote source has a version API or endpoint** you can query
3. **You need to parse a download page** to extract version information
4. **You want to provide a custom download URL** that differs from the template

::: info
For GitHub/GitLab packages, soar already handles version detection automatically. You typically don't need `version_command` unless you have special requirements.
:::

### Examples

**Query GitHub API for latest release:**

```toml
[packages.my-tool]
url = "https://example.com/downloads/my-tool-{version}.tar.gz"
version_command = """
  curl -s https://api.github.com/repos/owner/my-tool/releases/latest | \\
  jq -r '.tag_name'
"""
```

This fetches the latest release tag from GitHub and outputs it. Soar then uses that version to substitute into the `url` field.

**Query a version.json endpoint:**

```toml
[packages.api-tool]
url = "https://example.com/downloads/tool-{version}.zip"
version_command = """
  VERSION=$(curl -s https://example.com/api/tool/latest | jq -r '.version')
  echo "$VERSION"
  echo "https://example.com/downloads/tool-$VERSION.zip"
  echo "$(curl -sI https://example.com/downloads/tool-$VERSION.zip | grep -i content-length | awk '{print $2}' | tr -d '\\r')"
"""
```

This queries a version.json API endpoint, extracts the version, constructs the download URL, and even fetches the file size.

**Parse a download page for version:**

```toml
[packages.scrape-tool]
url = "https://example.com/downloads/tool-{version}.tar.gz"
version_command = """
  curl -s https://example.com/downloads.html | \\
  grep -oP 'tool-\\d+\\.\\d+\\.\\d+\\.tar\\.gz' | \\
  sed 's/tool-//' | sed 's/.tar.gz//' | \\
  sort -V | tail -n1
"""
```

This scrapes the downloads page, extracts version numbers from filenames, and returns the latest one.

**Strip version prefix:**

```toml
[packages.custom-tool]
url = "https://example.com/{version}/download.tar.gz"
# Remote returns "v1.2.3" but we want "1.2.3"
version_command = """
  VERSION=$(curl -s https://example.com/latest | grep -oP 'v\\d+\\.\\d+\\.\\d+')
  echo "${VERSION#v}"  # Strip 'v' prefix
"""
```

### Best Practices

1. **Use jq for JSON APIs**: More reliable than grep for parsing
    ```toml
    version_command = "curl -s https://api.example.com/latest | jq -r '.version'"
    ```

2. **Handle missing API endpoints gracefully**: Return a fallback version
    ```toml
    version_command = "curl -s https://example.com/version.txt || echo '0.0.0'"
    ```

3. **Use -s flag with curl**: Suppress progress bars for clean output
    ```toml
    version_command = "curl -s https://example.com/latest-version"
    ```

4. **Test your command manually**: Verify it outputs exactly 3 lines
    ```sh
    curl -s https://api.github.com/repos/owner/repo/releases/latest | jq -r '.tag_name'
    echo ""
    echo ""
    ```

5. **Consider rate limiting**: Add delays or respect rate limits when querying APIs frequently

## Simple vs Detailed Configuration

### When to Use Simple Version Strings

Simple strings are ideal for straightforward package installations:

```toml
[packages]
# Latest version from default repository
bat = "*"

# Specific version (auto-pinned for repo packages)
ripgrep = "14.1.0"

# Inline table for minor customization
fd = { version = "9.0.0", repo = "soarpkgs" }
```

**Use simple strings when:**
- Installing from the default repository with standard settings
- You only need to specify version and optionally repo/profile
- No custom asset patterns, hooks, or build configuration needed
- Quick one-line configuration is sufficient

### When to Use Detailed Tables

Full table format is needed for advanced configurations:

```toml
[packages.myapp]
github = "owner/repo"
asset_pattern = "myapp_*_linux_amd64.tar.gz"
hooks = { post_install = "myapp --init" }
pinned = true
```

**Use detailed tables when you need:**
- GitHub/GitLab releases with `asset_pattern`, `tag_pattern`, or `include_prerelease`
- Direct URL installation with `url`
- Version fetching via `version_command` for URL packages
- Build from source with `build` commands and dependencies
- Multiple binary mappings with `binaries`
- Lifecycle hooks (`post_install`, `pre_remove`, etc.)
- Sandbox restrictions for untrusted packages
- Portable mode for AppImage/FlatImage/RunImage/Wrappe
- Custom install patterns or `binary_only`
- Explicit pinning control (especially for URL packages)

### Decision Guide

| Requirement | Format | Example |
|-------------|--------|---------|
| Latest version from repo | Simple string | `pkg = "*"` |
| Specific version from repo | Simple string | `pkg = "1.2.3"` |
| From custom repository | Inline table | `pkg = { version = "*", repo = "custom" }` |
| GitHub/GitLab releases | Full table | See GitHub/GitLab sections above |
| Direct URL download | Full table | `pkg = { url = "https://..." }` |
| Build from source | Full table | See BuildConfig section |
| Multiple binaries | Full table | See BinaryMapping section |
| Post-install setup | Full table | `pkg = { version = "1.0", hooks = {...} }` |
| Security sandboxing | Full table | `pkg = { version = "1.0", sandbox = {...} }` |

## Complete Examples

::: code-group

```toml [Minimal]
[packages]
bat = "*"
ripgrep = "*"
soar = "0.5.2"
7z = { repo = "soarpkgs" }
```

```toml [Advanced]
[defaults]
profile = "default"
binary_only = false
install_patterns = ["!*.log", "!SBUILD"]

[packages]
bat = "*"

[packages.obsidian]
version = "1.5.0"
pinned = true
portable = { home = "~/.obsidian-data" }

[packages.gh-cli]
github = "cli/cli"
asset_pattern = "gh_*_linux_amd64.tar.gz"

[packages.custom-tool]
url = "https://example.com/tool-1.0.0.tar.gz"

[packages.custom-tool.build]
commands = ["make -j$NPROC", "make install PREFIX=$INSTALL_DIR"]
dependencies = ["gcc", "make"]

[packages.untrusted-tool]
url = "https://example.com/tool-1.0.0.tar.gz"
hooks = { post_install = "$INSTALL_DIR/setup.sh" }
sandbox = { require = true, network = false }
```

:::

## Full Reference Example

The configuration below uses every available option at least once. Because several options are alternatives to one another, the example spreads them across multiple packages rather than forcing them into a single entry.

::: info Pick one source per package
Each package draws from a single source: a registry entry (`pkg_id` and `repo`), a direct `url`, a `github` repo, a `gitlab` repo, or a `version_command`. The source-specific fields follow from that choice, so treat this as a field reference rather than a template to copy verbatim.
:::

```toml
[defaults]
profile = "default"                          # default profile for every package
binary_only = false                          # extract everything, not just the binary
install_patterns = ["!*.log", "!SBUILD"]     # default include/exclude globs

# Sandbox defaults are merged into every package. Per-package sandbox blocks
# override these fields, and fs_read/fs_write are concatenated.
[defaults.sandbox]
enabled = true                               # sandbox hooks and builds when Landlock is available
require = false                              # do not fail when Landlock is unavailable
network = false                              # block outbound network access
fs_read = ["/etc/ssl"]                       # extra readable paths
fs_write = ["/tmp/soar-build"]               # extra writable paths

# Registry package pinned to a specific variant, version, and repository.
[packages.bat]
pkg_id = "catlike.tools.bat.official"        # disambiguate packages that share a name
version = "0.24.0"                           # specific version ("*" means latest)
repo = "soarpkgs"                            # install from this repository
pinned = true                                # never update automatically
profile = "default"                          # override the default profile
binary_only = true                           # only extract the binary
install_patterns = ["!*.1"]                  # override the default patterns

# GitHub release, selecting the asset, tag, and a remapped architecture.
[packages.gh-cli]
github = "cli/cli"                           # owner/repo
asset_pattern = "gh_*_linux_amd64.tar.gz"    # glob that selects the asset
tag_pattern = "v*"                           # glob that selects the release tag
include_prerelease = false                   # skip pre-releases
extract_root = "gh_2.40.0_linux_amd64"       # treat this subdirectory as the archive root
arch_map = { x86_64 = "amd64", aarch64 = "arm64" }  # remap arch names for the source

# GitLab release with a checksum pin.
[packages.glab]
gitlab = "gitlab-org/cli"
asset_pattern = "*linux_amd64.tar.gz"
bsum = "9f2d...hex..."                        # BLAKE3 checksum; install aborts on mismatch

# Direct URL install with custom type, entrypoint, nested archive, and binaries.
[packages.custom-tool]
url = "https://example.com/tool-1.0.0.tar.gz"  # direct download (a "local" package)
bsum = "abc1...hex..."                        # verify the download
pkg_type = "archive"                         # override package-type detection
entrypoint = "tool"                          # executable name inside the package
nested_extract = "inner/payload.tar.gz"      # extract an archive nested inside the archive
binaries = [                                 # map several executables to symlink names
  { source = "bin/tool", link_as = "tool" },
  { source = "bin/toolctl", link_as = "tc" },
]

# AppImage kept self-contained with portable directories. Portable dirs apply
# only to AppImage, RunImage, FlatImage, and Wrappe packages (FlatImage honors
# only config, Wrappe only path); archives and plain binaries ignore them.
[packages.obsidian]
url = "https://example.com/Obsidian-1.5.0.AppImage"  # an AppImage source
pkg_type = "appimage"

[packages.obsidian.portable]
path = "~/.obsidian"                         # base path shortcut (sets home and config)
home = "~/.obsidian/home"                    # portable HOME (overrides path)
config = "~/.obsidian/config"                # portable config dir (overrides path)
share = "~/.obsidian/share"                  # portable data dir
cache = "~/.obsidian/cache"                  # portable cache dir

# Resolve version and download URL with a custom command, plus lifecycle hooks.
[packages.nightly-app]
version_command = "curl -s https://example.com/latest"  # line 1: version, line 2: URL, line 3: size (optional)

[packages.nightly-app.hooks]
post_download = "echo downloaded"            # after download, before extraction
post_extract = "echo extracted"              # after extraction
post_install = "$INSTALL_DIR/setup.sh"       # after symlinks are created
pre_remove = "echo removing"                 # before the package is removed

# Build from source with a per-package sandbox override.
[packages.from-source]
url = "https://example.com/src-1.0.0.tar.gz"

[packages.from-source.build]
commands = ["make -j$NPROC", "make install PREFIX=$INSTALL_DIR"]  # env: $INSTALL_DIR $PKG_NAME $PKG_VERSION $NPROC
dependencies = ["gcc", "make"]               # soar warns if these are missing from PATH

[packages.from-source.sandbox]
enabled = true
require = true                               # fail if Landlock is unavailable
network = true                               # allow network access (needs kernel 6.7+)
fs_read = ["/opt/sdk"]
fs_write = ["/tmp/build"]
```

## Applying Packages

### Basic Apply

To install all packages defined in your `packages.toml`:

```sh
soar apply
```

### Apply Options

| Option | Flag | Description |
|--------|------|-------------|
| Prune | `--prune` | Remove packages not listed in packages.toml |
| Dry run | `--dry-run` | Show what would be done without making changes |
| Yes | `--yes` | Auto-confirm all prompts |
| Config | `--packages <path>` | Use custom packages.toml path |
| No verify | `--no-verify` | Skip checksum verification (security risk) |

### Pruning Unlisted Packages

To also remove packages that are not listed in your `packages.toml`:

```sh
soar apply --prune
```

::: warning
The `--prune` flag will remove any installed packages not defined in your `packages.toml`. Make sure your configuration includes all packages you want to keep.
:::

### Dry Run

Preview what would be installed without making changes:

```sh
soar apply --dry-run
```

### Custom Config File

Use a packages.toml from a custom location:

```sh
soar apply --packages /path/to/custom-packages.toml
```

## Defpackages Command

Generate a template `packages.toml` with examples:

```sh
soar defpackages
```

This creates `~/.config/soar/packages.toml` with commented examples showing all available options.

## Environment Override

You can override the default packages.toml path using the `SOAR_PACKAGES_CONFIG` environment variable:

```sh
export SOAR_PACKAGES_CONFIG=/path/to/my-packages.toml
soar apply
```

## Best Practices

- **Version Pinning**: Pin versions for production tools, use `*` for development
- **Profiles**: Set default profile in `[defaults]`, override per-package if needed
- **Portable Mode**: Use for AppImage/FlatImage/RunImage/Wrappe packages to keep data self-contained
- **Hooks**: Use `post_install` for setup, `pre_remove` for cleanup
- **Sandbox**: Enable for untrusted tools to restrict filesystem and network access
- **Dry Run**: Always run `soar apply --dry-run` to verify changes
- **Version Commands**: Use `version_command` for URL packages to enable automatic updates

For troubleshooting, see [Health Check](./health.md)
