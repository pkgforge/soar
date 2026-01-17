
## [0.11.1](https://github.com/pkgforge/soar/compare/soar-core-v0.11.0...soar-core-v0.11.1) - 2026-01-17

### ⚙️ Miscellaneous Tasks

- Updated the following local packages: soar-config, soar-db, soar-package - ([0000000](https://github.com/pkgforge/soar/commit/0000000))

## [0.11.0](https://github.com/pkgforge/soar/compare/soar-core-v0.10.0...soar-core-v0.11.0) - 2026-01-17

### ⛰️  Features

- *(apply)* Allow applying ghcr packages - ([06e2b73](https://github.com/pkgforge/soar/commit/06e2b73fce7f4189527b8868bb9adfe14d0600cc))
- *(cli)* Add system-wide package management ([#141](https://github.com/pkgforge/soar/pull/141)) - ([f8d4f1c](https://github.com/pkgforge/soar/commit/f8d4f1c4e0e230427cd037355ba4a23da5b28a6b))
- *(install)* Add entrypoint option and executable discovery fallbacks - ([b77cffd](https://github.com/pkgforge/soar/commit/b77cffdd6cbdfd66518c1613313d53e1c102a7a2))
- *(packages)* Add snapshot version support with URL placeholders - ([099f96c](https://github.com/pkgforge/soar/commit/099f96c2dea4a559b47cad6da98dd0ee10633a02))
- *(packages)* Add github/gitlab as first-class package sources ([#142](https://github.com/pkgforge/soar/pull/142)) - ([2fc3c3b](https://github.com/pkgforge/soar/commit/2fc3c3b4f8e08dd9eac828dbf4f77128f186c91f))
- *(packages)* Add hooks, build commands, and sandbox support ([#140](https://github.com/pkgforge/soar/pull/140)) - ([a776d61](https://github.com/pkgforge/soar/commit/a776d61c7e7f57567a05b18c1baf683c96f08dff))
- *(sandbox)* Add landlock for sandboxing - ([32687c6](https://github.com/pkgforge/soar/commit/32687c67cce0f880d44d407376b5cb7b57b75f48))
- *(update)* Allow updating remote URL packages ([#137](https://github.com/pkgforge/soar/pull/137)) - ([af13bb6](https://github.com/pkgforge/soar/commit/af13bb637c8c4c4a89cfdac451e39b105e7ee378))

### 🐛 Bug Fixes

- *(clippy)* Apply clippy suggestions - ([1b45180](https://github.com/pkgforge/soar/commit/1b45180380790576d50f5c2430038efb0ca6d3a5))
- *(packages)* Skip version fetching when installed version matches ([#143](https://github.com/pkgforge/soar/pull/143)) - ([4325206](https://github.com/pkgforge/soar/commit/4325206829ddc161b9243782bedbb0b47a612c28))

### 🚜 Refactor

- *(db)* Drop with_pkg_id - ([fa99208](https://github.com/pkgforge/soar/commit/fa99208ec1132c720c0065c7ab3eb235db187d34))
- *(error)* Don't override error messages - ([e44342f](https://github.com/pkgforge/soar/commit/e44342f3c23b9cdbe23df2739bcf04bde4138025))
- *(query)* Update query field icons - ([695a427](https://github.com/pkgforge/soar/commit/695a427ef6a4874cb212cdceed192f94150c5548))

## [0.10.0](https://github.com/pkgforge/soar/compare/soar-core-v0.9.0...soar-core-v0.10.0) - 2025-12-28

### 🐛 Bug Fixes

- *(install)* Fix force reinstall cleanup and resume file corruption - ([c6150f7](https://github.com/pkgforge/soar/commit/c6150f72855249bd048194514dd3bdbca1beb21c))

## [0.9.0](https://github.com/pkgforge/soar/compare/soar-core-v0.8.1...soar-core-v0.9.0) - 2025-12-26

### ⛰️  Features

- *(install)* Allow remote package install - ([e060033](https://github.com/pkgforge/soar/commit/e060033ed1da14a9370650c5eddce6fc1f771c8d))
- *(progress)* Allow disabling progress bar - ([29e04ff](https://github.com/pkgforge/soar/commit/29e04ff5c41cad2aa55140a5ea938c278debb69d))

### 🐛 Bug Fixes

- *(install)* Handle resume on package install - ([f92350f](https://github.com/pkgforge/soar/commit/f92350fb2f57dc84ee4df06881e17a9d59a28eee))
- *(update)* Resolve random package install on update - ([eaa0058](https://github.com/pkgforge/soar/commit/eaa0058548462f987e290e5f883927691ff9fb3c))

### 🚜 Refactor

- *(integration)* Integrate soar with modular crates ([#123](https://github.com/pkgforge/soar/pull/123)) - ([2d340e5](https://github.com/pkgforge/soar/commit/2d340e54ac79fd31087370712f4e189b3391bd16))
- *(log)* Add more debug logs - ([96f5ac9](https://github.com/pkgforge/soar/commit/96f5ac927f7eefdebead243841dc71efd9825c65))
- *(log)* Add debug logs - ([cdbf808](https://github.com/pkgforge/soar/commit/cdbf8085f78d31518686b7be65772d70eb0108dc))

### ⚙️ Miscellaneous Tasks

- *(ci)* Ignore libsqlite-sys from machete - ([ca0f988](https://github.com/pkgforge/soar/commit/ca0f988df9973df521e73f50fb5ef1745f2295ea))
- *(docs)* Update readme, bump msrv - ([5158af0](https://github.com/pkgforge/soar/commit/5158af067ecf3981585aad4f3097d675f65331d1))
- *(docs)* Fix readme - ([90d8abb](https://github.com/pkgforge/soar/commit/90d8abb9206a304be4c3d8cd5d11ae40584242d6))

## [0.8.1](https://github.com/pkgforge/soar/compare/soar-core-v0.8.0...soar-core-v0.8.1) - 2025-09-19

### 🐛 Bug Fixes

- *(sql)* Fix sql syntax - ([58b3a05](https://github.com/pkgforge/soar/commit/58b3a05460fa6ee29873736c278f6be2abd0dac8))

## [0.8.0](https://github.com/pkgforge/soar/compare/soar-core-v0.7.0...soar-core-v0.8.0) - 2025-09-17

### ⛰️  Features

- *(portable_cache)* Add support for creating portable cache dir - ([09787c2](https://github.com/pkgforge/soar/commit/09787c24b0a4cd6dedc9647a74d8318b1bb8e7dc))

### 🐛 Bug Fixes

- *(nest)* Show error if no nest is removed - ([e157596](https://github.com/pkgforge/soar/commit/e157596bbebdfa0aa21e391d42ab844055011274))

### 📚 Documentation

- *(readme)* Simplify readme - ([9b09e1f](https://github.com/pkgforge/soar/commit/9b09e1f92eba35edb4c97cd7f280de755ce78deb))

### ⚙️ Miscellaneous Tasks

- *(migrations)* Merge database migrations - ([53229ea](https://github.com/pkgforge/soar/commit/53229eac6a145b2f8b90c558d871c6412c5b379a))

## [0.7.0](https://github.com/pkgforge/soar/compare/soar-core-v0.6.0...soar-core-v0.7.0) - 2025-08-23

### ⛰️  Features

- *(nest)* Add sync interval for nest, parallelize fetch nest metadata - ([ccffd4c](https://github.com/pkgforge/soar/commit/ccffd4cef92bd8185e0b9d314938f909895bfda7))
- *(nest)* Implement initial nest support - ([278a20c](https://github.com/pkgforge/soar/commit/278a20c95a7b56a28de809d1ff10cd0e50abf6d3))

## [0.6.0](https://github.com/pkgforge/soar/compare/soar-core-v0.5.0...soar-core-v0.6.0) - 2025-08-17

### 🐛 Bug Fixes

- *(portable)* Improve portable directory handling - ([dd88b3b](https://github.com/pkgforge/soar/commit/dd88b3bb02c71722297aab550b2f13cd6a41dfdc))

## [0.5.0](https://github.com/pkgforge/soar/compare/soar-core-v0.4.2...soar-core-v0.5.0) - 2025-07-12

### 🐛 Bug Fixes

- *(checksum)* Handle checksum verification for direct downloads - ([db48108](https://github.com/pkgforge/soar/commit/db481080c7ac1d8f8542b4c2ca5a3559f97203c0))
- *(clippy)* Apply clippy suggestions - ([18e4a51](https://github.com/pkgforge/soar/commit/18e4a51cf50481d674d9480c36a97451007a9215))

## [0.4.2](https://github.com/pkgforge/soar/compare/soar-core-v0.4.1...soar-core-v0.4.2) - 2025-06-26

### ⛰️  Features

- *(repositories)* Add new repositories - ([a6e0a7d](https://github.com/pkgforge/soar/commit/a6e0a7d59b06be31202ff185101e4da91b9a7739))
- *(repositories)* Enable repositories based on platform - ([b865447](https://github.com/pkgforge/soar/commit/b865447667f7ed536a7a6b39f05ba5233a9f08f0))

### 🚜 Refactor

- *(repositories)* Make repositories list maintainable and flexible - ([a3752ec](https://github.com/pkgforge/soar/commit/a3752ece95933eca7d7f95945f5c7127613dc992))

### 📚 Documentation

- *(readme)* Add refs on hosts, redistribution & sponsors ([#67](https://github.com/pkgforge/soar/pull/67)) - ([50b2011](https://github.com/pkgforge/soar/commit/50b2011c0b58f18fd82f966132d829800127ce71))

### ⚙️ Miscellaneous Tasks

- Add CI attestations, cross-rs, and improve install script ([#75](https://github.com/pkgforge/soar/pull/75)) - ([8fae192](https://github.com/pkgforge/soar/commit/8fae19287124b9f1c25c8971919aa7d2ea9d7132))

## [0.4.1](https://github.com/pkgforge/soar/compare/soar-core-v0.4.0...soar-core-v0.4.1) - 2025-06-12

### ⛰️  Features

- *(repository)* Handle recurse provides - ([10878a7](https://github.com/pkgforge/soar/commit/10878a786a22897864dafdfcab82e4f46732e7f7))

### 🐛 Bug Fixes

- *(metadata)* Filter non-existing repos and prevent empty db creation - ([3353ab5](https://github.com/pkgforge/soar/commit/3353ab55699251aea8f8541a690ce417087c8e3e))

## [0.4.0](https://github.com/pkgforge/soar/compare/soar-core-v0.3.3...soar-core-v0.4.0) - 2025-06-03

### 🚜 Refactor

- *(checksum)* Save checksum from metadata as is for installed package - ([55b1f34](https://github.com/pkgforge/soar/commit/55b1f34911543743f52d92fd5618d1e47134896c))

## [0.3.3](https://github.com/pkgforge/soar/compare/soar-core-v0.3.2...soar-core-v0.3.3) - 2025-06-02

### 🐛 Bug Fixes

- *(database)* Update package insert statement - ([7c3ab9d](https://github.com/pkgforge/soar/commit/7c3ab9dec424dd69ba419809adb5cdc49831c464))

## [0.3.2](https://github.com/pkgforge/soar/compare/soar-core-v0.3.1...soar-core-v0.3.2) - 2025-06-01

### ⛰️  Features

- *(config)* Allow selectively enabling repos for default config - ([6acab85](https://github.com/pkgforge/soar/commit/6acab852eb04f7c37584f80f475bf773d2241d74))
- *(config)* Allow stealth mode (skip reading config file) - ([6ee0954](https://github.com/pkgforge/soar/commit/6ee0954a2dc8a62fd6121323e3a8a52f387560c6))
- *(config)* Add global overrides for repo config - ([9f15193](https://github.com/pkgforge/soar/commit/9f151931da874b8edd6e6c1eb2df1af2849e5f25))
- *(config)* Allow env vars, add comments on default config - ([6799a70](https://github.com/pkgforge/soar/commit/6799a70ef7f83c3b7434776089716ecd8bda7183))
- *(package)* Support portable share dir - ([57bd08d](https://github.com/pkgforge/soar/commit/57bd08d7b0cd8e1878f76853a4d29eda6209e269))
- *(package)* Symlink all binaries in install dir if no provides - ([8defec2](https://github.com/pkgforge/soar/commit/8defec279d33e78fb8b2a772a94b14b500e0a4e5))
- *(package)* Add support for extracting archives - ([cc139cb](https://github.com/pkgforge/soar/commit/cc139cb64b35fe74f624c4b2bbf7faf99f8ed71d))
- *(runimage)* Support portable dir for runimages - ([a084b19](https://github.com/pkgforge/soar/commit/a084b1931dac8fd5fe0ba86ff3af97c70d653a20))

### 🐛 Bug Fixes

- *(package)* Handle provide without target - ([6ff23b7](https://github.com/pkgforge/soar/commit/6ff23b76c36ef40091d0be5b7a46d19834ddf662))
- *(package)* Apply sig variant patterns automatically - ([25ee70e](https://github.com/pkgforge/soar/commit/25ee70e93bc497e4e2a4b665969af963f79515a9))
- *(query)* Include all columns in default database query - ([be82784](https://github.com/pkgforge/soar/commit/be82784e473831820a044c7bbc0fd68a229f3862))
- *(signature)* Skip signature verification if original file doesn't exist - ([cf0da95](https://github.com/pkgforge/soar/commit/cf0da95961dbd5ab263ba66d0a2b2334ea3f1abf))
- *(update)* Prevent updating partially installed packages - ([b4b718d](https://github.com/pkgforge/soar/commit/b4b718d30acd1b29a2d8c962eaaf7a3d73bfb7bf))

### 🚜 Refactor

- *(metadata)* Update metadata database fields - ([0d8dc7f](https://github.com/pkgforge/soar/commit/0d8dc7f3b703ba815290e5228e0c2403f3f483b1))

### 📚 Documentation

- *(readme)* Refactor readme & install script ([#49](https://github.com/pkgforge/soar/pull/49)) - ([63594c3](https://github.com/pkgforge/soar/commit/63594c37f93fa402e4ab899178c5c1fd34d88352))

## [0.3.1](https://github.com/pkgforge/soar/compare/soar-core-v0.3.0...soar-core-v0.3.1) - 2025-05-04

### 🐛 Bug Fixes

- *(provides)* Only allow provides with link to pkg_name - ([2be5dee](https://github.com/pkgforge/soar/commit/2be5dee941ef425d33327b9e2170d2a6c84ccf1b))

### 🚜 Refactor

- *(stable)* Remove use of unstable features - ([4084db5](https://github.com/pkgforge/soar/commit/4084db5041d788c1c6cf319b4a77cd5ede256699))

## [0.3.0](https://github.com/pkgforge/soar/compare/soar-core-v0.2.0...soar-core-v0.3.0) - 2025-03-22

### 🐛 Bug Fixes

- *(clippy)* Apply clippy suggestions - ([0be9c71](https://github.com/pkgforge/soar/commit/0be9c71c4e3c9917ea35c92bc02a2a1b4a98cf33))
- *(fs)* Remove filtering from process_dir, delegate to caller - ([e60139b](https://github.com/pkgforge/soar/commit/e60139bc5dafbcfd485df102d1feda57faae4393))

## [0.2.0](https://github.com/pkgforge/soar/compare/soar-core-v0.1.10...soar-core-v0.2.0) - 2025-03-10

### ⛰️  Features

- *(install)* Add partial support for excluding files on install - ([f496bf5](https://github.com/pkgforge/soar/commit/f496bf5f67dc9c71fab1c61d53e33f8047cab862))
- *(package)* Track excluded package installation files - ([a7ca6c0](https://github.com/pkgforge/soar/commit/a7ca6c01301784cf6f06c3a31b6bf47f174f39df))
- *(package)* Handle multiple desktop/icon integration - ([c5b6e4a](https://github.com/pkgforge/soar/commit/c5b6e4aeb8235372b77281b532dfdee7c3b73e79))
- *(package)* Handle replaced pkg_id - ([61a47fb](https://github.com/pkgforge/soar/commit/61a47fb0aa52e47719c845e21d94e524fa26466e))

## [0.1.10](https://github.com/pkgforge/soar/compare/soar-core-v0.1.9...soar-core-v0.1.10) - 2025-03-01

### ⛰️  Features

- *(health)* Add basic health functionality - ([b5ba25b](https://github.com/pkgforge/soar/commit/b5ba25b090daf36023ff752bd06a4592a445030a))

### 🐛 Bug Fixes

- *(config)* Handle bin and repositories path - ([e7537de](https://github.com/pkgforge/soar/commit/e7537de771d9540ea0838b873d2f903ca4055c05))

### Contributors

* @QaidVoid


## [0.1.9](https://github.com/pkgforge/soar/compare/soar-core-v0.1.8...soar-core-v0.1.9) - 2025-02-26


## [0.1.8](https://github.com/pkgforge/soar/compare/soar-core-v0.1.7...soar-core-v0.1.8) - 2025-02-25

### 🐛 Bug Fixes

- *(integration)* Create parent dir if doesn't exist - ([c450fae](https://github.com/pkgforge/soar/commit/c450fae16496b3edb5f59708de947959b866b12a))

### 🚜 Refactor

- *(cleanup)* Improve cleanup - ([83b2813](https://github.com/pkgforge/soar/commit/83b2813aad4291589498cf2016b4bbc4dd517838))
- *(error)* Improve I/O error messages - ([ca7b971](https://github.com/pkgforge/soar/commit/ca7b97147ee478243712926db561038abda6f5a2))

### ⚙️ Miscellaneous Tasks

- *(deps)* Update dependencies - ([8e5dc91](https://github.com/pkgforge/soar/commit/8e5dc910a9e6bb93c39f3a1655d5352d921836ac))


## [0.1.7](https://github.com/pkgforge/soar/compare/soar-core-v0.1.6...soar-core-v0.1.7) - 2025-02-17

### 🐛 Bug Fixes

- *(metadata)* Fix metadata sync interval handling - ([c2de6a7](https://github.com/pkgforge/soar/commit/c2de6a78d83cbbeaf9b8eec69daef6a6a5fbf0ea))


## [0.1.6](https://github.com/pkgforge/soar/compare/soar-core-v0.1.5...soar-core-v0.1.6) - 2025-02-15

### ⛰️  Features

- *(signature)* Add minisign signature verification - ([afe39a6](https://github.com/pkgforge/soar/commit/afe39a6f59373a6be985806062bde2294a35ab3f))
- *(sync)* Add option to set sync interval for each repository - ([06c7b64](https://github.com/pkgforge/soar/commit/06c7b646d1a5044f33b9c5019db9cdb53f4bb640))
- *(wrappe)* Add wrappe desktop integration support - ([a8d362f](https://github.com/pkgforge/soar/commit/a8d362f5e30e3e43da178e89480ff6f7b83f9a79))

### 🐛 Bug Fixes

- *(run)* Use ghcr_blob to pull the binary - ([322cc01](https://github.com/pkgforge/soar/commit/322cc01d62b2fc18ce107cf001c8ebce845107b1))
- *(size)* Calculate directory size for installed packages info - ([0698f0f](https://github.com/pkgforge/soar/commit/0698f0f741fbd7583f1e6aff62b99ad6a9b99723))


## [0.1.5](https://github.com/pkgforge/soar/compare/soar-core-v0.1.4...soar-core-v0.1.5) - 2025-02-11

### ⛰️  Features

- *(config)* Add ability to use custom config path and set custom root for default config - ([04d2e9b](https://github.com/pkgforge/soar/commit/04d2e9ba40d8e76e1ed789b69d51e1bb2031f698))

### 🐛 Bug Fixes

- *(install)* Improve force install - ([17fcb2e](https://github.com/pkgforge/soar/commit/17fcb2e9463528c6121f8d46f4b1b1f434059bf2))
- *(metadata)* Handle etag updates correctly - ([d5787a7](https://github.com/pkgforge/soar/commit/d5787a7bde93c4922bfd192be38357dbd7398260))


## [0.1.4](https://github.com/pkgforge/soar/compare/soar-core-v0.1.3...soar-core-v0.1.4) - 2025-02-11

### ⛰️  Features

- *(install)* Track portable dirs - ([6daca67](https://github.com/pkgforge/soar/commit/6daca67d37d4447149131542b67df338b10c52b7))
- *(repos)* Allow setting up external repos - ([6ef67bf](https://github.com/pkgforge/soar/commit/6ef67bf3a3272e895f7b07f6f5082f3d6db6ead7))

### 🐛 Bug Fixes

- *(download)* Retry on GHCR rate limit - ([393df6a](https://github.com/pkgforge/soar/commit/393df6a43d8e41447474645fd696eb70234f272d))
- *(repos)* Use platform specific external repos - ([cc017b5](https://github.com/pkgforge/soar/commit/cc017b58ec8e5b151773e064198d8857dde7aa2d))

### 🚜 Refactor

- *(error)* Improve config errors - ([c8f39ab](https://github.com/pkgforge/soar/commit/c8f39ab28e5a82d7c16235a2dc3d0a35ed43664b))
- *(type)* Loosen up package types - ([41acaea](https://github.com/pkgforge/soar/commit/41acaea42e1950b3ed67e593023f65743d23329e))


## [0.1.3](https://github.com/pkgforge/soar/compare/soar-core-v0.1.2...soar-core-v0.1.3) - 2025-02-04

### ⛰️  Features

- *(metadata)* Add support for zstd compressed sqlite database - ([1cae955](https://github.com/pkgforge/soar/commit/1cae9551e49d4e3819e1f7c9c15edd059155711d))

### 🐛 Bug Fixes

- *(install)* Use ghcr size, switch to official ghcr API - ([58b812c](https://github.com/pkgforge/soar/commit/58b812ca2611c9771b219b8ac716e64ae49f0141))

### ⚡ Performance

- *(metadata)* Parallelize metadata fetch, use gzip on request - ([3863707](https://github.com/pkgforge/soar/commit/3863707a33d00cd066fa6ad3e071d55c384c6476))

### ⚙️ Miscellaneous Tasks

- *(config)* Update default repository URLs to use sdb.zstd format - ([b76127e](https://github.com/pkgforge/soar/commit/b76127e3997623f6508237f4532750c005113c8f))


## [0.1.2](https://github.com/pkgforge/soar/compare/soar-core-v0.1.1...soar-core-v0.1.2) - 2025-01-30

### 🐛 Bug Fixes

- *(icon)* Fix desktop icon integration - ([7d09ff4](https://github.com/pkgforge/soar/commit/7d09ff43d35daa7173787a0a06ec378bb3b44d40))
- *(integration)* Skip desktop integration for static/dynamic package - ([0d10c12](https://github.com/pkgforge/soar/commit/0d10c12819863bbd541cb6aa974876514e71dbeb))
- *(remove)* Ignore error if package path is already removed - ([58cb283](https://github.com/pkgforge/soar/commit/58cb283109854f0fafe6515cf256521fac49da2a))

### ⚡ Performance

- *(remove)* Don't load metadata databases on package removal - ([229e265](https://github.com/pkgforge/soar/commit/229e2654322f7a7d01945935b2df3a50f156ef27))


## [0.1.1](https://github.com/pkgforge/soar/compare/soar-core-v0.1.0...soar-core-v0.1.1) - 2025-01-27

### 🐛 Bug Fixes

- *(update)* Handle multi-profile update - ([569347f](https://github.com/pkgforge/soar/commit/569347f2ee7ad137917428ec9454c81f43c7708c))

### ⚙️ Miscellaneous Tasks

- *(cargo)* Update cargo manifest - ([ad18d0c](https://github.com/pkgforge/soar/commit/ad18d0c6d3a3089815ed050844a76265e4900aa2))


## [0.1.0] - 2025-01-27

### ⛰️  Features

- *(ghcr)* Use ghcr as default download source for package - ([671fa9b](https://github.com/pkgforge/soar/commit/671fa9b2b87ccefac6618591c00d6782dfe88469))
- *(install)* Implement install with pkg_id - ([f8573a1](https://github.com/pkgforge/soar/commit/f8573a1689f74b08bb87caa32a937d7fb1fb5e1d))
- *(json_where)* Add json array condition support - ([0b84535](https://github.com/pkgforge/soar/commit/0b8453514dbc8039cc402f779e04cdec895f949e))
- *(package)* Enhance pkg_id handling for install/update - ([63cf070](https://github.com/pkgforge/soar/commit/63cf0703a7af761fcb37a67ef3bc10d52c11ea71))
- *(profile)* Add profile support - ([45c6c97](https://github.com/pkgforge/soar/commit/45c6c97c50fb93992b3317b08a329817a4350acb))
- *(provides)* Add provides support - ([937a447](https://github.com/pkgforge/soar/commit/937a447dcde90e1c630c54866a405d7a9613331b))
- *(use-package)* Implement use package and improve installation - ([723bf3b](https://github.com/pkgforge/soar/commit/723bf3b74156702bae2959ebcfcffaec73cbf05b))

### 🐛 Bug Fixes

- *(install)* Fix installation error handling - ([8b540d4](https://github.com/pkgforge/soar/commit/8b540d4faea4039ad6f357f7d638b3528c3e3a58))
- *(struct)* Fix database and package struct to use new metadata - ([322af28](https://github.com/pkgforge/soar/commit/322af283e7a269191dc7921a23eefcd42d502276))
- *(update)* Fix package update functionality - ([c6bf461](https://github.com/pkgforge/soar/commit/c6bf461393365a94897d54f0eeffd7b50825258e))

### 🚜 Refactor

- *(db)* Use builder pattern for queries and map using column names - ([b2827f7](https://github.com/pkgforge/soar/commit/b2827f7ebf2e2eb0dd017ab59db57b2f50b0ad3d))
- *(db)* Simplify database migration - ([1975da5](https://github.com/pkgforge/soar/commit/1975da5b5f000ad4a7a9341915bce0aabe3e41c5))
- *(db)* Simplify database query builders - ([82b20b9](https://github.com/pkgforge/soar/commit/82b20b9dff81dba73171ac5df94a6d6b78fcc6d6))
- *(ghcr)* Use pkgforge ghcr api - ([f745fff](https://github.com/pkgforge/soar/commit/f745fff8f5e6e95067e7ede1ebe80593ef3ca3eb))
- *(project)* Rewrite and switch to sqlite - ([6c3d5f5](https://github.com/pkgforge/soar/commit/6c3d5f58b3b576505805242a938f378340023b4b))

### ⚡ Performance

- *(query)* Optimize packages list SQL query - ([826f343](https://github.com/pkgforge/soar/commit/826f3430b164e9b2f42ac25981f05af74a1e25ef))
