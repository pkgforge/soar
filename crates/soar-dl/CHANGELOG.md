
## [0.9.0](https://github.com/pkgforge/soar/compare/soar-dl-v0.8.0...soar-dl-v0.9.0) - 2026-02-21

### 🚜 Refactor

- *(download)* Remove proxy api - ([1d3e0ac](https://github.com/pkgforge/soar/commit/1d3e0acc8346834009711cb9f1ad4fbd3454849e))

## [0.8.0](https://github.com/pkgforge/soar/compare/soar-dl-v0.7.3...soar-dl-v0.8.0) - 2026-02-04

### ⛰️  Features

- *(self)* Add release notes display and improve update UX - ([e63648c](https://github.com/pkgforge/soar/commit/e63648c0ded70e694a89ab16a65c10649692adf7))

## [0.7.3](https://github.com/pkgforge/soar/compare/soar-dl-v0.7.2...soar-dl-v0.7.3) - 2026-01-24

### ⛰️  Features

- *(platforms)* Allow fallback token env for github/gitlab - ([ca94243](https://github.com/pkgforge/soar/commit/ca942433caf6a37f2816d2da87891b0bb1f6a593))

### 🐛 Bug Fixes

- *(dl)* Handle ureq StatusCode in fallback logic - ([27f5738](https://github.com/pkgforge/soar/commit/27f5738e78f5eb9e83eda9dc99879c2ae2381087))
- *(test)* Fix failing doctest - ([54e9107](https://github.com/pkgforge/soar/commit/54e91075754d78b0b7bd218eec4c680176af9b69))

## [0.7.2](https://github.com/pkgforge/soar/compare/soar-dl-v0.7.1...soar-dl-v0.7.2) - 2026-01-17

### ⛰️  Features

- *(apply)* Allow applying ghcr packages - ([06e2b73](https://github.com/pkgforge/soar/commit/06e2b73fce7f4189527b8868bb9adfe14d0600cc))

### 🐛 Bug Fixes

- *(clippy)* Apply clippy suggestions - ([1b45180](https://github.com/pkgforge/soar/commit/1b45180380790576d50f5c2430038efb0ca6d3a5))

### 🚜 Refactor

- *(error)* Don't override error messages - ([e44342f](https://github.com/pkgforge/soar/commit/e44342f3c23b9cdbe23df2739bcf04bde4138025))

## [0.7.1](https://github.com/pkgforge/soar/compare/soar-dl-v0.7.0...soar-dl-v0.7.1) - 2025-12-28

### 🐛 Bug Fixes

- *(install)* Use deterministic hash for package without checksum - ([7a7a060](https://github.com/pkgforge/soar/commit/7a7a06049c61ba38a52921c51cb90b57aee4b809))
- *(install)* Fix force reinstall cleanup and resume file corruption - ([c6150f7](https://github.com/pkgforge/soar/commit/c6150f72855249bd048194514dd3bdbca1beb21c))

## [0.7.0] - 2025-12-26

### ⛰️  Features

- *(crate)* Init soar-dl crate ([#102](https://github.com/pkgforge/soar/pull/102)) - ([8be00ab](https://github.com/pkgforge/soar/commit/8be00ab414accb3d03302b6bf85073919d73565d))

## [0.6.3] - 2025-06-03

### Changed

- Only create extract dir if the download is archive

### Fixed

- Fix file target when output path is provided

## [0.6.2] - 2025-06-01

### Changed

- Update dependencies

## [0.6.1] - 2025-05-17

### Added

- Add OCI resumability

### Changed

- Use async stdout
- Set default overwrite prompt
- Treat URL as direct link if only it has scheme and host

## [0.6.0] - 2025-05-04

### Added

- Add resumability and overwrite prompting
- Add glob support

### Changed

- Allow specifying http headers, proxy and user agent
- Use shared http client
- Allow specifying extract directory; fix extract when output is not specified
- Handle encoded tags, allow / and trim quotes in tags

## [0.5.3] - 2025-04-06

### Added

- Add support for streaming response to stdout

### Changed

- Revert "use hickory-dns"

## [0.5.2] - 2025-04-06

### Changed

- Update dependencies
- Use hickory-dns

## [0.5.1] - 2025-04-01

### Fixed

- Fix archive extract dir

## [0.5.0] - 2025-03-22

### Added

- Add support for archives

### Changed

- Prioritize filename from response header if not provided

## [0.4.2] - 2025-02-28

### Changed

- Truncate existing file instead of append

### Fixed

- Fix gitlab regex

## [0.4.0] - 2025-02-24

### Changed

- Fetch directly using tag api if tag is provided

## [0.3.5] - 2025-02-16

### Changed

- Return error if url is invalid

## [0.3.4] - 2025-02-08

### Changed

- Enhance OCI download state & support retries on OCI rate limit

## [0.3.3] - 2025-01-27

### Fixed

- Fix parsing github release without name

## [0.3.2] - 2025-01-25

### Added

- Add keyword matching support for OCI downloads
- Add custom API and concurrency support for OCI downloads

## [0.3.1] - 2025-01-18

### Fixed

- Fix oci download progress

## [0.3.0] - 2025-01-18

### Added

- Add oci blob download support
- Add support for download OCI packages

### Changed

- Simplify download state

## [0.2.0] - 2025-01-11

### Changed

- Handle github/gitlab project passed as link

## [0.1.2] - 2024-12-19

### Added

- Add name field to releases

## [0.1.1] - 2024-12-05

### Added

- Add workflow

### Changed

- Handle tags
- Initialize soar-dl
- Initial commit

[0.6.3]: https://github.com/pkgforge/soar-dl/compare/v0.6.2..v0.6.3
[0.6.2]: https://github.com/pkgforge/soar-dl/compare/v0.6.1..v0.6.2
[0.6.1]: https://github.com/pkgforge/soar-dl/compare/v0.6.0..v0.6.1
[0.6.0]: https://github.com/pkgforge/soar-dl/compare/v0.5.3..v0.6.0
[0.5.3]: https://github.com/pkgforge/soar-dl/compare/v0.5.2..v0.5.3
[0.5.2]: https://github.com/pkgforge/soar-dl/compare/v0.5.1..v0.5.2
[0.5.1]: https://github.com/pkgforge/soar-dl/compare/v0.5.0..v0.5.1
[0.5.0]: https://github.com/pkgforge/soar-dl/compare/v0.4.2..v0.5.0
[0.4.2]: https://github.com/pkgforge/soar-dl/compare/v0.4.0..v0.4.2
[0.4.0]: https://github.com/pkgforge/soar-dl/compare/v0.3.5..v0.4.0
[0.3.5]: https://github.com/pkgforge/soar-dl/compare/v0.3.4..v0.3.5
[0.3.4]: https://github.com/pkgforge/soar-dl/compare/v0.3.3..v0.3.4
[0.3.3]: https://github.com/pkgforge/soar-dl/compare/v0.3.2..v0.3.3
[0.3.2]: https://github.com/pkgforge/soar-dl/compare/v0.3.1..v0.3.2
[0.3.1]: https://github.com/pkgforge/soar-dl/compare/v0.3.0..v0.3.1
[0.3.0]: https://github.com/pkgforge/soar-dl/compare/v0.2.0..v0.3.0
[0.2.0]: https://github.com/pkgforge/soar-dl/compare/v0.1.2..v0.2.0
[0.1.2]: https://github.com/pkgforge/soar-dl/compare/v0.1.1..v0.1.2
