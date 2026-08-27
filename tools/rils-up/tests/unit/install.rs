use sha2::{Digest, Sha256};

use super::verify_checksum;

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
