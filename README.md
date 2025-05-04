<div align="center">

[crates-shield]: https://img.shields.io/crates/v/soar-cli
[crates-url]: https://crates.io/crates/soar-cli
[discord-shield]: https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=discord
[discord-url]: https://discord.gg/djJUs48Zbu
[doc-shield]: https://img.shields.io/badge/docs-soar.qaidvoid.dev-blue
[doc-url]: https://soar.qaidvoid.dev
[issues-shield]: https://img.shields.io/github/issues/pkgforge/soar.svg
[issues-url]: https://github.com/pkgforge/soar/issues
[license-shield]: https://img.shields.io/github/license/pkgforge/soar.svg
[license-url]: https://github.com/pkgforge/soar/blob/main/LICENSE
[packages-shield]: https://img.shields.io/badge/dynamic/json?url=https://raw.githubusercontent.com/pkgforge/metadata/refs/heads/main/TOTAL_INSTALLABLE.json&query=$[6].total&label=packages&labelColor=grey&style=flat&link=https://pkgs.pkgforge.dev
[packages-url]: https://pkgs.pkgforge.dev
[stars-shield]: https://img.shields.io/github/stars/pkgforge/soar.svg
[stars-url]: https://github.com/pkgforge/soar/stargazers

[![Crates.io][crates-shield]][crates-url]
[![Discord][discord-shield]][discord-url]
[![Documentation][doc-shield]][doc-url]
[![Issues][issues-shield]][issues-url]
[![License: MIT][license-shield]][license-url]
[![Packages][packages-shield]][packages-url]
[![Stars][stars-shield]][stars-url]

</div>

<p align="center">
    <a href="https://soar.qaidvoid.dev/installation">
        <img src="https://soar.pkgforge.dev/gif?version=v0.5.8" alt="soar-list" width="850">
    </a><br>
</p>

<p align="center">
    Soar is a Fast, Modern, Bloat-Free Distro-Independent Package Manager that <a href="https://docs.pkgforge.dev/soar/comparisons"> Just Works</a><br>
    Supports <a href="https://docs.pkgforge.dev/formats/binaries/static">Static Binaries</a>, <a href="https://docs.pkgforge.dev/formats/packages/appimage">AppImages</a>, and other <a href="https://docs.pkgforge.dev/formats/packages">Portable formats</a> on any <a href="https://docs.pkgforge.dev/repositories/soarpkgs/faq#portability">*Unix-based Distro</a>
</p>

# Soar Package Manager

## 🌟 Key Features

| Feature | Description |
|:--:|:--|
| **Universal Package Format Support** | Soar can install and manage portable package formats including [static binaries](https://docs.pkgforge.dev/formats/binaries/static), [self-extractable archives](https://docs.pkgforge.dev/formats/packages/archive), and [AppImages](https://docs.pkgforge.dev/formats/packages/appimage). |
| **System Integration** | Soar [automatically integrates](https://soar.qaidvoid.dev/#desktop-integration) installed packages with your system to provide a native-like experience. |
| **Flexible Repository System** | While Soar comes preconfigured with [official repositories](https://docs.pkgforge.dev/repositories), you can [configure custom repositories](https://soar.qaidvoid.dev/configuration#custom-repository-support) that use any build format as long as they provide compatible metadata. The `.SBUILD` format is only required for the official repositories, not for custom ones. |
| **Security First** | Soar enforces security through checksums and signing verification for package installations. |
| **External Repository Support** | Soar can access packages from sources like [ivan-hc/AM](https://github.com/ivan-hc/AM) and [appimage.github.io](https://github.com/AppImage/appimage.github.io) through metadata provided by pkgforge. These external sources don't directly work with soar but are made compatible through pkgforge's metadata conversion. **Note:** Packages from external repositories are not verified. |
| **Fast Package Operations** | Soar provides efficient package searching, installation, and management with minimal overhead. |

## 📦 Packages Available Through Official Repositories

Packages in the official Soar repositories have these characteristics:

| Feature | Description |
|:--:|:--|
| **Portable Packages** | Packages are designed to be [portable](https://docs.pkgforge.dev/formats/) across distributions, either through [static linking](https://docs.pkgforge.dev/formats/binaries/static) or by bundling all dependencies. This makes them [distro-agnostic](https://docs.pkgforge.dev/soar/readme/packages#portability). |
| **Extensive Collection** | Official repositories host one of the [largest collections](https://docs.pkgforge.dev/soar/readme/packages#total) of portable packages. Browse them with `soar list` or at [pkgs.pkgforge.dev](https://pkgs.pkgforge.dev/). |
| **Prebuilt Binaries** | 100% of official packages are provided as [prebuilts](https://docs.pkgforge.dev/repositories/soarpkgs/faq#cache), making installation limited only by download speed. |
| **Quality Compilation** | Around 80% of packages are compiled from source with optimizations for performance (LTO), security (ASLR/PIE), and size (MUSL). |
| **High Security Standards** | Official packages are built with [SLSA Build L2 Security Guarantees](https://docs.pkgforge.dev/soar/readme/security). |
| **Community Contributions** | The [`.SBUILD`](https://docs.pkgforge.dev/sbuild/introduction) format in [pkgforge/soarpkgs](https://github.com/pkgforge/soarpkgs) allows community members to submit package definitions, similar to AUR. |
| **Cross-Distro Compatibility** | Some packages are repackaged from other distro repositories, allowing you to run applications from e.g., Arch repositories on Debian-based systems without containers. |
| **Decentralized** | The portable nature of packages means they can be downloaded and used independently of Soar if needed. |

## ☢️ Caveats
#### Soar doesn't reinvent things
Soar isn't a package manager in the traditional sense, neither is it a [new standard](https://xkcd.com/927/), thus soar doesn't handle core system tools/libraries by design.
This means soar is not a replacement for your distro's official package manager.<br>
Instead, Soar complements existing package managers by providing your distro with additional packages or newer version of packages that your distro may not provide.<br>
Soar is meant to coexist with existing package managers by avoiding conflicts, being completely functional in userspace & using XDG Specifications.<br>
For more questions, check our [FAQ](https://docs.pkgforge.dev/repositories/soarpkgs/faq) or [create a discussion](https://github.com/pkgforge/soar/discussions).

## 🔧 Installation

Soar comes as a single-file, statically-linked executable with no dependencies that you can simply [download](https://github.com/pkgforge/soar/releases/latest) & run.
- Docs: https://soar.qaidvoid.dev/installation.html
- Extra Guide & Information: https://docs.pkgforge.dev

## ⚙️ Configuration

Soar comes with [sane defaults](https://soar.qaidvoid.dev/configuration.html) & [official repositories](https://docs.pkgforge.dev/repositories/) preconfigured<br>
For additional configuration guide, see [here](https://soar.qaidvoid.dev/configuration.html)

## 🎯 Usage

Simply run `soar --help` for general options.
- General Guide & Manual is maintained at: [soar.qaidvoid.dev](https://soar.qaidvoid.dev/)
- Detailed guide regarding each format is at: [docs.pkgforge.dev](https://docs.pkgforge.dev/formats/packages)

## 📦 Packages
For reporting any issues related to packaging, please use our [Official package repository](https://docs.pkgforge.dev/repositories) at [pkgforge/soarpkgs](https://github.com/pkgforge/soarpkgs).<br>

## 💬 Community

Be a part of our community to interact with our team, get quick help, and share your ideas
- [![Discord](https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=Discord)](https://discord.gg/djJUs48Zbu)
- Other Channels: https://docs.pkgforge.dev/contact/chat

## 🤝 Contributing

We welcome contributions! Please feel free to fork the repository and submit
pull requests. If you have suggestions or feature requests, open an [discussion](https://github.com/pkgforge/soar/discussions) to
discuss.

Please feel free to:
1. Fork the repository
2. Create your feature branch
3. Submit a pull request

![Alt](https://repobeats.axiom.co/api/embed/7c089611431897ab74236ac506187c2f563c2886.svg "Repobeats analytics image")
[![Stargazers](https://reporoster.com/stars/dark/pkgforge/soar)](https://github.com/pkgforge/soar/stargazers)
[![Stargazers over time](https://starchart.cc/pkgforge/soar.svg?variant=dark)](https://starchart.cc/pkgforge/soar)

## 📝 License

This project is licensed under [MIT](https://spdx.org/licenses/MIT.html) - see the [LICENSE](LICENSE) file for details.<br>
[![FOSSA Status](https://app.fossa.com/api/projects/git%2Bgithub.com%2Fpkgforge%2Fsoar.svg?type=large)](https://app.fossa.com/projects/git%2Bgithub.com%2Fpkgforge%2Fsoar?ref=badge_large)

## Minimum Supported Rust Version (MSRV)

v1.82.0
