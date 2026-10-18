"""Render one acceptance catalog as a portable document and an optional Canvas.

Uses only the Python standard library. Criteria and original report text live in
the catalog; this renderer never assigns verdicts or moves report bundles.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path


def render(catalog_path: Path, output_path: Path, canvas_path: Path | None):
    catalog = json.loads(catalog_path.read_text(encoding="utf-8-sig"))
    cases = catalog["cases"]
    assert cases and len({c["id"] for c in cases}) == len(cases), "Missing/duplicate cases"
    covered = set()
    for case in cases:
        assert case["scope"] in ("current", "historical", "known-open")
        assert case["setup"] and case["steps"] and case["checks"]
        assert len({c["id"] for c in case["checks"]}) == len(case["checks"])
        assert all(c["expected"].strip() for c in case["checks"])
        covered.update(case["reports"])
    assert covered == set(catalog["reports"]), "Report coverage mismatch"
    criteria = json.dumps({"cases": cases, "reports": catalog["reports"]}, sort_keys=True, ensure_ascii=False)
    catalog["catalogHash"] = hashlib.sha256(criteria.encode()).hexdigest()
    for report in catalog["reports"].values():
        report["referenceImages"] = []
        for filename in report["attachments"]:
            path = Path(report["path"]).parent / filename
            if path.is_file() and path.suffix.lower() in (".png", ".jpg", ".jpeg"):
                mime = "image/png" if path.suffix.lower() == ".png" else "image/jpeg"
                report["referenceImages"].append({"name": filename, "url": f"data:{mime};base64," + base64.b64encode(path.read_bytes()).decode("ascii")})
    base = Path(__file__).parent
    html = (base / "document.html").read_text(encoding="utf-8")
    replacements = {
        "/* EVALUATION_STYLE */": (base / "style.css").read_text(encoding="utf-8"),
        "/* EVALUATION_DATA */": json.dumps(catalog, ensure_ascii=False).replace("<", "\\u003c"),
        "/* EVALUATION_SCRIPT */": (base / "app.js").read_text(encoding="utf-8").replace("</script", "<\\/script"),
    }
    for token, value in replacements.items():
        assert html.count(token) == 1
        html = html.replace(token, value)
    output_path.write_text(html, encoding="utf-8")
    if canvas_path:
        # The canvas embeds the same document rather than implementing a second
        # checklist. Its only added responsibility is persistent host state/theme.
        source = '''import { useCanvasState, useEffect, useRef, useHostTheme, H1, Text, Stack } from "cursor/canvas";
const HTML = __HTML__;
const CATALOG = __HASH__;
type Saved = {runs: unknown[]; active: string; handoff: string};
export default function SlateEvaluation() {
  const theme = useHostTheme();
  const [saved, setSaved] = useCanvasState<Saved | null>("slate-evaluation-" + CATALOG, null);
  const frame = useRef<HTMLIFrameElement | null>(null);
  const latest = useRef({saved, theme}); latest.current = {saved, theme};
  function initialize() {
    const t = latest.current.theme;
    frame.current?.contentWindow?.postMessage({type: "slate-evaluation-init", catalogHash: CATALOG,
      store: latest.current.saved, theme: {bg: t.bg.editor, fg: t.text.primary, muted: t.text.secondary,
      line: t.stroke.tertiary, accent: t.accent.primary, surface: t.bg.elevated}}, "*");
  }
  useEffect(() => {
    function receive(event: MessageEvent) {
      if (event.source !== frame.current?.contentWindow || event.data?.catalogHash !== CATALOG) return;
      if (event.data.type === "slate-evaluation-ready") initialize();
      if (event.data.type === "slate-evaluation-save" && Array.isArray(event.data.store?.runs)
          && typeof event.data.store?.active === "string") setSaved(event.data.store);
    }
    window.addEventListener("message", receive);
    return () => window.removeEventListener("message", receive);
  }, []);
  return <Stack gap={8} style={{background: theme.bg.editor, color: theme.text.primary, padding: 12}}>
    <H1>Slate acceptance checklist</H1>
    <Text tone="secondary">Pass / fail observations persist beside this document. Export results for the next repair pass.</Text>
    <iframe ref={frame} title="Slate interactive evaluation document" srcDoc={HTML}
      sandbox="allow-scripts allow-downloads allow-modals" onLoad={initialize}
      style={{width: "100%", height: "85vh", border: 0, background: theme.bg.editor}} />
  </Stack>;
}
'''
        source = source.replace("__HTML__", json.dumps(html, ensure_ascii=False)).replace("__HASH__", json.dumps(catalog["catalogHash"]))
        canvas_path.write_text(source, encoding="utf-8")
    print(json.dumps({"cases": len(cases), "checks": sum(len(c["checks"]) for c in cases),
                      "reports": len(covered), "catalogHash": catalog["catalogHash"], "output": str(output_path)}, indent=2))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("catalog", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--canvas", type=Path)
    args = parser.parse_args()
    render(args.catalog, args.output, args.canvas)
