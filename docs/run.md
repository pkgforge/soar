---
title: Run Packages
description: Run packages on demand without installing them into your PATH or desktop environment.
---

# Run Packages

Soar lets you run packages without installing them permanently on your system. The package files are still downloaded and stored, but they are not integrated into your system PATH or desktop environment. This is ideal for trying out applications or running one-off tasks.

## Run vs Install

| Feature | `soar run` | `soar install` |
|---------|-----------|----------------|
| **PATH Integration** | ❌ Not added to PATH | ✅ Added to profile's bin directory |
| **Desktop Integration** | ❌ No desktop entries | ✅ Desktop entries created |
| **Persistence** | ✅ Files stored for reuse | ✅ Permanently installed |
| **Symlinks** | ❌ No symlinks created | ✅ Symlinks for `provides` entries |
| **Quick Access** | Must use full command | Available from anywhere |
| **Cleanup** | Manual removal from cache directory | Managed via `soar remove` |
| **Use Case** | Try before installing, one-off tasks | Daily use applications |

::: tip When to use run
Reach for `run` when testing a new tool, running a security scanner occasionally, trying different versions, or when you do not want to clutter your PATH.
:::

## Basic Usage

The basic syntax for running a package is:

```sh
soar run <package> [command arguments...]
```

The first argument after `run` is the package name, followed by any arguments to pass to the package's binary.

### Example: run feroxbuster

```sh
soar run feroxbuster --url https://example.com --depth 2
```

### Example: run a specific version

```sh
soar run python@3.11 --version
```

## Command Options

The `run` command supports several options to control package selection and behavior.

### `--yes` / `-y`

Skip all prompts and automatically select the first available package variant.

```sh
soar run --yes feroxbuster
```

This is useful for scripting or when you are confident in the default selection.

### `--pkg-id <ID>`

Specify the exact package ID to run. This is useful when multiple packages provide the same binary.

```sh
soar run --pkg-id coreutils@9.5 ls --version
```

### `--repo-name <REPO>` / `-r <REPO>`

Specify which repository to fetch the package from.

```sh
soar run --repo-name main python
```

This is helpful when the same package exists in multiple repositories.

## Command Passing

Any arguments after the package name are passed directly to the package's binary:

```sh
soar run feroxbuster --url https://example.com --depth 2 --threads 10
```

In this example:

- `feroxbuster` is the package name.
- `--url https://example.com --depth 2 --threads 10` are arguments passed to feroxbuster.

::: info Argument parsing
The first non-option argument is treated as the package name. All subsequent arguments are passed to the package.
:::

### Example: running with complex arguments

```sh
soar run python -c "print('Hello from Soar!')"
```

```sh
soar run node --eval "console.log('Node.js via Soar')"
```

## Storage and Caching

Soar caches run packages in `~/.local/share/soar/cache/bin` to avoid re-downloading.

- **Downloads once.** The package is downloaded on first run.
- **Reuses cached binary.** Subsequent runs use the cached copy.
- **Validates checksums.** Soar validates checksums after download and prompts for confirmation on mismatch.
- **Separate from installed packages.** Run packages use the cache directory, not the packages directory.

::: info Cached binaries persist
Cached binaries persist for faster subsequent execution. Clean them manually from `~/.local/share/soar/cache/bin` or use `soar clean --cache` when no longer needed.
:::

## Practical Examples

### Try before you install

```sh
# Test a network scanner
soar run feroxbuster --help

# If you like it, install it permanently
soar install feroxbuster
```

### Run specific versions

```sh
# Run Python 3.11 for a specific script
soar run python@3.11 script.py

# Run Node.js 20 for a project
soar run node@20 --version
```

### Run from a specific repository

```sh
# Run from the 'testing' repository
soar run --repo-name testing experimental-tool

# Run with a specific package ID
soar run --pkg-id neovim@0.10.1 nvim --version
```

### One-off tasks

```sh
# Run a security audit without installing tools permanently
soar run semgrep --config auto /path/to/code

# Process a file with a specialized tool
soar run jq '.filter | map(select(.value > 10))' data.json
```

### Scripting with --yes

```sh
#!/bin/bash
# Automated security scan
soar run --yes feroxbuster --url "$TARGET" --output scan-results.txt
soar run --yes nuclei -u "$TARGET" -severity critical
```

## Cleanup

Run packages are stored in the cache directory and are not tracked in the installed packages database. To remove them:

```sh
# Clean all cached run packages
soar clean --cache

# Or manually remove specific packages from cache directory
rm ~/.local/share/soar/cache/bin/<package-name>
```

::: warning Run packages are not tracked by soar remove
Use `soar clean --cache` to remove cached run packages, or delete them manually from `~/.local/share/soar/cache/bin`.
:::

## Tips and Best Practices

1. **Use `--yes` in scripts** to avoid interactive prompts.
2. **Specify `--pkg-id`** when you need exact version control.
3. **Use `--repo-name`** to select packages from specific repositories.
4. **Pass `--help`** to see package-specific options: `soar run package --help`.
5. **Combine with shell features** for powerful one-liners.

::: tip Create aliases for frequently-run packages
```sh
alias py311='soar run python@3.11'
alias node20='soar run node@20'
```
:::
