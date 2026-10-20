"""Check the built public bundle, relative links, anchors and essential metadata."""
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import unquote, urlsplit

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
        for key in ("href", "src", "action"):
            if attrs.get(key):
                self.links.append(attrs[key])
        if tag == "img":
            assert "alt" in attrs, "Image needs alternate text or an explicit decorative alt"


pages = {p: Page(p.read_text(encoding="utf-8")) for p in OUTPUT.rglob("*.html")}
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
print(f"PASS: {len(pages)} pages; {checked} local links, assets and anchors; metadata; public bundle.")
