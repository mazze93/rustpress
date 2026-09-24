use anyhow::{Context, Result, bail, ensure};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Component, Path},
};
use walkdir::WalkDir;

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub fn label(value: &str) -> Result<()> {
    ensure!(
        (3..=96).contains(&value.len()),
        "label must be 3–96 characters"
    );
    ensure!(
        value.as_bytes()[0].is_ascii_alphanumeric(),
        "label must start with a letter or digit"
    );
    ensure!(
        value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
        "unsafe label"
    );
    ensure!(
        !value.contains("..") && !value.ends_with('.'),
        "ambiguous label"
    );
    Ok(())
}

pub fn relative(value: &str) -> Result<()> {
    ensure!(
        !value.is_empty() && !value.starts_with('/'),
        "unsafe path: {value}"
    );
    for segment in value.split('/') {
        ensure!(
            !segment.is_empty() && !segment.starts_with('.') && !segment.ends_with('.'),
            "unsafe path: {value}"
        );
        ensure!(
            segment
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c)),
            "v0.1 requires portable ASCII paths: {value}"
        );
        let stem = segment.split('.').next().unwrap_or("").to_ascii_lowercase();
        ensure!(
            ![
                "con", "prn", "aux", "nul", "com1", "com2", "com3", "com4", "com5", "com6", "com7",
                "com8", "com9", "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8",
                "lpt9"
            ]
            .contains(&stem.as_str()),
            "reserved path: {value}"
        );
    }
    Ok(())
}

pub fn no_symlinks(path: &Path) -> Result<()> {
    let absolute = if path.is_absolute() {
        path.to_owned()
    } else {
        std::env::current_dir()?.join(path)
    };
    let mut current = std::path::PathBuf::new();
    for component in absolute.components() {
        ensure!(
            !matches!(component, Component::ParentDir),
            "parent traversal in filesystem path"
        );
        current.push(component);
        let metadata = fs::symlink_metadata(&current)
            .with_context(|| format!("inspect {}", current.display()))?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "symlink rejected: {}",
            current.display()
        );
    }
    Ok(())
}

pub fn read(path: &Path, max: u64) -> Result<Vec<u8>> {
    no_symlinks(path)?;
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    let before = file.metadata()?;
    ensure!(
        before.is_file() && before.len() <= max,
        "not a regular bounded file: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    (&file).take(max + 1).read_to_end(&mut bytes)?;
    let after = file.metadata()?;
    ensure!(
        bytes.len() as u64 <= max
            && before.len() == after.len()
            && before.modified()? == after.modified()?,
        "file changed during read: {}",
        path.display()
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(
            before.ctime() == after.ctime() && before.ctime_nsec() == after.ctime_nsec(),
            "file changed during read"
        );
    }
    Ok(bytes)
}

pub fn snapshot(
    root: &Path,
    max_files: usize,
    max_total: u64,
    max_file: u64,
) -> Result<BTreeMap<String, Vec<u8>>> {
    no_symlinks(root)?;
    ensure!(root.is_dir(), "not a directory: {}", root.display());
    let mut result = BTreeMap::new();
    let mut cases = std::collections::BTreeSet::new();
    let mut total = 0;
    for entry in WalkDir::new(root).follow_links(false).max_open(16) {
        let entry = entry?;
        if entry.path() == root {
            continue;
        }
        ensure!(
            !entry.file_type().is_symlink(),
            "symlink rejected: {}",
            entry.path().display()
        );
        let name = entry
            .path()
            .strip_prefix(root)?
            .to_str()
            .context("non UTF-8 path")?
            .to_owned();
        relative(&name)?;
        ensure!(
            cases.insert(name.to_ascii_lowercase()),
            "case collision: {name}"
        );
        if entry.file_type().is_dir() {
            continue;
        }
        ensure!(result.len() < max_files, "file count exceeds {max_files}");
        let bytes = read(entry.path(), max_file.min(max_total - total))?;
        total += bytes.len() as u64;
        ensure!(total <= max_total, "total byte limit exceeded");
        result.insert(name, bytes);
    }
    Ok(result)
}

pub fn destination(source: &str) -> Result<(String, String)> {
    relative(source)?;
    let (stem, extension) = source.rsplit_once('.').context("missing extension")?;
    ensure!(
        extension == extension.to_ascii_lowercase(),
        "extensions must be lowercase"
    );
    let output = match extension {
        "html" | "htm" if stem == "index" => "index.html".into(),
        "html" | "htm" if stem.ends_with("/index") => format!("{stem}.html"),
        "html" | "htm" => format!("{stem}/index.html"),
        "css" | "png" | "jpg" | "jpeg" | "gif" | "webp" | "woff2" => source.into(),
        _ => bail!("unsupported extension: {source}"),
    };
    let url = if output == "index.html" {
        "/published/".into()
    } else if let Some(prefix) = output.strip_suffix("index.html") {
        format!("/published/{prefix}")
    } else {
        format!("/published/{output}")
    };
    Ok((output, url))
}
