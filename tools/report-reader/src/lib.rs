use plist::Value as P;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, io::Cursor};

/// Per-file input limit. The page reads it through `limit()`, so this is the
/// only place it is defined.
const LIMIT: usize = 64 * 1024 * 1024;

fn field<'a>(v: &'a P, k: &str) -> Option<&'a P> {
    v.as_dictionary()?.get(k)
}
fn string<'a>(v: &'a P, k: &str) -> Option<&'a str> {
    field(v, k)?.as_string()
}

// IOReportTypes.h: a packed element has a 32-byte header followed by four value slots.
fn scalar(c: &P) -> Option<i64> {
    let raw = field(c, "RawElements")?.as_data()?;
    let legend = field(c, "LegendChannel")?.as_array()?;
    if raw.len() != 64 || raw[16] != 1 || raw[17] != 0 || raw[20..24] != [1, 0, 0, 0] {
        return None;
    }
    let word = |i: usize| -> [u8; 8] { raw[i..i + 8].try_into().unwrap_or_default() };
    if Some(u64::from_le_bytes(word(0))) != field(c, "DriverID")?.as_unsigned_integer()
        || Some(u64::from_le_bytes(word(8))) != legend.first()?.as_unsigned_integer()
    {
        return None;
    }
    let n = i64::from_le_bytes(word(32));
    (n != i64::MIN).then_some(n)
}

fn channels(
    node: &P,
    group: &str,
    rows: &mut Vec<Value>,
    findings: &mut Vec<String>,
    depth: usize,
) -> Result<(), String> {
    if depth > 32 {
        return Err("Group nesting exceeds the reader limit.".into());
    }
    if let Some(items) = field(node, "IOReportChannels").and_then(P::as_array) {
        for item in items {
            let label = field(item, "LegendChannel")
                .and_then(P::as_array)
                .and_then(|a| a.last())
                .and_then(P::as_string)
                .unwrap_or("Unnamed channel")
                .trim();
            let value = scalar(item);
            if group == "Link Events" {
                link_event(label, value, findings);
            }
            rows.push(json!([
                group,
                label,
                value.map_or_else(|| "Format or value unavailable".into(), |x| x.to_string())
            ]));
        }
    }
    if let Some(groups) = field(node, "IOReportChannelGroups").and_then(P::as_dictionary) {
        for (key, sub) in groups {
            let name = if group.is_empty() {
                key.clone()
            } else {
                format!("{group} / {key}")
            };
            channels(sub, &name, rows, findings, depth + 1)?;
        }
    }
    Ok(())
}

fn link_event(label: &str, value: Option<i64>, findings: &mut Vec<String>) {
    match value {
        Some(n) if n > 0 => {
            if let Some(port) = label.strip_suffix(" Link Up Timeout") {
                findings.push(format!(
                    "{port} recorded {n} link-up timeouts. The controller waited for the PCIe link to become ready and its timer expired."
                ));
            } else if label.contains("Unexpected Link Down") || label.contains(" Error") {
                findings.push(format!("{label} recorded {n} events."));
            }
        }
        Some(0) => {
            if let Some(port) = label.strip_suffix(" Unexpected Link Down") {
                findings.push(format!("{port} recorded zero unexpected PCIe link drops."));
            }
        }
        _ => {}
    }
}

fn io_report(bytes: &[u8]) -> Result<Value, String> {
    let doc =
        P::from_reader(Cursor::new(bytes)).map_err(|_| "Could not read this property list.")?;
    field(&doc, "IOReportChannelGroups")
        .and_then(P::as_dictionary)
        .ok_or("This property list has no IOReport channel groups.")?;
    let mut rows = vec![];
    let mut findings = vec![];
    channels(&doc, "", &mut rows, &mut findings, 0)?;
    let device = field(&doc, "DeviceInfo");
    let model = device
        .and_then(|d| string(d, "hw-model"))
        .unwrap_or("unknown model");
    let build = device
        .and_then(|d| string(d, "os-version"))
        .unwrap_or("unknown build");
    findings.push("Counters summarize activity over the driver's measurement window. This snapshot gives no event timeline or Wi-Fi disconnect reason.".into());
    Ok(
        json!({"kind":"Hardware telemetry","summary":format!("{} channels · {} · build {}",rows.len(),model,build),"findings":findings,"columns":["Group","Channel","Raw scalar"],"rows":rows}),
    )
}

fn crash(v: &Value) -> Option<Value> {
    let process = v
        .get("procName")
        .or_else(|| v.get("processName"))
        .and_then(Value::as_str)?;
    let exception = v
        .pointer("/exception/type")
        .or_else(|| v.get("exceptionType"))
        .and_then(Value::as_str);
    let reason = v.pointer("/termination/indicator").and_then(Value::as_str);
    if exception.is_none() && reason.is_none() {
        return None;
    }
    let detail = match exception {
        Some("EXC_BAD_ACCESS") => "The process tried to access memory it could not use.",
        Some("EXC_CRASH") => "The report classifies the process termination as a crash.",
        _ => {
            "The report records a process termination. The fields below give the recorded classification."
        }
    };
    Some(
        json!({"kind":"Crash report","summary":process,"findings":[detail],"columns":["Field","Recorded value"],"rows":[["Exception",exception],["Termination",reason],["Time",v.get("captureTime").or(v.get("timestamp"))]]}),
    )
}

// ---- IndexedDB exports (extractors/idb_export.py, format version 2) ----

const IDB_FORMAT: u32 = 2;
/// Defensive render cap; the exporter already bounds each dashboard.json table
/// (records, history, blobs) to --preview-rows, 2000 by default.
const IDB_ROWS: usize = 10_000;

/// Cheap first pass: serde skips every field it does not name.
#[derive(Deserialize)]
struct Probe {
    tool: Option<String>,
    view: Option<String>,
}

#[derive(Deserialize)]
struct Dashboard {
    format_version: u32,
    summary: Summary,
    #[serde(default)]
    records: Vec<DashRecord>,
    /// Superseded values, deleted values and tombstones, deleted values first.
    #[serde(default)]
    history: Vec<DashRecord>,
    #[serde(default)]
    blobs: Vec<DashBlob>,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Deserialize)]
struct Summary {
    /// Every surviving `LevelDB` entry, history included.
    entries: u64,
    /// Live records only.
    records: u64,
    #[serde(default)]
    status: BTreeMap<String, u64>,
    blobs: u64,
    blob_bytes: u64,
    blobs_hashed_only: u64,
    warnings: u64,
    #[serde(default)]
    kinds: BTreeMap<String, u64>,
    #[serde(default)]
    blob_types: BTreeMap<String, u64>,
}

#[derive(Deserialize)]
struct DashRecord {
    status: String,
    sequence: u64,
    kind: String,
    database_id: Option<u64>,
    object_store_id: Option<u64>,
    key: String,
    size: u64,
    #[serde(default)]
    strings: Vec<String>,
}

#[derive(Deserialize)]
struct DashBlob {
    path: String,
    size: u64,
    mime: String,
    database_id: Option<u64>,
    sha256: String,
    copied: bool,
}

fn count(n: usize) -> u64 {
    u64::try_from(n).unwrap_or(u64::MAX)
}

fn tally(m: &BTreeMap<String, u64>) -> String {
    m.iter()
        .map(|(k, n)| format!("{k} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn idb_export(text: &str, view: Option<&str>) -> Result<Value, String> {
    match view {
        Some("dashboard") => {}
        Some("full") => {
            return Err("This is the full export (export.json). Drop dashboard.json from the same folder instead.".into());
        }
        _ => return Err("Unsupported IndexedDB export view.".into()),
    }
    let d: Dashboard =
        serde_json::from_str(text).map_err(|_| "Malformed IndexedDB dashboard export.")?;
    if d.format_version != IDB_FORMAT {
        return Err(format!(
            "IndexedDB export format {} is not supported. Re-run idb_export.py.",
            d.format_version
        ));
    }
    let s = &d.summary;
    let findings = idb_findings(&d);

    let place = |db: Option<u64>, store: Option<u64>| match (db, store) {
        (Some(db), Some(store)) => format!("{db} / {store}"),
        (Some(db), None) => db.to_string(),
        _ => String::new(),
    };
    let records = d.records.iter().chain(&d.history).map(|r| {
        let section = match r.status.as_str() {
            "live" => "Record",
            "deleted" => "Deleted value",
            "superseded" => "Superseded value",
            "tombstone" => "Deletion marker",
            _ => "Entry",
        };
        let strings = r.strings.join(", ");
        json!([
            section,
            r.kind,
            place(r.database_id, r.object_store_id),
            r.key,
            r.size,
            format!("seq {} · {strings}", r.sequence)
        ])
    });
    let blobs = d.blobs.iter().map(|b| {
        let sha: String = b.sha256.chars().take(12).collect();
        let copied = if b.copied { "copied" } else { "hashed only" };
        json!([
            "Blob",
            b.mime,
            place(b.database_id, None),
            b.path,
            b.size,
            format!("sha256 {sha}… · {copied}")
        ])
    });
    let rows: Vec<Value> = records.chain(blobs).take(IDB_ROWS).collect();
    Ok(json!({
        "kind": "IndexedDB export",
        "summary": format!("{} live records · {} history entries · {} blobs ({} bytes) · {} warnings", s.records, s.entries.saturating_sub(s.records), s.blobs, s.blob_bytes, s.warnings),
        "findings": findings,
        "columns": ["Section", "Kind", "Database / store", "Key or path", "Bytes", "Detail"],
        "rows": rows,
    }))
}

fn idb_findings(d: &Dashboard) -> Vec<String> {
    let s = &d.summary;
    let mut findings = vec![];
    if !s.kinds.is_empty() {
        findings.push(format!("Live records by key kind: {}.", tally(&s.kinds)));
    }
    if !s.blob_types.is_empty() {
        findings.push(format!("Blobs by sniffed type: {}.", tally(&s.blob_types)));
    }
    if s.blobs_hashed_only > 0 {
        findings.push(format!(
            "{} blobs exceeded the copy limit. They were hashed but not copied.",
            s.blobs_hashed_only
        ));
    }
    findings.extend(d.warnings.iter().map(|w| format!("Exporter warning: {w}")));
    if count(d.warnings.len()) < s.warnings {
        findings.push(format!(
            "{} more exporter warnings are listed in export.json.",
            s.warnings - count(d.warnings.len())
        ));
    }
    let historic: Vec<String> = s
        .status
        .iter()
        .filter(|(k, n)| k.as_str() != "live" && **n > 0)
        .map(|(k, n)| format!("{k} {n}"))
        .collect();
    if historic.is_empty() {
        findings.push("No superseded or deleted entries survive in this snapshot. LevelDB compaction erases history, so this does not show the data was never changed or deleted.".into());
    } else {
        findings.push(format!(
            "History recovered from uncompacted LevelDB files: {}. Deleted values are listed first. Compaction erases history, so its absence elsewhere proves nothing.",
            historic.join(", ")
        ));
    }
    for (name, shown, total) in [
        ("records", d.records.len(), s.records),
        (
            "history entries",
            d.history.len(),
            s.entries.saturating_sub(s.records),
        ),
        ("blobs", d.blobs.len(), s.blobs),
    ] {
        if count(shown) < total {
            findings.push(format!(
                "Showing the first {shown} of {total} {name}. export.json lists all of them."
            ));
        }
    }
    findings.push("Value strings are ASCII runs found heuristically, not decoded JavaScript values. Exact bytes are in records.jsonl.".into());
    findings
}

fn json_report(text: &str) -> Result<Value, String> {
    if let Some(Ok(probe)) = serde_json::Deserializer::from_str(text)
        .into_iter::<Probe>()
        .next()
        && probe.tool.as_deref() == Some("idb-export")
    {
        return idb_export(text, probe.view.as_deref());
    }
    let mut found = None;
    for item in serde_json::Deserializer::from_str(text).into_iter::<Value>() {
        let item = item.map_err(|_| "Malformed JSON report.")?;
        if let Some(c) = crash(&item) {
            found = Some(c);
        }
    }
    found.ok_or_else(|| {
        "This JSON is neither a supported crash report nor an IndexedDB export.".into()
    })
}

fn log_report(text: &str) -> Value {
    let mut counts = BTreeMap::new();
    let mut rows = vec![];
    let mut lines = 0;
    for (i, line) in text.lines().enumerate() {
        lines += 1;
        let lower = line.to_lowercase();
        let category = if lower.contains("timed out") || lower.contains("timeout") {
            "Timeout wording"
        } else if lower.contains("disconnect") {
            "Disconnect wording"
        } else if lower.contains("denied") {
            "Denial wording"
        } else if lower.contains("failed") || lower.contains("error") {
            "Failure or error wording"
        } else if lower.contains("warn") {
            "Warning wording"
        } else {
            continue;
        };
        *counts.entry(category).or_insert(0usize) += 1;
        if rows.len() < 100 {
            rows.push(json!([
                i + 1,
                category,
                line.chars().take(500).collect::<String>()
            ]));
        }
    }
    let mut notes: Vec<_> = counts
        .iter()
        .map(|(k, n)| {
            let meaning = match *k {
                "Timeout wording" => "Check whether the operation exceeded its wait time.",
                "Disconnect wording" => "Check which connection ended and whether it reconnected.",
                "Denial wording" => {
                    "Check which request was refused and which component refused it."
                }
                "Warning wording" => {
                    "The component may be flagging a condition it could recover from."
                }
                _ => "Check the named operation and its return status.",
            };
            format!("{k} ({n} matching lines). {meaning}")
        })
        .collect();
    if notes.is_empty() {
        notes.push("No lines matched the supported wording.".into());
    }
    notes.push("Matches identify lines to read in context. Negated messages and configuration names can also match. Showing the first 100 matches, up to 500 characters each.".into());
    json!({"kind":"Text log","summary":format!("{lines} lines reviewed"),"findings":notes,"columns":["Line","Match","Source text"],"rows":rows})
}

fn analyze(bytes: &[u8], name: &str) -> Result<Value, String> {
    if bytes.is_empty() {
        return Err("The file is empty.".into());
    }
    let ext = name.rsplit('.').next().unwrap_or("").to_lowercase();
    if bytes.starts_with(b"bplist") || matches!(ext.as_str(), "xml" | "plist") {
        return io_report(bytes);
    }
    let text = std::str::from_utf8(bytes)
        .map_err(
            |_| "Unsupported binary file. Choose a plist, IPS crash report, or UTF-8 text log.",
        )?
        .trim_start_matches('\u{feff}');
    if text.trim_start().starts_with("<?xml") || text.trim_start().starts_with("<plist") {
        return io_report(text.as_bytes());
    }
    if text.contains('\0') {
        return Err("Unsupported binary file.".into());
    }
    if ext == "ips" || ext == "json" {
        return json_report(text);
    }
    if !matches!(ext.as_str(), "log" | "txt" | "out" | "crash") {
        return Err("Unsupported file type.".into());
    }
    Ok(log_report(text))
}

thread_local! {
    static BUFFER: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Maximum accepted file size in bytes.
#[unsafe(no_mangle)]
pub extern "C" fn limit() -> usize {
    LIMIT
}

/// One fresh WebAssembly instance per file. Byte zero identifies the file type.
#[unsafe(no_mangle)]
pub extern "C" fn reserve(length: usize) -> *mut u8 {
    if length > LIMIT + 1 || length == 0 {
        return std::ptr::null_mut();
    }
    BUFFER.with_borrow_mut(|buf| {
        buf.resize(length, 0);
        buf.as_mut_ptr()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn translate() -> usize {
    BUFFER.with_borrow_mut(|buf| {
        let value = match buf.split_first() {
            Some((kind, bytes)) => {
                let ext = match kind {
                    1 => "plist",
                    2 => "ips",
                    3 => "log",
                    _ => "",
                };
                analyze(bytes, ext).unwrap_or_else(|msg| json!({"error":msg}))
            }
            None => json!({"error":"Empty input"}),
        };
        *buf = value.to_string().into_bytes();
        buf.len()
    })
}

#[unsafe(no_mangle)]
pub extern "C" fn result_ptr() -> *const u8 {
    BUFFER.with_borrow(Vec::as_ptr)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dashboard(version: u32, view: &str) -> String {
        json!({
            "tool": "idb-export", "format_version": version, "view": view,
            "summary": {"entries": 306, "records": 302, "blobs": 1, "blob_bytes": 108,
                        "blobs_hashed_only": 1, "warnings": 2,
                        "status": {"live": 302, "deleted": 1, "superseded": 2, "tombstone": 1},
                        "kinds": {"object_store_data": 301, "database_metadata": 1},
                        "blob_types": {"image/png": 1}},
            "records": [{"status": "live", "sequence": 12, "kind": "object_store_data",
                         "database_id": 1, "object_store_id": 1, "key": "user-a", "size": 18,
                         "strings": ["Alice"]}],
            "history": [{"status": "deleted", "sequence": 11, "kind": "object_store_data",
                         "database_id": 1, "object_store_id": 1, "key": "user-b", "size": 4,
                         "strings": ["gone"]}],
            "blobs": [{"path": "1/00/1", "size": 108, "mime": "image/png", "database_id": 1,
                       "sha256": "a1f76d954944aa5b", "copied": false}],
            "warnings": ["000003.log: truncated tail record"],
        })
        .to_string()
    }

    #[test]
    fn idb_dashboard_card() {
        let card = analyze(dashboard(2, "dashboard").as_bytes(), "x.json").unwrap();
        assert_eq!(card["kind"], "IndexedDB export");
        let rows = card["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1][0], "Deleted value");
        let findings = card["findings"].to_string();
        assert!(findings.contains("object_store_data 301"));
        assert!(findings.contains("deleted 1, superseded 2, tombstone 1"));
        assert!(findings.contains("first 1 of 302 records"));
        assert!(findings.contains("first 1 of 4 history entries"));
        assert!(findings.contains("1 more exporter warnings"));
        assert!(findings.contains("hashed but not copied"));
    }

    #[test]
    fn idb_full_view_redirects_to_dashboard() {
        let err = analyze(dashboard(2, "full").as_bytes(), "export.json").unwrap_err();
        assert!(err.contains("dashboard.json"));
    }

    #[test]
    fn idb_old_format_rejected() {
        let err = analyze(dashboard(1, "dashboard").as_bytes(), "x.json").unwrap_err();
        assert!(err.contains("format 1"));
    }

    #[test]
    fn crash_report_still_detected() {
        let ips = br#"{"procName":"Foo","exception":{"type":"EXC_BAD_ACCESS"}}"#;
        assert_eq!(analyze(ips, "a.ips").unwrap()["kind"], "Crash report");
    }

    #[test]
    fn unrelated_json_rejected() {
        let err = analyze(br#"{"hello":1}"#, "a.json").unwrap_err();
        assert!(err.contains("neither"));
        assert!(analyze(b"[1,2]", "a.json").is_err());
    }

    #[test]
    fn reserve_enforces_limit() {
        assert!(reserve(0).is_null());
        assert!(reserve(limit() + 2).is_null());
        assert!(!reserve(16).is_null());
    }
}
