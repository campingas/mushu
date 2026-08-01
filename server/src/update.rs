use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use futures_util::StreamExt;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    process::Command,
    sync::{Mutex, Notify},
};

const RELEASE_API: &str = "https://api.github.com/repos/campingas/mushu/releases/latest";
const RELEASE_DOWNLOAD_ROOT: &str = "https://github.com/campingas/mushu/releases/download";
const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
const CHECKSUM_LIMIT: u64 = 1024 * 1024;
const BINARY_LIMIT: u64 = 128 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Serialize)]
pub(crate) struct BuildIdentity {
    pub version: &'static str,
    pub tag: &'static str,
    pub sha: &'static str,
    pub kind: &'static str,
}

pub(crate) const BUILD: BuildIdentity = BuildIdentity {
    version: env!("CARGO_PKG_VERSION"),
    tag: env!("MUSHU_BUILD_TAG"),
    sha: env!("MUSHU_BUILD_SHA"),
    kind: env!("MUSHU_BUILD_KIND"),
};

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ReleaseSummary {
    pub tag: String,
    pub version: String,
}

#[derive(Clone, Debug)]
pub(crate) struct ReleaseMetadata {
    summary: ReleaseSummary,
    assets: Vec<ReleaseAsset>,
}

struct CachedRelease {
    checked_at: Instant,
    release: ReleaseMetadata,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
enum JobState {
    Idle,
    Installing { tag: String },
    Restarting { tag: String },
    Failed { tag: String, error: String },
}

#[derive(Debug, Serialize)]
pub(crate) struct UpdateView {
    pub build: BuildIdentity,
    pub latest: Option<ReleaseSummary>,
    pub update_available: bool,
    pub install_allowed: bool,
    pub reason: Option<&'static str>,
    pub check_error: Option<String>,
    #[serde(flatten)]
    job: JobState,
}

#[derive(Debug)]
pub(crate) enum StartError {
    Concurrent,
    DevelopmentBuild,
    Stale,
    NotNewer,
    Unavailable(anyhow::Error),
}

#[derive(Clone)]
pub(crate) struct UpdateManager {
    inner: Arc<Inner>,
}

struct Inner {
    client: reqwest::Client,
    cache: Mutex<Option<CachedRelease>>,
    job: Mutex<JobState>,
    installing: AtomicBool,
    restart_requested: AtomicBool,
    shutdown: Notify,
    executable: PathBuf,
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    draft: bool,
    prerelease: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Clone, Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
    size: u64,
}

impl UpdateManager {
    pub(crate) fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(45))
            .redirect(reqwest::redirect::Policy::limited(5))
            .user_agent(concat!("mushu-server/", env!("CARGO_PKG_VERSION")))
            .build()
            .context("failed to initialize update client")?;
        let executable = std::env::current_exe().context("failed to locate running executable")?;
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                cache: Mutex::new(None),
                job: Mutex::new(JobState::Idle),
                installing: AtomicBool::new(false),
                restart_requested: AtomicBool::new(false),
                shutdown: Notify::new(),
                executable,
            }),
        })
    }

    pub(crate) async fn view(&self, refresh: bool) -> UpdateView {
        let latest = self.latest(refresh).await;
        let mut job = self.inner.job.lock().await;
        reset_failed_job(&mut job, refresh, latest.is_ok());
        update_view(latest, job.clone())
    }

    pub(crate) async fn begin(&self, expected_tag: &str) -> Result<ReleaseMetadata, StartError> {
        if self
            .inner
            .installing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return Err(StartError::Concurrent);
        }

        let result = self.revalidate(expected_tag).await;
        match result {
            Ok(release) => {
                *self.inner.job.lock().await = JobState::Installing {
                    tag: release.summary.tag.clone(),
                };
                Ok(release)
            }
            Err(error) => {
                self.inner.installing.store(false, Ordering::Release);
                Err(error)
            }
        }
    }

    async fn revalidate(&self, expected_tag: &str) -> Result<ReleaseMetadata, StartError> {
        if BUILD.kind != "stable" {
            return Err(StartError::DevelopmentBuild);
        }
        let release = self.latest(true).await.map_err(StartError::Unavailable)?;
        if expected_tag != release.summary.tag {
            return Err(StartError::Stale);
        }
        let current =
            Version::parse(BUILD.version).map_err(|e| StartError::Unavailable(e.into()))?;
        let latest = Version::parse(&release.summary.version)
            .map_err(|e| StartError::Unavailable(e.into()))?;
        if latest <= current {
            return Err(StartError::NotNewer);
        }
        Ok(release)
    }

    pub(crate) async fn install(self, release: ReleaseMetadata) {
        let tag = release.summary.tag.clone();
        match self.perform_install(&release).await {
            Ok(()) => {
                *self.inner.job.lock().await = JobState::Restarting { tag };
                self.inner.restart_requested.store(true, Ordering::Release);
                self.inner.shutdown.notify_one();
            }
            Err(error) => {
                tracing::error!("update failed: {error:#}");
                *self.inner.job.lock().await = JobState::Failed {
                    tag,
                    error: error.to_string(),
                };
                self.inner.installing.store(false, Ordering::Release);
            }
        }
    }

    pub(crate) async fn wait_for_restart(&self) {
        self.inner.shutdown.notified().await;
    }

    pub(crate) fn restart_requested(&self) -> bool {
        self.inner.restart_requested.load(Ordering::Acquire)
    }

    pub(crate) fn executable(&self) -> &Path {
        &self.inner.executable
    }

    async fn latest(&self, refresh: bool) -> Result<ReleaseMetadata> {
        if !refresh {
            let cache = self.inner.cache.lock().await;
            if let Some(cached) = cache.as_ref() {
                if cached.checked_at.elapsed() < CACHE_TTL {
                    return Ok(cached.release.clone());
                }
            }
        }

        let response = self
            .inner
            .client
            .get(RELEASE_API)
            .send()
            .await
            .context("latest-release request failed")?
            .error_for_status()
            .context("latest-release request was rejected")?;
        let raw: GithubRelease = response
            .json()
            .await
            .context("latest-release response was invalid")?;
        if raw.draft || raw.prerelease {
            bail!("GitHub latest release was not stable");
        }
        let version = parse_release_tag(&raw.tag_name)?;
        let release = ReleaseMetadata {
            summary: ReleaseSummary {
                tag: raw.tag_name,
                version: version.to_string(),
            },
            assets: raw.assets,
        };
        *self.inner.cache.lock().await = Some(CachedRelease {
            checked_at: Instant::now(),
            release: release.clone(),
        });
        Ok(release)
    }

    async fn perform_install(&self, release: &ReleaseMetadata) -> Result<()> {
        let asset_name = platform_asset()?;
        let binary = exact_asset(release, asset_name, BINARY_LIMIT)?;
        let sums = exact_asset(release, "SHA256SUMS", CHECKSUM_LIMIT)?;
        validate_asset_url(&release.summary.tag, binary)?;
        validate_asset_url(&release.summary.tag, sums)?;

        let sums_bytes = download_bytes(&self.inner.client, sums, CHECKSUM_LIMIT).await?;
        let sums_text = std::str::from_utf8(&sums_bytes).context("SHA256SUMS was not UTF-8")?;
        let expected = exact_checksum(sums_text, asset_name)?;

        let staged = staged_path(&self.inner.executable)?;
        let result = async {
            let actual = download_file(&self.inner.client, binary, &staged, BINARY_LIMIT).await?;
            if actual != expected {
                bail!("checksum mismatch for {asset_name}");
            }
            copy_executable_permissions(&self.inner.executable, &staged)?;
            validate_staged_version(&staged, release).await?;
            atomic_replace(&self.inner.executable, &staged)?;
            Ok(())
        }
        .await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(&staged).await;
        }
        result
    }
}

fn reset_failed_job(job: &mut JobState, refresh: bool, check_succeeded: bool) {
    if refresh && check_succeeded && matches!(job, JobState::Failed { .. }) {
        *job = JobState::Idle;
    }
}

fn parse_release_tag(tag: &str) -> Result<Version> {
    let raw = tag
        .strip_prefix('v')
        .ok_or_else(|| anyhow!("release tag must start with v"))?;
    let version = Version::parse(raw).context("release tag was not semantic versioning")?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        bail!("latest release tag was not a plain stable version");
    }
    Ok(version)
}

fn update_view(release: Result<ReleaseMetadata>, job: JobState) -> UpdateView {
    let current = Version::parse(BUILD.version);
    match (release, current) {
        (Ok(release), Ok(current)) => {
            let latest = Version::parse(&release.summary.version);
            match latest {
                Ok(latest) => {
                    let stable = BUILD.kind == "stable";
                    let newer = latest > current;
                    UpdateView {
                        build: BUILD,
                        latest: Some(release.summary),
                        update_available: newer,
                        install_allowed: stable && newer,
                        reason: (!stable).then_some("development builds cannot self-update"),
                        check_error: None,
                        job,
                    }
                }
                Err(error) => unavailable_view(job, error.to_string()),
            }
        }
        (Err(error), _) => unavailable_view(job, error.to_string()),
        (_, Err(error)) => unavailable_view(job, error.to_string()),
    }
}

fn unavailable_view(job: JobState, error: String) -> UpdateView {
    UpdateView {
        build: BUILD,
        latest: None,
        update_available: false,
        install_allowed: false,
        reason: Some("release check unavailable"),
        check_error: Some(error),
        job,
    }
}

fn platform_asset() -> Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => Ok("mushu-server-macos-aarch64"),
        ("macos", "x86_64") => Ok("mushu-server-macos-x86_64"),
        ("linux", "aarch64") => Ok("mushu-server-linux-aarch64"),
        ("linux", "x86_64") => Ok("mushu-server-linux-x86_64"),
        (os, arch) => bail!("self-update is unsupported on {os}/{arch}"),
    }
}

fn exact_asset<'a>(
    release: &'a ReleaseMetadata,
    name: &str,
    limit: u64,
) -> Result<&'a ReleaseAsset> {
    let mut matches = release.assets.iter().filter(|asset| asset.name == name);
    let asset = matches
        .next()
        .with_context(|| format!("release is missing {name}"))?;
    if matches.next().is_some() {
        bail!("release contains duplicate {name} assets");
    }
    if asset.size == 0 || asset.size > limit {
        bail!("release asset {name} has an invalid size");
    }
    Ok(asset)
}

fn validate_asset_url(tag: &str, asset: &ReleaseAsset) -> Result<()> {
    let expected = format!("{RELEASE_DOWNLOAD_ROOT}/{tag}/{}", asset.name);
    if asset.browser_download_url != expected {
        bail!("release asset URL did not match the fixed repository and tag");
    }
    Ok(())
}

async fn download_bytes(
    client: &reqwest::Client,
    asset: &ReleaseAsset,
    limit: u64,
) -> Result<Vec<u8>> {
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .with_context(|| format!("failed to download {}", asset.name))?
        .error_for_status()
        .with_context(|| format!("download rejected for {}", asset.name))?;
    if response.content_length().is_some_and(|size| size > limit) {
        bail!("{} exceeded the download size limit", asset.name);
    }
    let mut bytes = Vec::with_capacity(asset.size.min(limit) as usize);
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("download stream failed")?;
        if bytes.len() as u64 + chunk.len() as u64 > limit {
            bail!("{} exceeded the download size limit", asset.name);
        }
        bytes.extend_from_slice(&chunk);
    }
    if bytes.len() as u64 != asset.size {
        bail!("{} size did not match release metadata", asset.name);
    }
    Ok(bytes)
}

async fn download_file(
    client: &reqwest::Client,
    asset: &ReleaseAsset,
    path: &Path,
    limit: u64,
) -> Result<String> {
    let response = client
        .get(&asset.browser_download_url)
        .send()
        .await
        .with_context(|| format!("failed to download {}", asset.name))?
        .error_for_status()
        .with_context(|| format!("download rejected for {}", asset.name))?;
    if response.content_length().is_some_and(|size| size > limit) {
        bail!("{} exceeded the download size limit", asset.name);
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .await
        .context("failed to create staged executable")?;
    let mut stream = response.bytes_stream();
    let mut size = 0u64;
    let mut digest = Sha256::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.context("binary download stream failed")?;
        size = size
            .checked_add(chunk.len() as u64)
            .context("binary download size overflow")?;
        if size > limit {
            bail!("{} exceeded the download size limit", asset.name);
        }
        digest.update(&chunk);
        file.write_all(&chunk)
            .await
            .context("failed to write staged executable")?;
    }
    if size != asset.size {
        bail!("{} size did not match release metadata", asset.name);
    }
    file.sync_all()
        .await
        .context("failed to fsync staged executable")?;
    drop(file);
    Ok(format!("{:x}", digest.finalize()))
}

fn exact_checksum(contents: &str, asset_name: &str) -> Result<String> {
    let mut found = None;
    for line in contents.lines() {
        let Some((digest, name)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        let name = name.trim_start().trim_start_matches('*');
        if name != asset_name {
            continue;
        }
        if found.is_some()
            || digest.len() != 64
            || !digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            bail!("SHA256SUMS contained an invalid or duplicate entry for {asset_name}");
        }
        found = Some(digest.to_ascii_lowercase());
    }
    found.with_context(|| format!("SHA256SUMS did not contain {asset_name}"))
}

fn staged_path(executable: &Path) -> Result<PathBuf> {
    let parent = executable
        .parent()
        .context("executable has no parent directory")?;
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .context("executable name was not UTF-8")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before Unix epoch")?
        .as_nanos();
    Ok(parent.join(format!(".{name}.update-{}-{nonce}", std::process::id())))
}

#[cfg(unix)]
fn copy_executable_permissions(source: &Path, staged: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(source)
        .context("failed to inspect running executable")?
        .permissions()
        .mode();
    std::fs::set_permissions(staged, std::fs::Permissions::from_mode(mode))
        .context("failed to preserve executable permissions")?;
    std::fs::File::open(staged)
        .context("failed to reopen staged executable")?
        .sync_all()
        .context("failed to fsync staged executable metadata")
}

#[cfg(not(unix))]
fn copy_executable_permissions(_source: &Path, _staged: &Path) -> Result<()> {
    bail!("self-update is supported only on Unix platforms")
}

async fn validate_staged_version(path: &Path, release: &ReleaseMetadata) -> Result<()> {
    let mut child = Command::new(path);
    child.arg("--version").kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(10), child.output())
        .await
        .context("staged --version timed out")?
        .context("failed to run staged --version")?;
    if !output.status.success() {
        bail!("staged --version failed");
    }
    let text = std::str::from_utf8(&output.stdout)
        .context("staged --version output was not UTF-8")?
        .trim();
    let prefix = format!(
        "mushu-server {} (tag {}, sha ",
        release.summary.version, release.summary.tag
    );
    let sha = text
        .strip_prefix(&prefix)
        .and_then(|rest| rest.strip_suffix(", stable)"));
    if !sha.is_some_and(|sha| sha.len() == 40 && sha.bytes().all(|byte| byte.is_ascii_hexdigit())) {
        bail!("staged --version did not identify the expected stable release");
    }
    Ok(())
}

#[cfg(unix)]
fn atomic_replace(executable: &Path, staged: &Path) -> Result<()> {
    let parent = executable
        .parent()
        .context("executable has no parent directory")?;
    let name = executable
        .file_name()
        .and_then(|name| name.to_str())
        .context("executable name was not UTF-8")?;
    let previous = parent.join(format!("{name}.previous"));
    let previous_staged = parent.join(format!(".{name}.previous-{}", std::process::id()));

    match std::fs::remove_file(&previous_staged) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("failed to remove stale .previous staging file"),
    }
    let preserve_result = (|| {
        std::fs::hard_link(executable, &previous_staged)
            .context("failed to preserve the running executable")?;
        FileSync::directory(parent)?.sync()?;
        std::fs::rename(&previous_staged, &previous).context("failed to publish .previous")?;
        FileSync::directory(parent)?.sync()?;
        Result::<()>::Ok(())
    })();
    if preserve_result.is_err() {
        let _ = std::fs::remove_file(&previous_staged);
    }
    preserve_result?;
    std::fs::rename(staged, executable).context("failed to atomically replace the executable")?;
    // The rename above is the commit point: the executable path now names the
    // verified release. A directory-sync failure cannot be reported as an
    // ordinary install failure, because retrying would corrupt `.previous`
    // and a later service restart would still launch the new binary.
    if let Err(error) = FileSync::directory(parent).and_then(FileSync::sync) {
        tracing::warn!("updated executable committed but directory fsync failed: {error:#}");
    }
    Ok(())
}

#[cfg(not(unix))]
fn atomic_replace(_executable: &Path, _staged: &Path) -> Result<()> {
    bail!("self-update is supported only on Unix platforms")
}

struct FileSync(std::fs::File);

impl FileSync {
    fn directory(path: &Path) -> Result<Self> {
        Ok(Self(
            std::fs::File::open(path).context("failed to open executable directory")?,
        ))
    }

    fn sync(self) -> Result<()> {
        self.0
            .sync_all()
            .context("failed to fsync executable directory")
    }
}

#[cfg(test)]
mod tests {
    use super::{
        atomic_replace, exact_asset, exact_checksum, parse_release_tag, platform_asset,
        reset_failed_job, update_view, validate_asset_url, JobState, ReleaseAsset, ReleaseMetadata,
        ReleaseSummary, BINARY_LIMIT,
    };
    use std::fs;

    #[test]
    fn release_tags_must_be_plain_stable_semver() {
        assert_eq!(parse_release_tag("v1.2.3").unwrap().to_string(), "1.2.3");
        assert!(parse_release_tag("1.2.3").is_err());
        assert!(parse_release_tag("v1.2.3-rc.1").is_err());
        assert!(parse_release_tag("v1.2.3+build").is_err());
    }

    #[test]
    fn checksum_requires_one_exact_asset_name() {
        let digest = "a".repeat(64);
        let contents = format!("{digest}  mushu-server-linux-x86_64\n{digest}  other");
        assert_eq!(
            exact_checksum(&contents, "mushu-server-linux-x86_64").unwrap(),
            digest
        );
        assert!(exact_checksum(&contents, "mushu-server-linux").is_err());
        assert!(exact_checksum(&format!("{digest}  target\n{digest}  target"), "target").is_err());
    }

    #[test]
    fn current_platform_has_a_release_asset() {
        let name = platform_asset().unwrap();
        assert!(name.starts_with("mushu-server-"));
    }

    #[test]
    fn release_assets_require_one_exact_bounded_name() {
        let name = platform_asset().unwrap();
        let asset = ReleaseAsset {
            name: name.into(),
            browser_download_url: format!(
                "https://github.com/campingas/mushu/releases/download/v1.2.3/{name}"
            ),
            size: 123,
        };
        let release = ReleaseMetadata {
            summary: ReleaseSummary {
                tag: "v1.2.3".into(),
                version: "1.2.3".into(),
            },
            assets: vec![asset.clone()],
        };
        assert_eq!(exact_asset(&release, name, BINARY_LIMIT).unwrap().size, 123);

        let duplicated = ReleaseMetadata {
            assets: vec![asset.clone(), asset],
            ..release
        };
        assert!(exact_asset(&duplicated, name, BINARY_LIMIT).is_err());
    }

    #[test]
    fn asset_urls_are_fixed_to_repository_tag_and_name() {
        let asset = ReleaseAsset {
            name: "mushu-server-linux-x86_64".into(),
            browser_download_url: "https://example.com/mushu-server-linux-x86_64".into(),
            size: 123,
        };
        assert!(validate_asset_url("v1.2.3", &asset).is_err());
    }

    #[test]
    fn update_view_keeps_build_identity_when_release_check_fails() {
        let view = update_view(Err(anyhow::anyhow!("offline")), JobState::Idle);
        assert_eq!(view.build.version, env!("CARGO_PKG_VERSION"));
        assert!(view.latest.is_none());
        assert!(!view.update_available);
        assert!(!view.install_allowed);
        assert_eq!(view.reason, Some("release check unavailable"));
        assert_eq!(view.check_error.as_deref(), Some("offline"));
    }

    #[test]
    fn successful_explicit_refresh_clears_a_failed_install() {
        let failed = || JobState::Failed {
            tag: "v1.2.3".into(),
            error: "download interrupted".into(),
        };

        let mut job = failed();
        reset_failed_job(&mut job, false, true);
        assert!(matches!(job, JobState::Failed { .. }));

        let mut job = failed();
        reset_failed_job(&mut job, true, false);
        assert!(matches!(job, JobState::Failed { .. }));

        let mut job = failed();
        reset_failed_job(&mut job, true, true);
        assert!(matches!(job, JobState::Idle));
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_preserves_previous_binary() {
        let root = std::env::temp_dir().join(format!(
            "mushu-update-replace-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let executable = root.join("mushu-server");
        let staged = root.join(".mushu-server.update-test");
        fs::write(&executable, b"old").unwrap();
        fs::write(&staged, b"new").unwrap();

        atomic_replace(&executable, &staged).unwrap();

        assert_eq!(fs::read(&executable).unwrap(), b"new");
        assert_eq!(
            fs::read(root.join("mushu-server.previous")).unwrap(),
            b"old"
        );
        assert!(!staged.exists());
        fs::remove_dir_all(root).unwrap();
    }
}
