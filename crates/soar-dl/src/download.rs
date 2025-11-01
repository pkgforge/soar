use std::{
    fs::{self, File, OpenOptions, Permissions},
    io::{Read as _, Write as _},
    os::unix::fs::PermissionsExt as _,
    path::{Path, PathBuf},
};

use soar_utils::fs::is_elf;
use ureq::{
    http::{
        header::{CONTENT_DISPOSITION, CONTENT_LENGTH, CONTENT_RANGE, ETAG},
        Response,
    },
    Body,
};

use crate::{
    error::DownloadError,
    http::Http,
    types::{OverwriteMode, Progress, ResumeInfo},
    utils::{filename_from_header, filename_from_url, resolve_output_path},
    xattr::{read_resume, remove_resume, write_resume},
};

pub struct Download {
    pub url: String,
    pub output: Option<String>,
    pub overwrite: OverwriteMode,
    pub extract: bool,
    pub extract_to: Option<PathBuf>,
    pub on_progress: Option<Box<dyn Fn(Progress) + Send + Sync>>,
}

impl Download {
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            output: None,
            overwrite: OverwriteMode::Prompt,
            extract: false,
            extract_to: None,
            on_progress: None,
        }
    }

    pub fn output(mut self, output: impl Into<String>) -> Self {
        self.output = Some(output.into());
        self
    }

    pub fn overwrite(mut self, overwrite: OverwriteMode) -> Self {
        self.overwrite = overwrite;
        self
    }

    pub fn extract(mut self, extract: bool) -> Self {
        self.extract = extract;
        self
    }

    pub fn extract_to(mut self, extract_to: impl Into<PathBuf>) -> Self {
        self.extract_to = Some(extract_to.into());
        self
    }

    pub fn progress<F>(mut self, on_progress: F) -> Self
    where
        F: Fn(Progress) + Send + Sync + 'static,
    {
        self.on_progress = Some(Box::new(on_progress));
        self
    }

    pub fn execute(self) -> Result<PathBuf, DownloadError> {
        if self.output.as_deref() == Some("-") {
            return self.download_to_stdout();
        }

        let resp = Http::head(&self.url)?;

        let header_filename = resp
            .headers()
            .get(CONTENT_DISPOSITION)
            .and_then(filename_from_header);
        let url_filename = filename_from_url(&self.url);

        let output_path =
            resolve_output_path(self.output.as_deref(), url_filename, header_filename)?;

        let resume_info = if output_path.is_file() {
            match self.overwrite {
                OverwriteMode::Skip => return Ok(output_path),
                OverwriteMode::Force => {
                    fs::remove_file(&output_path)?;
                    None
                }
                OverwriteMode::Prompt => {
                    if !prompt_overwrite(&output_path)? {
                        return Ok(output_path);
                    }
                    fs::remove_file(&output_path)?;
                    None
                }
            }
        } else {
            read_resume(&output_path)
        };

        if let Some(parent) = output_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        self.download_to_file(&output_path, resume_info)?;

        if is_elf(&output_path) {
            std::fs::set_permissions(&output_path, Permissions::from_mode(0o755))?;
        }

        remove_resume(&output_path)?;

        if self.extract {
            let extract_dir = self.extract_to.unwrap_or_else(|| {
                output_path
                    .parent()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| PathBuf::from("."))
            });

            compak::extract_archive(&output_path, &extract_dir)?;
        }

        Ok(output_path)
    }

    fn download_to_stdout(&self) -> Result<PathBuf, DownloadError> {
        let resp = Http::fetch(&self.url, None, None)?;
        let mut stdout = std::io::stdout();
        let mut reader = resp.into_body().into_reader();

        std::io::copy(&mut reader, &mut stdout)?;
        stdout.flush()?;

        Ok(PathBuf::from("-"))
    }

    fn download_to_file(
        &self,
        path: &Path,
        resume_info: Option<ResumeInfo>,
    ) -> Result<(), DownloadError> {
        let (resume_from, etag) = resume_info
            .as_ref()
            .map(|r| (Some(r.downloaded), r.etag.as_deref()))
            .unwrap_or((None, None));

        let resp = Http::fetch(&self.url, resume_from, etag)?;

        let status = resp.status();
        if resume_from.is_some() && status != 206 {
            // Server doesn't support resume, start from the beginning
            return self.download_to_file(path, None);
        }

        let total = Self::parse_content_length(&resp);
        let new_etag = resp
            .headers()
            .get(ETAG)
            .and_then(|h| h.to_str().ok())
            .map(String::from);

        if let Some(ref cb) = self.on_progress {
            cb(Progress::Starting {
                total,
            });
        }

        let mut file = if resume_from.is_some() {
            OpenOptions::new().append(true).open(path)?
        } else {
            File::create(path)?
        };

        let mut reader = resp.into_body().into_reader();
        let mut buffer = [0u8; 8192];
        let mut downloaded = resume_from.unwrap_or(0);

        loop {
            let n = reader.read(&mut buffer)?;
            if n == 0 {
                break;
            }

            file.write_all(&buffer[..n])?;
            downloaded += n as u64;

            if downloaded % (1024 * 1024) == 0 {
                write_resume(
                    path,
                    &ResumeInfo {
                        downloaded,
                        total,
                        etag: new_etag.clone(),
                        last_modified: None,
                    },
                )?;
            }

            if let Some(ref cb) = self.on_progress {
                cb(Progress::Chunk {
                    current: downloaded,
                    total,
                });
            }
        }

        if let Some(ref cb) = self.on_progress {
            cb(Progress::Complete {
                total,
            });
        }

        Ok(())
    }

    fn parse_content_length(resp: &Response<Body>) -> u64 {
        resp.headers()
            .get(CONTENT_RANGE)
            .and_then(|h| h.to_str().ok())
            .and_then(|range| range.rsplit_once('/').and_then(|(_, tot)| tot.parse().ok()))
            .or_else(|| {
                resp.headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|h| h.to_str().ok())
                    .and_then(|len| len.parse::<u64>().ok())
            })
            .unwrap_or(0)
    }
}

fn prompt_overwrite(path: &Path) -> std::io::Result<bool> {
    print!("Overwrite {}? [y/N] ", path.display());
    std::io::stdout().flush()?;

    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;

    Ok(matches!(line.trim().to_lowercase().as_str(), "y" | "yes"))
}
