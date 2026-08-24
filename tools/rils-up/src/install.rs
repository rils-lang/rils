use std::{
    fs::{self, File},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use flate2::read::GzDecoder;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tar::Archive;
use zip::ZipArchive;

use crate::{config, platform};

pub(crate) const DEFAULT_REPOSITORY: &str = "rils-lang/rils";
const USER_AGENT: &str = "rils-up (https://github.com/rils-lang/rils)";
const MAX_METADATA_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ASSET_BYTES: u64 = 512 * 1024 * 1024;
const MAX_EXTRACTED_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_ARCHIVE_ENTRIES: usize = 100_000;

#[derive(Debug, Deserialize)]
struct Release {
    tag_name: String,
    assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

pub(crate) struct InstallResult {
    pub(crate) version: String,
    pub(crate) installed: bool,
}

pub(crate) fn install(home: &Path, requested: &str) -> Result<InstallResult, String> {
    let release = fetch_release(requested)?;
    let version = config::validate_toolchain(&release.tag_name)?;
    let platform = platform::package_platform()?;
    let archive_name = format!(
        "rils-{version}-{platform}.{}",
        platform::archive_extension()
    );
    let archive_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == archive_name)
        .ok_or_else(|| format!("Release v{version} does not contain {archive_name}"))?;
    let checksums_asset = release
        .assets
        .iter()
        .find(|asset| asset.name == "SHA256SUMS")
        .ok_or_else(|| format!("Release v{version} does not contain SHA256SUMS"))?;

    fs::create_dir_all(home.join("toolchains"))
        .map_err(|error| format!("Could not create the Rils home: {error}"))?;
    install_manager_proxies(home)?;
    let target = home.join("toolchains").join(&version);
    if target.is_dir() {
        ensure_default(home, &version)?;
        return Ok(InstallResult {
            version,
            installed: false,
        });
    }

    println!("Downloading {archive_name}");
    let checksums = download(&checksums_asset.browser_download_url, MAX_METADATA_BYTES)?;
    let archive_bytes = download(&archive_asset.browser_download_url, MAX_ASSET_BYTES)?;
    verify_checksum(&archive_name, &archive_bytes, &checksums)?;

    let staging = StagingDirectory::new(home)?;
    let archive_path = staging.path.join(&archive_name);
    fs::write(&archive_path, &archive_bytes)
        .map_err(|error| format!("Could not write {}: {error}", archive_path.display()))?;
    extract_archive(&archive_path, &staging.path)?;
    let extracted = staging.path.join(format!("rils-{version}-{platform}"));
    validate_extracted_toolchain(&extracted)?;
    fs::rename(&extracted, &target).map_err(|error| {
        format!(
            "Could not install toolchain into {}: {error}",
            target.display()
        )
    })?;
    ensure_default(home, &version)?;
    Ok(InstallResult {
        version,
        installed: true,
    })
}

pub(crate) fn set_default(home: &Path, version: &str) -> Result<String, String> {
    let version = config::validate_toolchain(version)?;
    require_installed(home, &version)?;
    let mut settings = config::read_settings(home)?;
    settings.default = Some(version.clone());
    config::write_settings(home, &settings)?;
    Ok(version)
}

pub(crate) fn installed_toolchains(home: &Path) -> Result<Vec<String>, String> {
    let directory = home.join("toolchains");
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut versions = Vec::new();
    for entry in fs::read_dir(&directory)
        .map_err(|error| format!("Could not read {}: {error}", directory.display()))?
    {
        let entry = entry.map_err(|error| format!("Could not read toolchain entry: {error}"))?;
        if !entry
            .file_type()
            .map_err(|error| format!("Could not inspect {}: {error}", entry.path().display()))?
            .is_dir()
        {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if config::validate_toolchain(&name).is_ok() {
            versions.push(name);
        }
    }
    versions.sort_by(|left, right| {
        semver::Version::parse(left)
            .unwrap()
            .cmp(&semver::Version::parse(right).unwrap())
    });
    Ok(versions)
}

pub(crate) fn uninstall(home: &Path, version: &str) -> Result<String, String> {
    let version = config::validate_toolchain(version)?;
    if config::read_settings(home)?.default.as_deref() == Some(&version) {
        return Err(format!(
            "Cannot uninstall the default toolchain {version}; select another default first"
        ));
    }
    let target = home.join("toolchains").join(&version);
    if !target.is_dir() {
        return Err(format!("Rils toolchain {version} is not installed"));
    }
    fs::remove_dir_all(&target)
        .map_err(|error| format!("Could not remove {}: {error}", target.display()))?;
    Ok(version)
}

pub(crate) fn require_installed(home: &Path, version: &str) -> Result<PathBuf, String> {
    let path = home.join("toolchains").join(version);
    if path.is_dir() {
        Ok(path)
    } else {
        Err(format!(
            "Rils toolchain {version} is not installed; run `rils-up install {version}`"
        ))
    }
}

fn ensure_default(home: &Path, version: &str) -> Result<(), String> {
    let mut settings = config::read_settings(home)?;
    if settings.default.is_none() {
        settings.default = Some(version.to_owned());
        config::write_settings(home, &settings)?;
    }
    Ok(())
}

fn fetch_release(requested: &str) -> Result<Release, String> {
    let repository =
        std::env::var("RILS_RELEASE_REPOSITORY").unwrap_or_else(|_| DEFAULT_REPOSITORY.to_owned());
    let base = format!("https://api.github.com/repos/{repository}/releases");
    let url = if requested == "stable" || requested == "latest" {
        format!("{base}/latest")
    } else {
        let version = config::validate_toolchain(requested)?;
        format!("{base}/tags/v{version}")
    };
    let bytes = download(&url, MAX_METADATA_BYTES)?;
    serde_json::from_slice(&bytes)
        .map_err(|error| format!("GitHub returned invalid release metadata: {error}"))
}

pub(crate) fn download(url: &str, maximum_bytes: u64) -> Result<Vec<u8>, String> {
    let response = ureq::get(url)
        .set("Accept", "application/vnd.github+json")
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|error| format!("Could not download {url}: {error}"))?;
    if let Some(length) = response.header("Content-Length") {
        let length = length
            .parse::<u64>()
            .map_err(|error| format!("Invalid Content-Length from {url}: {error}"))?;
        if length > maximum_bytes {
            return Err(format!("Download from {url} exceeds the size limit"));
        }
    }
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(maximum_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read {url}: {error}"))?;
    if bytes.len() as u64 > maximum_bytes {
        return Err(format!("Download from {url} exceeds the size limit"));
    }
    Ok(bytes)
}

pub(crate) fn verify_checksum(name: &str, archive: &[u8], checksums: &[u8]) -> Result<(), String> {
    let checksums = std::str::from_utf8(checksums)
        .map_err(|error| format!("SHA256SUMS is not UTF-8: {error}"))?;
    let expected = checksums
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            Some((fields.next()?, fields.next()?))
        })
        .find_map(|(digest, file)| (file.trim_start_matches('*') == name).then_some(digest))
        .ok_or_else(|| format!("SHA256SUMS does not contain {name}"))?;
    let actual = format!("{:x}", Sha256::digest(archive));
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "Checksum mismatch for {name}: expected {expected}, got {actual}"
        ))
    }
}

fn extract_archive(archive_path: &Path, destination: &Path) -> Result<(), String> {
    if archive_path.extension().and_then(|value| value.to_str()) == Some("zip") {
        extract_zip(archive_path, destination)
    } else {
        extract_tar_gz(archive_path, destination)
    }
}

fn extract_zip(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("Could not open {}: {error}", archive_path.display()))?;
    let mut archive = ZipArchive::new(file)
        .map_err(|error| format!("Invalid ZIP archive {}: {error}", archive_path.display()))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err("ZIP archive contains too many entries".to_owned());
    }
    let mut extracted_bytes = 0_u64;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .map_err(|error| format!("Invalid ZIP entry: {error}"))?;
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(format!("ZIP entry is a symbolic link: {}", entry.name()));
        }
        extracted_bytes = extracted_bytes
            .checked_add(entry.size())
            .ok_or_else(|| "ZIP extracted size overflowed".to_owned())?;
        if extracted_bytes > MAX_EXTRACTED_BYTES {
            return Err("ZIP extracted content exceeds the size limit".to_owned());
        }
        let relative = entry
            .enclosed_name()
            .ok_or_else(|| format!("ZIP entry has an unsafe path: {}", entry.name()))?;
        let output = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&output)
                .map_err(|error| format!("Could not create {}: {error}", output.display()))?;
            continue;
        }
        if let Some(parent) = output.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("Could not create {}: {error}", parent.display()))?;
        }
        let mut file = File::create(&output)
            .map_err(|error| format!("Could not create {}: {error}", output.display()))?;
        io::copy(&mut entry, &mut file)
            .map_err(|error| format!("Could not extract {}: {error}", output.display()))?;
        set_executable_permissions(&output, entry.unix_mode())?;
    }
    Ok(())
}

fn extract_tar_gz(archive_path: &Path, destination: &Path) -> Result<(), String> {
    let file = File::open(archive_path)
        .map_err(|error| format!("Could not open {}: {error}", archive_path.display()))?;
    let mut archive = Archive::new(GzDecoder::new(file));
    let entries = archive
        .entries()
        .map_err(|error| format!("Invalid tar archive {}: {error}", archive_path.display()))?;
    let mut extracted_bytes = 0_u64;
    for (index, entry) in entries.enumerate() {
        if index >= MAX_ARCHIVE_ENTRIES {
            return Err("Tar archive contains too many entries".to_owned());
        }
        let mut entry = entry.map_err(|error| format!("Invalid tar entry: {error}"))?;
        let entry_type = entry.header().entry_type();
        if !entry_type.is_file() && !entry_type.is_dir() {
            return Err(format!(
                "Tar archive contains unsupported entry type {:?}",
                entry_type
            ));
        }
        extracted_bytes = extracted_bytes
            .checked_add(
                entry
                    .header()
                    .size()
                    .map_err(|error| format!("Invalid tar entry size: {error}"))?,
            )
            .ok_or_else(|| "Tar extracted size overflowed".to_owned())?;
        if extracted_bytes > MAX_EXTRACTED_BYTES {
            return Err("Tar extracted content exceeds the size limit".to_owned());
        }
        let path = entry
            .path()
            .map_err(|error| format!("Invalid tar path: {error}"))?
            .into_owned();
        if path
            .components()
            .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
        {
            return Err(format!("Tar entry has an unsafe path: {}", path.display()));
        }
        if !entry
            .unpack_in(destination)
            .map_err(|error| format!("Could not extract {}: {error}", path.display()))?
        {
            return Err(format!(
                "Tar entry escaped the destination: {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn validate_extracted_toolchain(root: &Path) -> Result<(), String> {
    for command in ["rils", "rils-analyzer"] {
        let path = root.join("bin").join(platform::executable_name(command));
        if !path.is_file() {
            return Err(format!(
                "Downloaded toolchain is missing {}",
                path.display()
            ));
        }
    }
    Ok(())
}

fn install_manager_proxies(home: &Path) -> Result<(), String> {
    let source = std::env::current_exe()
        .map_err(|error| format!("Could not locate the running rils-up: {error}"))?;
    let bin = home.join("bin");
    fs::create_dir_all(&bin)
        .map_err(|error| format!("Could not create {}: {error}", bin.display()))?;
    for command in ["rils-up", "rils", "rils-analyzer"] {
        let destination = bin.join(platform::executable_name(command));
        if paths_refer_to_same_file(&source, &destination) {
            continue;
        }
        let mut bytes = Vec::new();
        File::open(&source)
            .and_then(|mut file| file.read_to_end(&mut bytes))
            .map_err(|error| format!("Could not read {}: {error}", source.display()))?;
        config::atomic_write(&destination, &bytes)?;
        set_executable_permissions(&destination, Some(0o755))?;
    }
    Ok(())
}

fn paths_refer_to_same_file(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => false,
    }
}

#[cfg(unix)]
fn set_executable_permissions(path: &Path, archive_mode: Option<u32>) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mode = archive_mode.unwrap_or(0o755) | 0o111;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .map_err(|error| format!("Could not set permissions on {}: {error}", path.display()))
}

#[cfg(not(unix))]
fn set_executable_permissions(_path: &Path, _archive_mode: Option<u32>) -> Result<(), String> {
    Ok(())
}

struct StagingDirectory {
    path: PathBuf,
}

impl StagingDirectory {
    fn new(home: &Path) -> Result<Self, String> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| format!("System clock is before the Unix epoch: {error}"))?
            .as_nanos();
        let path = home.join(format!(".install-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)
            .map_err(|error| format!("Could not create {}: {error}", path.display()))?;
        Ok(Self { path })
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::verify_checksum;
    use sha2::{Digest, Sha256};

    #[test]
    fn verifies_named_checksum_entries() {
        let archive = b"rils package";
        let digest = format!("{:x}", Sha256::digest(archive));
        let checksums = format!("{digest}  rils-0.4.0-linux-x86_64.tar.gz\n");
        verify_checksum(
            "rils-0.4.0-linux-x86_64.tar.gz",
            archive,
            checksums.as_bytes(),
        )
        .unwrap();
    }

    #[test]
    fn rejects_mismatched_checksums() {
        let checksums = format!("{}  package.zip\n", "0".repeat(64));
        assert!(verify_checksum("package.zip", b"changed", checksums.as_bytes()).is_err());
    }
}
