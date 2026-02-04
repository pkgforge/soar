
## [0.5.0](https://github.com/pkgforge/soar/compare/soar-config-v0.4.0...soar-config-v0.5.0) - 2026-02-04

### ⛰️  Features

- *(config)* Allow setting path for desktop files - ([50c0335](https://github.com/pkgforge/soar/commit/50c033592d5611f4a982c20c45a0242b4826e93d))
- *(nest)* [**breaking**] Remove nest functionality - ([dc21853](https://github.com/pkgforge/soar/commit/dc21853a2506d93d5ade9e2c4015c3a12b24c199))

### 🐛 Bug Fixes

- *(config)* Fix default repositories detection - ([22c121e](https://github.com/pkgforge/soar/commit/22c121ed2f134274a1edca9a174a4efa076b91c9))

### 🚜 Refactor

- *(config)* Remove --external flag - ([3b53b8b](https://github.com/pkgforge/soar/commit/3b53b8bd91e322df21f7e4466f7d7640330fb613))

## [0.4.0](https://github.com/pkgforge/soar/compare/soar-config-v0.3.0...soar-config-v0.4.0) - 2026-01-24

### ⛰️  Features

- *(config)* Add placeholder support and remove update field - ([824d060](https://github.com/pkgforge/soar/commit/824d0600b342ad5c921fffb3677102377f74ec47))
- *(config)* Make link_as optional and add glob support in binary maps - ([c3945ee](https://github.com/pkgforge/soar/commit/c3945ee556b00713d9f71eb5119a7580d19d6ce1))

## [0.3.0](https://github.com/pkgforge/soar/compare/soar-config-v0.2.0...soar-config-v0.3.0) - 2026-01-17

### 🐛 Bug Fixes

- *(system)* [**breaking**] Change system install path to /opt/soar - ([e694e30](https://github.com/pkgforge/soar/commit/e694e305958fb5def3c5e06946e4e8fa4c625b1a))

## [0.2.0](https://github.com/pkgforge/soar/compare/soar-config-v0.1.1...soar-config-v0.2.0) - 2026-01-17

### ⛰️  Features

- *(cli)* Add system-wide package management ([#141](https://github.com/pkgforge/soar/pull/141)) - ([f8d4f1c](https://github.com/pkgforge/soar/commit/f8d4f1c4e0e230427cd037355ba4a23da5b28a6b))
- *(install)* Add entrypoint option and executable discovery fallbacks - ([b77cffd](https://github.com/pkgforge/soar/commit/b77cffdd6cbdfd66518c1613313d53e1c102a7a2))
- *(packages)* Add github/gitlab as first-class package sources ([#142](https://github.com/pkgforge/soar/pull/142)) - ([2fc3c3b](https://github.com/pkgforge/soar/commit/2fc3c3b4f8e08dd9eac828dbf4f77128f186c91f))
- *(packages)* Add hooks, build commands, and sandbox support ([#140](https://github.com/pkgforge/soar/pull/140)) - ([a776d61](https://github.com/pkgforge/soar/commit/a776d61c7e7f57567a05b18c1baf683c96f08dff))
- *(update)* Allow updating remote URL packages ([#137](https://github.com/pkgforge/soar/pull/137)) - ([af13bb6](https://github.com/pkgforge/soar/commit/af13bb637c8c4c4a89cfdac451e39b105e7ee378))

### 🐛 Bug Fixes

- *(packages)* Skip version fetching when installed version matches ([#143](https://github.com/pkgforge/soar/pull/143)) - ([4325206](https://github.com/pkgforge/soar/commit/4325206829ddc161b9243782bedbb0b47a612c28))

## [0.1.1](https://github.com/pkgforge/soar/compare/soar-config-v0.1.0...soar-config-v0.1.1) - 2025-12-28

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-utils - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.1.0] - 2025-12-26

### ⛰️  Features

- *(crate)* Init soar-config crate ([#108](https://github.com/pkgforge/soar/pull/108)) - ([135af26](https://github.com/pkgforge/soar/commit/135af260d83f009d1edb42f28599ba097280874a))
