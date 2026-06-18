---
title: Quick Start
description: Install Soar, set up your PATH, and manage your first packages in under five minutes.
---

# Quick Start Guide

Get Soar installed and managing packages in under five minutes.

## First Time Setup

### Step 1: Install Soar

```sh
curl -fsSL https://soar.qaidvoid.dev/install.sh | sh
```

### Step 2: Verify Installation

```sh
soar --version
```

### Step 3: Add to PATH

::: code-group

```sh [bash]
echo 'export PATH="$HOME/.local/bin:$HOME/.local/share/soar/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

```sh [zsh]
echo 'export PATH="$HOME/.local/bin:$HOME/.local/share/soar/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

:::

### Step 4: Install Your First Package

```sh
soar sync
soar install neovim
```

::: tip Need more options?
See the [Installation Guide](./installation.md) for manual installs, custom directories, and version pinning.
:::

## Daily Package Management

### Search Packages

```sh
soar search python
soar query neovim
```

### Install Packages

```sh
soar install git
soar install git curl wget
soar install https://example.com/package.AppImage
```

### List Packages

```sh
soar list
soar info
```

### Update Packages

```sh
soar update
soar update neovim git
```

### Remove Packages

```sh
soar remove neovim
```

### Run Without Installing

```sh
soar run neovim
```

## Switching from Another Package Manager

### From apt (Debian/Ubuntu)

| Task | apt Command | Soar Equivalent |
|------|-------------|-----------------|
| Update cache | `sudo apt update` | `soar sync` |
| Install | `sudo apt install <pkg>` | `soar install <pkg>` |
| Remove | `sudo apt remove <pkg>` | `soar remove <pkg>` |
| Update | `sudo apt upgrade` | `soar update` |
| Search | `apt search <query>` | `soar search <query>` |
| List installed | `apt list --installed` | `soar info` |

### From pacman (Arch Linux)

| Task | pacman Command | Soar Equivalent |
|------|---------------|-----------------|
| Update database | `sudo pacman -Sy` | `soar sync` |
| Install | `sudo pacman -S <pkg>` | `soar install <pkg>` |
| Remove | `sudo pacman -R <pkg>` | `soar remove <pkg>` |
| Update system | `sudo pacman -Syu` | `soar update` |
| Search | `pacman -Ss <query>` | `soar search <query>` |
| List installed | `pacman -Qe` | `soar info` |

## Managing Multiple Systems

Profiles let you maintain separate package environments.

### Create a Profile

Edit `~/.config/soar/config.toml`:

```toml
[profile.default]
root_path = "~/.local/share/soar"

[profile.dev]
root_path = "~/dev-tools"
```

### Use Profiles

```sh
soar --profile dev install neovim
soar --profile dev list
soar --profile dev update
```

### Set Default Profile

```toml
default_profile = "dev"
```

## Troubleshooting

Run diagnostics if something is not working:

```sh
soar health
```

For comprehensive troubleshooting, see [Health & Diagnostics](./health.md).

## What's Next?

- **[Configuration](./configuration.md)** - Customize settings and repositories
- **[Package Management](./package-management.md)** - Install, update, and manage packages
- **[Profiles](./profiles.md)** - Set up installation profiles
