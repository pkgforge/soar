use std::{
    fs::{self, File},
    io::{BufReader, BufWriter, Write},
    path::{Path, PathBuf},
};

use futures::try_join;
use image::{imageops::FilterType, DynamicImage, GenericImageView};
use regex::Regex;
use soar_dl::downloader::{DownloadOptions, Downloader};

use crate::{
    constants::{bin_path, PNG_MAGIC_BYTES},
    database::models::Package,
    utils::{calc_magic_bytes, create_symlink, home_data_path},
    SoarResult,
};

use super::{appimage::integrate_appimage, get_file_type};

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

pub async fn symlink_icon<P: AsRef<Path>>(real_path: P, pkg_name: &str) -> SoarResult<()> {
    let real_path = real_path.as_ref();
    let image = image::open(real_path)?;
    let (orig_w, orig_h) = image.dimensions();

    let normalized_image = normalize_image(image);
    let (w, h) = normalized_image.dimensions();

    if (w, h) != (orig_w, orig_h) {
        normalized_image.save(real_path)?;
    }

    let ext = real_path.extension().unwrap_or_default();
    let final_path = PathBuf::from(format!(
        "{}/icons/hicolor/{w}x{h}/apps/{pkg_name}-soar.{ext:#?}",
        home_data_path()
    ));

    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)?;
    }

    create_symlink(real_path, &final_path)
}

pub async fn symlink_desktop<P: AsRef<Path>>(real_path: P, package: &Package) -> SoarResult<()> {
    let real_path = real_path.as_ref();
    let content = fs::read_to_string(real_path)?;

    let final_content = {
        let re = Regex::new(r"(?m)^(Icon|Exec|TryExec)=(.*)").unwrap();

        re.replace_all(&content, |caps: &regex::Captures| match &caps[1] {
            "Icon" => format!("Icon={}", package.pkg),
            "Exec" | "TryExec" => {
                format!("{}={}/{}", &caps[1], bin_path().display(), package.pkg)
            }
            _ => unreachable!(),
        })
        .to_string()
    };

    let mut writer = BufWriter::new(File::create(real_path)?);
    writer.write_all(final_content.as_bytes())?;

    let final_path = PathBuf::from(format!(
        "{}/applications/{}-soar.desktop",
        home_data_path(),
        package.pkg_name
    ));

    create_symlink(real_path, &final_path)
}

pub async fn integrate_remote<P: AsRef<Path>>(
    package_path: P,
    package: &Package,
) -> SoarResult<()> {
    let package_path = package_path.as_ref();
    let icon_url = &package.icon;
    let desktop_url = &package.desktop;

    let mut icon_output_path = package_path.join(".DirIcon");
    let desktop_output_path = package_path.join(format!("{}.desktop", package.pkg));

    let downloader = Downloader::default();

    if let Some(icon_url) = icon_url {
        let options = DownloadOptions {
            url: icon_url.clone(),
            output_path: Some(icon_output_path.to_string_lossy().to_string()),
            progress_callback: None,
        };
        downloader.download(options).await?;

        let ext = if calc_magic_bytes(icon_output_path, 8)? == PNG_MAGIC_BYTES {
            "png"
        } else {
            "svg"
        };
        icon_output_path = package_path.join(format!("{}.{}", package.pkg, ext));
    }

    if let Some(desktop_url) = desktop_url {
        let options = DownloadOptions {
            url: desktop_url.clone(),
            output_path: Some(desktop_output_path.to_string_lossy().to_string()),
            progress_callback: None,
        };
        downloader.download(options).await?;
    } else {
        let content = create_default_desktop_entry(
            &package.pkg,
            &package.pkg_name,
            &package.category.replace(',', ";"),
        );
        fs::write(&desktop_output_path, &content)?;
    }

    try_join!(
        symlink_icon(&icon_output_path, &package.pkg_name),
        symlink_desktop(&desktop_output_path, &package)
    )?;

    Ok(())
}

pub fn setup_portable_dir<P: AsRef<Path>>(
    package_path: P,
    package: &Package,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
) -> SoarResult<()> {
    let package_path = package_path.as_ref();

    let pkg_config = package_path.with_extension("config");
    let pkg_home = package_path.with_extension("home");

    let (portable_home, portable_config) = if let Some(portable) = portable {
        (Some(portable.clone()), Some(portable.clone()))
    } else {
        (portable_home, portable_config)
    };

    if let Some(portable_home) = portable_home {
        if portable_home.is_empty() {
            fs::create_dir(&pkg_home)?;
        } else {
            let portable_home = PathBuf::from(portable_home)
                .join(&package.pkg_name)
                .with_extension("home");
            fs::create_dir_all(&portable_home)?;
            create_symlink(&portable_home, &pkg_home)?;
        }
    }

    if let Some(portable_config) = portable_config {
        if portable_config.is_empty() {
            fs::create_dir(&pkg_config)?;
        } else {
            let portable_config = PathBuf::from(portable_config)
                .join(&package.pkg_name)
                .with_extension("config");
            fs::create_dir_all(&portable_config)?;
            create_symlink(&portable_config, &pkg_config)?;
        }
    }

    Ok(())
}

fn create_default_desktop_entry(bin_name: &str, name: &str, categories: &str) -> Vec<u8> {
    format!(
        "[Desktop Entry]\n\
        Type=Application\n\
        Name={}\n\
        Icon={}\n\
        Exec={}\n\
        Categories={};\n",
        name, bin_name, bin_name, categories
    )
    .as_bytes()
    .to_vec()
}

pub async fn integrate_package<P: AsRef<Path>>(
    package_path: P,
    package: &Package,
    portable: Option<String>,
    portable_home: Option<String>,
    portable_config: Option<String>,
) -> SoarResult<()> {
    let package_path = package_path.as_ref();
    let bin_path = package_path.join(&package.pkg);
    let mut reader = BufReader::new(File::open(&bin_path)?);
    let file_type = get_file_type(&mut reader);

    match file_type {
        super::PackageFormat::AppImage => {
            if integrate_appimage(bin_path, package).await.is_ok() {
                setup_portable_dir(
                    package_path,
                    package,
                    portable,
                    portable_home,
                    portable_config,
                )?;
            }
        }
        super::PackageFormat::FlatImage => {
            setup_portable_dir(package_path, package, None, None, portable_config)?;
        }
        _ => {}
    }

    Ok(())
}