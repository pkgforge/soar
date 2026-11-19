---
title: Terminal Interface
description: Browse, install, update and maintain packages from soar's full-screen terminal interface.
---

# Terminal Interface

Soar ships a full-screen terminal interface that covers the day-to-day work in
one place: browse and search the catalog, manage installed packages and
updates, apply declarative packages, manage repositories, and run
maintenance. Everything works from the keyboard, and the mouse works too: the
wheel scrolls lists, and clicks switch tabs and move the cursor.

## Starting the Interface

Run:

```sh
soar tui
```

The command accepts soar's global options, so the interface runs with the same
configuration the CLI uses:

```sh
# Manage the system installation
soar -S tui

# Open the interface for a specific profile
soar -p work tui

# Disable colored output
soar --no-color tui
```

The interface follows `--no-color` and the `NO_COLOR` environment variable, and
falls back to bold and reversed text when colors are off.

Two restrictions apply:

- `soar --json tui` is refused. The interface draws its own screen and cannot
  emit JSON.
- The interface needs an interactive terminal. Piping it, as in
  `soar tui | cat`, is refused with a message that says so.

In system mode soar escalates once at startup, before the interface opens.
Profiles cannot be switched while it runs: restart with the profile you want.

## The Views

The tab bar in the header lists the views. Press the number of a view, press
`Tab` to cycle through them, or click a tab. Each view shows the keys that
work in it in the bar at the bottom of the screen, and `?` opens the full list.

### Browse

Browse shows every package in every enabled repository. Press `/` to type a
filter, which narrows the list as you type. Mark packages with `Space` and
install everything marked with `i`; with nothing marked, `i` installs the row
under the cursor. `I` opens a form with install options such as force
reinstall. `v` picks a specific version of the current package and installs
it, `x` runs a package without installing it, and `+` installs by name, URL,
or local path. `R` cycles the repository filter, and `Enter` toggles the
full-screen detail pane with the description, size, license, and available
versions.

### Installed

Installed lists what is installed in the current profile, with size, install
date, and state: healthy, broken, inactive because another variant is linked,
or pinned. Press `/` to filter, `Space` to mark, and `d` to remove the marked
packages, or the current one when nothing is marked. `s` switches a package to
another variant, and `u` checks updates for the marked packages or the current
row. `o` cycles the sort order between name, size, and date, and `Enter`
toggles the full-screen detail.

### Updates

Updates checks every installed package for a newer version when you enter the
view. Mark packages with `Space`, mark all of them with `a`, and apply the
updates you marked with `u`; with nothing marked, `u` applies everything
listed. `o` toggles whether the old versions are kept. Packages whose check
was skipped are counted and hidden until you press `S`. `Ctrl-r` runs a fresh
check.

### Apply

Apply shows what the declarative `packages.toml` would change before anything
happens: packages to install, update, and remove, packages already in sync,
and names it cannot find. Press `Enter` to apply after a confirmation that
names every change. `p` toggles prune, which removes installed packages the
file no longer lists, and the preview recomputes while it is on. `e` opens
`packages.toml` in your editor, and `n` creates the default file when none
exists.

### Repos

Repos lists the configured repositories and when each was last synced. Press
`s` to sync all repositories, `e` to enable or disable the current one, `a` to
add a repository, and `d` to remove it. `E` opens `config.toml` in your
editor.

### Health

Health reports the state of the installation: whether soar's binary directory
is in `PATH`, broken packages, and broken symlinks. Beside the report, a pane
lists the config, package, and cache paths soar uses. Press `c` to clean the
cache, `b` to remove broken symlinks, and `p` to remove broken packages. Each
asks for confirmation first, and `Ctrl-r` runs the checks again.

### Jobs

Jobs lists every job this session has run or queued, each with its outcome.
The log below the list collects soar's own messages and the output of hooks
and child processes, so nothing interrupts the interface while a job runs.
Press `Enter` to focus the selected job's summary, which lists failures with
their full errors, and `l` to focus the log. `x` aborts the selected job if it
is running, or drops it if it is still queued, and `c` clears the finished
jobs from the list.

## Keyboard Reference

### Global Keys

These work in every view while no dialog or text input is open.

| Key | Action |
|-----|--------|
| `1` to `7` | Switch to the Browse, Installed, Updates, Apply, Repos, Health, or Jobs view |
| `Tab` / `Shift+Tab` | Next / previous view |
| `?` | Open the help overlay, which lists the keys of the current view |
| `q` / `Ctrl-c` | Quit |
| `Ctrl-x` | Abort the running job |
| `Ctrl-r` | Reload the current view |
| `j` / `Down`, `k` / `Up` | Move the cursor down / up |
| `g` / `Home`, `G` / `End` | First / last row |
| `Ctrl-d` / `PageDown`, `Ctrl-u` / `PageUp` | Half a page down / up |
| `Esc` | Close the full-screen detail, then clear the filter, then clear the marks |

### View Keys

| View | Key | Action |
|------|-----|--------|
| Browse | `/` | Edit the filter |
| Browse | `Space` | Mark or unmark |
| Browse | `i` | Install the marked packages, or the current row |
| Browse | `I` | Install with options |
| Browse | `v` | Pick a version and install it |
| Browse | `x` | Run without installing |
| Browse | `+` | Install by name, URL, or path |
| Browse | `R` | Cycle the repository filter |
| Browse | `Enter` | Toggle the full-screen detail |
| Installed | `/` | Edit the filter |
| Installed | `Space` | Mark or unmark |
| Installed | `d` | Remove the marked packages, or the current row |
| Installed | `s` | Switch variant |
| Installed | `u` | Check updates for the marked packages, or the current row |
| Installed | `o` | Cycle the sort order: name, size, date |
| Installed | `Enter` | Toggle the full-screen detail |
| Updates | `Space` | Mark or unmark |
| Updates | `a` | Mark all, or clear the marks |
| Updates | `u` | Update the marked packages, or all when none are marked |
| Updates | `o` | Toggle keeping the old versions |
| Updates | `S` | Show or hide the skipped packages |
| Apply | `Enter` | Apply |
| Apply | `p` | Toggle prune |
| Apply | `e` | Edit `packages.toml` |
| Apply | `n` | Create the default `packages.toml`, when none exists |
| Repos | `s` | Sync all repositories |
| Repos | `e` | Enable or disable the current repository |
| Repos | `a` | Add a repository |
| Repos | `d` | Remove the current repository |
| Repos | `E` | Edit `config.toml` |
| Health | `c` | Clean the cache |
| Health | `b` | Remove broken symlinks |
| Health | `p` | Remove broken packages |
| Jobs | `Enter` | Focus the job detail |
| Jobs | `l` | Focus the log |
| Jobs | `x` | Abort the selected job if it is running, or drop it if queued |
| Jobs | `c` | Clear the finished jobs |

In the Jobs view, `Esc` returns the focus to the job list. After checking
updates for a single package in the Installed view, `Esc` backs the Updates
view out to the full list and checks everything again.

### Dialogs

Every change runs through a confirmation that names exactly what will change,
so nothing destructive happens without approval. `y` confirms and `n` or `Esc`
cancels. The arrow keys and `Tab` move between buttons, and `Enter` presses
the focused one. Destructive confirmations start with the focus on Cancel.

While a filter input has focus, every printable key goes into the text.
`Enter` keeps the filter and returns focus to the list, `Esc` clears it and
returns focus, and the arrow keys still move the cursor.

### Quitting While a Job Runs

`q` and `Ctrl-c` quit right away when no job is running or queued. Otherwise a
guard asks what to do:

- **Wait and quit**: let the running job finish, then quit. Queued jobs are
  dropped.
- **Abort and quit**: abort the running job and quit as soon as it stops.
  Queued jobs are dropped.
- **Cancel**: keep the interface open.

While quitting is pending, pressing `q` again offers `Force quit`, which exits
immediately and stops the running job wherever it is.

### Mouse

| Input | Action |
|-------|--------|
| Wheel | Scroll the list under the pointer, three rows per notch |
| Click on a tab | Switch to that view |
| Click on a list row | Move the cursor to that row; clicking the selected row toggles the full-screen detail |
| Click outside a confirm or picker | Close it, the same as `Esc` |

## Notes

- **One job at a time.** Install, remove, update, sync, apply, and clean run
  from a queue: one job runs and the rest wait in order. The Jobs view shows
  the queue.
- **Aborting stops at the next safe point.** `Ctrl-x` asks for confirmation
  and then stops the job cleanly: downloads stop, hooks and builds are killed,
  and packages not yet started are skipped. Everything that already finished
  stays finished, and kept downloads resume the next time. A repository sync
  and the system pass have no safe stop point, so `Ctrl-x` reports that
  instead of aborting them.
- **Output lands in the log.** Messages from soar, hooks, and child processes
  appear in the Jobs log. Nothing prints over the interface while it runs.
- **System packages are a second step.** Packages marked `system = true` in
  `packages.toml` are applied after the normal apply, in a separate privileged
  job. With sudo, a dialog asks for the administrator password and validates
  it before starting. With doas, the terminal is handed over so doas can ask
  for the password itself. Either way the pass runs
  `soar --system apply --only-system` with the same global options the session
  started with.

## Building Without It

The interface lives behind the `tui` cargo feature, which is enabled by
default. Building without default features produces a binary without it:

```sh
cargo build --no-default-features
```

## See Also

- [Declarative Packages](./declarative.md) for the `packages.toml` format
- [Health](./health.md) for the command-line equivalent of the checks
- [CLI Reference](./cli-reference.md) for the global options the interface
  accepts
