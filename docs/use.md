---
title: Use a Package Variant
description: Switch between installed variants of the same package without uninstalling any of them.
---

# Use Package From Different Family

Soar lets you switch between different variants of an installed package without uninstalling any of them. This is useful when you need to move between versions, implementations, or repository sources of the same package.

## Understanding Package Families

A **package family** is the set of installed variants that share the same package name. These variants can differ in several ways.

| Variant Type | Example | Description |
|--------------|---------|-------------|
| **Version** | `python@3.11` vs `python@3.12` | Different versions of the same package |
| **Implementation** | `cat` (GNU) vs `cat` (BusyBox) | Different implementations providing the same binary |
| **Repository** | `neovim` from `main` vs `testing` | Same package from different repositories |
| **Package ID** | `coreutils` vs `coreutils-ucr` | Different package IDs providing similar functionality |

### Example Package Families

```
Package: python
├── python#python@3.11 (from main repo)
├── python#python@3.12 (from main repo)
└── python#pypy@3.11 (from testing repo)

Package: cat
├── cat#coreutils@9.5 (provides: cat, ls, etc.)
├── cat#busybox@1.36 (provides: cat, ls, etc.)
└── cat#uutils-coreutils@0.0.23 (provides: cat, ls, etc.)
```

## When to Switch Packages

Common reasons to switch between variants include the following.

1. **Version testing**
   ```sh
   # Switch to Python 3.12 to test new features
   soar use python
   # Select python@3.12 from the list
   python --version
   ```

2. **Compatibility requirements**
   ```sh
   # Switch to older Node.js for a legacy project
   soar use node
   # Select node@18 from the list
   ```

3. **Alternative implementations**
   ```sh
   # Switch to BusyBox variants for embedded systems
   soar use cat
   # Select busybox@1.36 for smaller footprint
   ```

4. **Testing repository versions**
   ```sh
   # Try a package from the testing repository
   soar use neovim
   # Select neovim from testing repo
   ```

## How the Use Command Works

When you run `soar use <package>`, soar:

1. Checks whether multiple variants are installed. If only one is installed, there is nothing to switch and soar exits.
2. Displays the installed variants and prompts you to select one.
3. Overwrites the primary binary symlink to point at the selected variant.
4. Overwrites every provides symlink (for example `cat`, `ls`, `chmod`).
5. Marks the selected variant active in the database and the others inactive.

The steps below walk through the interactive selection.

### Step 1: List installed variants

When you run `soar use <package>`, Soar displays all installed variants.

```sh
$ soar use python

[1] python#python:3.11.5-main (15MB) *
[2] python#python:3.12.0-main (16MB)
[3] python#pypy:3.11.0-testing (12MB)

Select a variant [1-3]:
```

### Step 2: Select a variant

Select the number corresponding to your desired variant. Soar then does the following.

1. **Overwrites symlinks** for the package and its `provides` entries. Existing symlinks are replaced, not removed separately.
2. **Marks the selected variant as active** in the database.
3. **Marks other variants as inactive.** They remain installed but unlinked.

### Step 3: Verify the switch

```sh
$ python --version
Python 3.12.0
```

## Command Syntax

```sh
soar use <package_name>
```

- `<package_name>`: the **base package name only**, for example `python`, `cat`, or `node`.
  - Do not use version suffixes. Use `python`, not `python@3.12`.
  - Do not use pkg_id prefixes. Use `python`, not `python#pypy`.
- No additional flags or options are supported.
- The command is interactive when multiple variants are installed.
- If only one variant is installed, the command exits without prompting because there is nothing to switch.

## What Gets Switched

When you switch packages, Soar manages both the primary binary and every binary the package provides.

### Primary binary

The main binary name:

```sh
# Switching 'python' affects:
~/.local/share/soar/bin/python -> /path/to/python@3.12/bin/python
```

### Provided binaries

All binaries listed in the package's `provides` field:

```sh
# Switching 'coreutils' affects all its provides:
~/.local/share/soar/bin/cat -> /path/to/coreutils/bin/cat
~/.local/share/soar/bin/ls -> /path/to/coreutils/bin/ls
~/.local/share/soar/bin/chmod -> /path/to/coreutils/bin/chmod
# ... and more
```

::: warning Switching affects every provided binary
Switching a package affects ALL binaries it provides. If you switch `coreutils`, you switch `cat`, `ls`, `chmod`, and all other core utilities simultaneously.
:::

## Practical Examples

### Python version management

```sh
soar install python@3.11 python@3.12
soar use python  # Select python@3.12
python --version  # Python 3.12.0
```

### Alternative implementations

```sh
soar install coreutils uutils-coreutils
soar use cat  # Select uutils-coreutils
ls --version  # uutils-coreutils 0.0.23
```

## Managing Multiple Variants

### Viewing all installed variants

To see all installed packages and their variants:

```sh
soar info

# Output:
# Installed packages:
# python@3.11.5 (main)
# python@3.12.0 (main)
# node@18.0.0 (main)
# node@20.0.0 (main)
# coreutils@9.5 (main)
# busybox@1.36 (main)
```

### Removing unwanted variants

If you no longer need a variant:

```sh
# Remove a specific variant
soar remove python@3.11

# Remove all variants
soar remove --all python
```

## Limitations and Considerations

### Must be installed first

You can only switch between installed variants. If no variants are installed:

```sh
$ soar use python
Package is not installed
```

The command displays this message and exits gracefully with no error code.

### No automatic rollback

There is no automatic undo. You must manually switch back:

```sh
soar use python
# Select different variant
```

### Affects all provides

Switching affects all binaries the package provides:

```sh
soar use coreutils
# This switches cat, ls, chmod, mkdir, etc. all at once
```

## Best Practices

- **Install multiple versions** that you might need for development.
- **Document requirements** by noting required versions in the project README.
- **Use profiles** for different project requirements.
- **Test first** with `soar run` before switching in production.

## Comparison with Other Tools

| Feature | `soar use` | `update-alternatives` | `nvm` | `pyenv` |
|---------|-----------|----------------------|-------|---------|
| **Multiple Versions** | ✅ | ✅ | ✅ | ✅ |
| **Package Agnostic** | ✅ | ✅ | ❌ | ❌ |
| **Repository Support** | ✅ | ❌ | ❌ | ❌ |

## Troubleshooting

### Command not found after switch

```sh
soar use python
python: command not found
```

This means Soar's bin directory is not in your PATH. Add it permanently:

```sh
# Add to your ~/.bashrc, ~/.zshrc, or equivalent
export PATH="$HOME/.local/share/soar/bin:$PATH"
```

Then reload your shell:

```sh
source ~/.bashrc  # or source ~/.zshrc
```

### Old version still showing

```sh
soar use python
python --version  # Still shows old version
```

Check for cached paths:

```sh
hash -r  # Clear command hash table
python --version  # Should show new version
```

### No variants listed

If you do not see any variants when running `soar use`:

```sh
$ soar use python
Package is not installed
```

Install at least one variant of the package first:

```sh
soar install python
soar use python  # Now shows installed variants
```
