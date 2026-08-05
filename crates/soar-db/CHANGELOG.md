
## [0.6.1](https://github.com/pkgforge/soar/compare/soar-db-v0.6.0...soar-db-v0.6.1) - 2026-08-05

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-registry - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.6.0](https://github.com/pkgforge/soar/compare/soar-db-v0.5.5...soar-db-v0.6.0) - 2026-08-02

### ⛰️  Features

- *(install)* Record where a URL install came from - ([d6c83ad](https://github.com/pkgforge/soar/commit/d6c83adf7ee42783d1f415b3c1c2a601f6b6d9c1))
- *(remove)* Accept the URL a package was installed from - ([cb95df8](https://github.com/pkgforge/soar/commit/cb95df8a081d4d363ec1c9fde3b6e207dc3ab218))
- *(update)* Update URL-installed AppImages over zsync - ([509df1f](https://github.com/pkgforge/soar/commit/509df1fb1ca0d0e50d12f6eee1652d368acb71ba))
- [**breaking**] Consume the declarative index, drop the pkg_id requirement ([#186](https://github.com/pkgforge/soar/pull/186)) - ([3a35ad7](https://github.com/pkgforge/soar/commit/3a35ad7774e7ac3d8c055e4257cb3e9dff5be2fe))

### 🐛 Bug Fixes

- *(update)* Keep matching when a repo stops publishing families - ([a04c9a7](https://github.com/pkgforge/soar/commit/a04c9a75cf5807dd89aa5bbcaa3397f8aee97f14))

### 📚 Documentation

- Cover forge tokens and rate limits - ([ccdd34a](https://github.com/pkgforge/soar/commit/ccdd34ad3f09a90994b85e917a45885fd1c3e413))
- Refresh the readme and contributing guidelines - ([5ecd397](https://github.com/pkgforge/soar/commit/5ecd397e853d7d601677766ccf73dc68c063f015))

## [0.5.5](https://github.com/pkgforge/soar/compare/soar-db-v0.5.4...soar-db-v0.5.5) - 2026-07-16

### 🐛 Bug Fixes

- *(security)* Validate pkg_name and pkg_id as path components ([#184](https://github.com/pkgforge/soar/pull/184)) - ([97a0f57](https://github.com/pkgforge/soar/commit/97a0f57e3a4bd398dbf98c50be060a928e1aacff))
- *(security)* Validate repository names to block path traversal ([#183](https://github.com/pkgforge/soar/pull/183)) - ([c4b34f9](https://github.com/pkgforge/soar/commit/c4b34f9e0755ee43f2598dc4da783866394ea5fd))
- *(security)* Validate provides names to block path traversal ([#182](https://github.com/pkgforge/soar/pull/182)) - ([034b085](https://github.com/pkgforge/soar/commit/034b085b8938fd9b8e724d43372c3ef93b9ef411))

## [0.5.4](https://github.com/pkgforge/soar/compare/soar-db-v0.5.3...soar-db-v0.5.4) - 2026-06-25

### ⚙️ Miscellaneous Tasks

- Update Cargo.toml dependencies - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.5.3](https://github.com/pkgforge/soar/compare/soar-db-v0.5.2...soar-db-v0.5.3) - 2026-06-06

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-registry - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.5.2](https://github.com/pkgforge/soar/compare/soar-db-v0.5.1...soar-db-v0.5.2) - 2026-06-04

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-registry - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.5.1](https://github.com/pkgforge/soar/compare/soar-db-v0.5.0...soar-db-v0.5.1) - 2026-04-10

### ⛰️  Features

- *(search)* Add fuzzy search and "did you mean?" suggestions - ([934b0ff](https://github.com/pkgforge/soar/commit/934b0ffe6f9014a833f9c9bbe1b41772298932c5))

## [0.5.0](https://github.com/pkgforge/soar/compare/soar-db-v0.4.0...soar-db-v0.5.0) - 2026-02-24

### ⛰️  Features

- *(provides)* Add @ prefix to symlink packages directly to bin - ([cc8458a](https://github.com/pkgforge/soar/commit/cc8458ab722f4287315fee7a457be0191c10a19d))

### 🐛 Bug Fixes

- *(clippy)* Apply clippy suggestions - ([7b85532](https://github.com/pkgforge/soar/commit/7b85532d78baa32ee9541a2d764242656a8c07ba))
- *(provides)* Remove provides filter and add bin_symlink_names helper - ([5ed1951](https://github.com/pkgforge/soar/commit/5ed1951c71c47e12098e6485c607fd5c315fb5a4))

### 🚜 Refactor

- *(db)* Add pkg_family, drop recurse_provides - ([1d97b6d](https://github.com/pkgforge/soar/commit/1d97b6d0f9dc230a306fee936dc6571a0a658be3))
- *(system)* Add per-context system mode support - ([10544ac](https://github.com/pkgforge/soar/commit/10544ac8a2bd896152448f79650c6d98db0d960a))

### 📚 Documentation

- *(readme)* Update readme - ([4fc58a7](https://github.com/pkgforge/soar/commit/4fc58a774b4c968db8f4d69f7f809378573b4145))

### ⚙️ Miscellaneous Tasks

- *(manifest)* Remove deprecated authors field - ([0bf1231](https://github.com/pkgforge/soar/commit/0bf123139798f2efb1674c8a14eaaf4f4640dc2a))

## [0.4.0](https://github.com/pkgforge/soar/compare/soar-db-v0.3.2...soar-db-v0.4.0) - 2026-02-04

### ⛰️  Features

- *(nest)* [**breaking**] Remove nest functionality - ([dc21853](https://github.com/pkgforge/soar/commit/dc21853a2506d93d5ade9e2c4015c3a12b24c199))

## [0.3.2](https://github.com/pkgforge/soar/compare/soar-db-v0.3.1...soar-db-v0.3.2) - 2026-01-24

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-registry - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.3.1](https://github.com/pkgforge/soar/compare/soar-db-v0.3.0...soar-db-v0.3.1) - 2026-01-17

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-registry - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.3.0](https://github.com/pkgforge/soar/compare/soar-db-v0.2.0...soar-db-v0.3.0) - 2026-01-17

### 🚜 Refactor

- *(db)* Drop with_pkg_id - ([fa99208](https://github.com/pkgforge/soar/commit/fa99208ec1132c720c0065c7ab3eb235db187d34))
- *(error)* Don't override error messages - ([e44342f](https://github.com/pkgforge/soar/commit/e44342f3c23b9cdbe23df2739bcf04bde4138025))
- *(query)* Update query field icons - ([695a427](https://github.com/pkgforge/soar/commit/695a427ef6a4874cb212cdceed192f94150c5548))

## [0.2.0](https://github.com/pkgforge/soar/compare/soar-db-v0.1.0...soar-db-v0.2.0) - 2025-12-28

### 🐛 Bug Fixes

- *(install)* Fix force reinstall cleanup and resume file corruption - ([c6150f7](https://github.com/pkgforge/soar/commit/c6150f72855249bd048194514dd3bdbca1beb21c))

## [0.1.0] - 2025-12-26

### ⛰️  Features

- *(crate)* Init soar-db crate ([#98](https://github.com/pkgforge/soar/pull/98)) - ([8f84b79](https://github.com/pkgforge/soar/commit/8f84b791c7dd2a429baf1e529da0315b33bdc799))
