---
title: Installation
description: Install Soar on any Linux distribution via the install script, pre-built binaries, Cargo, or source.
---

# Installation

Install Soar on your Linux system using one of the methods below.

::: info Prerequisites
Linux (any distribution), curl or wget, and basic shell access.
:::

## System Requirements

- **Architectures:** x86_64, aarch64, riscv64
- **OS:** Any Linux distribution (kernel 4.0+ recommended)
- **Building from source:** Rust 1.88.0+

## Quick Installation

### Install Script (Recommended)

::: code-group

```sh [curl]
curl -fsSL https://soar.qaidvoid.dev/install.sh | sh
```

```sh [wget]
wget -qO- https://soar.qaidvoid.dev/install.sh | sh
```

:::

The script automatically detects your architecture, downloads the appropriate binary, and installs it to:

- `/usr/local/bin` (if running as root)
- `$HOME/.local/bin` (user installation, default)

#### Install Script Options

| Variable | Purpose | Example |
|----------|---------|---------|
| `SOAR_VERSION` | Specify version | `latest`, `nightly`, `0.4.0` |
| `SOAR_INSTALL_DIR` | Custom directory | `/usr/local/bin`, `$HOME/.local/bin` |
| `DEBUG` | Enable debug output | Set to any value |

**Examples:**

```sh
# Install specific version
curl -fsSL https://soar.qaidvoid.dev/install.sh | SOAR_VERSION=0.4.0 sh

# Install to custom directory
curl -fsSL https://soar.qaidvoid.dev/install.sh | SOAR_INSTALL_DIR=$HOME/.local/bin sh

# Enable debug output
curl -fsSL https://soar.qaidvoid.dev/install.sh | DEBUG=1 sh
```

## Manual Installation

### From Pre-built Binaries

1. **Download from [releases](https://github.com/pkgforge/soar/releases)**

2. **Choose your architecture:**
   - `soar-x86_64-linux` (Intel/AMD)
   - `soar-aarch64-linux` (ARM 64-bit)
   - `soar-riscv64-linux` (RISC-V 64-bit)

3. **Install:**

```sh
chmod +x soar-x86_64-linux
sudo mv soar-x86_64-linux /usr/local/bin/soar
```

### From Cargo

```sh
cargo install soar-cli
```

Requires the Rust toolchain. Takes longer due to compilation.

### Building from Source

```sh
git clone https://github.com/pkgforge/soar.git
cd soar
cargo install --path .
```

Requires Rust 1.88.0+.

## PATH Configuration

Two directories should be in your PATH:

- `$HOME/.local/bin` where the `soar` binary is installed
- `$HOME/.local/share/soar/bin` where packages installed by Soar are symlinked

::: code-group

```sh [bash]
echo 'export PATH="$HOME/.local/share/soar/bin:$HOME/.local/bin:$PATH"' >> ~/.bashrc
source ~/.bashrc
```

```sh [zsh]
echo 'export PATH="$HOME/.local/share/soar/bin:$HOME/.local/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

:::

## Uninstallation

### Remove Soar

```sh
# User installation
rm ~/.local/bin/soar
rm -rf ~/.config/soar ~/.local/share/soar

# System installation
sudo rm /usr/local/bin/soar
sudo rm -rf /etc/soar /opt/soar
```

Or use the self-uninstall command:

```sh
soar self uninstall
```

### Remove Packages

```sh
soar remove ffmpeg
soar remove --all
```

## Troubleshooting

**"Command not found" after installation:**

1. Verify binary exists: `ls -la ~/.local/bin/soar`
2. Check PATH: `echo $PATH`
3. Add to PATH (see [PATH Configuration](#path-configuration))
4. Restart shell: `source ~/.bashrc`

**"Permission denied" installing:**

- Use user installation: `SOAR_INSTALL_DIR=$HOME/.local/bin`
- Or use sudo: `sudo curl -fsSL https://soar.qaidvoid.dev/install.sh | sh`

**Download fails:**

- Try alternative CDN: `curl -fsSL https://soar.pkgforge.dev/install.sh | sh`
- Use wget: `wget -qO- https://soar.qaidvoid.dev/install.sh | sh`
- Download manually from [GitHub releases](https://github.com/pkgforge/soar/releases)

**Build from source fails:**

- Check Rust version: `rustc --version` (requires 1.88.0+)
- Update Rust: `rustup update`
- Install dependencies: `sudo apt install build-essential pkg-config libssl-dev`

For more help, run `soar health`, check the verbose output with `--verbose`, or visit [GitHub Issues](https://github.com/pkgforge/soar/issues).

## Next Steps

```sh
# Verify installation
soar --version

# Sync repositories
soar sync

# Install your first package
soar install ffmpeg
```

- **[Configuration](./configuration.md)** - Customize settings and repositories
- **[Package Management](./package-management.md)** - Install, update, and manage packages
- **[Profiles](./profiles.md)** - Set up installation profiles
