# Soar

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
[packages-shield]: https://img.shields.io/badge/dynamic/json?url=https://raw.githubusercontent.com/pkgforge/metadata/refs/heads/main/TOTAL_INSTALLABLE.json&query=$[5].total&label=packages&labelColor=grey&style=flat&link=https://pkgs.pkgforge.dev
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
        <img src="https://soar.pkgforge.dev/gif?version=v0.5.7" alt="soar-list" width="850">
    </a><br>
</p>

<p align="center">
    Soar is a Fast, Modern, Bloat-Free Distro-Independent Package Manager that <a href="https://docs.pkgforge.dev/soar/comparisons"> Just Works</a><br>
    Supports <a href="https://docs.pkgforge.dev/formats/binaries/static">Static Binaries</a>, <a href="https://docs.pkgforge.dev/formats/packages/appimage">AppImages</a>, and other <a href="https://docs.pkgforge.dev/formats/packages">Portable formats</a> on any <a href="https://docs.pkgforge.dev/repositories/soarpkgs/faq#portability">*Unix-based Distro</a>
</p>

## 🌟 Key Features

- [Distro Agnostic](https://docs.pkgforge.dev/soar/readme/packages#portability) (Read the [Manifesto](https://github.com/pkgforge/soarpkgs/blob/main/MANIFESTO.md))
- [Native Desktop Integration](https://soar.qaidvoid.dev/#desktop-integration)
- [SLSA Build L2 Security Guarantees](https://docs.pkgforge.dev/soar/readme/security)
- [Thousands of Prebuilt Packages](https://pkgs.pkgforge.dev/) ([Soar User Repository](https://github.com/pkgforge/soarpkgs))
- [Universal Package Support](https://soar.qaidvoid.dev/#universal-package-support)
- [& Much More](https://docs.pkgforge.dev/soar/comparisons)

## 🔧 Installation

Soar comes as a single-file, statically-linked executable with no dependencies that you can simply [download](https://github.com/pkgforge/soar/releases/latest) & run.
- Docs: https://soar.qaidvoid.dev/installation.html
- Extra Guide & Information: https://docs.pkgforge.dev

## ⚙️ Configuration

Soar comes with [sane defaults](https://soar.qaidvoid.dev/configuration.html) & [all repositories](https://docs.pkgforge.dev/repositories/) preconfigured at `~/.config/soar/config.toml`<br>
For additional configuration guide, click [here](https://soar.qaidvoid.dev/configuration.html)
> [!NOTE]
> Soar provides [External repositories](https://docs.pkgforge.dev/repositories/external), which aren't enabled by default.
> Enable them with `soar defconfig --external` if you haven't created configuration file yet. Or, add them manually using metadata from [here](https://meta.pkgforge.dev/external/).

## 🎯 Usage

Simply run `soar --help` for general options.
- General Guide & Manual is maintained at: [soar.qaidvoid.dev](https://soar.qaidvoid.dev/)
- Detailed guide regarding each format is at: [docs.pkgforge.dev](https://docs.pkgforge.dev/formats/packages)

## 📦 Packages
Our [Official repositories](https://docs.pkgforge.dev/repositories) use a novel recipe format called [`.SBUILD`](https://docs.pkgforge.dev/sbuild/introduction) at [pkgforge/soarpkgs](https://github.com/pkgforge/soarpkgs).
Our CI Infra builds these [packages](https://pkgs.pkgforge.dev/) periodically & syncs to the [cache](https://docs.pkgforge.dev/repositories/soarpkgs/faq#cache).<br>
Additionally we also support [AM](https://github.com/ivan-hc/AM) & [appimage.github.io](https://github.com/AppImage/appimage.github.io) as [external repositories](https://docs.pkgforge.dev/repositories/external)


What Packages are Available?
- Type `soar list` for a list
- Or visit [pkgs.pkgforge.dev](https://pkgs.pkgforge.dev/)<br>
To request new packages or report an issue with an existing one, please use the [pkgforge/soarpkgs](https://github.com/pkgforge/soarpkgs) repository.<br>

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
