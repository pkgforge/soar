---
title: Update Packages
description: Keep installed packages current with soar update, including options, version determination, and recovery behavior.
---

# Update Packages

Soar keeps your installed packages current with a single command family. This guide covers every update operation, the available options, and how Soar determines and applies new versions.

## Quick Start

Update all installed packages to their latest versions:

```sh
soar update
```

Update specific packages by name:

```sh
soar update <package1> <package2>
```

For example, update `7z` and `bat` together:

```sh
soar update 7z bat
```

## Update Options

The `update` command supports the following options:

| Option | Description |
|--------|-------------|
| `--ask` | Prompt for confirmation before updating each package |
| `--keep` | Keep the current version (only refresh metadata) |
| `--no-verify` | Skip checksum and signature verification |

### Ask for Confirmation

Use `--ask` to review each package before it is updated:

```sh
soar update --ask
soar update <package> --ask
```

Soar prompts for each package update so you can approve or skip it.

### Keep Current Version

Use `--keep` to refresh package metadata without moving to a newer version:

```sh
soar update <package> --keep
```

This updates the package database entry but maintains the currently installed version. It is useful for refreshing package information, re-verifying installations, and testing without version changes.

### Skip Verification

Use `--no-verify` to skip signature and checksum verification during updates:

```sh
soar update <package> --no-verify
```

::: danger Security risk
Skipping verification exposes you to potentially compromised updates. Only use it with sources you trust.
:::

## Version Determination

### Repository Packages

For packages installed from repositories, Soar determines the latest version from the package database:

```sh
soar update bat
```

This updates `bat` to the latest version available in the repository it was installed from.

### Packages Installed From a URL

A package installed from a URL records where it came from, so it can be checked
again later. You can name it by that URL or by the name it installed under:

```sh
soar update https://github.com/owner/repo/releases/download/v1.2.3/tool-linux-x86_64.AppImage
soar update tool
```

Soar decides what to update to in this order.

1. **The feed the artifact carries.** An AppImage can record where its updates
   come from in a `.upd_info` section. That is the publisher's own statement of
   how the package updates, so it is followed first, and the new artifact is
   fetched over [zsync](#delta-updates-over-zsync).
2. **The release the download came from.** Where the URL points at a GitHub or
   GitLab release, the newest release of that project decides. This is what
   covers archives and plain binaries, which have nowhere to carry a feed.
3. **Neither**, for a URL that is not a forge release and whose artifact
   declares nothing. There is no way to tell a new build from the installed one,
   so the package is left alone.

A feed that cannot be resolved at all, because the tag it names was deleted,
falls through to the release source rather than leaving the package stuck.

::: tip
Checking a release uses the forge's API, which is rate limited. GitHub's limit
of 60 requests an hour is the one you are likely to meet; setting
`GITHUB_TOKEN` or `GH_TOKEN` raises it to 5,000. See
[Forge Rate Limits](./configuration.md#forge-rate-limits).
:::

### Delta Updates Over zsync

A publisher offering a zsync feed publishes a control file describing the
artifact block by block. Soar uses it twice: to decide whether the artifact
differs from the installed copy, without downloading it, and then to fetch only
the blocks that changed, reusing the rest from the copy already on disk. The
result is verified against the control file's checksums before it replaces the
installed package.

Where a release publishes a `.zsync` file beside the artifact, an update
following that release uses it too.

## Profile Handling

The update process respects the original installation profile. If a package was installed with a specific profile, updates maintain that profile setting.

::: warning
The profile flag has no effect on package installation path. Updates use the profile that was active at the time of installation.
:::

## Cross-Repository Update Behavior

A package always updates from the same repository it was installed from. Even if a newer version exists in a different repository, Soar will not use it.

::: warning
The update process ignores updates from any repository other than the one the package was installed from.
:::

To switch a package to a different repository, remove it and add it again from the new source:

```sh
# Remove from the original repository
soar remove bat

# Install from the new repository
soar add bat:soarpkgs
```

Future updates then follow the new repository:

```sh
# Installed from 'official', so only 'official' is checked for updates
soar update bat
```

## Update Behavior

### What Happens During an Update

When you update a package, Soar:

1. Checks for newer versions in the source repository
2. Verifies signatures and checksums (unless `--no-verify` is used)
3. Downloads the new version
4. Backs up the current installation
5. Extracts the new version
6. Updates symlinks and database entries
7. Removes the old version (if successful)

If installation fails at any step, soar restores the previous version from the backup, as described below.

### Backup and Recovery

Soar maintains a backup during each update. If an update fails, the previous version remains intact:

```sh
soar update bat
```

### Batch Updates

Update multiple packages in one command, optionally with confirmation:

```sh
soar update bat ripgrep fd
soar update --ask bat ripgrep fd
```

## Best Practices

Keep packages up to date for security fixes and new features:

```sh
soar update
```

Review updates before they apply with `--ask`:

```sh
soar update --ask
```

Verify package sources before updating:

```sh
soar info <package>
```

For critical applications, back up your data before updating:

```sh
soar update --ask <critical-app>
```

Update specific packages when you are concerned about compatibility:

```sh
soar update <package1> <package2>
```

## Scenarios

### Update All Packages

```sh
soar update
```

Updates all installed packages to their latest versions from their original repositories.

### Update with Confirmation

```sh
soar update --ask
```

Review each package update before proceeding.

### Update a Specific Package

```sh
soar update ripgrep
```

Updates only the `ripgrep` package.

### Refresh Metadata Without Updating

```sh
soar update --keep
```

Updates package metadata in the database without changing installed versions.

### Move a Package to a Different Repository

```sh
# Current setup uses the 'official' repository
soar remove bat

# Switch to the 'soarpkgs' repository
soar add bat:soarpkgs

# Future updates use 'soarpkgs'
soar update bat
```

## Troubleshooting

### Update Fails with a Signature Error

Verify the package source is trusted, and only skip verification when you are certain:

```sh
soar info <package>
soar update --no-verify <package>  # Only if the source is trusted
```

### Update Is Stuck or Slow

Check your network connection and repository status:

```sh
soar sync
soar update <package>
```

### Version Did Not Change

Confirm the package actually has a newer version available:

```sh
soar query <package>
```

### Cannot Update from a Different Repository

This is by design. Remove the package and reinstall it from the new repository:

```sh
soar remove <package>
soar add <package>:<new-repo>
```

For more help, see [Health Check](./health.md).

## Related Commands

- [Installing Packages](./install.md)
- [Removing Packages](./remove.md)
- [Searching Packages](./search.md)
