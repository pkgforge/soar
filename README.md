<div align="center">

[crates-shield]: https://img.shields.io/crates/v/soar-cli
[crates-url]: https://crates.io/crates/soar-cli
[discord-shield]: https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=discord
[discord-url]: https://discord.gg/djJUs48Zbu
[doc-shield]: https://img.shields.io/badge/docs-soar.qaidvoid.dev-blue
[doc-url]: https://soar.qaidvoid.dev
[license-shield]: https://img.shields.io/github/license/pkgforge/soar.svg
[license-url]: https://github.com/pkgforge/soar/blob/main/LICENSE

[![Crates.io][crates-shield]][crates-url]
[![Discord][discord-shield]][discord-url]
[![Documentation][doc-shield]][doc-url]
[![License: MIT][license-shield]][license-url]

</div>

<h4 align="center">
  <a href="https://soar.qaidvoid.dev">📘 Documentation</a> |
</h4>

<p align="center">
    A fast, modern, bloat-free distro-independent package manager that <i>just works</i><br>
    Supports static binaries, AppImages, and other Portable formats (AppBundle, FlatImage, RunImage, Wrappe, etc.) on any Linux distribution.
</p>

## 📦 What is Soar?

Soar is a **package manager** - it doesn't build or host packages itself. Instead, it consumes package metadata from repositories and handles downloading, installing, and managing packages on your system.

**How it works:**
- **Repositories** (like [soarpkgs](https://github.com/pkgforge/soarpkgs) - the default) build and host packages, providing metadata in a standard format
- **Soar** fetches this metadata, lets you search/install packages, and manages your local installations
- **You** can use soarpkgs, add third-party repos, or even create your own

This separation means Soar works with any compatible repository - it's not tied to a specific package source.

## 🪄 Quickstart

> [!TIP]
> - Soar comes as a single-file, statically-linked executable with no dependencies that you can simply [download](https://github.com/pkgforge/soar/releases/latest) & run.
> - The [install script](https://github.com/pkgforge/soar/blob/main/install.sh) does this & more automatically for you.

```bash
# cURL
curl -fsSL "https://raw.githubusercontent.com/pkgforge/soar/main/install.sh" | sh

# wget
wget -qO- "https://raw.githubusercontent.com/pkgforge/soar/main/install.sh" | sh
```

> [!NOTE]
> - Please read & verify what's inside the script before running it
> - The script is also available through https://soar.qaidvoid.dev/install.sh
> - Additionally, if you want to customize your installation, please read the docs @ https://soar.qaidvoid.dev/installation.html
> - Next, check [Configuration](https://soar.qaidvoid.dev/configuration) & [Usage](https://soar.qaidvoid.dev/package-management)

## 🌟 Key Features

| Feature | Description |
|:--:|:--|
| **Universal** | Single binary, no dependencies, works on any Unix-like system without superuser privileges. |
| **Portable Formats** | Install static binaries, AppImages, and other self-contained archives with ease. |
| **System Integration** | Adds desktop entries and icons, and links manual pages and shell completions where your system already looks for them. |
| **Install From Anywhere** | Install from a repository, a direct URL, or a local file. A package installed from a GitHub or GitLab release is then kept up to date from that release. |
| **Delta Updates** | An AppImage advertising a zsync feed is updated by fetching only the parts that changed. |
| **Repository Agnostic** | Works with any repository that provides compatible metadata. Use [official soarpkgs repo](https://github.com/pkgforge/soarpkgs), third-party sources, or [create your own](https://soar.qaidvoid.dev/configuration#custom-repository-support). |
| **Security First** | Enforces security through checksums and signature verification for package installations. |
| **Fast & Efficient** | Minimal overhead with parallel downloads and efficient package operations. |


## 🔑 Forge Tokens

Installing or updating from a release uses that forge's API, which is rate
limited. GitHub's limit of 60 requests an hour is easy to reach, so setting
`GITHUB_TOKEN` or `GH_TOKEN` is worth doing if you track several packages that
way. GitLab counts per minute and is rarely a problem, but honours
`GITLAB_TOKEN` and `GL_TOKEN` all the same.

## 🤝 Contributing

We welcome contributions! Please feel free to fork the repository and submit pull requests. See [CONTRIBUTING.md](https://github.com/pkgforge/soar/blob/main/CONTRIBUTING.md) for contribution guidelines.

## 💬 Contact

We have a growing community on discord to discuss not only Soar/Pkgforge but also other cool projects, feel free to join & hangout anytime.
- [![Discord](https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=Discord)](https://discord.gg/djJUs48Zbu)

## Minimum Supported Rust Version (MSRV)

v1.88.0
