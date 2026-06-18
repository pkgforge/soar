---
title: Removing Packages
description: Remove installed packages with soar, including specific pkg_ids, multiple packages at once, and cleanup of broken or partial installations.
---

# Remove Packages

Soar provides straightforward commands for removing installed packages from your system. This guide covers the removal options, what happens during removal, and troubleshooting tips.

## Usage

To remove a package, use the `remove` command or one of its aliases.

### Removing a single package

::: code-group

```sh [remove]
soar remove <package>
```

```sh [r]
soar r <package>
```

```sh [del]
soar del <package>
```

:::

Example: remove `7z`.

```sh
soar remove 7z
```

### Command options

The `remove` command supports the following options.

| Option | Description |
|--------|-------------|
| `--yes` | Skip confirmation prompts. Automatically selects the first option when multiple packages match. |
| `--all` | Remove all installed variants of the specified package across different pkg_ids. |

#### Using --yes

Skip interactive prompts when removing packages.

```sh
# Remove without confirmation
soar remove --yes 7z

# Automatically select first match when multiple variants exist
soar remove --yes bat
```

#### Using --all

Remove all installed variants of a package.

```sh
# Remove all versions of bat from all pkg_ids
soar remove --all bat

# Remove with --yes to skip bulk confirmation
soar remove --all --yes cat
```

::: warning Removes every variant
Using `--all` will remove ALL installed variants of the package, including those from different repositories and pkg_ids. Use with caution.
:::

### Removing multiple packages

Remove multiple packages in a single command.

```sh
soar remove <package1> <package2> <package3>
```

Example: remove `7z` and `bat`.

```sh
soar remove 7z bat
```

::: info
If you provide only the package name without a pkg_id and multiple packages match, you will be prompted to select ONE package to remove. Use `--all` to remove all variants.
:::

### Removing a package from a specific pkg_id

```sh
soar remove <package>#<pkg_id>
```

Example: remove `cat` from the `git.busybox.net.busybox.standalone.glibc` pkg_id.

```sh
soar remove cat#git.busybox.net.busybox.standalone.glibc
```

### Removing all packages from a specific pkg_id

```sh
soar remove '<name>#all'
```

This searches for all pkg_ids that `cat` is in and prompts you to choose one.

```sh
soar remove 'cat#all'
```

After you select a pkg_id, all packages from that pkg_id will be removed.

## What Happens During Removal

When you remove a package, Soar performs these cleanup operations.

1. **Pre-Remove Hook** (if configured)
2. **Binary Symlink Removal** from `~/.local/share/soar/bin`
3. **Provides Symlink Cleanup** for alternative names
4. **Desktop Entry Removal** from `~/.local/share/applications`
5. **Icon Symlink Cleanup** from `~/.local/share/icons`
6. **Package Directory Removal** from `~/.local/share/soar/packages`
7. **Cache Handling**: download cache preserved (use `soar clean --cache` to reclaim)
8. **Database Cleanup**: removes the package record and portable entries

Example output:

```
Removed 7z#upstream.release:official (24.08)
  - Removed binary: ~/.local/share/soar/bin/7z
  - Removed directory: ~/.local/share/soar/packages/7z-24.08
  - Reclaimed 2.3 MiB
```

## Partial vs Complete Removal

### Complete removal

A complete removal occurs when:

- The package was successfully installed (`is_installed = true`)
- All files and symlinks are properly cleaned up
- The package is removed from the database

This is the normal and expected removal process.

### Partial removal (broken packages)

A partial or incomplete installation can occur when:

- The installation process was interrupted (network failure, system crash)
- Disk space ran out during installation
- The package was manually deleted from the filesystem

These are marked as **broken packages** in Soar's database (`is_installed = false`).

#### Identifying broken packages

To check for broken or incomplete installations:

```sh
soar health
```

Example output showing broken packages:

```
Broken Packages (1):
  7z#upstream.release:official /home/user/.local/share/soar/packages/7z-24.08
```

#### Removing broken packages

To remove all broken packages:

```sh
soar clean --broken
```

This command:

- Lists all broken packages in the database
- Removes their directories (if they still exist)
- Removes any leftover symlinks
- Cleans up database entries

## Troubleshooting

### Stuck or incomplete removals

Check system health and fix broken symlinks.

```sh
soar health
soar clean --broken-symlinks
```

### Package will not remove

Check file permissions, ensure the package is not running, and use verbose mode.

```sh
ls -la ~/.local/share/soar/packages/
pgrep -a <package>
soar --verbose remove <package>
```

For more help, see [Health Check](./health.md).

### Manual cleanup

For manual cleanup of stuck packages:

1. Find the package directory.

   ```sh
   soar info | grep <package>
   ```

2. Remove the directory manually.

   ```sh
   rm -rf ~/.local/share/soar/packages/<package-directory>
   ```

3. Remove symlinks manually.

   ```sh
   rm -f ~/.local/share/soar/bin/<package>
   ```

4. Run a health check.

   ```sh
   soar health
   ```

5. Clean up any remaining broken symlinks.

   ```sh
   soar clean --broken-symlinks
   ```

::: warning Destructive commands
The `rm -rf` and `rm -f` commands above permanently delete files. Double-check each path before running them, since there is no undo.
:::
