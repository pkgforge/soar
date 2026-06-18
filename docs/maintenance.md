---
title: Maintenance
description: Manage Soar itself, keep your installation healthy, and automate routine maintenance with cron or systemd timers.
---

# Maintenance

This section covers the features that keep Soar itself current and your
installation in good working order, from self updates to scheduled cleanups.

## Self Management

Soar provides commands to manage the package manager itself, including updating
to newer versions and complete uninstallation.

### Update Soar

```sh
soar self update
```

This updates Soar to the latest version by downloading pre-compiled binaries
from GitHub releases.

You can control which release channel to use through environment variables:

- `SOAR_NIGHTLY=1`: switches to the nightly (development) channel.
- `SOAR_RELEASE=1`: switches to the stable release channel.

These environment variables take precedence over the currently installed
channel. For example:

```sh
# Update within current channel
soar self update

# Switch to and update from nightly channel
SOAR_NIGHTLY=1 soar self update

# Switch to and update from stable channel
SOAR_RELEASE=1 soar self update
```

### Uninstall Soar

```sh
soar self uninstall
```

This completely removes Soar from your system. The command will:

- Remove the Soar binary from your system.
- Prompt for confirmation before proceeding.
- Preserve your configuration and packages by default.
- Offer options to remove user data if desired.

::: danger Irreversible action
Uninstalling cannot be undone. Consider backing up your configuration
(`~/.config/soar/config.toml`) and packages (`~/.local/share/soar/packages`)
before you proceed.
:::

#### Uninstall Options

```sh
# Uninstall Soar but keep configuration and packages
soar self uninstall

# After uninstallation, manually remove data if desired:
rm -rf ~/.config/soar
rm -rf ~/.local/share/soar
```

## Maintenance Best Practices

### Regular Health Checks

Run health checks periodically to confirm your installation stays in good
condition:

```sh
# Weekly health check
soar health
```

**What to look for:**

- PATH status: add Soar's binary directory to your `PATH` if it is not configured.
- Broken packages: incomplete installations that should be cleaned up.
- Broken symlinks: dangling symlinks from removed packages. Only `-soar` suffixed files are detected in the desktop and icons directories.

See [Health](./health.md) for the full diagnostics reference.

### Cache Management

Package caches can grow large over time, so regular cleanup helps reclaim disk
space:

```sh
# Clean everything (cache, broken packages, broken symlinks)
soar clean

# Clean only the cache
soar clean --cache

# Check cache size before cleaning
du -sh ~/.local/share/soar/cache
```

::: warning
The `--cache` flag deletes the entire cache directory.
:::

**Recommended schedule:**

- **Monthly**: if you install packages frequently.
- **Quarterly**: for moderate usage.
- **As needed**: when disk space is low.

### Sync Repository Metadata

Keep your package metadata up to date so you see the latest versions:

```sh
# Sync before searching or installing
soar sync
```

**Recommended schedule:**

- **Automatic**: let Soar sync based on each repository's `sync_interval` setting (default: 3 hours).
- **Manual**: before searching for new packages or running updates.

::: info
Repositories configured with `sync_interval = "always"` sync every time
`soar sync` is run.
:::

### Cleanup After Failed Installations

Failed installations can leave broken packages and symlinks behind:

```sh
# Check for issues
soar health

# Clean up if issues found
soar clean --broken
soar clean --broken-symlinks
```

**When to run:**

- After any installation failure.
- After system crashes during installations.
- Before major system updates.

## Scheduling Recommendations

You can automate routine maintenance with cron or systemd timers.

### Weekly Health Check (cron)

```bash
# Add to crontab with: crontab -e
# Run every Sunday at 2 AM
0 2 * * 0 /usr/bin/soar health > /tmp/soar-health.log 2>&1
```

### Monthly Cache Cleanup (cron)

```bash
# Add to crontab with: crontab -e
# Run on the 1st of every month at 3 AM
0 3 1 * * /usr/bin/soar clean --cache > /tmp/soar-clean.log 2>&1
```

### Systemd Timer

Create `/etc/systemd/system/soar-health.service`:

```ini
[Unit]
Description=Soar Health Check
After=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/bin/soar health
Nice=19
IOSchedulingClass=idle
```

Create `/etc/systemd/system/soar-health.timer`:

```ini
[Unit]
Description=Weekly Soar Health Check
Requires=soar-health.service

[Timer]
OnCalendar=weekly
Persistent=true

[Install]
WantedBy=timers.target
```

Enable the timer:

```bash
sudo systemctl enable --now soar-health.timer
```

## Manual Maintenance Checklist

Perform these tasks manually every one to three months.

1. **Health check**

   ```sh
   soar health
   ```

   Review any warnings or errors, then fix broken packages or symlinks if found.

2. **Update Soar**

   ```sh
   soar self update
   ```

   Keep Soar itself up to date and review the changelog for new features.

3. **Clean cache**

   ```sh
   soar clean --cache
   ```

   Free up disk space and ensure fresh downloads on the next install.

4. **Review packages**

   ```sh
   soar list-installed
   ```

   Identify unused packages and remove the ones you no longer need.

5. **Update packages**

   ```sh
   soar update
   ```

   Keep installed packages current and review changelogs for breaking changes.

## Maintenance Schedule Summary

| Task | Frequency | Command | Purpose |
|------|-----------|---------|---------|
| Health Check | Weekly | `soar health` | Identify issues early |
| Full Clean | Monthly | `soar clean` | Clean cache, broken packages, and symlinks |
| Sync Metadata | Automatic | `soar sync` | Get latest package info |
| Update Soar | Quarterly | `soar self update` | Get latest features |
| Broken Cleanup | As needed | `soar clean --broken --broken-symlinks` | Fix failed installs |
| Full Maintenance | Quarterly | All commands above | Complete system checkup |

## Troubleshooting Maintenance Issues

### Sync Failures

If `soar sync` fails:

1. Check network connectivity.
2. Verify repository URLs in `~/.config/soar/config.toml`.
3. Try syncing individual repositories.
4. Check whether the repository is temporarily unavailable.

### Cache Won't Clear

If `soar clean --cache` does not free space:

1. Check whether other processes are using cached files.
2. Manually remove the cache directory: `rm -rf ~/.local/share/soar/cache/*`.
3. Verify that Soar has write permissions.

### Persistent Broken Packages

If `soar health` keeps reporting broken packages:

1. Try reinstalling the broken package: `soar install --force <package>`.
2. If that fails, remove and reinstall: `soar remove <package> && soar install <package>`.
3. Check whether the package version is available in your repositories.
4. Report the issue to the repository maintainer.
