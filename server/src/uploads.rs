use std::{
    fs::{self, OpenOptions},
    io::{Cursor, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};
use image::{ImageFormat, ImageReader, Limits};
use rand_core::{OsRng, RngCore};
use tokio::sync::watch;
use tracing::warn;

use crate::wait_for_shutdown;

pub const MAX_IMAGE_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_DIMENSION: u32 = 4096;
pub const MAX_PIXELS: u64 = 12_000_000;
const RETENTION: Duration = Duration::from_secs(24 * 60 * 60);
const CLEANUP_INTERVAL: Duration = Duration::from_secs(15 * 60);
const MAX_DECODE_ALLOC: u64 = 128 * 1024 * 1024;
const FILE_PREFIX: &str = "upload-";
const FILE_SUFFIX: &str = ".png";

#[derive(Clone)]
pub struct UploadStore {
    dir: PathBuf,
}

#[derive(Debug)]
pub struct NormalizedImage {
    pub png: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

pub struct StoredUpload {
    pub id: String,
    pub path: PathBuf,
}

impl UploadStore {
    pub fn from_environment() -> Result<Self> {
        let cache = std::env::var_os("XDG_CACHE_HOME")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME")
                    .filter(|value| !value.is_empty())
                    .map(|home| PathBuf::from(home).join(".cache"))
            })
            .context("XDG_CACHE_HOME or HOME is required for image uploads")?;
        Self::new(cache.join("mushu/uploads"))
    }

    fn new(dir: PathBuf) -> Result<Self> {
        fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create upload directory {}", dir.display()))?;
        set_directory_mode(&dir)?;
        Ok(Self { dir })
    }

    pub fn cleanup_expired(&self) -> Result<()> {
        cleanup_expired_at(&self.dir, SystemTime::now())
    }

    pub async fn cleanup_loop(self, mut shutdown: watch::Receiver<bool>) {
        loop {
            tokio::select! {
                _ = tokio::time::sleep(CLEANUP_INTERVAL) => {}
                _ = wait_for_shutdown(&mut shutdown) => break,
            }
            let store = self.clone();
            match tokio::task::spawn_blocking(move || store.cleanup_expired()).await {
                Ok(Ok(())) => {}
                Ok(Err(error)) => warn!("upload cleanup failed: {error:#}"),
                Err(error) => warn!("upload cleanup task failed: {error}"),
            }
        }
    }

    pub fn store(&self, png: &[u8]) -> Result<StoredUpload> {
        let mut random = [0u8; 16];
        OsRng.fill_bytes(&mut random);
        let id = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let path = self.dir.join(format!("{FILE_PREFIX}{id}{FILE_SUFFIX}"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .with_context(|| format!("failed to create upload {}", path.display()))?;
        if let Err(error) = file.write_all(png).and_then(|()| file.sync_all()) {
            let _ = fs::remove_file(&path);
            return Err(error).context("failed to persist normalized upload");
        }
        Ok(StoredUpload { id, path })
    }
}

pub fn normalize_png(bytes: &[u8]) -> Result<NormalizedImage, &'static str> {
    if bytes.is_empty() {
        return Err("empty image");
    }
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err("image too large");
    }
    if image::guess_format(bytes).map_err(|_| "invalid image")? != ImageFormat::Png {
        return Err("unsupported image type");
    }
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DIMENSION);
    limits.max_image_height = Some(MAX_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    reader.limits(limits.clone());
    let (width, height) = reader.into_dimensions().map_err(|_| "invalid image")?;
    validate_dimensions(width, height)?;

    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Png);
    reader.limits(limits);
    let image = reader.decode().map_err(|_| "invalid image")?;
    let mut png = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut png), ImageFormat::Png)
        .map_err(|_| "image normalization failed")?;
    if png.len() > MAX_IMAGE_BYTES {
        return Err("normalized image too large");
    }
    Ok(NormalizedImage { png, width, height })
}

fn validate_dimensions(width: u32, height: u32) -> Result<(), &'static str> {
    if width == 0 || height == 0 {
        return Err("invalid image dimensions");
    }
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err("image dimensions too large");
    }
    if u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err("image has too many pixels");
    }
    Ok(())
}

fn cleanup_expired_at(dir: &Path, now: SystemTime) -> Result<()> {
    for entry in fs::read_dir(dir).context("failed to scan upload directory")? {
        let entry = entry.context("failed to read upload directory entry")?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !matching_upload_name(name) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())
            .with_context(|| format!("failed to inspect upload entry {name}"))?;
        if !metadata.file_type().is_file() {
            continue;
        }
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        if now
            .duration_since(modified)
            .is_ok_and(|age| age >= RETENTION)
        {
            fs::remove_file(entry.path())
                .with_context(|| format!("failed to remove expired upload {name}"))?;
        }
    }
    Ok(())
}

fn matching_upload_name(name: &str) -> bool {
    let Some(id) = name
        .strip_prefix(FILE_PREFIX)
        .and_then(|name| name.strip_suffix(FILE_SUFFIX))
    else {
        return false;
    };
    id.len() == 32
        && id
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(unix)]
fn set_directory_mode(dir: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to secure upload directory {}", dir.display()))
}

#[cfg(not(unix))]
fn set_directory_mode(_dir: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{DynamicImage, RgbaImage};

    fn temp_dir(name: &str) -> PathBuf {
        let mut random = [0u8; 8];
        OsRng.fill_bytes(&mut random);
        std::env::temp_dir().join(format!("mushu-{name}-{}", u64::from_le_bytes(random)))
    }

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        DynamicImage::ImageRgba8(RgbaImage::new(width, height))
            .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn normalizes_valid_png_and_rejects_wrong_format_or_dimensions() {
        let normalized = normalize_png(&png(20, 10)).unwrap();
        assert_eq!((normalized.width, normalized.height), (20, 10));
        assert_eq!(&normalized.png[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(normalize_png(b"not png").unwrap_err(), "invalid image");
        assert_eq!(
            validate_dimensions(MAX_DIMENSION + 1, 1).unwrap_err(),
            "image dimensions too large"
        );
        assert_eq!(
            validate_dimensions(4000, 4000).unwrap_err(),
            "image has too many pixels"
        );
    }

    #[test]
    fn stores_private_generated_png_path() {
        let dir = temp_dir("store");
        let store = UploadStore::new(dir.clone()).unwrap();
        let upload = store.store(b"png").unwrap();
        assert!(matching_upload_name(
            upload.path.file_name().unwrap().to_str().unwrap()
        ));
        assert_eq!(fs::read(&upload.path).unwrap(), b"png");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&dir).unwrap().permissions().mode() & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&upload.path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn cleanup_only_removes_expired_matching_regular_files() {
        let dir = temp_dir("cleanup");
        fs::create_dir_all(&dir).unwrap();
        let old = dir.join("upload-00000000000000000000000000000000.png");
        let current = dir.join("upload-11111111111111111111111111111111.png");
        let unrelated = dir.join("notes.png");
        fs::write(&old, b"old").unwrap();
        fs::write(&current, b"current").unwrap();
        fs::write(&unrelated, b"unrelated").unwrap();
        let now = SystemTime::now();
        let old_time = now - RETENTION - Duration::from_secs(1);
        filetime::set_file_mtime(&old, filetime::FileTime::from_system_time(old_time)).unwrap();
        cleanup_expired_at(&dir, now).unwrap();
        assert!(!old.exists());
        assert!(current.exists());
        assert!(unrelated.exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn cleanup_never_follows_matching_symlink() {
        use std::os::unix::fs::symlink;

        let dir = temp_dir("cleanup-symlink");
        fs::create_dir_all(&dir).unwrap();
        let target = dir.join("keep.txt");
        let link = dir.join("upload-22222222222222222222222222222222.png");
        fs::write(&target, b"keep").unwrap();
        symlink(&target, &link).unwrap();
        cleanup_expired_at(&dir, SystemTime::now() + RETENTION + Duration::from_secs(1)).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"keep");
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        fs::remove_dir_all(dir).unwrap();
    }
}
