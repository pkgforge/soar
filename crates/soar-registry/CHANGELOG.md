
## [0.6.2](https://github.com/pkgforge/soar/compare/soar-registry-v0.6.1...soar-registry-v0.6.2) - 2026-08-15

### ⛰️  Features

- *(cli)* Expose soar to frontends with JSON output and a plugin manifest ([#194](https://github.com/pkgforge/soar/pull/194)) - ([6846b89](https://github.com/pkgforge/soar/commit/6846b893dedb373ed6d4254b13548b43be407fe5))
- Serve soarpkgs on riscv64 and refresh the readme ([#198](https://github.com/pkgforge/soar/pull/198)) - ([2224691](https://github.com/pkgforge/soar/commit/22246912045678b45969e69b1ee23da6af43fd26))

### 🐛 Bug Fixes

- *(registry)* Read the date the metadata actually publishes ([#196](https://github.com/pkgforge/soar/pull/196)) - ([2772841](https://github.com/pkgforge/soar/commit/277284138cfd9e344d72048cca5be6cac7147ae8))

### 📚 Documentation

- Refresh README and CONTRIBUTING ([#199](https://github.com/pkgforge/soar/pull/199)) - ([6479cec](https://github.com/pkgforge/soar/commit/6479ceca42d8e1c452aaa11512457f10d98b6f5c))

## [0.6.1](https://github.com/pkgforge/soar/compare/soar-registry-v0.6.0...soar-registry-v0.6.1) - 2026-08-05

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-dl - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.6.0](https://github.com/pkgforge/soar/compare/soar-registry-v0.5.1...soar-registry-v0.6.0) - 2026-08-02

### ⛰️  Features

- [**breaking**] Consume the declarative index, drop the pkg_id requirement ([#186](https://github.com/pkgforge/soar/pull/186)) - ([3a35ad7](https://github.com/pkgforge/soar/commit/3a35ad7774e7ac3d8c055e4257cb3e9dff5be2fe))

### 📚 Documentation

- Cover forge tokens and rate limits - ([ccdd34a](https://github.com/pkgforge/soar/commit/ccdd34ad3f09a90994b85e917a45885fd1c3e413))
- Refresh the readme and contributing guidelines - ([5ecd397](https://github.com/pkgforge/soar/commit/5ecd397e853d7d601677766ccf73dc68c063f015))

## [0.5.1](https://github.com/pkgforge/soar/compare/soar-registry-v0.5.0...soar-registry-v0.5.1) - 2026-07-16

### ⛰️  Features

- *(metadata)* Support local metadata source ([#181](https://github.com/pkgforge/soar/pull/181)) - ([487850d](https://github.com/pkgforge/soar/commit/487850d4dc589d7456558c833b587f0921ed6e2a))

## [0.5.0](https://github.com/pkgforge/soar/compare/soar-registry-v0.4.3...soar-registry-v0.5.0) - 2026-06-25

### ⛰️  Features

- *(metadata)* Add metadata signature verification - ([ebd1b2f](https://github.com/pkgforge/soar/commit/ebd1b2fc2efea85cbb60289c910325d619c28fe0))

## [0.4.3](https://github.com/pkgforge/soar/compare/soar-registry-v0.4.2...soar-registry-v0.4.3) - 2026-06-06

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-config - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.4.2](https://github.com/pkgforge/soar/compare/soar-registry-v0.4.1...soar-registry-v0.4.2) - 2026-06-04

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-utils, soar-config, soar-dl - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.4.1](https://github.com/pkgforge/soar/compare/soar-registry-v0.4.0...soar-registry-v0.4.1) - 2026-04-10

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-config, soar-dl - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.4.0](https://github.com/pkgforge/soar/compare/soar-registry-v0.3.0...soar-registry-v0.4.0) - 2026-02-24

### 🐛 Bug Fixes

- *(sync)* Properly respect sync_interval for repository updates - ([84a653c](https://github.com/pkgforge/soar/commit/84a653cbad7b84373301e44974a388fec8db9028))

### 🚜 Refactor

- *(cli)* Use operations from shared crate ([#158](https://github.com/pkgforge/soar/pull/158)) - ([2a2f1be](https://github.com/pkgforge/soar/commit/2a2f1be5db831de95c2d99e114d02c80870f2165))
- *(db)* Add pkg_family, drop recurse_provides - ([1d97b6d](https://github.com/pkgforge/soar/commit/1d97b6d0f9dc230a306fee936dc6571a0a658be3))
- *(pubkey)* Use inline key string instead of fetching from URL - ([f2f3e5c](https://github.com/pkgforge/soar/commit/f2f3e5c1190fd79d18732ea2efb4b668d8130f03))

### 📚 Documentation

- *(readme)* Update readme - ([4fc58a7](https://github.com/pkgforge/soar/commit/4fc58a774b4c968db8f4d69f7f809378573b4145))

### ⚙️ Miscellaneous Tasks

- *(manifest)* Remove deprecated authors field - ([0bf1231](https://github.com/pkgforge/soar/commit/0bf123139798f2efb1674c8a14eaaf4f4640dc2a))

## [0.3.0](https://github.com/pkgforge/soar/compare/soar-registry-v0.2.2...soar-registry-v0.3.0) - 2026-02-04

### ⛰️  Features

- *(nest)* [**breaking**] Remove nest functionality - ([dc21853](https://github.com/pkgforge/soar/commit/dc21853a2506d93d5ade9e2c4015c3a12b24c199))

## [0.2.2](https://github.com/pkgforge/soar/compare/soar-registry-v0.2.1...soar-registry-v0.2.2) - 2026-01-24

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-config, soar-dl - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.2.1](https://github.com/pkgforge/soar/compare/soar-registry-v0.2.0...soar-registry-v0.2.1) - 2026-01-17

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-config - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.2.0](https://github.com/pkgforge/soar/compare/soar-registry-v0.1.1...soar-registry-v0.2.0) - 2026-01-17

### ⛰️  Features

- *(apply)* Allow applying ghcr packages - ([06e2b73](https://github.com/pkgforge/soar/commit/06e2b73fce7f4189527b8868bb9adfe14d0600cc))

### 🚜 Refactor

- *(error)* Don't override error messages - ([e44342f](https://github.com/pkgforge/soar/commit/e44342f3c23b9cdbe23df2739bcf04bde4138025))
- *(query)* Update query field icons - ([695a427](https://github.com/pkgforge/soar/commit/695a427ef6a4874cb212cdceed192f94150c5548))

## [0.1.1](https://github.com/pkgforge/soar/compare/soar-registry-v0.1.0...soar-registry-v0.1.1) - 2025-12-28

### ⚙️ Miscellaneous Tasks

- Update Cargo.toml dependencies - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.1.0] - 2025-12-26

### ⛰️  Features

- *(crate)* Init soar-registry crate ([#119](https://github.com/pkgforge/soar/pull/119)) - ([21070db](https://github.com/pkgforge/soar/commit/21070db1414c47c6cb391bb6261df07e007e77dd))
