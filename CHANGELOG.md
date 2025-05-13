
## [0.5.15](https://github.com/pkgforge/soar/compare/v0.5.14...v0.5.15) - 2025-05-04

### ⛰️  Features

- *(ask)* Support ask flag for install/update - ([228cb76](https://github.com/pkgforge/soar/commit/228cb7630553a5e5340d937ba7960127c39f0a92))
- *(info)* Add count flag to show unique installed package count only - ([e4fcf89](https://github.com/pkgforge/soar/commit/e4fcf895d5797045224276a34bc1caa3b7a08522))

### 🐛 Bug Fixes

- *(config)* Reload config after setting custom config path - ([18128ba](https://github.com/pkgforge/soar/commit/18128bab86214b151aa5057363a0c27c4b39b726))
- *(provides)* Only allow provides with link to pkg_name - ([2be5dee](https://github.com/pkgforge/soar/commit/2be5dee941ef425d33327b9e2170d2a6c84ccf1b))

### 🚜 Refactor

- *(list)* Improve package list output - ([1118025](https://github.com/pkgforge/soar/commit/111802552c9bc7608d2cd1bf126954163fdfac03))
- *(stable)* Remove use of unstable features - ([4084db5](https://github.com/pkgforge/soar/commit/4084db5041d788c1c6cf319b4a77cd5ede256699))

## [0.5.14](https://github.com/pkgforge/soar/compare/v0.5.13...v0.5.14) - 2025-03-22

### ⛰️  Features

- *(install)* Show installed path and symlinks - ([ab22401](https://github.com/pkgforge/soar/commit/ab22401832b9855ca8edbfb3b1df38636d2bb380))

### 🐛 Bug Fixes

- *(clean)* Remove package entirely on clean broken package - ([03d67be](https://github.com/pkgforge/soar/commit/03d67be974c9bade1bad6ec3a5f124d31473eb7f))
- *(clippy)* Apply clippy suggestions - ([0be9c71](https://github.com/pkgforge/soar/commit/0be9c71c4e3c9917ea35c92bc02a2a1b4a98cf33))
- *(fs)* Remove filtering from process_dir, delegate to caller - ([e60139b](https://github.com/pkgforge/soar/commit/e60139bc5dafbcfd485df102d1feda57faae4393))
- *(integration)* Fix check for no desktop integration note - ([1344248](https://github.com/pkgforge/soar/commit/1344248942d87dae379fcac84de631978d29f95b))

## [0.5.13](https://github.com/pkgforge/soar/compare/v0.5.12...v0.5.13) - 2025-03-10

### ⛰️  Features

- *(health)* Check if bin is in PATH - ([2c06017](https://github.com/pkgforge/soar/commit/2c06017a11e409b9207d55d86292e984ab105715))
- *(install)* Add partial support for excluding files on install - ([f496bf5](https://github.com/pkgforge/soar/commit/f496bf5f67dc9c71fab1c61d53e33f8047cab862))
- *(package)* Track excluded package installation files - ([a7ca6c0](https://github.com/pkgforge/soar/commit/a7ca6c01301784cf6f06c3a31b6bf47f174f39df))
- *(package)* Handle multiple desktop/icon integration - ([c5b6e4a](https://github.com/pkgforge/soar/commit/c5b6e4aeb8235372b77281b532dfdee7c3b73e79))
- *(package)* Handle replaced pkg_id - ([61a47fb](https://github.com/pkgforge/soar/commit/61a47fb0aa52e47719c845e21d94e524fa26466e))

## [0.5.12](https://github.com/pkgforge/soar/compare/v0.5.11...v0.5.12) - 2025-03-02

### 🐛 Bug Fixes

- *(args)* Make top level flags global - ([2b6d14b](https://github.com/pkgforge/soar/commit/2b6d14b5b0a90342920c15f5e3d638a4319457f7))
- *(self_update)* Fix channel switch - ([aff38ec](https://github.com/pkgforge/soar/commit/aff38ec43d6448fc87e9f1e261c551ff7b60270a))

## [0.5.11](https://github.com/pkgforge/soar/compare/v0.5.10...v0.5.11) - 2025-03-01

### 🐛 Bug Fixes

- *(self_update)* Use semver version comparison - ([96af984](https://github.com/pkgforge/soar/commit/96af984560e9924f63a75f0c65d2b4868c03afd5))

## [0.5.10](https://github.com/pkgforge/soar/compare/v0.5.9...v0.5.10) - 2025-03-01

### ⛰️  Features

- *(health)* Add basic health functionality - ([b5ba25b](https://github.com/pkgforge/soar/commit/b5ba25b090daf36023ff752bd06a4592a445030a))

### 🐛 Bug Fixes

- *(config)* Handle bin and repositories path - ([e7537de](https://github.com/pkgforge/soar/commit/e7537de771d9540ea0838b873d2f903ca4055c05))
- *(metadata)* Prevent crash on metadata fetch failure - ([42cf13f](https://github.com/pkgforge/soar/commit/42cf13f8375895121bb8d295a8d8a1fb0b568b28))

### Contributors

* @QaidVoid

## [0.5.9](https://github.com/pkgforge/soar/compare/v0.5.8..v0.5.9) - 2025-02-26

### 🐛 Bug Fixes

- *(deps)* Update soar-dl to resolve append bug - ([65d56ce](https://github.com/pkgforge/soar/commit/65d56ceee940d905df346c4e8e1c9dd079af0a95))
- *(exe)* Fix self executable path - ([2918a57](https://github.com/pkgforge/soar/commit/2918a576ba72401e3d698f3ed683a32f0e83eb58))
- *(run)* Make soar flags passable after package name - ([c35e7d0](https://github.com/pkgforge/soar/commit/c35e7d0fecc6a0de87ba6c5abb4e258c8241f81e))

### ⚙️ Miscellaneous Tasks

- *(script)* Improve install script (#24) - ([d83eb6e](https://github.com/pkgforge/soar/commit/d83eb6eb0e472ebb2d9e38b0a29e88c72192e0d9))


## [0.5.8](https://github.com/pkgforge/soar/compare/v0.5.7..v0.5.8) - 2025-02-25

### 🐛 Bug Fixes

- *(integration)* Create parent dir if doesn't exist - ([c450fae](https://github.com/pkgforge/soar/commit/c450fae16496b3edb5f59708de947959b866b12a))
- *(yes)* Handle auto-select first package in download - ([89aaa73](https://github.com/pkgforge/soar/commit/89aaa73c536d4ab33325973ce67e870f2986dd26))

### 🚜 Refactor

- *(cleanup)* Improve cleanup - ([83b2813](https://github.com/pkgforge/soar/commit/83b2813aad4291589498cf2016b4bbc4dd517838))
- *(error)* Improve I/O error messages - ([ca7b971](https://github.com/pkgforge/soar/commit/ca7b97147ee478243712926db561038abda6f5a2))

### ⚡ Performance

- *(run)* Improve run performance for cached binary - ([b4178b3](https://github.com/pkgforge/soar/commit/b4178b3d5c5327518cd854e1b69e9288e63b6fa5))


## [0.5.7](https://github.com/pkgforge/soar/compare/v0.5.6..v0.5.7) - 2025-02-17

### ⛰️  Features

- *(download)* Try downloading package if url is invalid - ([6bd2a34](https://github.com/pkgforge/soar/commit/6bd2a34123b0e7c41c8923e44ffd9ae205013438))

### 🐛 Bug Fixes

- *(config)* Print default config if config file doesn't exist - ([3ba2a63](https://github.com/pkgforge/soar/commit/3ba2a63e2e67db511ba57340b73a328615148db1))
- *(metadata)* Fix metadata sync interval handling - ([c2de6a7](https://github.com/pkgforge/soar/commit/c2de6a78d83cbbeaf9b8eec69daef6a6a5fbf0ea))
- *(query)* Handle full package query - ([bb944c0](https://github.com/pkgforge/soar/commit/bb944c0eef586c64e817370545522c63b59e9498))


## [0.5.6](https://github.com/pkgforge/soar/compare/v0.5.5..v0.5.6) - 2025-02-15

### ⛰️  Features

- *(signature)* Add minisign signature verification - ([afe39a6](https://github.com/pkgforge/soar/commit/afe39a6f59373a6be985806062bde2294a35ab3f))
- *(sync)* Add option to set sync interval for each repository - ([06c7b64](https://github.com/pkgforge/soar/commit/06c7b646d1a5044f33b9c5019db9cdb53f4bb640))
- *(wrappe)* Add wrappe desktop integration support - ([a8d362f](https://github.com/pkgforge/soar/commit/a8d362f5e30e3e43da178e89480ff6f7b83f9a79))

### 🐛 Bug Fixes

- *(env)* Use info instead of warn for `env` command output note - ([0cb5874](https://github.com/pkgforge/soar/commit/0cb5874651621b961fefa485f3319e52f41235c8))
- *(run)* Use ghcr_blob to pull the binary - ([322cc01](https://github.com/pkgforge/soar/commit/322cc01d62b2fc18ce107cf001c8ebce845107b1))
- *(size)* Calculate directory size for installed packages info - ([0698f0f](https://github.com/pkgforge/soar/commit/0698f0f741fbd7583f1e6aff62b99ad6a9b99723))


## [0.5.5](https://github.com/pkgforge/soar/compare/v0.5.4..v0.5.5) - 2025-02-11

### ⛰️  Features

- *(config)* Add subcommand to print or edit config - ([e2e6687](https://github.com/pkgforge/soar/commit/e2e668737fcdc9f00d1a622a1803f8f218403499))
- *(config)* Add ability to use custom config path and set custom root for default config - ([04d2e9b](https://github.com/pkgforge/soar/commit/04d2e9ba40d8e76e1ed789b69d51e1bb2031f698))

### 🐛 Bug Fixes

- *(install)* Improve force install - ([17fcb2e](https://github.com/pkgforge/soar/commit/17fcb2e9463528c6121f8d46f4b1b1f434059bf2))
- *(metadata)* Handle etag updates correctly - ([d5787a7](https://github.com/pkgforge/soar/commit/d5787a7bde93c4922bfd192be38357dbd7398260))

### ⚡ Performance

- *(list)* Optimise package search and list - ([81576e8](https://github.com/pkgforge/soar/commit/81576e8c5664228999373b71f66e88249d0e97f3))


## [0.5.4](https://github.com/pkgforge/soar/compare/v0.5.3..v0.5.4) - 2025-02-11

### ⛰️  Features

- *(inspect)* Read logs and build script from existing install - ([5ee8912](https://github.com/pkgforge/soar/commit/5ee89120ee31526a59e7294289d2ac34d0036963))
- *(install)* Track portable dirs - ([6daca67](https://github.com/pkgforge/soar/commit/6daca67d37d4447149131542b67df338b10c52b7))
- *(install)* Add flag to suppress install notes - ([8b4ae6f](https://github.com/pkgforge/soar/commit/8b4ae6fed85acd656abeec73710df86562c93b6b))
- *(repos)* Allow setting up external repos - ([6ef67bf](https://github.com/pkgforge/soar/commit/6ef67bf3a3272e895f7b07f6f5082f3d6db6ead7))

### 🐛 Bug Fixes

- *(download)* Retry on GHCR rate limit - ([393df6a](https://github.com/pkgforge/soar/commit/393df6a43d8e41447474645fd696eb70234f272d))
- *(repos)* Use platform specific external repos - ([cc017b5](https://github.com/pkgforge/soar/commit/cc017b58ec8e5b151773e064198d8857dde7aa2d))

### 🚜 Refactor

- *(error)* Improve config errors - ([c8f39ab](https://github.com/pkgforge/soar/commit/c8f39ab28e5a82d7c16235a2dc3d0a35ed43664b))
- *(install)* Show package notes after installation - ([55b5526](https://github.com/pkgforge/soar/commit/55b55269491c87847e79ebf64ea40f1959e4b186))
- *(type)* Loosen up package types - ([41acaea](https://github.com/pkgforge/soar/commit/41acaea42e1950b3ed67e593023f65743d23329e))

### ⚙️ Miscellaneous Tasks

- *(workflow)* Update github workflows - ([baffeff](https://github.com/pkgforge/soar/commit/baffeff5ab1c8360b0d54f4cfbdaf80dfa910a4e))


## [0.5.3](https://github.com/pkgforge/soar/compare/v0.5.2..v0.5.3) - 2025-02-04

### ⛰️  Features

- *(metadata)* Add support for zstd compressed sqlite database - ([1cae955](https://github.com/pkgforge/soar/commit/1cae9551e49d4e3819e1f7c9c15edd059155711d))
- *(self)* Allow switching soar release channels - ([25acb9c](https://github.com/pkgforge/soar/commit/25acb9cb83919ca75c2d20157c1b884fb9bd4114))

### 🐛 Bug Fixes

- *(install)* Use ghcr size, switch to official ghcr API - ([58b812c](https://github.com/pkgforge/soar/commit/58b812ca2611c9771b219b8ac716e64ae49f0141))
- *(nightly)* Fix nightly version - ([9f7bd79](https://github.com/pkgforge/soar/commit/9f7bd79551bdbcc31902d0e5d1aab78db1984cd9))

### ⚡ Performance

- *(metadata)* Parallelize metadata fetch, use gzip on request - ([3863707](https://github.com/pkgforge/soar/commit/3863707a33d00cd066fa6ad3e071d55c384c6476))

### ⚙️ Miscellaneous Tasks

- *(config)* Update default repository URLs to use sdb.zstd format - ([b76127e](https://github.com/pkgforge/soar/commit/b76127e3997623f6508237f4532750c005113c8f))


## [0.5.2](https://github.com/pkgforge/soar/compare/v0.5.1..v0.5.2) - 2025-01-30

### 🐛 Bug Fixes

- *(icon)* Fix desktop icon integration - ([7d09ff4](https://github.com/pkgforge/soar/commit/7d09ff43d35daa7173787a0a06ec378bb3b44d40))
- *(integration)* Skip desktop integration for static/dynamic package - ([0d10c12](https://github.com/pkgforge/soar/commit/0d10c12819863bbd541cb6aa974876514e71dbeb))
- *(remove)* Ignore error if package path is already removed - ([58cb283](https://github.com/pkgforge/soar/commit/58cb283109854f0fafe6515cf256521fac49da2a))
- *(self_update)* Fix version check - ([86d02cc](https://github.com/pkgforge/soar/commit/86d02ccf1e8f89ae4c3c2073a859c9d7d28809ef))

### ⚡ Performance

- *(remove)* Don't load metadata databases on package removal - ([229e265](https://github.com/pkgforge/soar/commit/229e2654322f7a7d01945935b2df3a50f156ef27))
- *(state)* Lazy load databases - ([823dea4](https://github.com/pkgforge/soar/commit/823dea48287eb367172ce1cfc3462d6ae63eee25))

### ⚙️ Miscellaneous Tasks

- *(script)* Update install script - ([126e5d4](https://github.com/pkgforge/soar/commit/126e5d4c094671ac6421fa8271e8b50d086c023d))


## [0.5.1](https://github.com/pkgforge/soar/compare/v0.5.0..v0.5.1) - 2025-01-27

### 🐛 Bug Fixes

- *(update)* Handle multi-profile update - ([569347f](https://github.com/pkgforge/soar/commit/569347f2ee7ad137917428ec9454c81f43c7708c))

### ⚙️ Miscellaneous Tasks

- *(cargo)* Update cargo manifest - ([ad18d0c](https://github.com/pkgforge/soar/commit/ad18d0c6d3a3089815ed050844a76265e4900aa2))


## [0.5.0](https://github.com/pkgforge/soar/compare/v0.4.8..v0.5.0) - 2025-01-27

### ⛰️  Features

- *(color)* Add no-color support - ([0d66b76](https://github.com/pkgforge/soar/commit/0d66b7688f6c886a520ec5ebf2cdc121a29fa646))
- *(ghcr)* Use ghcr as default download source for package - ([671fa9b](https://github.com/pkgforge/soar/commit/671fa9b2b87ccefac6618591c00d6782dfe88469))
- *(install)* Implement install with pkg_id - ([f8573a1](https://github.com/pkgforge/soar/commit/f8573a1689f74b08bb87caa32a937d7fb1fb5e1d))
- *(json_where)* Add json array condition support - ([0b84535](https://github.com/pkgforge/soar/commit/0b8453514dbc8039cc402f779e04cdec895f949e))
- *(package)* Enhance pkg_id handling for install/update - ([63cf070](https://github.com/pkgforge/soar/commit/63cf0703a7af761fcb37a67ef3bc10d52c11ea71))
- *(profile)* Add profile support - ([45c6c97](https://github.com/pkgforge/soar/commit/45c6c97c50fb93992b3317b08a329817a4350acb))
- *(provides)* Add provides support - ([937a447](https://github.com/pkgforge/soar/commit/937a447dcde90e1c630c54866a405d7a9613331b))
- *(soar-db)* Initialize soar-db - ([be59788](https://github.com/pkgforge/soar/commit/be59788433eebf03ee56e19402391701eb3b84a1))
- *(use-package)* Implement use package and improve installation - ([723bf3b](https://github.com/pkgforge/soar/commit/723bf3b74156702bae2959ebcfcffaec73cbf05b))

### 🐛 Bug Fixes

- *(install)* Fix installation error handling - ([8b540d4](https://github.com/pkgforge/soar/commit/8b540d4faea4039ad6f357f7d638b3528c3e3a58))
- *(path)* Fix home path - ([b4d3a53](https://github.com/pkgforge/soar/commit/b4d3a53658089edfb26ced1199cf03f968c03d97))
- *(script)* Fix install script - ([115056f](https://github.com/pkgforge/soar/commit/115056ff251ee0e2c8e2f8cb859e97049a7e046b))
- *(struct)* Fix database and package struct to use new metadata - ([322af28](https://github.com/pkgforge/soar/commit/322af283e7a269191dc7921a23eefcd42d502276))
- *(update)* Fix package update functionality - ([c6bf461](https://github.com/pkgforge/soar/commit/c6bf461393365a94897d54f0eeffd7b50825258e))

### 🚜 Refactor

- *(db)* Use builder pattern for queries and map using column names - ([b2827f7](https://github.com/pkgforge/soar/commit/b2827f7ebf2e2eb0dd017ab59db57b2f50b0ad3d))
- *(db)* Simplify database migration - ([1975da5](https://github.com/pkgforge/soar/commit/1975da5b5f000ad4a7a9341915bce0aabe3e41c5))
- *(db)* Simplify database query builders - ([82b20b9](https://github.com/pkgforge/soar/commit/82b20b9dff81dba73171ac5df94a6d6b78fcc6d6))
- *(ghcr)* Use pkgforge ghcr api - ([f745fff](https://github.com/pkgforge/soar/commit/f745fff8f5e6e95067e7ede1ebe80593ef3ca3eb))
- *(project)* Rewrite and switch to sqlite - ([6c3d5f5](https://github.com/pkgforge/soar/commit/6c3d5f58b3b576505805242a938f378340023b4b))
- *(run)* Enhance run capability - ([58d49a1](https://github.com/pkgforge/soar/commit/58d49a113ea0fd98ecc3dc99c30b1dc5ab4f3e38))

### 📚 Documentation

- *(readme)* Update README (#13) - ([25a3947](https://github.com/pkgforge/soar/commit/25a3947124a192ec70350d98c34b0d2b2a2b4629))

### ⚡ Performance

- *(query)* Optimize packages list SQL query - ([826f343](https://github.com/pkgforge/soar/commit/826f3430b164e9b2f42ac25981f05af74a1e25ef))

### ⚙️ Miscellaneous Tasks

- *(readme)* Add gif, new doc links, community chat & more (#8) - ([cfe7341](https://github.com/pkgforge/soar/commit/cfe73416e2b4b4a349480d437e65bfd57a0e7724))
- *(workflow)* Employ @pkgforge-bot to auto respond to Issues & Discussions (#7) - ([8bda58b](https://github.com/pkgforge/soar/commit/8bda58b22758b6760a325357589951aa3ed57931))

## New Contributors ❤️

* @Azathothas made their first contribution in [#13](https://github.com/pkgforge/soar/pull/13)

## [0.4.8](https://github.com/pkgforge/soar/compare/v0.4.7..v0.4.8) - 2024-11-25

### ⛰️  Features

- *(builder)* Add initial support for build scripts - ([39acf1a](https://github.com/pkgforge/soar/commit/39acf1abaa5c801f98e671bc957ed85cc1e9ee28))
- *(download)* Add gitlab support - ([4a34c82](https://github.com/pkgforge/soar/commit/4a34c828cc2bc91ce8d11faae475df8bb8ec35d9))
- *(download)* Use pkgforge api to fetch github assets - ([9a20792](https://github.com/pkgforge/soar/commit/9a20792b697237957b60cb6b0f2a84eb76bfd191))
- *(download)* Support comma-separated keywords in filters - ([38a4eb1](https://github.com/pkgforge/soar/commit/38a4eb1d4a5fdf145896e3c1ed04b8e2e2707b08))
- *(github)* Accept GITHUB_TOKEN for github downloads - ([d6c2b57](https://github.com/pkgforge/soar/commit/d6c2b57bb2a51e180624ee2454d56023773888c4))
- *(self)* Add self update - ([e4ba2af](https://github.com/pkgforge/soar/commit/e4ba2af100db09f490412eb1b6ad7ffb1654d600))

### 🐛 Bug Fixes

- *(config)* Override config using env, make inner paths optional - ([58f5a17](https://github.com/pkgforge/soar/commit/58f5a1771fa222a22905d047538a050e17c12be9))
- *(download)* Fix github regex - ([cd6e048](https://github.com/pkgforge/soar/commit/cd6e0488cb5f31b21b1a7843d8027a7431a19da2))
- *(package)* Sort package selection order - ([7b6c490](https://github.com/pkgforge/soar/commit/7b6c490c37abf425b1b8408d131773777c2556d1))


## [0.4.7](https://github.com/pkgforge/soar/compare/v0.4.6..v0.4.7) - 2024-11-13

### 🐛 Bug Fixes

- *(download)* Fix github regex pattern and make filter case-insensitive - ([546cb62](https://github.com/pkgforge/soar/commit/546cb622d37285ec1ccc57eab6a40ac834ae9bab))
- *(flatimage)* Fix flatimage portable config symlink path - ([37075ec](https://github.com/pkgforge/soar/commit/37075ec3795de426c64b88abcd1854a52298cfe2))
- Read config, allow stdin anywhere, ignore invalid package - ([0a8d1bd](https://github.com/pkgforge/soar/commit/0a8d1bd6ec4c99762fd08c9f23117ea929844c78))


## [0.4.6](https://github.com/pkgforge/soar/compare/v0.4.5..v0.4.6) - 2024-11-12

### 🐛 Bug Fixes

- *(args)* Fix clap responses - ([af655eb](https://github.com/pkgforge/soar/commit/af655eb5e4cfb5214738c0989868d12d84eccc00))


## [0.4.5](https://github.com/pkgforge/soar/compare/v0.4.4..v0.4.5) - 2024-11-12

### ⛰️  Features

- *(cli)* Allow stdin input as args - ([5e1fcaf](https://github.com/pkgforge/soar/commit/5e1fcafe4134b948ec8e860332d448e75fa90d44))
- *(download)* Add ergonomic flags for github asset matching - ([e47083d](https://github.com/pkgforge/soar/commit/e47083d3fc87b39fe938d035748de89f89161c45))
- *(download)* Allow regex filter for github asset - ([85736a6](https://github.com/pkgforge/soar/commit/85736a6de8a8cb63aaa7197c5f1cdf8c880e1e5b))
- *(download)* Allow specifying tagname for github downloads - ([fcf5ba4](https://github.com/pkgforge/soar/commit/fcf5ba4328eb7e9ebaec72e43a6235fb6cbf3857))
- *(download)* Add support for downloading github release - ([9ca101d](https://github.com/pkgforge/soar/commit/9ca101d1a4e7105c0ac5da4ded625f032e12513c))

### 📚 Documentation

- *(readme)* Add autoplay videos - ([80cfceb](https://github.com/pkgforge/soar/commit/80cfceb122d519ab57b460386d51182e9884391c))

### ⚙️ Miscellaneous Tasks

- *(workflow)* Update release workflow - ([e0b9a58](https://github.com/pkgforge/soar/commit/e0b9a5886bcdafb27a2af0cae42f72ec6d5beda1))


## [0.4.4](https://github.com/pkgforge/soar/compare/v0.4.3..v0.4.4) - 2024-11-09

### ⛰️  Features

- *(env)* Add environment variables support - ([426c380](https://github.com/pkgforge/soar/commit/426c3803a35801f94e71851ed9ba5773b5c6ff2f))
- *(log)* Add tracing, verbosity, json output - ([424b0e3](https://github.com/pkgforge/soar/commit/424b0e35eb36a4ef3779bb4c69c054f4137130a4))

### 🐛 Bug Fixes

- *(log)* Write info to stdout - ([295d6f7](https://github.com/pkgforge/soar/commit/295d6f7801af0a7714bf7b7409c602586a6885b9))

### 🚜 Refactor

- *(install)* Use filename as binary name for local install - ([ff004ae](https://github.com/pkgforge/soar/commit/ff004aed99e972bc7f0812354c54d4498e413bc6))


## [0.4.3](https://github.com/pkgforge/soar/compare/v0.4.2..v0.4.3) - 2024-11-08

### 🐛 Bug Fixes

- *(install)* Fix package case handling & replacement - ([5af3cfc](https://github.com/pkgforge/soar/commit/5af3cfc43a63ee1201baebd24c628a5f5246cf4d))
- *(install)* Add constraints to local installs binary name - ([bfe004f](https://github.com/pkgforge/soar/commit/bfe004fdf7e8d6fc3fc1be27818ad9cc4a892978))

### 🚜 Refactor

- *(search)* Add description search and limit - ([4bbe1f3](https://github.com/pkgforge/soar/commit/4bbe1f397a157734218c2df8a9e88e3a4a1187ad))


## [0.4.2](https://github.com/pkgforge/soar/compare/v0.4.1..v0.4.2) - 2024-11-05

### ⛰️  Features

- *(install)* Implement local package install - ([457f117](https://github.com/pkgforge/soar/commit/457f117c69d2ad646c2c8780ab329d88d9fb755a))

### 🐛 Bug Fixes

- *(flatimage)* Handle flatimage portable config and non-existent desktop - ([33448e2](https://github.com/pkgforge/soar/commit/33448e2f2bfb21072b565d537f52d6c93e6a9b88))

### 🚜 Refactor

- *(config)* Move default soar dir, use without config file - ([ca7437b](https://github.com/pkgforge/soar/commit/ca7437b9f970677af8fb3d90d08082ea998faa7f))

### ⚙️ Miscellaneous Tasks

- *(icon)* Add logo - ([70c9fd1](https://github.com/pkgforge/soar/commit/70c9fd1345e0a8b1385bec8b3264f25100f09e90))
- *(workflow|cargo)* Auto-assign issues/PRs, update repo url - ([e17258e](https://github.com/pkgforge/soar/commit/e17258e603190e05f2e6ff1ad6ef76a73aff1b60))


## [0.4.1](https://github.com/pkgforge/soar/compare/v0.4.0..v0.4.1) - 2024-11-04

### 🐛 Bug Fixes

- *(sigpipe)* Terminate if pipe is broken - ([bc50076](https://github.com/pkgforge/soar/commit/bc50076f6cee0101a927f40757c74ed0067bf0ee))

### ⚙️ Miscellaneous Tasks

- *(cargo)* Update package name - ([381dd66](https://github.com/pkgforge/soar/commit/381dd66c80842debd78226e752ee474c5a2ae9d8))


## [0.4.0](https://github.com/pkgforge/soar/compare/v0.3.1..v0.4.0) - 2024-11-04

### ⛰️  Features

- *(download)* Add progressbar & output file path support - ([f7dcea8](https://github.com/pkgforge/soar/commit/f7dcea8ef6a19e3a8496c78d1ea9097846ecff28))
- *(download)* Fallback to download package if invalid URL - ([eccbb87](https://github.com/pkgforge/soar/commit/eccbb87e640af2477e3c55fe41c0e344f6b25da0))
- *(flatimage)* Integrate flatimage using remote files - ([e94d480](https://github.com/pkgforge/soar/commit/e94d48085fb2e64f61b09053d0c6578d2e7761cb))
- *(inspect)* Add inspect command to view build script - ([bcef36c](https://github.com/pkgforge/soar/commit/bcef36cbc0045230357ca37afb5c7480f4cab046))
- *(progress)* Re-implement installation progress bar - ([89ed804](https://github.com/pkgforge/soar/commit/89ed804e396944b4e53a8091c0024e261509add5))
- *(yes)* Skip prompts and select first value - ([286743e](https://github.com/pkgforge/soar/commit/286743e60c900a915fd6821ff47e13a66ceaf234))

### 🐛 Bug Fixes

- *(download)* Don't hold downloads in memory - ([baf33d9](https://github.com/pkgforge/soar/commit/baf33d997a8f2a75d965094aa129ad44348fc194))
- *(health)* Check fusermount3 and use fusermount as fallback - ([3cef007](https://github.com/pkgforge/soar/commit/3cef007d12351c2226f1006961795b7a6a4f4ed8))
- *(image)* Fix image rendering - ([b190bd0](https://github.com/pkgforge/soar/commit/b190bd0eaa09fd2357939fd0986e62d94fcfcb4a))
- *(package)* Fix multi-repo install handling - ([8654fbb](https://github.com/pkgforge/soar/commit/8654fbbc4c84c7f632f9e971732f60b960c01fd9))
- *(remove)* Improve package removal - ([3f0307a](https://github.com/pkgforge/soar/commit/3f0307aab929ed83e2f602cf33763162095cd343))
- *(update)* Fix update progressbar - ([948a42e](https://github.com/pkgforge/soar/commit/948a42eab471a6dde413636ba0b8c0933e7d47c0))

### 🚜 Refactor

- *(health)* Separate user namespaces and fuse issues - ([4b7fd4f](https://github.com/pkgforge/soar/commit/4b7fd4f9219ce93a8b7612b38f1d68cf38b5ee0d))
- *(image)* Reduce image handling complexity - ([39e9c1b](https://github.com/pkgforge/soar/commit/39e9c1b3e97a6c628abe5d092adafba37ff30b9d))
- *(list)* Sort list output - ([2c8d894](https://github.com/pkgforge/soar/commit/2c8d8945ad80d4578d815b72b5791fd111257f26))
- *(project)* Minor refactor - ([0b0bd06](https://github.com/pkgforge/soar/commit/0b0bd06811fbe3d7a91d6e46a5b2598a4ffe5957))

### 📚 Documentation

- *(README)* Fix installation instructions - ([b2fc746](https://github.com/pkgforge/soar/commit/b2fc74664da9463a82d1f445d1560c28d7134f66))
- *(readme)* Update README - ([2fb53cc](https://github.com/pkgforge/soar/commit/2fb53cc42378d17c64388a7b780298ab82de103e))

### ⚙️ Miscellaneous Tasks

- *(script)* Update install script - ([a18cba3](https://github.com/pkgforge/soar/commit/a18cba3092c892173d00551796d1b8c489cf8324))
- *(script)* Add install script - ([7bea339](https://github.com/pkgforge/soar/commit/7bea3393b1d9f6ada476b9f3b55b875051ef8f6f))
- *(workflow)* Remove existing nightly before publishing new - ([e1171af](https://github.com/pkgforge/soar/commit/e1171af85b6816c512cdf1ab91c01580ba5195a8))


## [0.3.1](https://github.com/pkgforge/soar/compare/v0.3.0..v0.3.1) - 2024-10-26

### 🐛 Bug Fixes

- *(config)* Fix default config url - ([1862a7e](https://github.com/pkgforge/soar/commit/1862a7eb7ca6106bd3834ec6cf24a85e9e09ccc3))


## [0.3.0](https://github.com/pkgforge/soar/compare/v0.2.0..v0.3.0) - 2024-10-26

### ⛰️  Features

- *(appimage)* Allow providing portable home/config dir for appimage - ([446958e](https://github.com/pkgforge/soar/commit/446958e3a57a58c0a42de3f2103f6f7995a791cf))
- *(appimage)* Implement appimage integration - ([3d7fbe1](https://github.com/pkgforge/soar/commit/3d7fbe198e53c1e0b3d88e48d7f917e0f0c6ee30))
- *(collection)* Allow dynamic collection names - ([d37bad0](https://github.com/pkgforge/soar/commit/d37bad073642e04276140c3e40d85399fa9a86c5))
- *(color)* Implement colorful logging - ([61d9ceb](https://github.com/pkgforge/soar/commit/61d9ceb1f39c43fa86cc2da8ab8292e4ffa2ec70))
- *(health)* Include fuse check - ([ee9d3b7](https://github.com/pkgforge/soar/commit/ee9d3b7984ce67c13f712d7efc22c3619b18903e))
- *(health)* Add health check command - ([293960f](https://github.com/pkgforge/soar/commit/293960fa9eb5365a34d5794ef8889ff111087aac))
- *(image)* Add halfblock image support - ([a1e2dc3](https://github.com/pkgforge/soar/commit/a1e2dc37d5b9b30f76e7e8c59a4126afe517b58f))
- *(image)* Add sixel support - ([88433d3](https://github.com/pkgforge/soar/commit/88433d3c2b399f4269b4885514b88b1ca7c5a14b))
- *(image)* Kitty graphics protocol image support for query - ([fb1da68](https://github.com/pkgforge/soar/commit/fb1da6891f1dfcf24ef2f9ad50d7cba68d3b0b87))
- *(pkg)* Fetch remote image/desktop file if pkg is not appimage - ([2e5b15e](https://github.com/pkgforge/soar/commit/2e5b15e1622d60f99d1e29a5885cbf0f31691a84))

### 🐛 Bug Fixes

- *(appimage)* Sanity checks for kernel features & user namespace - ([b8dd511](https://github.com/pkgforge/soar/commit/b8dd511d2425848b2f479660ce9349c7ec90a243))
- *(appimage)* Prevent creating portable dirs by default - ([cc66cd3](https://github.com/pkgforge/soar/commit/cc66cd3580eb4b8d039ac09c2ae279f3c1c1ba26))
- *(appimage)* Set default portable path if arg is not provided - ([5a34205](https://github.com/pkgforge/soar/commit/5a34205d6e2016cd336021f520dae6b0996810a7))
- *(appimage)* Use path check for ownership - ([7181629](https://github.com/pkgforge/soar/commit/7181629ad4b94c7bcefa3d50348f3964be80aae7))
- *(appimage)* Handle symlinks and use proper icon path - ([aee9282](https://github.com/pkgforge/soar/commit/aee92820469db7a39aea30c5cc1fca56ba7a8e05))
- *(fetch)* Fetch default icons only when fetcher is called - ([fdefcd5](https://github.com/pkgforge/soar/commit/fdefcd59d54fe3357f0c096cca26d1fdedf27001))
- *(image)* Fetch default fallback image - ([bc92204](https://github.com/pkgforge/soar/commit/bc9220451e2f22d6fba8761d487afee4485f2fd1))
- *(registry)* Update outdated local registry - ([6a967df](https://github.com/pkgforge/soar/commit/6a967df7a249e1ebb42a61cbec661908d0b2343d))
- *(userns-check)* Check clone_newuser support - ([2e1cf13](https://github.com/pkgforge/soar/commit/2e1cf1332af9a858482ddd48cea035d0e8ead98c))
- *(wrap)* Fix text wrapping - ([e7b6d71](https://github.com/pkgforge/soar/commit/e7b6d71e38720ad95bf4914fe63e6395b0d8f0ab))

### 🚜 Refactor

- *(collection)* Rename root_path to collection - ([a480c85](https://github.com/pkgforge/soar/commit/a480c8581a7531ed9b8c94ebedf16975c4bdaf63))
- *(color)* Update colors in query - ([adc257b](https://github.com/pkgforge/soar/commit/adc257bf8235b17512eae113d8f96a5916aa1e6a))
- *(package)* Reduce hard-coded collections - ([041e824](https://github.com/pkgforge/soar/commit/041e824fca58e3c2c24f5417e1a7a772ce563746))

### ⚙️ Miscellaneous Tasks

- *(readme)* Update Readme - ([8f43a68](https://github.com/pkgforge/soar/commit/8f43a6843e73530dcca086591831bb0c415f78a0))
- *(workflow)* Run nightly on every commit - ([42ddf90](https://github.com/pkgforge/soar/commit/42ddf90857a1c9a0ff264dbac45e1fda114c0935))
- *(workflow)* Add nightly workflow - ([f697a5f](https://github.com/pkgforge/soar/commit/f697a5f86adc4c75822e0c8fc3b3a0e7dacd9479))

## New Contributors ❤️

* @dependabot[bot] made their first contribution in [#1](https://github.com/pkgforge/soar/pull/1)

## [0.2.0](https://github.com/pkgforge/soar/compare/v0.1.0..v0.2.0) - 2024-10-11

### ⛰️  Features

- *(download)* Introduce ability to download arbitrary files - ([7f7339a](https://github.com/pkgforge/soar/commit/7f7339ab6d3d8a5aba7f8ba44997589ffd50fc94))
- *(run)* Run remote binary without metadata - ([695e0da](https://github.com/pkgforge/soar/commit/695e0dac7e696f759722f2e3d173365446ab6a32))

### 🐛 Bug Fixes

- *(inspect)* Show error if log can't be fetched, and warn if log too large - ([82785fb](https://github.com/pkgforge/soar/commit/82785fb5206c9491143544e76caa44e31c7c9122))
- *(run)* Fix run command - ([c2409fe](https://github.com/pkgforge/soar/commit/c2409fe5136bd65079e45b1e0b5c47c921b44f94))

### 🚜 Refactor

- *(output)* Update command outputs - ([0967773](https://github.com/pkgforge/soar/commit/09677738ff6ad1b6d7a10359dd2a4650e1b474a2))


## [0.1.0] - 2024-10-10

### ⛰️  Features

- *(cli)* Implement CLI commands structure - ([11f6214](https://github.com/pkgforge/soar/commit/11f62145740ca7cdf8aa94b58aa48fa3b498e9f0))
- *(config)* Implement config loading - ([abbaaf6](https://github.com/pkgforge/soar/commit/abbaaf66f2325641415487db1b4705e052300131))
- *(info)* Implement display installed package info - ([a79e9dd](https://github.com/pkgforge/soar/commit/a79e9dd9709ebbcdd74349f02f0be2ae160d02e6))
- *(inspect)* Add command to inspect CI logs - ([50d6b60](https://github.com/pkgforge/soar/commit/50d6b609abe37b421a353496be69637b1a022818))
- *(install)* Track and implement installed packages list - ([51e2f96](https://github.com/pkgforge/soar/commit/51e2f968b4d9306154e61e2ebb44ea6df4483f1a))
- *(install)* Implement package install - ([aaf1c89](https://github.com/pkgforge/soar/commit/aaf1c894f9c0caf5292afe9e7b4b1de2d5550d5e))
- *(list)* List available packages - ([17a50b7](https://github.com/pkgforge/soar/commit/17a50b76cb921a026940ff8f8451a30e86dbb3cb))
- *(query)* Query detailed package info - ([0f6facd](https://github.com/pkgforge/soar/commit/0f6facd18041485ce8ac6b56ad8b07f5e79afdf0))
- *(remove)* Implement packages removal - ([e676064](https://github.com/pkgforge/soar/commit/e6760645621eea1119e48b073bb14f11c24b4b15))
- *(run)* Run packages without installing them - ([16e820a](https://github.com/pkgforge/soar/commit/16e820a2145f7c2fa32d9deaf7621e813b2e1bb7))
- *(search)* Implement package search feature - ([313c2a5](https://github.com/pkgforge/soar/commit/313c2a54c4149f948cb78b544299029f646a70e1))
- *(symlink)* Implement ownership check for binary symlinks - ([6575072](https://github.com/pkgforge/soar/commit/65750728261d769d953ec9426d27ec53d5a8ed1a))
- *(update)* Implement update package - ([c58269b](https://github.com/pkgforge/soar/commit/c58269b9a1a5668c68bb3ea93142c56f7a558276))
- *(use)* Add ability to switch package variants - ([de2264d](https://github.com/pkgforge/soar/commit/de2264db461d85beab921179f1761abf49fe20cf))

### 🐛 Bug Fixes

- *(install)* Use case-sensitive package name - ([1abd650](https://github.com/pkgforge/soar/commit/1abd6500073614e4adc245a1d97887bfa418df8e))
- *(parse)* Fix remote registry parser - ([b8175c5](https://github.com/pkgforge/soar/commit/b8175c513c7bd4f4827ccf9a2df3defb5bdbbbd8))
- *(update)* Resolve update deadlock - ([e8c56bc](https://github.com/pkgforge/soar/commit/e8c56bcf1ba913b832a4307f0329bf6564d61cff))

### 🚜 Refactor

- *(command)* Update commands and cleanup on sync - ([555737c](https://github.com/pkgforge/soar/commit/555737c044f3cd0c4e5750808941f14621fe03d5))
- *(package)* Use binary checksum in install path - ([4a6e3c4](https://github.com/pkgforge/soar/commit/4a6e3c406904df96a039860c83940ed7c66f6192))
- *(project)* Re-organize whole codebase - ([2705168](https://github.com/pkgforge/soar/commit/270516888e8cff65b078f15bc91217ef5ee6b7d2))
- *(project)* Update data types and improve readability - ([ac4a93a](https://github.com/pkgforge/soar/commit/ac4a93a01c7460331c98d844874020781cd5f074))
- *(project)* Reduce complexity - ([cfc5962](https://github.com/pkgforge/soar/commit/cfc59628235d4600f4462357c3bbe48f4b3445e9))

### ⚙️ Miscellaneous Tasks

- *(README)* Add readme - ([9531d23](https://github.com/pkgforge/soar/commit/9531d23049553fc9b04befe9ad939fd17a3ac02c))
- *(hooks)* Add cliff & git commit hooks - ([6757cf7](https://github.com/pkgforge/soar/commit/6757cf75aa08e7b966503a142bbc4f1a44634902))

## New Contributors ❤️

* @QaidVoid made their first contribution

<!-- generated by git-cliff -->
