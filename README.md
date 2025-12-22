<div align="center">

[crates-shield]: https://img.shields.io/crates/v/soar-cli
[crates-url]: https://crates.io/crates/soar-cli
[discord-shield]: https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=discord
[discord-url]: https://discord.gg/djJUs48Zbu
[doc-shield]: https://img.shields.io/badge/docs-soar.qaidvoid.dev-blue
[doc-url]: https://soar.qaidvoid.dev
[license-shield]: https://img.shields.io/github/license/pkgforge/soar.svg
[license-url]: https://github.com/pkgforge/soar/blob/main/LICENSE
[packages-shield]: https://img.shields.io/badge/dynamic/json?url=https://raw.githubusercontent.com/pkgforge/metadata/refs/heads/main/TOTAL_INSTALLABLE.json&query=$[6].total&label=packages&labelColor=grey&style=flat&link=https://pkgs.pkgforge.dev
[packages-url]: https://pkgs.pkgforge.dev

[![Crates.io][crates-shield]][crates-url]
[![Discord][discord-shield]][discord-url]
[![Documentation][doc-shield]][doc-url]
[![License: MIT][license-shield]][license-url]
[![Packages][packages-shield]][packages-url]

</div>

<h4 align="center">
  <a href="https://soar.qaidvoid.dev">📘 Documentation</a> |
  <a href="https://docs.pkgforge.dev">🔮 PackageForge</a>
</h4>

<p align="center">
    A fast, modern, bloat-free distro-independent package manager that <a href="https://docs.pkgforge.dev/soar/comparisons"> <i>just works</i></a><br>
    Supports <a href="https://docs.pkgforge.dev/formats/binaries/static">Static Binaries</a>, <a href="https://docs.pkgforge.dev/formats/packages/appimage">AppImages</a>, and other <a href="https://docs.pkgforge.dev/formats/packages">Portable formats</a> on any <a href="https://docs.pkgforge.dev/repositories/soarpkgs/faq#portability"><i>*Unix-based</i> distro</a>
</p>

## 📦 What is Soar?

Soar is a **package manager** - it doesn't build or host packages itself. Instead, it consumes package metadata from repositories and handles downloading, installing, and managing packages on your system.

**How it works:**
- **Repositories** (like [pkgforge](https://docs.pkgforge.dev/repositories)) build and host packages, providing metadata in a [standard format](https://docs.pkgforge.dev/repositories/bincache/metadata)
- **Soar** fetches this metadata, lets you search/install packages, and manages your local installations
- **You** can use the default pkgforge repositories, add third-party ones, or even create your own

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
> - The script is also available through https://soar.qaidvoid.dev/install.sh & https://soar.pkgforge.dev/install.sh
> - Additionally, if you want to customize your installation, please read the docs @ https://soar.qaidvoid.dev/installation.html
> - For extra guide & information on infra backends & adding more repos: https://docs.pkgforge.dev
> - Next, check [Configuration](https://soar.qaidvoid.dev/configuration) & [Usage](https://soar.qaidvoid.dev/package-management)

## 🌟 Key Features

| Feature | Description |
|:--:|:--|
| **Universal** | Single binary, no dependencies, works on any Unix-like system without superuser privileges. |
| **Portable Formats** | Install [static binaries](https://docs.pkgforge.dev/formats/binaries/static), [AppImages](https://docs.pkgforge.dev/formats/packages/appimage), and other [self-contained archives](https://docs.pkgforge.dev/formats/packages/archive) with ease. |
| **System Integration** | Automatically adds desktop entries and icons for a native feel. |
| **Repository Agnostic** | Works with any repository that provides compatible metadata. Use [official pkgforge repos](https://docs.pkgforge.dev/repositories), third-party sources, or [create your own](https://soar.qaidvoid.dev/configuration#custom-repository-support). |
| **Security First** | Enforces security through checksums and signature verification for package installations. |
| **Fast & Efficient** | Minimal overhead with parallel downloads and efficient package operations. |


## 📀 Default Repositories

Soar comes pre-configured with `pkgforge` repositories. These are the default package sources, but you can add or replace them with any compatible repository.

> **Note:** _✅ --> Enabled by Default_

| 🏆 Tier | 🤖 Architecture | 📦 Repositories | ℹ️ Status |
|---------|---------|---------------------------|-------------------|
| **Tier 1** | **`aarch64-Linux`** | [bincache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/bincache), [pkgcache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/pkgcache) | Almost as many packages as `x86_64-Linux`, fully supported |
| **Tier 1** | **`x86_64-Linux`** | [bincache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/bincache), [pkgcache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/pkgcache) | Primary target & most supported |
| **Tier 2** | **`riscv64-Linux`** | [bincache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/bincache), [pkgcache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/pkgcache) | Experimental, with [gradual progress](https://github.com/pkgforge/soarpkgs/issues/198) |


## 🤝 Contributing

We welcome contributions! Please feel free to fork the repository and submit pull requests. See [CONTRIBUTING.md](https://github.com/pkgforge/soar/blob/main/CONTRIBUTING.md) for contribution guidelines.

## 💬 Contact

We have a growing community on discord to discuss not only Soar/Pkgforge but also other cool projects, feel free to join & hangout anytime.
- [![Discord](https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=Discord)](https://discord.gg/djJUs48Zbu)
- Other Channels: https://docs.pkgforge.dev/contact/chat

## 🎀 Sponsors

- CICD run on free [Github Runners](https://docs.github.com/actions/using-github-hosted-runners/about-github-hosted-runners), Container Registry & [Package Storage](https://docs.github.com/en/packages/learn-github-packages/introduction-to-github-packages) on [ghcr.io](https://docs.github.com/packages/working-with-a-github-packages-registry/working-with-the-container-registry). These & much more are all generously provided by [GitHub](https://github.com/) [<img src="https://github.com/github.png?size=64" width="30" height="30">](https://github.com/)

- [`riscv64`](https://riscv.org/) [<img src="https://github.com/user-attachments/assets/cf5b988d-657a-47eb-889d-a1bdb014857a" width="30" height="30">](https://riscv.org/) CICD test machines are provided by [10x Engineer's](https://10xengineers.ai/) [<img src="https://github.com/user-attachments/assets/a2cceb62-9045-43b9-b5b2-384565f27ca5" width="30" height="30">](https://cloud-v.co/) [Cloud-V](https://cloud-v.co/) [<img src="https://github.com/user-attachments/assets/74d0fd73-4439-45d4-a756-b1c0c74d1816" width="30" height="30">](https://cloud-v.co/)

## Minimum Supported Rust Version (MSRV)

v1.82.0
