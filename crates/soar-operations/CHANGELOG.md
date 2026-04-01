
## [0.2.0](https://github.com/pkgforge/soar/compare/soar-operations-v0.1.0...soar-operations-v0.2.0) - 2026-04-01

### ⛰️  Features

- *(cli)* Add `soar repo` subcommand for repository management - ([08d7c18](https://github.com/pkgforge/soar/commit/08d7c18697ff7a8467c5d60475877db1dff45636))
- *(packages)* Add arch_map for custom arch name mapping - ([61c0efb](https://github.com/pkgforge/soar/commit/61c0efb1e95127bde2574480a3971ff2f57e125a))
- *(repo)* Add repository management operations (add, update, remove) - ([fc76b6f](https://github.com/pkgforge/soar/commit/fc76b6f9b97d3ae53b760d33fd1a2cf258eb165a))
- *(search)* Add fuzzy search and "did you mean?" suggestions - ([934b0ff](https://github.com/pkgforge/soar/commit/934b0ffe6f9014a833f9c9bbe1b41772298932c5))

## [0.1.0](https://github.com/pkgforge/soar/compare/soar-operations-v0.0.0...soar-operations-v0.1.0) - 2026-02-24

### ⛰️  Features

- *(crates)* Add soar-operations for frontend-agnostic operations ([#157](https://github.com/pkgforge/soar/pull/157)) - ([932b1e5](https://github.com/pkgforge/soar/commit/932b1e55d6eb3e878115ae9c3ad9cd97ea1f4ebc))
- *(provides)* Add @ prefix to symlink packages directly to bin - ([cc8458a](https://github.com/pkgforge/soar/commit/cc8458ab722f4287315fee7a457be0191c10a19d))

### 🐛 Bug Fixes

- *(config)* Respect repository enabled flag - ([efb6b31](https://github.com/pkgforge/soar/commit/efb6b3108e6e690d2caa32bdb3d0bfdf93cc59d5))
- *(health)* Use absolute path for health check - ([f88bf7e](https://github.com/pkgforge/soar/commit/f88bf7e782f1eeedad3f96c109daef2862cb16da))
- *(provides)* Remove provides filter and add bin_symlink_names helper - ([5ed1951](https://github.com/pkgforge/soar/commit/5ed1951c71c47e12098e6485c607fd5c315fb5a4))

### 🚜 Refactor

- *(cli)* Use operations from shared crate ([#158](https://github.com/pkgforge/soar/pull/158)) - ([2a2f1be](https://github.com/pkgforge/soar/commit/2a2f1be5db831de95c2d99e114d02c80870f2165))
- *(pubkey)* Use inline key string instead of fetching from URL - ([f2f3e5c](https://github.com/pkgforge/soar/commit/f2f3e5c1190fd79d18732ea2efb4b668d8130f03))
- *(system)* Add per-context system mode support - ([10544ac](https://github.com/pkgforge/soar/commit/10544ac8a2bd896152448f79650c6d98db0d960a))
