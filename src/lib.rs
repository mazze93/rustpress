pub mod content;
pub mod paths;

use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
};

const MAX_FILES: usize = 5000;
const MAX_TOTAL: u64 = 150_000_000;
const MAX_FILE: u64 = 20_000_000;
const POLICY: &str = "rustpress-static-v1:allowlist-html;local-raster-assets;no-css-resources;ascii-paths;no-active-content";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub max_files: usize,
    pub max_total_bytes: u64,
    pub max_html_bytes: u64,
    pub max_asset_bytes: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seal {
    pub schema: u32,
    pub compiler: String,
    pub release: String,
    pub policy: String,
    pub source_tree_sha256: String,
    pub release_tree_sha256: String,
    pub files: BTreeMap<String, String>,
}

pub struct Verified {
    pub root: PathBuf,
    pub digest: String,
    pub seal: Seal,
    pub files: BTreeMap<String, Vec<u8>>,
}

fn write_tree(root: &Path, tree: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    for (name, bytes) in tree {
        let path = root.join(name);
        fs::create_dir_all(path.parent().context("missing parent")?)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    Ok(())
}

fn json<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(value)?)
}

fn hashes(files: &BTreeMap<String, Vec<u8>>) -> BTreeMap<String, String> {
    files
        .iter()
        .map(|(name, bytes)| (name.clone(), paths::hash(bytes)))
        .collect()
}

fn state(site: &Path) -> Result<PathBuf> {
    paths::no_symlinks(site)?;
    let root = site.join(".rustpress");
    if root.exists() {
        paths::no_symlinks(&root)?;
    } else {
        fs::create_dir(&root)?;
    }
    Ok(root)
}

fn release_root(site: &Path, release: &str) -> Result<PathBuf> {
    paths::label(release)?;
    let path = site.join(".rustpress/releases").join(release);
    paths::no_symlinks(&path)?;
    Ok(path)
}

fn run(program: &str, args: &[&str], cwd: &Path, deploy: bool) -> Result<()> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(cwd).env_clear();
    for key in [
        "PATH",
        "HOME",
        "TMPDIR",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "NO_PROXY",
        "SSL_CERT_FILE",
        "NODE_EXTRA_CA_CERTS",
        "SystemRoot",
    ] {
        if let Some(value) = std::env::var_os(key) {
            cmd.env(key, value);
        }
    }
    cmd.env("CI", "true")
        .env("ASTRO_TELEMETRY_DISABLED", "1")
        .env("WRANGLER_SEND_METRICS", "false");
    if deploy {
        for key in ["CLOUDFLARE_API_TOKEN", "CLOUDFLARE_ACCOUNT_ID"] {
            cmd.env(
                key,
                std::env::var(key).with_context(|| format!("{key} is required"))?,
            );
        }
    }
    let status = cmd.status().with_context(|| format!("run {program}"))?;
    ensure!(status.success(), "{program} failed with {status}");
    Ok(())
}

fn site_snapshot(site: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut files = BTreeMap::new();
    for name in [
        "package.json",
        "package-lock.json",
        "astro.config.mjs",
        "tsconfig.json",
        "wrangler.json",
        "rustpress.toml",
    ] {
        files.insert(name.into(), paths::read(&site.join(name), MAX_FILE)?);
    }
    for directory in ["src", "public"] {
        let root = site.join(directory);
        if root.exists() {
            for (name, bytes) in paths::snapshot(&root, MAX_FILES, MAX_TOTAL, MAX_FILE)? {
                ensure!(
                    !name.starts_with("published/"),
                    "site/public/published is generated; remove it before stage"
                );
                files.insert(format!("{directory}/{name}"), bytes);
            }
        }
    }
    let package: serde_json::Value = serde_json::from_slice(&files["package.json"])?;
    ensure!(
        package["scripts"]["build"] == "astro build",
        "build script must be exactly 'astro build'"
    );
    ensure!(
        package["devDependencies"]["wrangler"].as_str().is_some_and(
            |v| v.split('.').count() == 3 && v.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        ),
        "pin an exact Wrangler version"
    );
    validate_wrangler(&files["wrangler.json"])?;
    Ok(files)
}

fn validate_wrangler(bytes: &[u8]) -> Result<()> {
    let config: serde_json::Value = serde_json::from_slice(bytes)?;
    let object = config
        .as_object()
        .context("Wrangler config must be an object")?;
    for key in object.keys() {
        ensure!(
            [
                "$schema",
                "name",
                "compatibility_date",
                "workers_dev",
                "preview_urls",
                "assets",
                "routes"
            ]
            .contains(&key.as_str()),
            "unsupported Wrangler key: {key}"
        );
    }
    ensure!(
        config["workers_dev"] == false && config["preview_urls"] == false,
        "unreviewed preview hosts must be disabled"
    );
    ensure!(
        config["assets"]["directory"] == "./dist",
        "assets directory must be ./dist"
    );
    ensure!(
        config["assets"]["html_handling"] == "auto-trailing-slash"
            && config["assets"]["not_found_handling"] == "404-page",
        "unsupported asset handling"
    );
    ensure!(
        config["routes"].as_array().is_some_and(|r| r.len() == 1),
        "one explicit hostname required"
    );
    ensure!(
        config["routes"][0]["custom_domain"] == true,
        "custom domain required"
    );
    let host = config["routes"][0]["pattern"]
        .as_str()
        .context("missing hostname")?;
    ensure!(
        host.contains('.')
            && host
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b".-".contains(&b)),
        "invalid hostname"
    );
    Ok(())
}

pub fn stage(source: &Path, site: &Path, release: &str, previous: Option<&str>) -> Result<()> {
    paths::label(release)?;
    let settings = site_snapshot(site)?;
    let config: Config = toml::from_str(std::str::from_utf8(&settings["rustpress.toml"])?)?;
    ensure!(
        config.max_files > 0
            && config.max_files <= 2000
            && config.max_total_bytes > 0
            && config.max_total_bytes <= 100_000_000
            && config.max_asset_bytes <= 10_000_000
            && config.max_html_bytes <= 2_000_000,
        "limits exceed v0.1 safety ceiling"
    );
    let root = state(site)?;
    let lock_path = root.join("press.lock");
    if lock_path.exists() {
        paths::no_symlinks(&lock_path)?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.try_lock_exclusive()
        .context("another pressing is in progress")?;
    let releases = root.join("releases");
    fs::create_dir_all(&releases)?;
    paths::no_symlinks(&releases)?;
    let destination = releases.join(release);
    ensure!(!destination.exists(), "label already pressed: {release}");
    let old_entries: Vec<content::Entry> = if let Some(label) = previous {
        let old = verify(site, label, None)?;
        serde_json::from_slice(&old.files["manifest.json"])?
    } else {
        Vec::new()
    };
    let source_files = paths::snapshot(
        source,
        config.max_files,
        config.max_total_bytes,
        config.max_asset_bytes.max(config.max_html_bytes),
    )?;
    for (name, bytes) in &source_files {
        let limit = if name.ends_with(".html") || name.ends_with(".htm") {
            config.max_html_bytes
        } else {
            config.max_asset_bytes
        };
        ensure!(bytes.len() as u64 <= limit, "file exceeds limit: {name}");
    }
    let corpus = content::compile(source_files, config.max_html_bytes)?;
    content::validate_pressed(&corpus.entries, &corpus.tree)?;
    let build = tempfile::tempdir()?;
    write_tree(build.path(), &settings)?;
    write_tree(&build.path().join("public/published"), &corpus.tree)?;
    // Public manifest deliberately excludes filesystem sources and input paths.
    let public = serde_json::json!({
        "schema": 1, "release": release, "source_tree_sha256": corpus.source_digest,
        "entries": corpus.entries.iter().map(|e| serde_json::json!({
            "url": e.url, "sha256": e.sha256, "bytes": e.bytes,
            "title": e.metadata.title, "description": e.metadata.description,
            "date": e.metadata.date
        })).collect::<Vec<_>>()
    });
    fs::write(build.path().join("public/pressing.json"), json(&public)?)?;
    run(
        "npm",
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
        build.path(),
        false,
    )?;
    run("npm", &["run", "build"], build.path(), false)?;
    let dist = paths::snapshot(&build.path().join("dist"), MAX_FILES, MAX_TOTAL, MAX_FILE)?;
    for (name, bytes) in &corpus.tree {
        ensure!(
            dist.get(&format!("published/{name}")) == Some(bytes),
            "Astro modified pressed bytes: {name}"
        );
    }
    ensure!(
        dist.contains_key("_headers") && dist.contains_key("404.html"),
        "site must supply _headers and 404.html"
    );
    let mut files = BTreeMap::new();
    for (name, bytes) in dist {
        files.insert(format!("tree/{name}"), bytes);
    }
    for (name, bytes) in settings {
        files.insert(format!("inputs/{name}"), bytes);
    }
    files.insert("manifest.json".into(), json(&corpus.entries)?);
    let current: BTreeMap<_, _> = corpus
        .entries
        .iter()
        .map(|e| (e.url.clone(), e.sha256.clone()))
        .collect();
    let old: BTreeMap<_, _> = old_entries
        .iter()
        .map(|e| (e.url.clone(), e.sha256.clone()))
        .collect();
    let mut changes = std::collections::BTreeSet::new();
    for url in current.keys().chain(old.keys()) {
        if current.get(url) != old.get(url) {
            changes.insert(url.clone());
            if url.ends_with('/') {
                changes.insert(format!("{url}index.html"));
            }
        }
    }
    // These contain the new colophon and archive, even if corpus bytes repeat.
    changes.extend(["/".into(), "/index.html".into(), "/pressing.json".into()]);
    files.insert(
        "changed-urls.txt".into(),
        format!("{}\n", changes.into_iter().collect::<Vec<_>>().join("\n")).into_bytes(),
    );
    let all_hashes = hashes(&files);
    let tree_hashes: BTreeMap<_, _> = all_hashes
        .iter()
        .filter(|(k, _)| k.starts_with("tree/"))
        .collect();
    let seal = Seal {
        schema: 1,
        compiler: env!("CARGO_PKG_VERSION").into(),
        release: release.into(),
        policy: POLICY.into(),
        source_tree_sha256: corpus.source_digest,
        release_tree_sha256: paths::hash(&json(&tree_hashes)?),
        files: all_hashes,
    };
    let seal_bytes = json(&seal)?;
    let digest = paths::hash(&seal_bytes);
    files.insert("release.json".into(), seal_bytes);
    let temp = tempfile::Builder::new()
        .prefix("press-")
        .tempdir_in(&root)?;
    write_tree(temp.path(), &files)?;
    ensure!(!destination.exists(), "label already pressed");
    fs::rename(temp.path(), &destination)?;
    fs::File::open(&releases)?.sync_all()?;
    verify(site, release, Some(&digest))?;
    println!(
        "rustpress · pressed\n{release}\nseal  {digest}\n{} corpus files\n{}\nReview this digest independently before deploy.",
        corpus.entries.len(),
        destination.display()
    );
    Ok(())
}

pub fn verify(site: &Path, release: &str, expect: Option<&str>) -> Result<Verified> {
    let root = release_root(site, release)?;
    let mut files = paths::snapshot(&root, MAX_FILES * 2, MAX_TOTAL * 2, MAX_FILE)?;
    let seal_bytes = files
        .remove("release.json")
        .context("missing release.json")?;
    let digest = paths::hash(&seal_bytes);
    if let Some(expected) = expect {
        ensure!(
            digest == expected,
            "seal differs from independently reviewed digest"
        );
    }
    let seal: Seal = serde_json::from_slice(&seal_bytes)?;
    ensure!(
        seal.schema == 1 && seal.release == release && seal.policy == POLICY,
        "unsupported seal or release label mismatch"
    );
    ensure!(
        hashes(&files) == seal.files,
        "sealed file inventory/hash mismatch"
    );
    let tree_hashes: BTreeMap<_, _> = seal
        .files
        .iter()
        .filter(|(k, _)| k.starts_with("tree/"))
        .collect();
    ensure!(
        paths::hash(&json(&tree_hashes)?) == seal.release_tree_sha256,
        "deployment tree hash mismatch"
    );
    let entries: Vec<content::Entry> =
        serde_json::from_slice(files.get("manifest.json").context("missing manifest")?)?;
    let corpus = files
        .iter()
        .filter_map(|(k, v)| {
            k.strip_prefix("tree/published/")
                .map(|p| (p.to_string(), v.clone()))
        })
        .collect();
    content::validate_pressed(&entries, &corpus)?;
    validate_wrangler(
        files
            .get("inputs/wrangler.json")
            .context("missing sealed Wrangler config")?,
    )?;
    Ok(Verified {
        root,
        digest,
        seal,
        files,
    })
}

pub fn deploy(site: &Path, release: &str, expect: &str, dry_run: bool) -> Result<()> {
    let verified = verify(site, release, Some(expect))?;
    let wrangler: serde_json::Value =
        serde_json::from_slice(&verified.files["inputs/wrangler.json"])?;
    println!(
        "rustpress · deployment review\npressing {release}\nseal {}\nhost {}\nworker {}\n{}",
        verified.digest,
        wrangler["routes"][0]["pattern"],
        wrangler["name"],
        String::from_utf8_lossy(&verified.files["changed-urls.txt"])
    );
    if dry_run {
        println!("dry run · no files changed, no credentials required, no network");
        return Ok(());
    }
    for key in ["CLOUDFLARE_API_TOKEN", "CLOUDFLARE_ACCOUNT_ID"] {
        ensure!(
            !std::env::var(key).unwrap_or_default().is_empty(),
            "{key} required"
        );
    }
    let root = state(site)?;
    let lock_path = root.join("deploy.lock");
    if lock_path.exists() {
        paths::no_symlinks(&lock_path)?;
    }
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.try_lock_exclusive()
        .context("another deployment is in progress")?;
    let workspace = tempfile::tempdir()?;
    let mut projection = BTreeMap::new();
    for (name, bytes) in &verified.files {
        if let Some(path) = name.strip_prefix("tree/") {
            projection.insert(format!("dist/{path}"), bytes.clone());
        }
    }
    for name in ["package.json", "package-lock.json", "wrangler.json"] {
        projection.insert(
            name.into(),
            verified.files[&format!("inputs/{name}")].clone(),
        );
    }
    write_tree(workspace.path(), &projection)?;
    run(
        "npm",
        &["ci", "--ignore-scripts", "--no-audit", "--no-fund"],
        workspace.path(),
        false,
    )?;
    let local = workspace.path().join("node_modules/.bin/wrangler");
    let package: serde_json::Value = serde_json::from_slice(&paths::read(
        &workspace.path().join("node_modules/wrangler/package.json"),
        MAX_FILE,
    )?)?;
    let expected_package: serde_json::Value =
        serde_json::from_slice(&verified.files["inputs/package.json"])?;
    ensure!(
        package["version"] == expected_package["devDependencies"]["wrangler"],
        "Wrangler version mismatch"
    );
    let copied = paths::snapshot(
        &workspace.path().join("dist"),
        MAX_FILES,
        MAX_TOTAL,
        MAX_FILE,
    )?;
    for (name, bytes) in &copied {
        ensure!(
            verified.files.get(&format!("tree/{name}")) == Some(bytes),
            "projection hash mismatch"
        );
    }
    let receipts = root.join("deployments");
    fs::create_dir_all(&receipts)?;
    paths::no_symlinks(&receipts)?;
    let mut receipt = tempfile::Builder::new()
        .prefix(&format!("{release}-"))
        .suffix(".json")
        .tempfile_in(&receipts)?;
    // Failure after remote acceptance is recorded as unknown, never called rolled back.
    let start = serde_json::json!({"release":release,"seal":expect,"status":"attempting","host":wrangler["routes"][0]["pattern"],"wrangler":package["version"]});
    receipt.write_all(&json(&start)?)?;
    receipt.as_file().sync_all()?;
    let (_, receipt_path) = receipt.keep()?;
    let result = run(
        local.to_str().context("non UTF-8 tool path")?,
        &["deploy", "--config", "wrangler.json"],
        workspace.path(),
        true,
    );
    let final_receipt = serde_json::json!({
        "release":release,"seal":expect,"status":if result.is_ok() {"deployed"} else {"unknown"},
        "host":wrangler["routes"][0]["pattern"],"wrangler":package["version"],
        "unix_seconds":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_secs()
    });
    fs::write(&receipt_path, json(&final_receipt)?)?;
    result?;
    println!("rustpress · deployed\nreceipt {}", receipt_path.display());
    Ok(())
}
