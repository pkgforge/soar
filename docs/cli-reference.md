---
title: CLI Reference
description: Global CLI options, environment variables, and common flag combinations for the soar package manager.
---

# CLI Reference

This page documents the global options that apply across soar commands, along with the environment variables that influence soar's behavior.

## Quick Reference

| Option | Short | Description |
|--------|-------|-------------|
| `--verbose` | `-v` | Increase output verbosity |
| `--quiet` | `-q` | Suppress all output except errors |
| `--json` | `-j` | Output results in JSON format |
| `--no-color` | - | Disable colored output |
| `--no-progress` | - | Disable progress bars |
| `--profile` | `-p` | Use a specific profile |
| `--config` | `-c` | Specify custom config file path |
| `--proxy` | `-P` | Set HTTP/HTTPS proxy server |
| `--header` | `-H` | Add custom HTTP headers |
| `--user-agent` | `-A` | Set custom User-Agent string |
| `--ipv4` | `-4` | Connect over IPv4 only |
| `--ipv6` | `-6` | Connect over IPv6 only |
| `--system` | `-S` | Operate in system-wide mode (requires root) |

## Verbosity Control

### `--verbose` / `-v`

Increase output verbosity. It can be used multiple times for more detail (`-vv`, `-vvv`).

```bash
soar -v install neovim
soar -vv sync
```

### `--quiet` / `-q`

Suppress all non-error output.

```bash
soar -q install nodejs
```

## Output Format

### `--json` / `-j`

Output results in JSON format for parsing.

```bash
soar --json query neovim
soar --json search python | jq '.[] | .name'
```

## Display Options

### `--no-color`

Disable colored output.

```bash
soar --no-color install ffmpeg
```

### `--no-progress`

Disable progress bars.

```bash
soar --no-progress sync > sync.log
```

## Configuration

### `--profile` / `-p`

Use a specific profile.

```bash
soar --profile work install vscode
```

Profiles are defined in `~/.config/soar/config.toml`:

```toml
[profile.work]
root_path = "/opt/soar-work"
```

### `--config` / `-c`

Specify a custom configuration file path.

```bash
soar --config /path/to/config.toml install neovim
```

## Network Options

### `--proxy` / `-P`

Set an HTTP/HTTPS proxy server.

```bash
soar --proxy http://proxy.example.com:8080 install python
soar --proxy http://user:pass@proxy.example.com:8080 sync
```

When this option is omitted, soar falls back to the `ALL_PROXY`, `HTTPS_PROXY` and `HTTP_PROXY`
environment variables, honoring `NO_PROXY` for hosts that should bypass the proxy. Passing
`--proxy` overrides all of them, including `NO_PROXY`.

### `--header` / `-H`

Add custom HTTP headers.

```bash
soar --header "Authorization: Bearer mytoken" sync
soar -H "X-Api-Key: secret123" install package
```

### `--user-agent` / `-A`

Set a custom User-Agent string.

```bash
soar --user-agent "MyApp/1.0" install python
```

### `--ipv4` / `-4` and `--ipv6` / `-6`

Restrict connections to a single address family. By default soar tries every address a host
resolves to, in whatever order the system resolver returns them.

These are useful on networks where one family is advertised but not actually routable. If a host
publishes AAAA records and the network drops IPv6 traffic instead of rejecting it, soar waits out
a 30 second connect timeout per address before moving on. Passing `-4` skips IPv6 addresses
entirely.

```bash
soar -4 install soar
soar --ipv4 sync
```

Both are filters rather than preferences, so `-4` fails with "host not found" on a host that
publishes only AAAA records. The two options cannot be combined.

## System Mode

### `--system` / `-S`

Operate in system-wide mode. This requires root.

```bash
sudo soar --system install docker
```

System paths:

- Config: `/etc/soar/config.toml`
- Root: `/opt/soar`
- Binaries: `/opt/soar/bin`

## Common Combinations

::: code-group

```bash [Scripting]
soar --json --quiet install python
```

```bash [Debugging]
soar -vv --no-progress install neovim
```

```bash [CI/CD with proxy]
soar --json --proxy http://proxy.corp.com:8080 sync
```

```bash [System installation]
sudo soar --system install docker
```

:::

## Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `ALL_PROXY` | Set proxy for all schemes | `export ALL_PROXY=socks5://proxy:1080` |
| `HTTPS_PROXY` | Set HTTPS proxy | `export HTTPS_PROXY=http://proxy:8080` |
| `HTTP_PROXY` | Set HTTP proxy | `export HTTP_PROXY=http://proxy:8080` |
| `NO_PROXY` | Hosts that bypass the proxy | `export NO_PROXY=localhost,*.internal` |
| `SOAR_CONFIG` | Custom config file path | `export SOAR_CONFIG=/path/to/config.toml` |
| `SOAR_PACKAGES_CONFIG` | Custom packages.toml path | `export SOAR_PACKAGES_CONFIG=/path/to/packages.toml` |
| `NO_COLOR` | Disable colored output | `export NO_COLOR=1` |
| `SOAR_ROOT` | Override root directory | `export SOAR_ROOT=/custom/soar` |
| `SOAR_BIN` | Override bin path | `export SOAR_BIN=/custom/bin` |
| `SOAR_DB` | Override database path | `export SOAR_DB=/custom/db` |
| `SOAR_CACHE` | Override cache path | `export SOAR_CACHE=/custom/cache` |
| `SOAR_PACKAGES` | Override packages path | `export SOAR_PACKAGES=/custom/packages` |
| `SOAR_REPOSITORIES` | Override repositories path | `export SOAR_REPOSITORIES=/custom/repos` |
| `SOAR_PORTABLE_DIRS` | Override portable dirs path | `export SOAR_PORTABLE_DIRS=/custom/portable` |
| `SOAR_DESKTOP` | Override desktop entries path | `export SOAR_DESKTOP=/custom/applications` |
| `SOAR_STEALTH` | Use default config without reading file | `export SOAR_STEALTH=1` |
| `SOAR_NIGHTLY` | Force nightly update channel (self update) | `export SOAR_NIGHTLY=1` |
| `SOAR_RELEASE` | Force stable update channel (self update) | `export SOAR_RELEASE=1` |

### System mode

With `--system`, Soar reads the `SOAR_SYSTEM_`-prefixed variant of every path
variable above and ignores the unprefixed one, so an exported `SOAR_ROOT` never
redirects the system tree into your home:

| User mode | System mode | Default in system mode |
|-----------|-------------|------------------------|
| `SOAR_CONFIG` | `SOAR_SYSTEM_CONFIG` | `/etc/soar/config.toml` |
| `SOAR_PACKAGES_CONFIG` | `SOAR_SYSTEM_PACKAGES_CONFIG` | `/etc/soar/packages.toml` |
| `SOAR_ROOT` | `SOAR_SYSTEM_ROOT` | `/opt/soar` |
| `SOAR_BIN` | `SOAR_SYSTEM_BIN` | `/opt/soar/bin` |
| `SOAR_DB` | `SOAR_SYSTEM_DB` | `/opt/soar/db` |
| `SOAR_CACHE` | `SOAR_SYSTEM_CACHE` | `/opt/soar/cache` |
| `SOAR_PACKAGES` | `SOAR_SYSTEM_PACKAGES` | `/opt/soar/packages` |
| `SOAR_REPOSITORIES` | `SOAR_SYSTEM_REPOSITORIES` | `/opt/soar/repos` |
| `SOAR_PORTABLE_DIRS` | `SOAR_SYSTEM_PORTABLE_DIRS` | `/opt/soar/portable-dirs` |
| `SOAR_DESKTOP` | `SOAR_SYSTEM_DESKTOP` | `/usr/local/share/applications` |

A `$SOAR_*` reference inside a system config file follows the same rule, so
`db_path = "$SOAR_ROOT/db"` in `/etc/soar/config.toml` reads `SOAR_SYSTEM_ROOT`.

The `SOAR_SYSTEM_*` values are forwarded across the `sudo` or `doas` escalation,
so a read-only `--system` command and a privileged one always resolve to the
same tree. Forwarding makes the escalation `sudo env VAR=... soar ...`, which a
sudoers rule that whitelists the `soar` binary by path will reject.

Do not whitelist `/usr/bin/env` to work around that. A sudoers rule permitting
`env` grants arbitrary root, because `sudo env /bin/sh` then becomes a root
shell. Use one of these instead:

- Leave the `SOAR_SYSTEM_*` variables unset in the calling shell and put the
  system paths in `/etc/soar/config.toml`. The config file needs no forwarding,
  and with nothing to forward Soar invokes the binary directly.
- Become root first, with `sudo -i` or equivalent, and run `soar --system` from
  that shell. No escalation happens, so the root shell's own environment is
  read directly.

Sudoers environment settings such as `env_keep` do not help here. The wrapper is
added whenever a `SOAR_SYSTEM_*` variable is set in the calling shell, before
sudo is involved at all.

## See Also

- [Configuration](./configuration.md) for the configuration file reference
- [Profiles](./profiles.md) for managing multiple installation profiles
- [Installation](./installation.md) for the installation guide
- [Health & Diagnostics](./health.md) for health checks and debugging
