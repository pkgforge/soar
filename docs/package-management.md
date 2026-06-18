---
title: Package Management
description: Overview of soar's package management commands for installing, removing, updating, searching, and inspecting packages.
---

# Package Management

Soar provides a comprehensive set of commands for managing packages on your system. This section is the entry point for every package management operation, from installing and removing packages to searching repositories, switching variants, and running packages without installation.

Each topic below has a dedicated page with full details, flags, and examples.

## Core Operations

- [Installing Packages](./install.md) installs packages from repositories, a `pkg_id`, a URL, or a local file, including portable installation for AppImages.
- [Removing Packages](./remove.md) removes one or more installed packages.
- [Updating Packages](./update.md) keeps all or selected packages up to date.

## Package Discovery

- [Searching Packages](./search.md) finds packages across repositories, with case-sensitive search and detailed lookups.
- [Listing Packages](./list.md) views available packages and inspects installed ones.

## Package Inspection

- [Inspection Commands](./inspection.md) views build logs, inspects build scripts, and queries package details.

## Variants and Execution

- [Using Package Variants](./use.md) switches between different variants of an installed package.
- [Running Packages](./run.md) executes a package without installing it.

## Declarative Management

- [Declarative Packages](./declarative.md) defines packages in `~/.config/soar/packages.toml` and applies them with `soar apply`.

## System Maintenance

- [Maintenance Commands](./maintenance.md) cleans the cache, syncs repositories, and views the environment.

::: tip
New to soar? Start with [Installing Packages](./install.md), then explore [Searching](./search.md) and [Listing](./list.md) to discover what is available.
:::
