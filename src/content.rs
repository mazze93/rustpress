use crate::paths;
use anyhow::{Context, Result, bail, ensure};
use lol_html::{Settings, element, rewrite_str};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Metadata {
    pub title: Option<String>,
    pub description: Option<String>,
    pub date: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub draft: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub source: String,
    pub output: String,
    pub url: String,
    pub bytes: usize,
    pub sha256: String,
    pub metadata: Metadata,
}

pub struct Corpus {
    pub tree: BTreeMap<String, Vec<u8>>,
    pub entries: Vec<Entry>,
    pub source_digest: String,
}

pub fn frontmatter(text: &str) -> Result<(Metadata, &str)> {
    if let Some(rest) = text.strip_prefix("---\n") {
        let (header, body) = rest
            .split_once("\n---\n")
            .context("unclosed YAML frontmatter")?;
        let metadata = serde_yaml_ng::from_str(header).context("invalid frontmatter")?;
        Ok((metadata, body))
    } else {
        ensure!(
            !text.starts_with("---"),
            "frontmatter must use LF line endings"
        );
        Ok((Metadata::default(), text))
    }
}

// Resource-bearing CSS is outside the v0.1 policy. Parse tokens rather than
// regex-matching: escapes and comments must not hide url(), @import or image-set().
pub fn css(text: &str) -> Result<()> {
    fn tokens<'i, 't>(
        p: &mut cssparser::Parser<'i, 't>,
    ) -> std::result::Result<(), cssparser::ParseError<'i, ()>> {
        while !p.is_exhausted() {
            let token = p.next()?.clone();
            use cssparser::Token::*;
            match token {
                AtKeyword(_) | UnquotedUrl(_) | BadUrl(_) | BadString(_) => {
                    return Err(p.new_custom_error(()));
                }
                Function(name) => {
                    let safe = [
                        "rgb",
                        "rgba",
                        "hsl",
                        "hsla",
                        "calc",
                        "min",
                        "max",
                        "clamp",
                        "var",
                        "linear-gradient",
                        "radial-gradient",
                        "repeating-linear-gradient",
                        "translate",
                        "translatex",
                        "translatey",
                        "rotate",
                        "scale",
                        "cubic-bezier",
                    ];
                    if !safe.contains(&name.to_ascii_lowercase().as_str()) {
                        return Err(p.new_custom_error(()));
                    }
                    p.parse_nested_block(tokens)?;
                }
                ParenthesisBlock | SquareBracketBlock | CurlyBracketBlock => {
                    p.parse_nested_block(tokens)?
                }
                Ident(name)
                    if ["behavior", "-moz-binding"]
                        .contains(&name.to_ascii_lowercase().as_str()) =>
                {
                    return Err(p.new_custom_error(()));
                }
                _ => {}
            }
        }
        Ok(())
    }
    let mut input = cssparser::ParserInput::new(text);
    tokens(&mut cssparser::Parser::new(&mut input))
        .map_err(|_| anyhow::anyhow!("CSS contains unsupported or resource-bearing syntax"))?;
    Ok(())
}

pub fn asset(name: &str, bytes: &[u8]) -> Result<()> {
    let extension = name.rsplit('.').next().unwrap_or("");
    let valid = match extension {
        "css" => {
            css(std::str::from_utf8(bytes)?)?;
            true
        }
        "png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        "woff2" => bytes.starts_with(b"wOF2"),
        _ => false,
    };
    ensure!(valid, "invalid signature or unsupported asset: {name}");
    Ok(())
}

fn resolve(
    value: &str,
    source: &str,
    routes: &BTreeMap<String, String>,
    drafts: &BTreeSet<String>,
    pressed: bool,
) -> Result<String> {
    ensure!(
        !value.is_empty() && value.trim() == value,
        "empty or padded URL"
    );
    ensure!(
        !value.chars().any(|c| c.is_control()) && !value.contains(['\\', '%', ':', '?']),
        "unsupported URL: {value}"
    );
    if value.starts_with('#') {
        return Ok(value.into());
    }
    let (path, fragment) = value
        .split_once('#')
        .map_or((value, String::new()), |(p, f)| (p, format!("#{f}")));
    if path.starts_with('/') {
        ensure!(
            routes.values().any(|r| r == path),
            "URL outside pressing or missing: {value}"
        );
        return Ok(format!("{path}{fragment}"));
    }
    ensure!(!pressed, "noncanonical URL in pressed HTML: {value}");
    let mut parts: Vec<&str> = source.split('/').collect();
    parts.pop();
    for part in path.split('/') {
        match part {
            "." | "" => {}
            ".." => {
                ensure!(parts.pop().is_some(), "link escapes corpus: {value}");
            }
            p => parts.push(p),
        }
    }
    let target = parts.join("/");
    ensure!(
        !drafts.contains(&target),
        "link targets excluded draft: {target}"
    );
    let mapped = routes
        .get(&target)
        .or_else(|| routes.get(&format!("{target}/index.html")))
        .or_else(|| routes.get(&format!("{target}/index.htm")))
        .context(format!("missing target: {source} -> {value}"))?;
    Ok(format!("{mapped}{fragment}"))
}

pub fn html(
    text: &str,
    source: &str,
    routes: &BTreeMap<String, String>,
    drafts: &BTreeSet<String>,
    pressed: bool,
) -> Result<String> {
    ensure!(
        text.trim_start()
            .to_ascii_lowercase()
            .starts_with("<!doctype html>"),
        "HTML requires <!doctype html>: {source}"
    );
    let allowed = [
        "html",
        "head",
        "title",
        "meta",
        "link",
        "body",
        "main",
        "article",
        "section",
        "header",
        "footer",
        "nav",
        "aside",
        "div",
        "span",
        "p",
        "a",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "ul",
        "ol",
        "li",
        "dl",
        "dt",
        "dd",
        "blockquote",
        "q",
        "pre",
        "code",
        "kbd",
        "samp",
        "strong",
        "em",
        "b",
        "i",
        "u",
        "s",
        "small",
        "sub",
        "sup",
        "br",
        "hr",
        "figure",
        "figcaption",
        "img",
        "picture",
        "source",
        "table",
        "caption",
        "thead",
        "tbody",
        "tfoot",
        "tr",
        "th",
        "td",
        "time",
        "details",
        "summary",
        "abbr",
        "mark",
        "wbr",
    ];
    let result = rewrite_str(
        text,
        Settings {
            element_content_handlers: vec![element!("*", |el| {
                let mut check = || -> Result<()> {
                    let tag = el.tag_name();
                    ensure!(allowed.contains(&tag.as_str()), "forbidden element: {tag}");
                    for attr in el.attributes() {
                        let name = attr.name();
                        let generic = [
                            "id",
                            "class",
                            "lang",
                            "dir",
                            "title",
                            "role",
                            "hidden",
                            "aria-label",
                            "aria-labelledby",
                            "aria-describedby",
                            "aria-hidden",
                            "tabindex",
                        ];
                        let specific: &[&str] = match tag.as_str() {
                            "a" => &["href"],
                            "img" => &[
                                "src", "srcset", "alt", "width", "height", "loading", "decoding",
                                "sizes",
                            ],
                            "source" => &["srcset", "sizes", "type", "media"],
                            "link" => &["rel", "href"],
                            "meta" => &["charset", "name", "content"],
                            "ol" => &["start", "reversed", "type"],
                            "li" => &["value"],
                            "time" => &["datetime"],
                            "th" | "td" => &["colspan", "rowspan", "scope", "headers"],
                            "details" => &["open"],
                            _ => &[],
                        };
                        ensure!(
                            generic.contains(&name.as_str()) || specific.contains(&name.as_str()),
                            "forbidden attribute: {tag}[{name}]"
                        );
                    }
                    if tag == "link" {
                        ensure!(
                            el.get_attribute("rel").as_deref() == Some("stylesheet"),
                            "only stylesheet links are accepted"
                        );
                    }
                    if tag == "meta" {
                        if let Some(name) = el.get_attribute("name") {
                            ensure!(
                                ["description", "viewport", "author", "date", "keywords"]
                                    .contains(&name.as_str()),
                                "unsupported meta name"
                            );
                        } else {
                            ensure!(
                                el.get_attribute("charset")
                                    .is_some_and(|v| v.eq_ignore_ascii_case("utf-8")),
                                "meta requires UTF-8 charset or an allowed name"
                            );
                        }
                    }
                    for attr in ["href", "src"] {
                        if let Some(value) = el.get_attribute(attr) {
                            let target = resolve(&value, source, routes, drafts, pressed)?;
                            if tag == "link" {
                                ensure!(
                                    target.ends_with(".css"),
                                    "stylesheet must be a local CSS file"
                                );
                            }
                            if (tag == "img" || tag == "source") && attr == "src" {
                                ensure!(
                                    [".png", ".jpg", ".jpeg", ".gif", ".webp"].iter().any(|e| {
                                        target.split('#').next().unwrap_or("").ends_with(e)
                                    }),
                                    "image must be a raster asset"
                                );
                            }
                            el.set_attribute(attr, &target)?;
                        }
                    }
                    if let Some(value) = el.get_attribute("srcset") {
                        let mut candidates = Vec::new();
                        for candidate in value.split(',') {
                            let parts: Vec<_> = candidate.split_whitespace().collect();
                            ensure!((1..=2).contains(&parts.len()), "invalid srcset");
                            if parts.len() == 2 {
                                let d = parts[1];
                                ensure!(
                                    (d.ends_with('w') || d.ends_with('x'))
                                        && d[..d.len() - 1]
                                            .parse::<f64>()
                                            .is_ok_and(|n| n.is_finite() && n > 0.0),
                                    "invalid srcset descriptor"
                                );
                            }
                            let target = resolve(parts[0], source, routes, drafts, pressed)?;
                            ensure!(
                                [".png", ".jpg", ".jpeg", ".gif", ".webp"]
                                    .iter()
                                    .any(|e| target.ends_with(e)),
                                "srcset must reference raster assets"
                            );
                            candidates.push(if parts.len() == 2 {
                                format!("{target} {}", parts[1])
                            } else {
                                target
                            });
                        }
                        el.set_attribute("srcset", &candidates.join(", "))?;
                    }
                    Ok(())
                };
                check().map_err(Into::into)
            })],
            ..Settings::default()
        },
    )?;
    Ok(result)
}

pub fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

pub fn compile(files: BTreeMap<String, Vec<u8>>, max_html: u64) -> Result<Corpus> {
    let source_digest = paths::hash(&serde_json::to_vec(
        &files
            .iter()
            .map(|(p, b)| (p, paths::hash(b)))
            .collect::<BTreeMap<_, _>>(),
    )?);
    let mut documents = BTreeMap::new();
    let mut routes = BTreeMap::new();
    let mut outputs = BTreeSet::new();
    let mut drafts = BTreeSet::new();
    let mut source_cases = BTreeSet::new();
    for (name, bytes) in &files {
        ensure!(
            source_cases.insert(name.to_ascii_lowercase()),
            "source case collision: {name}"
        );
        let (output, url) = paths::destination(name)?;
        ensure!(
            outputs.insert(output.to_ascii_lowercase()),
            "route collision: {name}"
        );
        if name.ends_with(".html") || name.ends_with(".htm") {
            ensure!(bytes.len() as u64 <= max_html, "HTML too large: {name}");
            let (meta, body) = frontmatter(std::str::from_utf8(bytes)?)?;
            if meta.draft {
                drafts.insert(name.clone());
                continue;
            }
            documents.insert(name.clone(), (meta, body.to_owned()));
        } else {
            asset(name, bytes)?;
        }
        routes.insert(name.clone(), url);
    }
    if !routes.values().any(|url| url == "/published/") {
        routes.insert("(generated index)".into(), "/published/".into());
    }
    // Detect file/directory prefix collisions before touching a filesystem.
    for path in &outputs {
        for (i, _) in path.match_indices('/') {
            ensure!(
                !outputs.contains(&path[..i]),
                "file/directory collision: {path}"
            );
        }
    }
    let mut tree = BTreeMap::new();
    let mut entries = Vec::new();
    for (name, original) in files {
        if drafts.contains(&name) {
            continue;
        }
        let (output, url) = paths::destination(&name)?;
        let (metadata, bytes) = if let Some((mut metadata, text)) = documents.remove(&name) {
            if metadata.title.is_none() {
                let title = Rc::new(RefCell::new(String::new()));
                let out = title.clone();
                rewrite_str(
                    &text,
                    Settings {
                        element_content_handlers: vec![lol_html::text!("title", move |t| {
                            out.borrow_mut().push_str(t.as_str());
                            Ok(())
                        })],
                        ..Settings::default()
                    },
                )?;
                if !title.borrow().is_empty() {
                    metadata.title = Some(title.borrow().clone());
                }
            }
            (
                metadata,
                html(&text, &name, &routes, &drafts, false)?.into_bytes(),
            )
        } else {
            (Metadata::default(), original)
        };
        entries.push(Entry {
            source: name,
            output: output.clone(),
            url,
            bytes: bytes.len(),
            sha256: paths::hash(&bytes),
            metadata,
        });
        tree.insert(output, bytes);
    }
    ensure!(!entries.is_empty(), "no publishable files");
    if !tree.contains_key("index.html") {
        let rows = entries
            .iter()
            .filter(|e| e.output.ends_with(".html"))
            .map(|e| {
                format!(
                    "<li><a href=\"{}\">{}</a></li>",
                    e.url,
                    escape(e.metadata.title.as_deref().unwrap_or(&e.source))
                )
            })
            .collect::<String>();
        let bytes = format!("<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\"><title>Pressed works</title></head><body><main><h1>Pressed works</h1><ul>{rows}</ul></main></body></html>").into_bytes();
        entries.push(Entry {
            source: "(generated index)".into(),
            output: "index.html".into(),
            url: "/published/".into(),
            bytes: bytes.len(),
            sha256: paths::hash(&bytes),
            metadata: Metadata {
                title: Some("Pressed works".into()),
                ..Metadata::default()
            },
        });
        tree.insert("index.html".into(), bytes);
    }
    entries.sort_by(|a, b| a.output.cmp(&b.output));
    Ok(Corpus {
        tree,
        entries,
        source_digest,
    })
}

pub fn validate_pressed(entries: &[Entry], tree: &BTreeMap<String, Vec<u8>>) -> Result<()> {
    let routes = entries
        .iter()
        .map(|e| (e.source.clone(), e.url.clone()))
        .collect();
    let mut seen = BTreeSet::new();
    for e in entries {
        paths::relative(&e.output)?;
        ensure!(
            seen.insert(e.output.to_ascii_lowercase()),
            "duplicate manifest path"
        );
        let bytes = tree.get(&e.output).context("missing manifest file")?;
        ensure!(
            bytes.len() == e.bytes && paths::hash(bytes) == e.sha256,
            "manifest hash mismatch: {}",
            e.output
        );
        if e.output.ends_with(".html") {
            html(
                std::str::from_utf8(bytes)?,
                &e.source,
                &routes,
                &BTreeSet::new(),
                true,
            )?;
        } else {
            asset(&e.output, bytes)?;
        }
    }
    if entries.len() != tree.len() {
        bail!("unlisted corpus files");
    }
    Ok(())
}
