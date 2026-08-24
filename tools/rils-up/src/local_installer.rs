use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
};

use sha2::{Digest, Sha256};

use crate::{config, install};

const INSTALLER_MAGIC: &[u8; 16] = b"RILS-INSTALL-V1!";
const VERSION_FIELD_BYTES: usize = 64;
const FOOTER_BYTES: usize = 8 + 32 + VERSION_FIELD_BYTES + INSTALLER_MAGIC.len();
const MAX_PAYLOAD_BYTES: u64 = 512 * 1024 * 1024;

pub(crate) fn invoked_installer() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.file_stem().map(|name| name.to_owned()))
        .is_some_and(|name| {
            name.to_string_lossy()
                .to_ascii_lowercase()
                .starts_with("rils-installer-")
        })
}

pub(crate) fn run() -> Result<u8, String> {
    if std::env::args_os()
        .skip(1)
        .any(|argument| matches!(argument.to_str(), Some("-h" | "--help")))
    {
        println!("Install the embedded Rils toolchain into the user RILS_HOME");
        println!("\nUsage: rils-installer-<version>-<platform>");
        return Ok(0);
    }
    let executable = std::env::current_exe()
        .map_err(|error| format!("Could not locate the installer executable: {error}"))?;
    let payload = read_payload(&executable)?;
    let home = config::rils_home()?;
    let result =
        install::install_local_archive(&home, &payload.version, &payload.bytes, &payload.manager)?;
    if result.installed {
        println!("Installed Rils {}", result.version);
    } else {
        println!("Rils {} is already installed", result.version);
    }
    println!("rils-up installed at {}", home.join("bin").display());
    println!("Add that directory to PATH, then run `rils --version`");
    Ok(0)
}

struct InstallerPayload {
    version: String,
    bytes: Vec<u8>,
    manager: Vec<u8>,
}

fn read_payload(path: &Path) -> Result<InstallerPayload, String> {
    let mut file =
        File::open(path).map_err(|error| format!("Could not open {}: {error}", path.display()))?;
    let file_length = file
        .metadata()
        .map_err(|error| format!("Could not inspect {}: {error}", path.display()))?
        .len();
    if file_length < FOOTER_BYTES as u64 {
        return Err("Installer footer is missing".to_owned());
    }
    file.seek(SeekFrom::End(-(FOOTER_BYTES as i64)))
        .map_err(|error| format!("Could not read installer footer: {error}"))?;
    let mut footer = [0_u8; FOOTER_BYTES];
    file.read_exact(&mut footer)
        .map_err(|error| format!("Could not read installer footer: {error}"))?;
    if &footer[FOOTER_BYTES - INSTALLER_MAGIC.len()..] != INSTALLER_MAGIC {
        return Err("Installer footer has an unknown format".to_owned());
    }

    let payload_length = u64::from_le_bytes(
        footer[..8]
            .try_into()
            .map_err(|_| "Installer payload length is invalid")?,
    );
    if payload_length > MAX_PAYLOAD_BYTES || payload_length > file_length - FOOTER_BYTES as u64 {
        return Err("Installer payload length exceeds its bounds".to_owned());
    }
    let expected_hash = &footer[8..40];
    let version_bytes = &footer[40..40 + VERSION_FIELD_BYTES];
    let version_end = version_bytes
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(VERSION_FIELD_BYTES);
    let version = std::str::from_utf8(&version_bytes[..version_end])
        .map_err(|error| format!("Installer version is not UTF-8: {error}"))?;
    let version = config::validate_toolchain(version)?;

    let payload_offset = file_length - FOOTER_BYTES as u64 - payload_length;
    if payload_offset > 64 * 1024 * 1024 {
        return Err("Embedded rils-up exceeds the size limit".to_owned());
    }
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("Could not locate embedded rils-up: {error}"))?;
    let mut manager = Vec::with_capacity(payload_offset as usize);
    file.by_ref()
        .take(payload_offset)
        .read_to_end(&mut manager)
        .map_err(|error| format!("Could not read embedded rils-up: {error}"))?;
    if manager.len() as u64 != payload_offset {
        return Err("Embedded rils-up was truncated".to_owned());
    }
    file.seek(SeekFrom::Start(payload_offset))
        .map_err(|error| format!("Could not locate installer payload: {error}"))?;
    let mut bytes = Vec::with_capacity(payload_length as usize);
    file.take(payload_length)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read installer payload: {error}"))?;
    if bytes.len() as u64 != payload_length {
        return Err("Installer payload was truncated".to_owned());
    }
    let actual_hash = Sha256::digest(&bytes);
    if actual_hash.as_slice() != expected_hash {
        return Err("Installer payload checksum does not match".to_owned());
    }
    Ok(InstallerPayload {
        version,
        bytes,
        manager,
    })
}
