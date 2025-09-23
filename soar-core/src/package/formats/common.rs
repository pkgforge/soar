use std::{
    env,
    ffi::OsStr,
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use image::{imageops::FilterType, DynamicImage, GenericImageView};
use regex::Regex;
use soar_dl::downloader::{DownloadOptions, Downloader};
use soar_dl::utils::FileMode;
use soar_utils::{
    error::FileSystemResult,
    fs::{create_symlink, read_file_signature, walk_dir},
    path::{desktop_dir, icons_dir},
};

use crate::{
    config::get_config,
    constants::PNG_MAGIC_BYTES,
    database::models::{Package, PackageExt},
    error::{ErrorContext, SoarError},
    SoarResult,
};

use super::{
    appimage::integrate_appimage, get_file_type, wrappe::setup_wrappe_portable_dir, PackageFormat,
};

const SUPPORTED_DIMENSIONS: &[(u32, u32)] = &[
    (16, 16),
    (24, 24),
    (32, 32),
    (48, 48),
    (64, 64),
    (72, 72),
    (80, 80),
    (96, 96),
    (128, 128),
    (192, 192),
    (256, 256),
    (512, 512),
];

fn find_nearest_supported_dimension(width: u32, height: u32) -> (u32, u32) {
    SUPPORTED_DIMENSIONS
        .iter()
        .min_by_key(|&&(w, h)| {
            let width_diff = (w as i32 - width as i32).abs();
            let height_diff = (h as i32 - height as i32).abs();
            width_diff + height_diff
        })
        .cloned()
        .unwrap_or((width, height))
}

fn normalize_image(image: DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    let (new_width, new_height) = find_nearest_supported_dimension(width, height);

    if (width, height) != (new_width, new_height) {
        image.resize(new_width, new_height, FilterType::Lanczos3)
    } else {
        image
    }
}

pub fn symlink_icon<P: AsRef<Path>>(real_path: P) -> SoarResult<PathBuf> {
    let real_path = real_path.as_ref();
    let icon_name = real_path.file_stem().unwrap();
    let ext = real_path.extension();

    let (w, h) = if ext == Some(OsStr::new("svg")) {
        (128, 128)
    } else {
        let image = image::open(real_path)?;
        let (orig_w, orig_h) = image.dimensions();

        let normalized_image = normalize_image(image);
        let (w, h) = normalized_image.dimensions();

        if (w, h) != (orig_w, orig_h) {
            normalized_image.save(real_path)?;
        }

        (w, h)
    };

    let final_path = icons_dir()
        .join(format!("{w}x{h}"))
        .join("apps")
        .join(format!(
            "{}-soar.{}",
            icon_name.to_string_lossy(),
            ext.unwrap_or_default().to_string_lossy()
        ));

    create_symlink(real_path, &final_path)?;
    Ok(final_path)
}

pub fn symlink_desktop<P: AsRef<Path>, T: PackageExt>(
    real_path: P,
    package: &T,
) -> SoarResult<PathBuf> {
    let pkg_name = package.pkg_name();
    let real_path = real_path.as_ref();
    let content = fs::read_to_string(real_path)
        .with_context(|| format!("reading content of desktop file: {}", real_path.display()))?;
    let file_name = real_path.file_stem().unwrap();

    let final_content = {
        let re = Regex::new(r"(?m)^(Icon|Exec|TryExec)=(.*)").unwrap();

        re.replace_all(&content, |caps: &regex::Captures| match &caps[1] {
            "Icon" => format!("Icon={}-soar", file_name.to_string_lossy()),
            "Exec" | "TryExec" => {
                let value = &caps[0];
                let bin_path = get_config().get_bin_path().unwrap();
                let new_value = format!("{}/{}", &bin_path.display(), pkg_name);

                if value.contains("{{pkg_path}}") {
                    value.replace("{{pkg_path}}", &new_value)
                } else {
                    format!("{}={}", &caps[1], new_value)
                }
            }
            _ => unreachable!(),
        })
        .to_string()
    };

    let mut writer = BufWriter::new(
        File::create(real_path)
            .with_context(|| format!("creating desktop file {}", real_path.display()))?,
    );
    writer
        .write_all(final_content.as_bytes())
        .with_context(|| format!("writing desktop file to {}", real_path.display()))?;

    let final_path = desktop_dir().join(format!("{}-soar.desktop", file_name.to_string_lossy()));

    create_symlink(real_path, &final_path)?;
    Ok(final_path)
}

pub async fn integrate_remote<P: AsRef<Path>>(
    package_path: P,
    package: &Package,
) -> SoarResult<()> {
    let package_path = package_path.as_ref();
    let icon_url = &package.icon;
    let desktop_url = &package.desktop;

    let mut icon_output_path = package_path.join(".DirIcon");
    let desktop_output_path = package_path.join(format!("{}.desktop", package.pkg_name));

    let downloader = Downloader::default();

    if let Some(icon_url) = icon_url {
        let options = DownloadOptions {
            url: icon_url.clone(),
            output_path: Some(icon_output_path.to_string_lossy().to_string()),
            progress_callback: None,
            extract_archive: false,
            extract_dir: None,
            file_mode: FileMode::SkipExisting,
            prompt: None,
        };
        downloader.download(options).await?;

        let ext = if read_file_signature(icon_output_path, 8)? == PNG_MAGIC_BYTES {
            "png"
        } else {
            "svg"
        };
        icon_output_path = package_path.join(format!("{}.{}", package.pkg_name, ext));
    }

    if let Some(desktop_url) = desktop_url {
        let options = DownloadOptions {
            url: desktop_url.clone(),
            output_path: Some(desktop_output_path.to_string_lossy().to_string()),
            progress_callback: None,
            extract_archive: false,
            extract_dir: None,
            file_mode: FileMode::SkipExisting,
            prompt: None,
        };
        downloader.download(options).await?;
    } else {
        let content = create_default_desktop_entry(&package.pkg_name, "Utility");
        fs::write(&desktop_output_path, &content).with_context(|| {
            format!("writing to desktop file {}", desktop_output_path.display())
        })?;
    }

    symlink_icon(&icon_output_path)?;
    symlink_desktop(&desktop_output_path, package)?;

    Ok(())
}

pub fn create_portable_link<P: AsRef<Path>>(
    portable_path: P,
    real_path: P,
    pkg_name: &str,
    extension: &str,
) -> SoarResult<()> {
    let base_dir = env::current_dir()
        .map_err(|_| SoarError::Custom("Error retrieving current directory".into()))?;
    let portable_path = portable_path.as_ref();
    let portable_path = if portable_path.is_absolute() {
        portable_path
    } else {
        &base_dir.join(portable_path)
    };
    let portable_path = portable_path.join(pkg_name).with_extension(extension);

    fs::create_dir_all(&portable_path)
        .with_context(|| format!("creating directory {}", portable_path.display()))?;
    create_symlink(&portable_path, &real_path.as_ref().to_path_buf())?;
    Ok(())
}

pub fn setup_portable_dir<P: AsRef<Path>, T: PackageExt>(
    bin_path: P,
    package: &T,
    portable: Option<&str>,
    portable_home: Option<&str>,
    portable_config: Option<&str>,
    portable_share: Option<&str>,
    portable_cache: Option<&str>,
) -> SoarResult<()> {
    let portable_dir_base = get_config().get_portable_dirs()?.join(format!(
        "{}-{}",
        package.pkg_name(),
        package.pkg_id()
    ));
    let bin_path = bin_path.as_ref();

    let pkg_name = package.pkg_name();
    let pkg_config = bin_path.with_extension("config");
    let pkg_home = bin_path.with_extension("home");
    let pkg_share = bin_path.with_extension("share");
    let pkg_cache = bin_path.with_extension("cache");

    let (portable_home, portable_config, portable_share, portable_cache) =
        if let Some(portable) = portable {
            (
                Some(portable),
                Some(portable),
                Some(portable),
                Some(portable),
            )
        } else {
            (
                portable_home,
                portable_config,
                portable_share,
                portable_cache,
            )
        };

    for (opt, target, kind) in [
        (portable_home, &pkg_home, "home"),
        (portable_config, &pkg_config, "config"),
        (portable_share, &pkg_share, "share"),
        (portable_cache, &pkg_cache, "cache"),
    ] {
        if let Some(val) = opt {
            let base = if val.is_empty() {
                &portable_dir_base
            } else {
                Path::new(val)
            };
            create_portable_link(base, target, pkg_name, kind)?;
        }
    }

    Ok(())
}

fn create_default_desktop_entry(name: &str, categories: &str) -> Vec<u8> {
    format!(
        "[Desktop Entry]\n\
        Type=Application\n\
        Name={name}\n\
        Icon={name}\n\
        Exec={name}\n\
        Categories={categories};\n",
    )
    .as_bytes()
    .to_vec()
}

pub async fn integrate_package<P: AsRef<Path>, T: PackageExt>(
    install_dir: P,
    package: &T,
    portable: Option<&str>,
    portable_home: Option<&str>,
    portable_config: Option<&str>,
    portable_share: Option<&str>,
    portable_cache: Option<&str>,
) -> SoarResult<()> {
    let install_dir = install_dir.as_ref();
    let pkg_name = package.pkg_name();
    let bin_path = install_dir.join(pkg_name);

    let mut has_desktop = false;
    let mut has_icon = false;
    let mut symlink_action = |path: &Path| -> FileSystemResult<()> {
        let ext = path.extension();
        if ext == Some(OsStr::new("desktop")) {
            has_desktop = true;
            // FIXME: handle error
            symlink_desktop(path, package).unwrap();
        }
        Ok(())
    };
    walk_dir(install_dir, &mut symlink_action)?;

    let mut symlink_action = |path: &Path| -> FileSystemResult<()> {
        let ext = path.extension();
        if ext == Some(OsStr::new("png")) || ext == Some(OsStr::new("svg")) {
            has_icon = true;
            // FIXME: handle error
            symlink_icon(path).unwrap();
        }
        Ok(())
    };
    walk_dir(install_dir, &mut symlink_action)?;

    let mut reader = BufReader::new(
        File::open(&bin_path).with_context(|| format!("opening {}", bin_path.display()))?,
    );
    let file_type = get_file_type(&mut reader)?;

    match file_type {
        PackageFormat::AppImage | PackageFormat::RunImage => {
            if matches!(file_type, PackageFormat::AppImage) {
                let _ = integrate_appimage(install_dir, &bin_path, package, has_icon, has_desktop)
                    .await;
            }
            setup_portable_dir(
                bin_path,
                package,
                portable,
                portable_home,
                portable_config,
                portable_share,
                portable_cache,
            )?;
        }
        PackageFormat::FlatImage => {
            setup_portable_dir(
                format!("{}/.{}", bin_path.parent().unwrap().display(), pkg_name),
                package,
                None,
                None,
                portable_config,
                None,
                None,
            )?;
        }
        PackageFormat::Wrappe => {
            setup_wrappe_portable_dir(&bin_path, pkg_name, portable)?;
        }
        _ => {}
    }

    Ok(())
}
