use rustpress::{content, paths};
use std::collections::BTreeMap;

fn page(body: &str) -> Vec<u8> {
    format!("<!doctype html><html lang=\"en\"><head><title>Test</title></head><body>{body}</body></html>").into_bytes()
}
fn compile(items: &[(&str, Vec<u8>)]) -> anyhow::Result<content::Corpus> {
    content::compile(
        items
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect(),
        2_000_000,
    )
}

#[test]
fn route_normalization() {
    for (path, output, url) in [
        ("index.html", "index.html", "/published/"),
        ("essay.html", "essay/index.html", "/published/essay/"),
        ("notes/index.htm", "notes/index.html", "/published/notes/"),
        ("assets/a.png", "assets/a.png", "/published/assets/a.png"),
    ] {
        assert_eq!(
            paths::destination(path).unwrap(),
            (output.into(), url.into())
        );
    }
}

#[test]
fn reject_unsafe_paths_and_labels() {
    for path in [
        "../x.html",
        "/x.html",
        "a//b.html",
        "a\\b.html",
        ".secret",
        "x%2f.html",
        "café.html",
        "A .html",
        "a./b",
        "CON.html",
        "NUL",
        "a\n.html",
    ] {
        assert!(paths::relative(path).is_err(), "{path}");
    }
    for label in [
        "..", "../bad", ".hidden", "ab", "x/../foo", "foo..bar", "a b",
    ] {
        assert!(paths::label(label).is_err());
    }
}

#[test]
fn rewrites_source_relative_links_and_assets() {
    let c = compile(&[
        (
            "notes/one.html",
            page("<a href=\"../two.html#ok\">Two</a><img src=\"../a.png\" alt=\"test\">"),
        ),
        ("two.html", page("<h1 id=\"ok\">Two</h1>")),
        ("a.png", b"\x89PNG\r\n\x1a\n".to_vec()),
    ])
    .unwrap();
    let html = String::from_utf8(c.tree["notes/one/index.html"].clone()).unwrap();
    assert!(html.contains("/published/two/#ok"));
    assert!(html.contains("/published/a.png"));
    content::validate_pressed(&c.entries, &c.tree).unwrap();
}

#[test]
fn active_content_matrix() {
    for html in [
        "<script>alert(1)</script>",
        "<img src=\"a.png\" onerror=\"alert(1)\">",
        "<iframe src=\"x\"></iframe>",
        "<form></form>",
        "<svg><a href=\"x\"></a></svg>",
        "<math></math>",
        "<base href=\"/\">",
        "<meta http-equiv=\"refresh\" content=\"0;url=x\">",
        "<a href=\"javascript:alert(1)\">x</a>",
        "<a href=\"jav&#x61;script:alert(1)\">x</a>",
        "<a href=\"https://example.com\">x</a>",
        "<a href=\"//example.com\">x</a>",
        "<img src=\"data:image/png;base64,AAAA\">",
        "<div style=\"background:url(https://evil.test)\">x</div>",
        "<object></object>",
        "<a href=\"#\" ping=\"https://evil.test\">x</a>",
        "<template><script>x</script></template>",
        "<input autofocus>",
        "<link rel=\"preload\" href=\"a.css\">",
        "<div xmlns=\"x\">x</div>",
    ] {
        assert!(compile(&[("a.html", page(html))]).is_err(), "{html}");
    }
}

#[test]
fn css_escape_and_network_matrix() {
    for css in [
        "a{background:url(https://evil.test)}",
        "a{background:u\\72l(x)}",
        "@import 'x.css';",
        "@\\69mport 'x.css';",
        "a{background:image-set('https://evil.test')}",
        "a{width:expression(alert(1))}",
    ] {
        assert!(content::css(css).is_err(), "{css}");
    }
    content::css("a { color: rgb(1,2,3); width:clamp(1rem,50vw,20rem); background:linear-gradient(red,blue) }").unwrap();
}

#[test]
fn collisions_and_missing_targets() {
    for items in [
        vec![("foo.html", page("a")), ("foo/index.html", page("b"))],
        vec![("Index.html", page("a")), ("index.html", page("b"))],
        vec![("a.html", page("<a href=\"no.html\">x</a>"))],
        vec![("a.html", page("<a href=\"../out.html\">x</a>"))],
    ] {
        assert!(compile(&items).is_err());
    }
}

#[test]
fn draft_and_frontmatter_fail_closed() {
    let draft = [
        b"---\ntitle: Draft\ndraft: true\n---\n".to_vec(),
        page("draft"),
    ]
    .concat();
    let error = compile(&[
        ("a.html", page("<a href=\"draft.html\">draft</a>")),
        ("draft.html", draft.clone()),
    ])
    .err()
    .unwrap();
    assert!(error.to_string().contains("excluded draft"));
    let c = compile(&[("a.html", page("ok")), ("draft.html", draft)]).unwrap();
    assert!(!c.tree.contains_key("draft/index.html"));
    for text in [
        "---\ninvalid: x\n---\n",
        "---\ndraft: [\n---\n",
        "---\ntitle: test",
    ] {
        assert!(content::frontmatter(text).is_err());
    }
}

#[test]
fn srcset_is_rewritten() {
    let c = compile(&[
        (
            "a.html",
            page("<img srcset=\"p.png 1x, p.png 2x\" alt=\"x\">"),
        ),
        ("p.png", b"\x89PNG\r\n\x1a\n".to_vec()),
    ])
    .unwrap();
    assert!(
        String::from_utf8_lossy(&c.tree["a/index.html"])
            .contains("/published/p.png 1x, /published/p.png 2x")
    );
}

#[test]
fn determinism_and_tamper_detection() {
    let inputs = [("a.html", page("original"))];
    let a = compile(&inputs).unwrap();
    let mut b = compile(&inputs).unwrap();
    assert_eq!(a.tree, b.tree);
    assert_eq!(a.source_digest, b.source_digest);
    b.tree.insert("a/index.html".into(), page("changed"));
    assert!(content::validate_pressed(&b.entries, &b.tree).is_err());
    let mut c = compile(&inputs).unwrap();
    c.tree.insert("extra.html".into(), page("injected"));
    assert!(content::validate_pressed(&c.entries, &c.tree).is_err());
}

#[test]
fn document_can_link_to_generated_index() {
    let c = compile(&[("a.html", page("<a href=\"/published/\">Archive</a>"))]).unwrap();
    content::validate_pressed(&c.entries, &c.tree).unwrap();
}

#[test]
fn limits_and_signatures() {
    assert!(content::compile(BTreeMap::from([("a.html".into(), page("x"))]), 5).is_err());
    assert!(compile(&[("a.png", page("not an image"))]).is_err());
    assert!(compile(&[("x.svg", b"<svg/>".to_vec())]).is_err());
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("a.html"), page("x")).unwrap();
    assert!(paths::snapshot(d.path(), 0, 1000, 1000).is_err());
    assert!(paths::snapshot(d.path(), 5, 3, 1000).is_err());
}

#[cfg(unix)]
#[test]
fn symlinks_rejected_at_root_nested_and_inside_root() {
    use std::os::unix::fs::symlink;
    let d = tempfile::tempdir().unwrap();
    let source = d.path().join("source");
    std::fs::create_dir(&source).unwrap();
    std::fs::write(source.join("a.html"), page("x")).unwrap();
    symlink(&source, d.path().join("linked")).unwrap();
    assert!(paths::snapshot(&d.path().join("linked"), 20, 10000, 10000).is_err());
    symlink(source.join("a.html"), source.join("b.html")).unwrap();
    assert!(paths::snapshot(&source, 20, 10000, 10000).is_err());
    assert!(paths::read(&source.join("b.html"), 10000).is_err());
}

proptest::proptest! {
    #[test]
    fn route_never_escapes(name in "[a-z][a-z0-9_-]{0,40}") {
        let (output,url) = paths::destination(&format!("{name}.html")).unwrap();
        proptest::prop_assert!(!output.starts_with('/'));
        proptest::prop_assert!(!output.contains(".."));
        proptest::prop_assert!(url.starts_with("/published/"));
        proptest::prop_assert!(url.ends_with('/'));
    }
}
