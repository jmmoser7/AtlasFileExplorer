"""Check the built public bundle, relative links, anchors and essential metadata."""
from html.parser import HTMLParser
import json
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit

from release_history import CATEGORIES, RELEASES, REPOSITORY, render_release_notes

ROOT = Path(__file__).resolve().parent.parent
OUTPUT = ROOT / "target" / "slate-site"


class Page(HTMLParser):
    def __init__(self, source):
        super().__init__()
        self.ids, self.links, self.headings = set(), [], 0
        self.title, self.description, self.viewport, self.lang = False, False, False, False
        self.feed(source)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if "id" in attrs:
            assert attrs["id"] not in self.ids, f"Duplicate ID: {attrs['id']}"
            self.ids.add(attrs["id"])
        if tag == "h1":
            self.headings += 1
        if tag == "title":
            self.title = True
        if tag == "html":
            self.lang = bool(attrs.get("lang"))
        if tag == "meta":
            self.description |= attrs.get("name") == "description"
            self.viewport |= attrs.get("name") == "viewport"
        for key in ("href", "src", "action", "poster", "data-video-src", "data-captions"):
            if attrs.get(key):
                self.links.append(attrs[key])
        if tag == "img":
            assert "alt" in attrs, "Image needs alternate text or an explicit decorative alt"


sources = {path: path.read_text(encoding="utf-8") for path in OUTPUT.rglob("*.html")}
pages = {path: Page(source) for path, source in sources.items()}
assert len(pages) == 3, "Expected home, documentation and suggestion pages"
checked = 0
for path, page in pages.items():
    assert page.headings == 1, f"{path}: expected one h1"
    assert page.title and page.description and page.viewport and page.lang, f"{path}: missing metadata"
    for link in page.links:
        url = urlsplit(link)
        if url.scheme or url.netloc:
            assert url.scheme == "https", f"Insecure external link: {link}"
            continue
        target = (path.parent / unquote(url.path)).resolve() if url.path else path
        if target.is_dir():
            target /= "index.html"
        assert target.is_relative_to(OUTPUT.resolve()), f"Link escapes public site: {link}"
        assert target.exists(), f"{path}: missing {link}"
        if url.fragment:
            assert target in pages and unquote(url.fragment) in pages[target].ids, f"Missing anchor: {link}"
        checked += 1
    assert any("assets/theme.css" in link for link in page.links), f"{path}: missing native palette"

assert not list(OUTPUT.rglob("*.py")), "Build scripts must not be published"
assert not list(OUTPUT.rglob("*.md")), "Internal docs must not be published"
assert (OUTPUT / ".nojekyll").exists()
home_path = OUTPUT / "index.html"
home = pages[home_path]
home_source = sources[home_path]
history = json.loads((ROOT / "website" / "release-history.json").read_text(encoding="utf-8"))
stable_tag = next(
    entry["tag"] for entry in history
    if "base_version" not in entry and "hold_notice" not in entry and not entry.get("source_only")
)
stable_label = f"Build {stable_tag[1:]}"
assert "release-notes" in home.ids and "#release-notes" in home.links, "Missing release history link"
build_labels = re.findall(
    r'<span\b[^>]*\bid="([^"]+)"[^>]*\bdata-stable-build\b[^>]*>([^<]+)</span>',
    home_source,
)
assert build_labels == [("hero-build", stable_label), ("download-build", stable_label)], "Missing stable build labels"
assert home.links.count(f"{RELEASES}/latest/download/AtlasSuite-stable-Setup.exe") == 2, "Expected both stable installers"
assert re.search(
    r'\bid="download-build"[^>]*>[^<]+</span>\s*<a href="#release-notes">', home_source,
), "Release notes should sit beside the Downloads build label"
release_rows = re.findall(
    r'<article class="release-entry" id="release-([^"]+)">\s*'
    r'<div class="release-entry-header">[^\n]+</div>\s*'
    r'<a class="text-link" href="([^"]+)">',
    home_source,
)
assert release_rows == [(entry["tag"], entry["download"]) for entry in history], (
    "Expected one pinned download beneath its own release heading in history order"
)
held_tags = re.findall(
    r'<article class="release-entry" id="release-([^"]+)">\s*'
    r'<div class="release-entry-header">[^\n]+<strong class="release-hold-status">On hold</strong></div>',
    home_source,
)
assert held_tags == [entry["tag"] for entry in history if "hold_notice" in entry]
source_entries = [entry for entry in history if entry.get("source_only")]
for entry in source_entries:
    tag = entry["tag"]
    article = home_source.split(f'id="release-{tag}">', 1)[1].split("</article>", 1)[0]
    assert f'{REPOSITORY}/tree/{tag}' in article
    assert f'href="{REPOSITORY}/archive/refs/tags/{tag}.zip">Download {tag[1:]} source (ZIP)' in article
    assert "Source only" in article and "Windows installer unavailable" in article

# A newer source tag must never become a fallback Windows download.
sample = {
    "tag": "v99.0.0", "date": "2099-01-01", "source_only": True,
    "download": f"{REPOSITORY}/archive/refs/tags/v99.0.0.zip",
    **{key: [] for key, _ in CATEGORIES},
}
version, _ = render_release_notes([sample, *history])
assert version == stable_tag[1:]
for changes in [
    {"download": sample["download"].replace("v99.0.0", "v98.0.0")},
    {"download": f"{RELEASES}/download/v99.0.0/AtlasSuite-stable-Setup.exe"},
    {"source_only": False},
    {"hold_notice": "Example"},
]:
    try:
        render_release_notes([{**sample, **changes}, *history])
    except ValueError:
        pass
    else:
        raise AssertionError(f"Invalid source-only entry was accepted: {changes}")

assert re.findall(r"<h5>([^<]+)</h5>", home_source) == [title for _, title in CATEGORIES] * len(history)
print(f"PASS: {len(pages)} pages; {checked} local links, assets and anchors; metadata; public bundle.")
