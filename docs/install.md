---
title: Installing Packages
description: Install packages with soar from repositories, specific pkg_ids, repositories, or direct URLs, with options for portable, binary-only, and non-interactive installs.
---

# Installing Packages

Soar offers several flexible ways to install packages. This guide covers every installation method, the supported input types, and the flags that control how packages are fetched and set up.

## Installation Flow

`soar install` accepts several forms of input and resolves each one differently:

| Input | Example | How soar resolves it |
|-------|---------|----------------------|
| Package name | `soar install bat` | Searches all synced repositories. If several packages match, soar prompts you to choose, or selects the first match when `--yes` is set. |
| `name#pkg_id` | `soar install cat#git.busybox.net.busybox.standalone.glibc` | Searches a specific package family. |
| `name:repo` | `soar install 7z:soarpkgs` | Searches a specific repository. |
| `name@version` | `soar install soar@0.5.2` | Pins the package at a specific version. |
| URL | `soar install https://example.com/app.AppImage` | Downloads and installs directly from the URL. |

Once the input resolves to a package, soar downloads and installs it. The sections below cover each input type in detail.

## Basic Installation

To install a package, use the `install` command or one of its aliases.

::: code-group

```sh [install]
soar install <package>
```

```sh [i]
soar i <package>
```

```sh [add]
soar add <package>
```

:::

Example: install the `soar` package.

```sh
soar add soar
```

## Input Types

A single argument to `soar add` can take several forms, each resolving the package differently.

### Package name

The plain name is searched across all synced repositories.

```sh
soar add bat
```

### Specific pkg_id

Packages can be organized into a pkg_id (like a family). Append `#<pkg_id>` to install from a specific one.

```sh
soar add <package>#<pkg_id>
```

Example: install the `cat` package from the `git.busybox.net.busybox.standalone.glibc` pkg_id.

```sh
soar add cat#git.busybox.net.busybox.standalone.glibc
```

### Specific repository

Append `:<repository_name>` to install from a specific repository.

```sh
soar add <package>:<repository_name>
```

Example: install the `7z` package from the `soarpkgs` repository.

```sh
soar add 7z:soarpkgs
```

### Pinned version

Append `@<version>` to pin the package at a specific version.

```sh
soar add <package>@<version>
```

Example: install the `soar` package and pin it at version `0.5.2`.

```sh
soar add soar@0.5.2
```

::: warning No unpin yet
Currently there is no way to unpin the package. This will be introduced gradually.
:::

### URL

You can install packages directly from a URL.

```sh
soar add <url>
```

Example: install an AppImage from a URL.

```sh
soar add https://example.com/releases/myapp-1.0.0.appimage
```

When installing from a URL, Soar attempts to automatically detect package metadata. You can override this behavior with the following flags.

| Flag | Description |
|------|-------------|
| `--name` | Override the package name |
| `--version` | Override the version |
| `--pkg-type` | Override the package type (e.g., appimage, flatimage, archive) |
| `--pkg-id` | Override the package ID |
| `--binary-only` | Install only binaries, skip other files |
| `--no-verify` | Skip checksum and signature verification |
| `--portable [DIR]` | Set portable dir for home & config (optional value) |
| `--portable-home [DIR]` | Set custom home directory (optional value) |
| `--portable-config [DIR]` | Set custom config directory (optional value) |
| `--portable-share [DIR]` | Set custom share directory (optional value) |
| `--portable-cache [DIR]` | Set custom cache directory (optional value) |
| `--show` | Show all available variants for interactive selection |

Basic example:

```sh
soar add https://example.com/app.appimage --name myapp --version 2.0.0
```

Portable installation:

```sh
soar add https://example.com/app.AppImage \
  --name myapp \
  --portable-home ~/myapp
```

## Installing Multiple Packages

List several packages after the command to install them together.

```sh
soar add <package1> <package2> <package3>
```

Example: install the `bat` and `7z` packages.

```sh
soar add bat 7z
```

## Installing Whole pkg_ids

To install every package provided by a pkg_id, prefix the pkg_id with `#`.

```sh
soar add '#git.busybox.net.busybox.standalone.glibc'
```

If you do not know the full pkg_id but know that `cat` is in it, use `#all`. This searches for every pkg_id that contains `cat` and prompts you to choose one.

```sh
soar add 'cat#all'
```

## Portable Installation

Portable mode creates symlinks for application data directories (home, config, share, cache) to custom locations. This keeps application data self-contained or allows running from removable media.

::: warning Supported package types
Portable mode **only works** for AppImage, FlatImage, RunImage, and Wrappe packages. Static binaries and archive packages do **not support** portable mode.
:::

To install a package in portable mode:

```sh
soar add <package> --portable
```

You can specify custom directories for different data types.

| Flag | Description |
|------|-------------|
| `--portable [DIR]` | Set base portable directory (applies to home and config). Optional value: if no directory specified, uses package installation directory |
| `--portable-home [DIR]` | Custom home directory (creates symlink). Optional value |
| `--portable-config [DIR]` | Custom config directory (creates symlink). Optional value |
| `--portable-share [DIR]` | Custom share directory (creates symlink). Optional value |
| `--portable-cache [DIR]` | Custom cache directory (creates symlink). Optional value |

Example: install with a custom home directory.

```sh
soar add obsidian.AppImage --portable-home ~/.obsidian-data
```

Example: install with multiple custom directories.

```sh
soar add myapp.AppImage --portable-home ~/myapp --portable-config ~/myapp/config --portable-share ~/myapp/share --portable-cache ~/myapp/cache
```

::: info
Portable options create symlinks from the package's expected directories to your custom locations. These settings are stored in the database and reused on reinstallation.
:::

## Installation Flags

### Force installation

To force installation even if the package already exists, use the `--force` flag.

```sh
soar add <package> --force
```

Example: install the `bat` package even if it already exists.

```sh
soar add bat --force
```

### Binary-only installation

By default, Soar extracts all files from a package. The `--binary-only` flag skips extracting non-essential files to save disk space.

```sh
soar add <package> --binary-only
```

This flag excludes:

- `*.png` and `*.svg` (icon files)
- `*.desktop` (desktop entry files)
- `LICENSE` (license files)
- `CHECKSUM` (checksum files)

Example: install `ripgrep` without icons, desktop files, and license.

```sh
soar add ripgrep --binary-only
```

::: info
This option is useful for minimal installations. However, excluding desktop files (`*.desktop`) means the package will not appear in your system's application menu.
:::

### Suppress package notes

Some packages display important information after installation. To suppress these notes, use the `--no-notes` flag.

```sh
soar add <package> --no-notes
```

Example: install `neovim` without displaying post-installation notes.

```sh
soar add neovim --no-notes
```

::: warning
Package notes often contain critical setup instructions or configuration tips. Use this flag with caution.
:::

### Interactive installation

By default, Soar installs a package automatically once your query resolves to a single match. Use the `--ask` flag to force a confirmation prompt before installation.

```sh
soar add <package> --ask
```

`--ask` is the opposite of `--yes`: it always prompts for confirmation before proceeding. To interactively choose between multiple variants of a package, use [`--show`](#show-package-information) instead.

### Non-interactive installation

By default, Soar prompts for confirmation before installing packages if multiple packages are found for the given query. To skip this prompt, use the `--yes` flag.

```sh
soar add <package> --yes
```

Example: install the `cat` package without confirmation.

```sh
soar add cat --yes
```

::: info
The `--yes` flag is useful for non-interactive installations, but it is generally recommended to use it with caution. It will install the first package if multiple packages are found.
:::

### Skip signature verification

By default, Soar verifies package signatures for security. To skip signature verification, use the `--no-verify` flag.

```sh
soar add <package> --no-verify
```

::: danger Security risk
Skipping signature verification exposes you to potentially compromised packages. Only use `--no-verify` with packages from trusted sources or during testing and development.
:::

Example: install a package from a trusted development build.

```sh
soar add https://internal.example.com/builds/myapp.appimage --no-verify
```

### Package ID override

To explicitly specify the package ID, useful when multiple packages share the same name, use the `--pkg-id` flag.

```sh
soar add <package> --pkg-id <package_id>
```

Example: install `cat` from a specific package ID.

```sh
soar add cat --pkg-id git.busybox.net.busybox.standalone.glibc
```

This is equivalent to using the `cat#git.busybox.net.busybox.standalone.glibc` syntax but can be more readable in scripts.

### Show package information

To interactively browse and select package variants before installing, use the `--show` flag.

```sh
soar add <package> --show
```

This opens an interactive picker that displays:

- All available versions and variants of the package
- `[installed]` marker next to already-installed versions
- Package details (name, version, repository, pkg_id)

Example: browse all `bat` variants interactively.

```sh
soar add bat --show
```

::: info
Unlike a non-interactive display, `--show` always presents an interactive selection menu. You can choose which variant to install or cancel without installing anything.
:::

## Advanced Scenarios

### Combining flags

You can combine multiple installation options for complex scenarios.

```sh
soar add bat --yes --no-notes
soar add ripgrep --yes --binary-only
```

### Self-contained portable setup

For AppImage, FlatImage, RunImage, and Wrappe applications that need to be completely self-contained, for example on a USB drive:

```sh
soar add obsidian.AppImage \
  --portable-home /media/usb/obsidian/home \
  --portable-config /media/usb/obsidian/config
```

## Troubleshooting

### Package not found

Check the package name spelling, sync repositories, or try installing from URL directly.

```sh
soar search <name>
soar sync
```

### Multiple packages found

Use `--ask` to choose interactively, specify a repository with `<package>:<repo>`, or use `--yes` for the first match.

### Permission denied

Verify profile permissions or use `sudo` with `--system` mode.

### Portable mode not working

Portable mode only works for AppImage, FlatImage, RunImage, and Wrappe packages. Static binaries and archives are not supported.

For more troubleshooting, see [Health Check](./health.md).

## Related Topics

- [Removing Packages](./remove.md)
- [Updating Packages](./update.md)
- [Searching Packages](./search.md)
