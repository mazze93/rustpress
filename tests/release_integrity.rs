use anyhow::Result;
use rustpress::{paths, verify};
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use tempfile::TempDir;
use walkdir::WalkDir;

const RELEASE: &str = "commissioning-001";
const SEAL: &str = "7f6eb4411677800d0c404c041838bd96990a63c5c90218d4f583f4c963e03d16";

// Keep the TempDir alive for the whole test. Never edit the tracked fixture.
fn fixture() -> Result<TempDir> {
    let site = tempfile::tempdir()?;
    let target = site.path().join(".rustpress/releases").join(RELEASE);
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("pressings")
        .join(RELEASE);
    for entry in WalkDir::new(&source) {
        let entry = entry?;
        let destination = target.join(entry.path().strip_prefix(&source)?);
        if entry.file_type().is_dir() {
            fs::create_dir_all(destination)?;
        } else {
            assert!(
                entry.file_type().is_file(),
                "fixture must contain regular files"
            );
            fs::copy(entry.path(), destination)?;
        }
    }
    // Establish validity before every mutation, avoiding false-positive failures.
    verify(&site.path().canonicalize()?, RELEASE, Some(SEAL))?;
    Ok(site)
}

fn inventory(root: &Path) -> Result<BTreeMap<std::path::PathBuf, Option<String>>> {
    WalkDir::new(root)
        .into_iter()
        .map(|entry| {
            let entry = entry?;
            let digest = if entry.file_type().is_file() {
                Some(paths::hash(&fs::read(entry.path())?))
            } else {
                None
            };
            Ok((entry.path().strip_prefix(root)?.to_owned(), digest))
        })
        .collect()
}

#[test]
fn valid_bundle_needs_no_source_or_current_site_config() -> Result<()> {
    let site = fixture()?;
    let root = site.path().canonicalize()?;
    assert!(!root.join("src").exists());
    assert!(!root.join("rustpress.toml").exists());
    assert!(!root.join("wrangler.json").exists());
    let verified = verify(&root, RELEASE, Some(SEAL))?;
    assert_eq!(verified.digest, SEAL);
    assert_eq!(verified.seal.release, RELEASE);
    assert_eq!(verified.seal.files.len(), 59);
    Ok(())
}

#[test]
fn changed_tree_file_is_rejected() -> Result<()> {
    let site = fixture()?;
    let root = site.path().canonicalize()?;
    fs::write(
        root.join(".rustpress/releases")
            .join(RELEASE)
            .join("tree/index.html"),
        "changed after review",
    )?;
    let error = verify(&root, RELEASE, Some(SEAL))
        .err()
        .expect("must reject changed bytes");
    assert!(error.to_string().contains("inventory/hash mismatch"));
    Ok(())
}

#[test]
fn extra_unlisted_file_is_rejected() -> Result<()> {
    let site = fixture()?;
    let root = site.path().canonicalize()?;
    fs::write(
        root.join(".rustpress/releases")
            .join(RELEASE)
            .join("tree/unlisted.txt"),
        "unreviewed",
    )?;
    let error = verify(&root, RELEASE, Some(SEAL))
        .err()
        .expect("must reject extra file");
    assert!(error.to_string().contains("inventory/hash mismatch"));
    Ok(())
}

#[test]
fn missing_sealed_file_is_rejected() -> Result<()> {
    let site = fixture()?;
    let root = site.path().canonicalize()?;
    fs::remove_file(
        root.join(".rustpress/releases")
            .join(RELEASE)
            .join("inputs/wrangler.json"),
    )?;
    let error = verify(&root, RELEASE, Some(SEAL))
        .err()
        .expect("must reject missing file");
    assert!(error.to_string().contains("inventory/hash mismatch"));
    Ok(())
}

#[test]
fn wrong_independently_reviewed_seal_is_rejected() -> Result<()> {
    let site = fixture()?;
    let root = site.path().canonicalize()?;
    let error = verify(&root, RELEASE, Some(&"0".repeat(64)))
        .err()
        .expect("must reject wrong seal");
    assert!(error.to_string().contains("independently reviewed digest"));
    Ok(())
}

#[test]
fn cli_dry_run_needs_no_credentials_or_tools_and_preserves_inventory() -> Result<()> {
    let site = fixture()?;
    let root = site.path().canonicalize()?;
    let before = inventory(&root)?;
    // Child-only environment clearing avoids unsafe global mutation and test races.
    let output = Command::new(env!("CARGO_BIN_EXE_rustpress"))
        .env_clear()
        .env("PATH", root.join("no-tools"))
        .current_dir(&root)
        .args(["deploy", "--site"])
        .arg(&root)
        .args(["--release", RELEASE, "--expect", SEAL, "--dry-run"])
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains(SEAL));
    assert!(stdout.contains("dry run"));
    assert_eq!(inventory(&root)?, before);
    assert!(!root.join(".rustpress/deploy.lock").exists());
    assert!(!root.join(".rustpress/deployments").exists());
    Ok(())
}
