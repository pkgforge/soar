# Contributing to Soar

Thank you for your interest in contributing to Soar! This document covers the
workflow and conventions that keep the contribution process smooth for everyone.

## Table of Contents

- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Commit Guidelines](#commit-guidelines)
- [Pull Request Process](#pull-request-process)
- [Code Style and Quality](#code-style-and-quality)

## Getting Started

1. **Fork the repository** on GitHub.
2. **Clone your fork** locally:
   ```sh
   git clone https://github.com/YOUR-USERNAME/soar.git
   cd soar
   ```
3. **Add the upstream remote** so you can pull in changes from the main project:
   ```sh
   git remote add upstream https://github.com/pkgforge/soar.git
   ```

## Development Workflow

1. Create a branch for your feature or bugfix.
2. Make your changes, and add or update tests where it makes sense.
3. Run the [quality checks](#quality-checks) locally.
4. Keep your branch current with upstream:
   ```sh
   git pull upstream main --rebase
   ```

## Commit Guidelines

We follow the [Conventional Commits](https://www.conventionalcommits.org/)
specification. It keeps the history readable and easy to follow.

### Commit Message Format

Each commit message consists of a **header**, an optional **body**, and an
optional **footer**:

```
<type>(<scope>): <short summary>

<body>

<footer>
```

The **header** is mandatory (it can be looser if you expect the commits to be
squashed):

- **type**: the kind of change. One of `feat`, `fix`, `docs`, `style`,
  `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
- **scope**: where the change lands, e.g. `cli`, `repo`, `package`.
- **summary**: present tense, not capitalized, no trailing period, under 72
  characters.

### Examples

```
feat(cli): add search filtering by package type
```

```
fix(repo): resolve metadata caching issue

The metadata cache wasn't invalidated when repository sources changed,
leading to stale package information. Rebuild the cache whenever source
files are modified.

Fixes #123
```

### Practices to Avoid

- Vague messages like "bug fix" or "update". Say what changed, and where it
  isn't obvious, why.
- Bundling unrelated changes into one commit.

## Pull Request Process

1. **Ensure your code compiles** and the quality checks pass before opening a PR.
2. **Reference any relevant issues** in the description.

### Draft Pull Requests

For work in progress you want early feedback on, or when there are known
blockers:

1. Open the PR as a **Draft**.
2. Note in the description that it is a work in progress.
3. Describe the specific blockers or where you need help.
4. Convert to a regular PR once it is ready for final review.

## Code Style and Quality

### General Guidelines

- Use meaningful variable and function names.
- Comment what the code cannot say for itself: why a decision was made, what
  goes wrong without it, what a value is guarding against. A comment that
  restates the line below it is noise.
- Document public items, since they are what other crates and the docs see.

### Quality Checks

CI runs these, so running them first saves a round trip:

```sh
cargo test --locked --all-features --workspace
cargo +nightly fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

Clippy runs with warnings denied, and formatting uses the nightly toolchain,
which honours options stable `rustfmt` ignores.

### Database Migrations

Migrations under `crates/soar-db/migrations` are the one part of a change that
cannot be undone on a user's machine, so they deserve a second look:

- The **core** database holds a user's installed packages. Assume every
  migration runs against real data that cannot be regenerated.
- The **metadata** database is a cache built from a repository index, so it can
  be thrown away and rebuilt.
- Rebuilding a table drops it, and dropping a table with a foreign key pointing
  at it deletes the referencing rows. Turning foreign keys off is only possible
  outside a transaction, which needs `run_in_transaction = false` in the
  migration's `metadata.toml`.
- Test a migration against a database that has rows in it, not an empty one.
