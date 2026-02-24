
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
