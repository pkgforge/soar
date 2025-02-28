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
        <img src="https://soar.pkgforge.dev/gif?version=v0.5.8" alt="soar-list" width="850">
    </a><br>
</p>

<p align="center">
    Soar is a Fast, Modern, Bloat-Free Distro-Independent Package Manager that <a href="https://docs.pkgforge.dev/soar/comparisons"> Just Works</a><br>
    Supports <a href="https://docs.pkgforge.dev/formats/binaries/static">Static Binaries</a>, <a href="https://docs.pkgforge.dev/formats/packages/appimage">AppImages</a>, and other <a href="https://docs.pkgforge.dev/formats/packages">Portable formats</a> on any <a href="https://docs.pkgforge.dev/repositories/soarpkgs/faq#portability">*Unix-based Distro</a>
</p>

| 🌟 Key Features |
|:--:|
| ◆ **Portable Packages** |
| We only include packages that are [`portable`](https://docs.pkgforge.dev/formats/) i.e., they don't need dependencies/libraries. We do this by [statically linking](https://docs.pkgforge.dev/formats/binaries/static) everything wherever possible into a single executable. Otherwise, we bundle all dependencies/libraries into a [self-extractable](https://docs.pkgforge.dev/formats/packages/archive) or [fuse-mountable](https://docs.pkgforge.dev/formats/packages/appimage) bundle. Soar supports all these [universal formats](https://soar.qaidvoid.dev/#universal-package-support) out of the box. This makes Soar [Distro Agnostic](https://docs.pkgforge.dev/soar/readme/packages#portability), meaning most packages are truly standalone and will work on any Linux distro out of the box. |
| ◆ **System Integration** |
| Soar [natively integrates](https://soar.qaidvoid.dev/#desktop-integration) any package you install so they show up properly in your system menus, paths etc., just like a native package. |
| ◆ **AUR like Package Repository** |
| Our [Official repositories](https://docs.pkgforge.dev/repositories) use a novel recipe format called [`.SBUILD`](https://docs.pkgforge.dev/sbuild/introduction) at [pkgforge/soarpkgs](https://github.com/pkgforge/soarpkgs). Anyone can submit their own `.SBUILD` for a package & our [linters](https://github.com/pkgforge/sbuilder) will validate it, our [maintainers](https://github.com/orgs/pkgforge/people) will approve it & Our CI Infra will build these [packages](https://pkgs.pkgforge.dev/). We compile most (`80%`) our packages from source with sensible profiles: MUSL for lightweightness, LTO for performance, ASLR/PIE for security. We also re-package some packages from other distros by using [container based formats](https://docs.pkgforge.dev/formats/packages). For example, if a package is only on Arch repos, but you use a debian distro, you can still run our packages which was sourced from Arch without changing your distro or using docker/distrobox. |
| ◆ **Chaotic by Default** |
| Soar provides up-to-date [**prebuilts**](https://docs.pkgforge.dev/repositories/soarpkgs/faq#cache) for 100% of it's [official packages](https://github.com/pkgforge/soar#-packages). This means installs are instant, limited only by your bandwidth. |
| ◆ **Secure by Default** |
| Soar enforces security through checksums & signing. And our CI Infra meet [SLSA Build L2 Security Guarantees](https://docs.pkgforge.dev/soar/readme/security). |
| ◆ **Large Pool of Packages** |
| We have the [largest collection](https://docs.pkgforge.dev/soar/readme/packages#total) of portable packages. It is likely we already have your favourite packages, to check type `soar list` for a list or visit [pkgs.pkgforge.dev](https://pkgs.pkgforge.dev/). To request new packages or report an issue with an existing one, please use the [pkgforge/soarpkgs](https://github.com/pkgforge/soarpkgs) repository. |
| ◆ **External Repositories** |
| In addition to our [officially curated repositories](https://docs.pkgforge.dev/repositories/), we also provide support for external repositories like [ivan-hc/AM](https://github.com/ivan-hc/AM), [appimage.github.io](https://github.com/AppImage/appimage.github.io), [appimagehub](https://docs.pkgforge.dev/repositories/external/appimagehub) & [more](https://docs.pkgforge.dev/repositories/external). However be **careful** as we don't curate/verify any of these external sources. |
| ◆ **Custom Repositories** |
| Users can setup their [own custom repositories](https://soar.qaidvoid.dev/configuration#custom-respository-support) if they prefer not to use the [default ones](https://docs.pkgforge.dev/repositories/) managed by us. This allows users to have full control over what packages, what sources and from where everything is installed. |
| ◆ **Decentralized** |
| Users can download any of our packages without using soar and then use another package manager to install them. |

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
