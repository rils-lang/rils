use super::{Release, ReleaseAsset, latest_manager_release, manager_version_from_asset};

#[test]
fn parses_manager_version_without_confusing_checksum_assets() {
    let suffix = "-windows-x86_64.exe";
    assert_eq!(
        manager_version_from_asset("rils-up-0.2.1-windows-x86_64.exe", suffix)
            .unwrap()
            .to_string(),
        "0.2.1"
    );
    assert!(
        manager_version_from_asset("rils-up-0.2.1-windows-x86_64.exe.sha256", suffix).is_none()
    );
}

#[test]
fn selects_highest_stable_manager_asset() {
    let releases = vec![release("0.1.0", false), release("0.2.0", false)];
    let selected = latest_manager_release(&releases, "-linux-x86_64")
        .unwrap()
        .unwrap();
    assert_eq!(selected.version.to_string(), "0.2.0");
}

fn release(version: &str, prerelease: bool) -> Release {
    Release {
        draft: false,
        prerelease,
        assets: vec![
            ReleaseAsset {
                name: format!("rils-up-{version}-linux-x86_64"),
                browser_download_url: "https://example.invalid/manager".to_owned(),
            },
            ReleaseAsset {
                name: "SHA256SUMS".to_owned(),
                browser_download_url: "https://example.invalid/checksums".to_owned(),
            },
        ],
    }
}
