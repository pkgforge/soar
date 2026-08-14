<div align="center">

[crates-shield]: https://img.shields.io/crates/v/soar-cli
[crates-url]: https://crates.io/crates/soar-cli
[downloads-shield]: https://img.shields.io/github/downloads/pkgforge/soar/total?label=downloads
[downloads-url]: https://github.com/pkgforge/soar/releases
[discord-shield]: https://img.shields.io/discord/1313385177703256064?logo=%235865F2&label=discord
[discord-url]: https://discord.gg/djJUs48Zbu
[doc-shield]: https://img.shields.io/badge/docs-soar.qaidvoid.dev-blue
[doc-url]: https://soar.qaidvoid.dev
[license-shield]: https://img.shields.io/github/license/pkgforge/soar.svg
[license-url]: https://github.com/pkgforge/soar/blob/main/LICENSE

# Soar

[![Crates.io][crates-shield]][crates-url]
[![Downloads][downloads-shield]][downloads-url]
[![Discord][discord-shield]][discord-url]
[![Documentation][doc-shield]][doc-url]
[![License: MIT][license-shield]][license-url]

**A fast, modern, distro-independent package manager that _just works_.**

Install static binaries, AppImages, and other portable formats
(AppBundle, FlatImage, RunImage, onelf, and more) on any Linux distribution.

</div>

## 📦 What is Soar?

Soar installs packages; it does not build or host them. Repositories publish
metadata in a standard format, and Soar reads that metadata to search, install,
and update packages right under your home directory.

That split is the whole point. [soarpkgs](https://github.com/pkgforge/soarpkgs)
is the default repository, but Soar is not tied to it. Add a third-party one, or
[run your own](https://soar.qaidvoid.dev/configuration#repositories).

It runs on any Linux distribution, with no superuser, no runtime dependencies,
and no distribution packages touched. What you can install depends on the
repository. soarpkgs publishes packages for **x86_64**, **aarch64**, and
**riscv64**.

## 🪄 Install

Soar is a single statically-linked binary. Grab it with the install script:

```bash
# curl
curl -fsSL "https://raw.githubusercontent.com/pkgforge/soar/main/install.sh" | sh

# wget
wget -qO- "https://raw.githubusercontent.com/pkgforge/soar/main/install.sh" | sh
```

> [!NOTE]
> - Read and verify the script before piping it to a shell.
> - It is also served from https://soar.qaidvoid.dev/install.sh.
> - Prefer to do it yourself? [Download a release](https://github.com/pkgforge/soar/releases/latest)
>   and drop the binary on your `PATH`.
> - To customize the install, see the [installation docs](https://soar.qaidvoid.dev/installation).

## 🚀 Usage

```bash
soar sync                  # fetch repository metadata
soar search ripgrep        # find a package
soar install ripgrep       # install it
soar run ripgrep           # run it once, without installing
soar list                  # everything available
soar info                  # what you have installed
soar update                # update everything
soar remove ripgrep        # remove it
```

A package can also come straight from a URL or a local file, and one installed
from a GitHub or GitLab release keeps tracking that release:

```bash
soar install https://github.com/owner/repo/releases/download/v1/tool
```

`soar --help` lists the rest. Full documentation lives at
[soar.qaidvoid.dev](https://soar.qaidvoid.dev/package-management).

## 🌟 Features

| Feature | Description |
|:--|:--|
| **Universal** | One statically-linked binary. No dependencies, no superuser, any Linux distribution. |
| **Portable formats** | Static binaries, AppImages, and other self-contained formats, installed the same way. |
| **Install from anywhere** | From a repository, a direct URL, or a local file. Releases installed from GitHub or GitLab stay up to date. |
| **Delta updates** | AppImages advertising a zsync feed update by fetching only the parts that changed. |
| **System integration** | Desktop entries, icons, man pages, and shell completions land where your system already looks. |
| **Secure by default** | Checksums and signatures are verified before anything is installed. |
| **Fast** | Parallel downloads and low-overhead package operations. |

## 🔑 Forge Tokens

Installing or updating from a release uses that forge's API, which is rate
limited. GitHub allows 60 requests an hour unauthenticated, which is easy to
reach, so set `GITHUB_TOKEN` or `GH_TOKEN` if you track several packages that
way. GitLab counts per minute and is rarely a problem, but honours
`GITLAB_TOKEN` and `GL_TOKEN` all the same.

## 🤝 Contributing

Contributions are welcome. Fork the repository, open a pull request, and see
[CONTRIBUTING.md](https://github.com/pkgforge/soar/blob/main/CONTRIBUTING.md)
for the guidelines.

## Minimum Supported Rust Version (MSRV)

v1.88.0
