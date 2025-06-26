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
        <img src="https://soar.pkgforge.dev/gif?version=v0.6.3+1" alt="soar-list" width="750">
    </a><br>
</p>

<h4 align="center">
  <a href="https://soar.qaidvoid.dev">📘 Documentation</a> |
  <a href="https://docs.pkgforge.dev">🔮 PackageForge</a>
</h4>

<p align="center">
    Soar is a Fast, Modern, Bloat-Free Distro-Independent Package Manager that <a href="https://docs.pkgforge.dev/soar/comparisons"> <i>Just Works</i></a><br>
    Supports <a href="https://docs.pkgforge.dev/formats/binaries/static">Static Binaries</a>, <a href="https://docs.pkgforge.dev/formats/packages/appimage">AppImages</a>, and other <a href="https://docs.pkgforge.dev/formats/packages">Portable formats</a> on any <a href="https://docs.pkgforge.dev/repositories/soarpkgs/faq#portability"><i>*Unix-based</i> Distro</a>
</p>


## 🪄 Quickstart

> [!TIP]
> - Soar comes as a single-file, statically-linked executable with no dependencies that you can simply [download](https://github.com/pkgforge/soar/releases/latest) & run.
> - The [install script](https://github.com/pkgforge/soar/blob/main/install.sh) does this & more automatically for you.
 
```bash
❯ cURL
curl -fsSL "https://raw.githubusercontent.com/pkgforge/soar/main/install.sh" | sh

❯ wget
wget -qO- "https://raw.githubusercontent.com/pkgforge/soar/main/install.sh" | sh
```

> [!NOTE]
> - Please read & verify what's inside the script before running it
> - The script is also available through https://soar.qaidvoid.dev/install.sh & https://soar.pkgforge.dev/install.sh
> - Additionally, if you want to customize your installation, please read the docs @ https://soar.qaidvoid.dev/installation.html
> - For, extra Guide & Information on infra backends & adding more repos: https://docs.pkgforge.dev
> - Next, Check [Configuration](https://soar.qaidvoid.dev/configuration) & [Usage](https://soar.qaidvoid.dev/package-management)

## 🌟 Key Features

> [!TIP]
> - The comparison page @ https://docs.pkgforge.dev/soar/readme goes into more detail.

| Feature | Description |
|:--:|:--|
| **Universal Package Format Support** | Soar can install and manage portable package formats including [static binaries](https://docs.pkgforge.dev/formats/binaries/static), [self-extractable archives](https://docs.pkgforge.dev/formats/packages/archive), and [AppImages](https://docs.pkgforge.dev/formats/packages/appimage). |
| **System Integration** | Soar [automatically integrates](https://soar.qaidvoid.dev/#desktop-integration) installed packages with your system to provide a native-like experience. |
| **Flexible Repository System** | While Soar comes preconfigured with [official repositories](https://docs.pkgforge.dev/repositories), you can [configure custom repositories](https://soar.qaidvoid.dev/configuration#custom-repository-support) that use any build format as long as they provide compatible metadata. The `.SBUILD` format is only required for the official repositories, not for custom ones. |
| **Security First** | Soar enforces security through checksums and signing verification for package installations. |
| **Userspace** | Soar works completely in Userspace without Superuser (admin/sudo) Privileges. |
| **External Repository Support** | Soar can access packages from sources like [ivan-hc/AM](https://github.com/ivan-hc/AM) and [appimage.github.io](https://github.com/AppImage/appimage.github.io) through metadata provided by pkgforge. These external sources don't directly work with soar but are made compatible through pkgforge's metadata conversion. **Note:** Packages from external repositories are not verified. |
| **Fast Package Operations** | Soar provides efficient package searching, installation, and management with minimal overhead. |

## 📦 Packages

> [!TIP]
> Check out the detailed documentation @ https://docs.pkgforge.dev/repositories/soarpkgs

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
| **Re-Distribution** | All FOSS projects are granted free access to our complete package catalogue for [re-distribution](https://docs.pkgforge.dev/repositories/soarpkgs/re-distribution). |

> [!TIP]
> - You can [self host](https://soar.qaidvoid.dev/configuration#custom-repository-support) your own repos if you need soar to run on other [supported hosts](https://docs.pkgforge.dev/sbuild/specification/20.x_exec#host).
> - Or you can also [sponsor us](https://github.com/pkgforge/soar#-sponsors) with Build Servers for CICD & Testing. 


### 📀 Default Hosts

> **Note:** _✅ --> Enabled by Default_

| 🏆 Tier | 🤖 Host | 📦 Repos | ℹ️ Status |
|---------|---------|---------------------------|-------------------|
| **Tier 1** | **`aarch64-Linux`** | [bincache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/bincache), [pkgcache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/pkgcache), [pkgforge-cargo<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-cargo), [pkgforge-go<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-go), [cargo-bins](https://docs.pkgforge.dev/repositories/external/cargo-bins), [appimage-github-io](https://docs.pkgforge.dev/repositories/external/appimage-github-io), [appimagehub](https://docs.pkgforge.dev/repositories/external/appimagehub) | Almost as many packages as `x86_64-Linux`, fully supported |
| **Tier 1** | **`x86_64-Linux`** | [bincache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/bincache), [pkgcache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/pkgcache), [pkgforge-cargo<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-cargo), [pkgforge-go<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-go), [cargo-bins](https://docs.pkgforge.dev/repositories/external/cargo-bins), [ivan-hc-am](https://docs.pkgforge.dev/repositories/external/ivan-hc-am), [appimage-github-io](https://docs.pkgforge.dev/repositories/external/appimage-github-io), [appimagehub](https://docs.pkgforge.dev/repositories/external/appimagehub) | Primary target & most supported |
| **Tier 2** | **`loongarch64-Linux`** | [pkgforge-cargo<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-cargo), [pkgforge-go<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-go) | Experimental & least supported |
| **Tier 2** | **`riscv64-Linux`** | [bincache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/bincache), [pkgcache<sup>✅</sup>](https://docs.pkgforge.dev/repositories/pkgcache), [pkgforge-cargo<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-cargo), [pkgforge-go<sup>✅</sup>](https://docs.pkgforge.dev/repositories/external/pkgforge-go) | Experimental, with [gradual progress](https://github.com/pkgforge/soarpkgs/issues/198) |

## ☢️ Caveats

> [!NOTE]
> - If you have additional questions, check our [FAQ](https://docs.pkgforge.dev/repositories/soarpkgs/faq) or [create a discussion](https://github.com/pkgforge/soar/discussions).

| Design Decisions | The Rationale |
|:--:|:--|
| **Not Reinvent things** | Soar isn't a package manager in the traditional sense, neither is it a [new standard](https://xkcd.com/927/). Think of soar as an amalgamation & the natural progression of tools like [AM](https://github.com/ivan-hc/AM), [bin](https://github.com/marcosnils/bin), [eget](https://github.com/zyedidia/eget), [hysp](https://github.com/pwnwriter/hysp), [nami](https://github.com/txthinking/nami) & [zap](https://github.com/srevinsaju/zap). |
| **Not a System Package Manager** | Soar intentionally complements rather than replaces your distro's package manager. Unlike [Homebrew](https://github.com/Homebrew/brew), we don't handle core system tools/libraries — we let distro package managers excel at that job. Soar provides additional packages or newer versions while avoiding conflicts by operating entirely in userspace and following XDG specifications. |
| **Not a Devtools Manager** | Soar doesn't handle development toolchains by design. We do have completely static/relocatable toolchains in our repo, but it will always be better to just use dedicated tools like [asdf](https://github.com/asdf-vm/asdf), [aqua](https://github.com/aquaproj/aqua), [chsrc](https://github.com/RubyMetric/chsrc), [mise](https://github.com/jdx/mise), [vfox](https://github.com/version-fox/vfox), [x-cmd](https://github.com/x-cmd/x-cmd) etc. |

## 🐞 Bug Reports & Feature Requests

> [!WARNING]
> For reporting any issues related to packaging (Not Soar Core), please use our [Official package repository](https://docs.pkgforge.dev/repositories) at [pkgforge/soarpkgs](https://github.com/pkgforge/soarpkgs).

Soar is a FOSS project primarily maintained by [@QaidVoid](https://github.com/QaidVoid) & a few other [core contributors](https://github.com/pkgforge/soar/graphs/contributors), who are all volunteers and do it as a hobby.<br>
To save our time triaging & to ensure your issue/feature-request gets addressed quickly, make sure to:
- Search the [Issues](https://github.com/pkgforge/soar/issues) & [Discussion](https://github.com/pkgforge/soar/discussions?discussions_q=) tab (Both Closed/Opened) for same/similar issue in case it was already addressed.
- [Open a Discussion](https://github.com/pkgforge/soar/discussions/new/choose) instead of an issue if you have a question. Issues should be only created for bug reports and feature requests.<br>
- Use our [Issue Templates](https://github.com/pkgforge/soar/issues/new/choose) rather than a blank issue.<br>

> [!NOTE]
> - We assign a specific [priority level (`p0-p3`)](https://github.com/pkgforge/soar/labels) for each [valid issue](https://github.com/pkgforge/soar/issues) created.
> - Based on the assigned [priority level (`p0-p3`)](https://github.com/pkgforge/soar/labels) & our free time, we will do our best to respond/address it.
> - However, this is not a guarantee or an [SLA](https://en.wikipedia.org/wiki/Service-level_agreement). Please have patience & wait before tagging us again for a response. We thank you for your understanding.

## 💬 Community

We have a growing community on discord to discuss not only Soar/Pkgforge but also other cool projects, feel free to join & hangout anytime.
- [![Discord](https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=Discord)](https://discord.gg/djJUs48Zbu)
- Other Channels: https://docs.pkgforge.dev/contact/chat

## 🤝 Contributing

> [!WARNING]
> While we welcome contributions of all kinds, please read [CONTRIBUTING.md](https://github.com/pkgforge/soar/blob/main/CONTRIBUTING.md) before submitting us a PR.

Please feel free to:
1. Fork the repository
2. Create your feature branch
3. Submit a pull request

---

## 📊 Repo Stats

![Alt](https://repobeats.axiom.co/api/embed/7c089611431897ab74236ac506187c2f563c2886.svg "Repobeats analytics image")
[![Stargazers](https://reporoster.com/stars/dark/pkgforge/soar)](https://github.com/pkgforge/soar/stargazers)
[![Stargazers over time](https://starchart.cc/pkgforge/soar.svg?variant=dark)](https://starchart.cc/pkgforge/soar)

## 🎀 Sponsors

- CICD run on free [Github Runners](https://docs.github.com/actions/using-github-hosted-runners/about-github-hosted-runners), Container Registry & [Package Storage](https://docs.github.com/en/packages/learn-github-packages/introduction-to-github-packages) on [ghcr.io](https://docs.github.com/packages/working-with-a-github-packages-registry/working-with-the-container-registry). These & much more are all generously provided by [GitHub](https://github.com/) [<img src="https://github.com/github.png?size=64" width="30" height="30">](https://github.com/)

- [`riscv64`](https://riscv.org/) [<img src="https://github.com/user-attachments/assets/cf5b988d-657a-47eb-889d-a1bdb014857a" width="30" height="30">](https://riscv.org/) CICD test machines are provided by [10x Engineer's](https://10xengineers.ai/) [<img src="https://github.com/user-attachments/assets/a2cceb62-9045-43b9-b5b2-384565f27ca5" width="30" height="30">](https://cloud-v.co/) [Cloud-V](https://cloud-v.co/) [<img src="https://github.com/user-attachments/assets/74d0fd73-4439-45d4-a756-b1c0c74d1816" width="30" height="30">](https://cloud-v.co/)

## 📝 License

This project is licensed under [MIT](https://spdx.org/licenses/MIT.html) - see the [LICENSE](LICENSE) file for details.<br><br>
[![FOSSA Status](https://app.fossa.com/api/projects/git%2Bgithub.com%2Fpkgforge%2Fsoar.svg?type=large)](https://app.fossa.com/projects/git%2Bgithub.com%2Fpkgforge%2Fsoar?ref=badge_large)

## Minimum Supported Rust Version (MSRV)

v1.82.0
