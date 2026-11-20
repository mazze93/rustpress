"""Build the standalone, offline-capable report reader and its RustPress route.

Also mirrors the reader and its extractors into ~/Projects/forensic_dashboard/
when that directory exists (the offline copy is generated, never hand-edited).
"""

import base64
import hashlib
import os
import re
import shutil
import subprocess
from pathlib import Path

root = Path(__file__).resolve().parent
public = root.parent.parent / "site/public"


def cargo_env() -> dict[str, str]:
    """Environment with cargo on PATH.

    Without a cargo proxy on PATH, put the pinned toolchain's bin directory
    first so cargo can also find rustc.
    """
    env = dict(os.environ)
    if shutil.which("cargo") is None:
        which = subprocess.run(
            ["rustup", "which", "cargo"], cwd=root, check=True, capture_output=True, text=True
        )
        env["PATH"] = str(Path(which.stdout.strip()).parent) + os.pathsep + env.get("PATH", "")
    return env


subprocess.run(
    [
        "cargo",
        "build",
        "--locked",
        "--release",
        "--target",
        "wasm32-unknown-unknown",
        "--target-dir",
        "target",
    ],
    cwd=root,
    check=True,
    env=cargo_env(),
)
wasm = (root / "target/wasm32-unknown-unknown/release/report_reader.wasm").read_bytes()
html = (root / "reader.html.in").read_text()


def source_hash(tag: str) -> str:
    match = re.search(f"<{tag}>(.*?)</{tag}>", html, re.S)
    if match is None:
        raise SystemExit(f"reader.html.in has no bare <{tag}> block to hash for the CSP")
    digest = hashlib.sha256(match[1].encode()).digest()
    return "'sha256-" + base64.b64encode(digest).decode() + "'"


policy = (
    f"default-src 'none'; script-src {source_hash('script')} 'wasm-unsafe-eval'; "
    f"style-src {source_hash('style')}; connect-src 'none'; base-uri 'none'; form-action 'none'"
)
html = html.replace("WASM_PAYLOAD", base64.b64encode(wasm).decode())
html = html.replace(
    "<title>", f'<meta http-equiv="Content-Security-Policy" content="{policy}"><title>', 1
)
route = public / "reports/index.html"
route.parent.mkdir(exist_ok=True)
route.write_text(html)
headers = public / "_headers"
marker = "# Generated report reader policy\n"
prior = headers.read_text()
block = (
    marker
    + "/reports/\n  ! Content-Security-Policy\n  Content-Security-Policy: "
    + policy
    + "; frame-ancestors 'none'\n# End report reader policy\n"
)
if marker in prior:
    prior = re.sub(
        re.escape(marker) + r".*?# End report reader policy\n", lambda _: block, prior, flags=re.S
    )
else:
    prior = prior.rstrip() + "\n\n" + block
headers.write_text(prior)

dashboard = root.parents[3] / "forensic_dashboard"
if dashboard.is_dir():
    (dashboard / "Report Reader.html").write_text(html)
    for extractor in sorted((root / "extractors").glob("*.py")):
        if not extractor.name.startswith("test_"):
            shutil.copyfile(extractor, dashboard / extractor.name)
    print(f"mirrored reader + extractors to {dashboard}")
