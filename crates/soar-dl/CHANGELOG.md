
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
