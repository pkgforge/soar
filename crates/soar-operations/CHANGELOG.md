
## [0.0.1](https://github.com/pkgforge/soar/compare/soar-operations-v0.0.0...soar-operations-v0.0.1) - 2026-02-16

### ⛰️  Features

- *(crates)* Add soar-operations for frontend-agnostic operations ([#157](https://github.com/pkgforge/soar/pull/157)) - ([932b1e5](https://github.com/pkgforge/soar/commit/932b1e55d6eb3e878115ae9c3ad9cd97ea1f4ebc))
- *(download)* Allow regex filter for github asset - ([85736a6](https://github.com/pkgforge/soar/commit/85736a6de8a8cb63aaa7197c5f1cdf8c880e1e5b))
- *(inspect)* Add inspect command to view build script - ([bcef36c](https://github.com/pkgforge/soar/commit/bcef36cbc0045230357ca37afb5c7480f4cab046))
- *(json_where)* Add json array condition support - ([0b84535](https://github.com/pkgforge/soar/commit/0b8453514dbc8039cc402f779e04cdec895f949e))
- *(use)* Add ability to switch package variants - ([de2264d](https://github.com/pkgforge/soar/commit/de2264db461d85beab921179f1761abf49fe20cf))

### 🐛 Bug Fixes

- *(config)* Fix default config url - ([1862a7e](https://github.com/pkgforge/soar/commit/1862a7eb7ca6106bd3834ec6cf24a85e9e09ccc3))
- *(flatimage)* Handle flatimage portable config and non-existent desktop - ([33448e2](https://github.com/pkgforge/soar/commit/33448e2f2bfb21072b565d537f52d6c93e6a9b88))
- *(install)* Improve force install - ([17fcb2e](https://github.com/pkgforge/soar/commit/17fcb2e9463528c6121f8d46f4b1b1f434059bf2))
- *(path)* Fix home path - ([b4d3a53](https://github.com/pkgforge/soar/commit/b4d3a53658089edfb26ced1199cf03f968c03d97))

### 🚜 Refactor

- *(cli)* Use operations from shared crate ([#158](https://github.com/pkgforge/soar/pull/158)) - ([2a2f1be](https://github.com/pkgforge/soar/commit/2a2f1be5db831de95c2d99e114d02c80870f2165))
- *(command)* Update commands and cleanup on sync - ([555737c](https://github.com/pkgforge/soar/commit/555737c044f3cd0c4e5750808941f14621fe03d5))
- *(integration)* Integrate soar with modular crates ([#123](https://github.com/pkgforge/soar/pull/123)) - ([2d340e5](https://github.com/pkgforge/soar/commit/2d340e54ac79fd31087370712f4e189b3391bd16))
- *(project)* Rewrite and switch to sqlite - ([6c3d5f5](https://github.com/pkgforge/soar/commit/6c3d5f58b3b576505805242a938f378340023b4b))
- *(project)* Minor refactor - ([0b0bd06](https://github.com/pkgforge/soar/commit/0b0bd06811fbe3d7a91d6e46a5b2598a4ffe5957))
- *(system)* Add per-context system mode support - ([10544ac](https://github.com/pkgforge/soar/commit/10544ac8a2bd896152448f79650c6d98db0d960a))

### 📚 Documentation

- *(README)* Fix installation instructions - ([b2fc746](https://github.com/pkgforge/soar/commit/b2fc74664da9463a82d1f445d1560c28d7134f66))
- *(readme)* Simplify readme - ([9b09e1f](https://github.com/pkgforge/soar/commit/9b09e1f92eba35edb4c97cd7f280de755ce78deb))
- *(readme)* Add refs on hosts, redistribution & sponsors ([#67](https://github.com/pkgforge/soar/pull/67)) - ([50b2011](https://github.com/pkgforge/soar/commit/50b2011c0b58f18fd82f966132d829800127ce71))
- *(readme)* Refactor readme & install script ([#49](https://github.com/pkgforge/soar/pull/49)) - ([63594c3](https://github.com/pkgforge/soar/commit/63594c37f93fa402e4ab899178c5c1fd34d88352))
- *(readme)* Update readme ([#27](https://github.com/pkgforge/soar/pull/27)) - ([8ee5c74](https://github.com/pkgforge/soar/commit/8ee5c74828a9c060894a8c6f5bb69e2a786ce353))
- *(readme)* Update README ([#13](https://github.com/pkgforge/soar/pull/13)) - ([25a3947](https://github.com/pkgforge/soar/commit/25a3947124a192ec70350d98c34b0d2b2a2b4629))
- *(readme)* Add autoplay videos - ([80cfceb](https://github.com/pkgforge/soar/commit/80cfceb122d519ab57b460386d51182e9884391c))
- *(readme)* Update README - ([2fb53cc](https://github.com/pkgforge/soar/commit/2fb53cc42378d17c64388a7b780298ab82de103e))

### ⚙️ Miscellaneous Tasks

- *(README)* Add readme - ([9531d23](https://github.com/pkgforge/soar/commit/9531d23049553fc9b04befe9ad939fd17a3ac02c))
- *(docs)* Update readme, bump msrv - ([5158af0](https://github.com/pkgforge/soar/commit/5158af067ecf3981585aad4f3097d675f65331d1))
- *(docs)* Fix readme - ([90d8abb](https://github.com/pkgforge/soar/commit/90d8abb9206a304be4c3d8cd5d11ae40584242d6))
- *(icon)* Add logo - ([70c9fd1](https://github.com/pkgforge/soar/commit/70c9fd1345e0a8b1385bec8b3264f25100f09e90))
- *(readme)* Add gif, new doc links, community chat & more ([#8](https://github.com/pkgforge/soar/pull/8)) - ([cfe7341](https://github.com/pkgforge/soar/commit/cfe73416e2b4b4a349480d437e65bfd57a0e7724))
- *(readme)* Update Readme - ([8f43a68](https://github.com/pkgforge/soar/commit/8f43a6843e73530dcca086591831bb0c415f78a0))
- *(script)* Update install script - ([a18cba3](https://github.com/pkgforge/soar/commit/a18cba3092c892173d00551796d1b8c489cf8324))
- *(script)* Add install script - ([7bea339](https://github.com/pkgforge/soar/commit/7bea3393b1d9f6ada476b9f3b55b875051ef8f6f))
- Release ([#84](https://github.com/pkgforge/soar/pull/84)) - ([a3c7c55](https://github.com/pkgforge/soar/commit/a3c7c556ca710227363a505b439371d51338746d))
- Add CI attestations, cross-rs, and improve install script ([#75](https://github.com/pkgforge/soar/pull/75)) - ([8fae192](https://github.com/pkgforge/soar/commit/8fae19287124b9f1c25c8971919aa7d2ea9d7132))
