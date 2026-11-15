//! Dependency sets of onelf packages, kept by soar.
//!
//! An onelf package pins the sets it runs on in `.onelf/sets`: a label, the
//! URL the build is published at, and the BLAKE3 of the build. Soar fetches
//! each one into `onelf/platform` under the profile root and links it into
//! the provided store the onelf runtime reads before its own cache,
//! `onelf/platform` under the XDG data directory. A launch then neither
//! fetches the set nor keeps a copy of its own, and the runtime never
//! sweeps what soar provides.
//!
//! Each install directory records the pins its package needs, so finding
//! out which sets are still wanted reads those records rather than the
//! packages. A set no record names is removed along with its link.

use std::{
    collections::HashSet,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    sync::Arc,
};

use soar_config::config::Config;
use soar_dl::{
    download::Download,
    types::{OverwriteMode, Progress},
};
use soar_package::{PackageFormat, formats::onelf, get_file_type};
use soar_utils::{
    lock::FileLock,
    path::{is_safe_component, xdg_data_home},
};
use tracing::{debug, warn};

use crate::{
    SoarResult,
    error::{ErrorContext, SoarError},
};

/// The record of the sets a package pins, kept in its install directory.
pub const PINS_FILE: &str = ".soar_onelf_sets";

/// Where an onelf package records the sets it pins.
const SETS_RESOURCE: &str = ".onelf/sets";

/// One dependency set a package pins.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SetPin {
    label: String,
    url: String,
    blake3: String,
}

impl SetPin {
    /// The name the build has in every store: its label directory and its
    /// hash.
    fn relative(&self) -> String {
        format!("{}/{}.onelf", self.label, self.blake3)
    }
}

/// The pins a `.onelf/sets` table declares. An entry without a usable
/// label, URL or hash is skipped, as the runtime skips it.
fn parse_pins(text: &str) -> Vec<SetPin> {
    let Ok(table) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    table
        .iter()
        .filter_map(|(label, entry)| {
            let url = entry.get("url")?.as_str()?;
            let blake3 = entry.get("blake3")?.as_str()?.to_ascii_lowercase();
            let label_ok = is_safe_component(label) && !label.starts_with('.');
            let hash_ok = blake3.len() == 64 && blake3.bytes().all(|b| b.is_ascii_hexdigit());
            (label_ok && hash_ok).then(|| {
                SetPin {
                    label: label.clone(),
                    url: url.to_string(),
                    blake3,
                }
            })
        })
        .collect()
}

/// Where soar keeps the set builds of `profile`, or of the default profile.
fn store_dir(config: &Config, profile: Option<&str>) -> SoarResult<PathBuf> {
    let profile = match profile {
        Some(name) => config.get_profile(name)?,
        None => config.default_profile()?,
    };
    Ok(profile
        .get_root_path(config.is_system())?
        .join("onelf/platform"))
}

/// The provided store the onelf runtime reads, which soar links builds into.
fn links_dir(config: &Config) -> PathBuf {
    let base = if config.is_system() {
        PathBuf::from("/usr/local/share")
    } else {
        xdg_data_home()
    };
    base.join("onelf/platform")
}

/// The onelf binaries among `binaries`, each once.
fn onelf_binaries(binaries: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    for path in binaries {
        let Ok(path) = fs::canonicalize(path) else {
            continue;
        };
        let is_onelf = File::open(&path)
            .ok()
            .and_then(|file| get_file_type(&mut BufReader::new(file)).ok())
            .is_some_and(|format| format == PackageFormat::Onelf);
        if is_onelf && !seen.contains(&path) {
            seen.push(path);
        }
    }
    seen
}

/// How a set fetch is reported: `on_fetch` is called with the set's label
/// as its download begins, and the download reports through `progress`.
pub struct FetchReport<'a> {
    /// Called with the label of each set before it is fetched.
    pub on_fetch: &'a dyn Fn(&str),
    /// Receives the progress of each set's download.
    pub progress: Option<Arc<dyn Fn(Progress) + Send + Sync>>,
}

/// Fetches and links the sets the onelf binaries among `binaries` pin, and
/// records them in `install_dir`.
///
/// A set already in soar's store, or already provided by anything else, is
/// not fetched again. A set that cannot be fetched is reported and left
/// for the runtime to fetch at launch, as it would without soar.
///
/// # Errors
///
/// Returns [`SoarError`] if the profile's root cannot be resolved or the
/// record cannot be written.
pub fn provide_sets(
    config: &Config,
    profile: Option<&str>,
    install_dir: &Path,
    binaries: &[PathBuf],
    report: &FetchReport,
) -> SoarResult<()> {
    let mut pins: Vec<SetPin> = Vec::new();
    for binary in onelf_binaries(binaries) {
        let text = match onelf::read_file(&binary, SETS_RESOURCE) {
            Ok(Some(bytes)) => String::from_utf8_lossy(&bytes).into_owned(),
            Ok(None) => continue,
            Err(err) => {
                warn!(path = %binary.display(), error = %err, "could not read the sets an onelf package pins");
                continue;
            }
        };
        for pin in parse_pins(&text) {
            if !pins.contains(&pin) {
                pins.push(pin);
            }
        }
    }
    if pins.is_empty() {
        return Ok(());
    }

    let record: String = pins
        .iter()
        .map(|pin| format!("{}\n", pin.relative()))
        .collect();
    let record_path = install_dir.join(PINS_FILE);
    fs::write(&record_path, record)
        .with_context(|| format!("writing {}", record_path.display()))?;

    let store = store_dir(config, profile)?;
    let links = links_dir(config);
    for pin in &pins {
        if let Err(err) = provide(config, pin, &store, &links, report) {
            warn!(set = pin.label, error = %err, "could not provide the set; the package fetches it at launch");
        }
    }
    Ok(())
}

/// Fetches `pin` into `store` unless a build is already provided, then
/// links it into `links`.
fn provide(
    config: &Config,
    pin: &SetPin,
    store: &Path,
    links: &Path,
    report: &FetchReport,
) -> SoarResult<()> {
    let relative = pin.relative();
    let stored = store.join(&relative);
    let link = links.join(&relative);

    // Reported before the lock, so an install waiting on another one's
    // fetch of the same set says what it waits for.
    if !stored.is_file() && !link.is_file() {
        (report.on_fetch)(&pin.label);
    }
    // One install at a time fetches and links a given set; another that
    // pins it waits here, then finds the build in the store.
    let _lock = FileLock::acquire(&format!("onelf-set-{}", pin.blake3))
        .map_err(|err| SoarError::Custom(format!("locking set {}: {err}", pin.label)))?;

    if !stored.is_file() {
        if link.is_file() {
            debug!(set = pin.label, "set already provided");
            return Ok(());
        }
        fetch(config, pin, &stored, report)?;
    }

    match fs::symlink_metadata(&link) {
        // A link whose build is gone is replaced; anything else at the name
        // already provides a build there, or is not soar's to touch.
        Ok(meta) if meta.file_type().is_symlink() && !link.exists() => {
            fs::remove_file(&link).with_context(|| format!("removing {}", link.display()))?;
        }
        Ok(_) => return Ok(()),
        Err(_) => {}
    }
    let parent = link.parent().unwrap_or(links);
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    std::os::unix::fs::symlink(&stored, &link)
        .with_context(|| format!("linking {}", link.display()))?;
    Ok(())
}

/// Downloads the build `pin` names to `stored`, verified against its hash.
/// The download lands beside it and is renamed into place only once it
/// verifies, so `stored` never holds a partial build.
fn fetch(config: &Config, pin: &SetPin, stored: &Path, report: &FetchReport) -> SoarResult<()> {
    if !pin.url.starts_with("https://") {
        return Err(SoarError::Custom(format!(
            "{} is not an https:// URL",
            pin.url
        )));
    }
    let parent = stored.parent().unwrap_or(stored);
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    let part = parent.join(format!(".{}.onelf.part", pin.blake3));
    debug!(set = pin.label, url = pin.url, "fetching set");
    let mut download = Download::new(&pin.url)
        .output(part.to_string_lossy())
        .overwrite(OverwriteMode::Force)
        .connections(config.download_concurrency.unwrap_or(8))
        .checksum(&pin.blake3);
    if let Some(progress) = report.progress.clone() {
        download = download.progress(move |p| progress(p));
    }
    if let Err(err) = download.execute() {
        fs::remove_file(&part).ok();
        return Err(SoarError::Custom(format!("fetching {}: {err}", pin.url)));
    }
    fs::rename(&part, stored).with_context(|| format!("storing {}", stored.display()))?;
    Ok(())
}

/// The set builds any installed package of any profile records.
fn wanted_sets(config: &Config) -> HashSet<String> {
    let mut wanted = HashSet::new();
    for profile in config.profile.values() {
        let Ok(packages) = profile.get_packages_path(config.is_system()) else {
            continue;
        };
        let Ok(entries) = fs::read_dir(&packages) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Ok(record) = fs::read_to_string(entry.path().join(PINS_FILE)) {
                wanted.extend(record.lines().map(str::to_string));
            }
        }
    }
    wanted
}

/// Removes the set builds in the store of `profile` that no installed
/// package records any more, with their links.
///
/// # Errors
///
/// Returns [`SoarError`] if the profile's root cannot be resolved.
pub fn prune_sets(config: &Config, profile: Option<&str>) -> SoarResult<()> {
    let store = store_dir(config, profile)?;
    if store.is_dir() {
        prune_store(&store, &links_dir(config), &wanted_sets(config));
    }
    Ok(())
}

/// Removes every build in `store` that `wanted` does not name, and its
/// link in `links` when the link is soar's own. A link pointing anywhere
/// else is left alone.
fn prune_store(store: &Path, links: &Path, wanted: &HashSet<String>) {
    let Ok(labels) = fs::read_dir(store) else {
        return;
    };
    for label in labels.flatten() {
        let label_name = label.file_name().to_string_lossy().into_owned();
        let Ok(builds) = fs::read_dir(label.path()) else {
            continue;
        };
        for build in builds.flatten() {
            let name = build.file_name().to_string_lossy().into_owned();
            if !name.ends_with(".onelf") || name.starts_with('.') {
                continue;
            }
            let relative = format!("{label_name}/{name}");
            if wanted.contains(&relative) {
                continue;
            }
            let stored = build.path();
            let link = links.join(&relative);
            if fs::read_link(&link).is_ok_and(|target| target == stored) {
                fs::remove_file(&link).ok();
                if let Some(parent) = link.parent() {
                    fs::remove_dir(parent).ok();
                }
            }
            debug!(set = relative, "removing a set no package pins");
            fs::remove_file(&stored).ok();
        }
        fs::remove_dir(label.path()).ok();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "eadb7e22b8afff286414b648d29f0c3656707d35d7f2531c93cdefa5aba6bf3b";

    #[test]
    fn a_sets_table_yields_its_usable_pins() {
        let text = format!(
            "[gtk3]\nurl = \"https://example.com/gtk3.onelf\"\nblake3 = \"{}\"\npackages = []\n\n\
             [\"../escape\"]\nurl = \"https://example.com/x\"\nblake3 = \"{HASH}\"\n\n\
             [short]\nurl = \"https://example.com/y\"\nblake3 = \"abc\"\n\n\
             [nourl]\nblake3 = \"{HASH}\"\n",
            HASH.to_ascii_uppercase()
        );
        assert_eq!(
            parse_pins(&text),
            [SetPin {
                label: "gtk3".into(),
                url: "https://example.com/gtk3.onelf".into(),
                blake3: HASH.into(),
            }]
        );
        assert!(parse_pins("not toml [").is_empty());
    }

    /// A build no package records goes with soar's link to it, a recorded
    /// one stays, and a link that is not soar's survives.
    #[test]
    fn only_unrecorded_builds_and_their_own_links_are_pruned() {
        use std::os::unix::fs::symlink;

        let dir = tempfile::tempdir().unwrap();
        let (store, links) = (dir.path().join("store"), dir.path().join("links"));
        let build = |label: &str, hash: char| {
            let relative = format!("{label}/{}.onelf", hash.to_string().repeat(64));
            let stored = store.join(&relative);
            fs::create_dir_all(stored.parent().unwrap()).unwrap();
            fs::write(&stored, b"build").unwrap();
            let link = links.join(&relative);
            fs::create_dir_all(link.parent().unwrap()).unwrap();
            (relative, stored, link)
        };
        let (kept, kept_stored, kept_link) = build("gtk3", 'a');
        symlink(&kept_stored, &kept_link).unwrap();
        let (_, gone_stored, gone_link) = build("gtk3", 'b');
        symlink(&gone_stored, &gone_link).unwrap();
        let (_, lone_stored, foreign_link) = build("qt6", 'c');
        let elsewhere = dir.path().join("elsewhere.onelf");
        fs::write(&elsewhere, b"build").unwrap();
        symlink(&elsewhere, &foreign_link).unwrap();

        prune_store(&store, &links, &HashSet::from([kept]));

        assert!(kept_stored.is_file() && kept_link.is_file());
        assert!(!gone_stored.exists() && gone_link.symlink_metadata().is_err());
        assert!(!lone_stored.exists() && !store.join("qt6").exists());
        assert!(foreign_link.is_file(), "a link that is not soar's stays");
    }
}
