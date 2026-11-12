"""Build the public site using Slate's actual theme and icon. No dependencies."""
from html import escape
from hashlib import sha256
import json
import os
from pathlib import Path
import re
import shutil
import tomllib
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "website"
OUTPUT = ROOT / "target" / "slate-site"
PUBLIC_FILES = (
    "index.html", "styles.css", "release.js", "capability-videos.js", "documentation/index.html",
    "suggestions/index.html", "suggestions/suggestions.js",
)


def demo_entries():
    entries = json.loads((SOURCE / "demo-album.json").read_text(encoding="utf-8"))
    if not isinstance(entries, list):
        raise ValueError("demo-album.json must contain an ordered list")
    seen = set()
    for entry in entries:
        if not isinstance(entry, dict) or set(entry) - {"id", "title", "description"}:
            raise ValueError("Each album entry needs an id, title, and optional description")
        name, title = entry.get("id"), entry.get("title")
        description = entry.get("description", "")
        if not isinstance(name, str) or not re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name) or name in seen:
            raise ValueError("Each album id must be a unique lowercase slug")
        if not isinstance(title, str) or not title.strip() or not isinstance(description, str):
            raise ValueError("Each album title must be nonempty text and its description must be text")
        seen.add(name)
        yield name, title.strip(), description.strip()


def copy_demo_media(name, extension):
    relative = f"assets/demos/{name}.{extension}"
    asset = SOURCE / relative
    if not asset.is_file() or not asset.stat().st_size:
        return ""
    destination = OUTPUT / relative
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(asset, destination)
    return relative


def build_demonstrations():
    entries = list(demo_entries())
    videos, titles = [], []
    for name, title, description in entries:
        video = copy_demo_media(name, "mp4")
        if not video:
            continue
        poster = copy_demo_media(name, "jpg")
        captions = copy_demo_media(name, "vtt")
        hidden = " hidden" if videos else ""
        description_id = f"demo-{name}-description"
        description_attr = f' aria-describedby="{description_id}"' if description else ""
        description_markup = (
            f'<p class="demo-visually-hidden" id="{description_id}">{escape(description)}</p>' if description else ""
        )
        thumbnail = f'        <img class="demo-poster" src="{poster}" alt="" draggable="false">\n' if poster else ""
        active = "" if videos else " data-active"
        videos.append(
            f'<div class="demo-media" data-demo-slide="{name}" data-video-src="{video}" '
            f'data-captions="{captions}"{description_attr}{active}{hidden}>\n'
            f'{thumbnail}'
            '      </div>'
        )
        titles.append(f'<div data-demo-copy{hidden}><h3>{escape(title)}</h3>{description_markup}</div>')

    if not videos:
        title = entries[0][1] if entries else "Slate demonstrations"
        videos.append('<div class="demo-media" data-active><p class="demo-fallback">Video unavailable</p></div>')
        titles.append(f'<div><h3>{escape(title)}</h3></div>')
    page = OUTPUT / "index.html"
    html = page.read_text(encoding="utf-8")
    html = html.replace("<!-- Demo video markup -->", "\n      ".join(videos))
    html = html.replace("<!-- Demo title markup -->", "\n      ".join(titles))
    page.write_text(html, encoding="utf-8")


def build():
    endpoint = os.environ.get("SLATE_SUGGESTION_ENDPOINT", "").strip()
    if endpoint:
        url = urlsplit(endpoint)
        if (url.scheme != "https" or not url.hostname or url.username is not None
                or url.query or url.fragment or url.path != "/suggestions"):
            raise ValueError("SLATE_SUGGESTION_ENDPOINT must be an HTTPS /suggestions URL")
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for relative in PUBLIC_FILES:
        destination = OUTPUT / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SOURCE / relative, destination)
    build_demonstrations()
    suggestion_page = OUTPUT / "suggestions/index.html"
    suggestion_page.write_text(suggestion_page.read_text(encoding="utf-8").replace(
        'data-endpoint=""', f'data-endpoint="{escape(endpoint)}"'), encoding="utf-8")
    assets = OUTPUT / "assets"
    assets.mkdir(exist_ok=True)
    shutil.copyfile(ROOT / "apps/slate/assets/slate.ico", assets / "slate.ico")
    tokens = tomllib.loads((ROOT / "crates/atlas-shell/ui-tokens.toml").read_text(encoding="utf-8"))
    dark = tokens["theme"]["dark"]
    names = ("bg", "grid_dot", "card", "border", "border_strong", "ink", "sub",
             "accent", "select", "select_fill", "panel")
    declarations = [f"  --{key.replace('_', '-')}: {dark[key]};" for key in names]
    declarations.append("  --muted: color-mix(in srgb, var(--ink) 65%, var(--sub));")
    # The video album uses Slate Home's projection and spring tuning.
    rack_keys = ("side_step_frac", "center_bulge_frac", "bulge_width", "angle_max_deg",
                 "angle_width", "depth_max", "depth_width", "focal", "corner_bevel_frac",
                 "spring_stiffness", "spring_damping")
    declarations.extend(f"  --demo-rack-{key.replace('_', '-')}: {tokens['home'][key]};" for key in rack_keys)
    (assets / "theme.css").write_text(
        "/* Generated from atlas-shell/ui-tokens.toml. Do not edit. */\n:root {\n"
        + "\n".join(declarations) + "\n}\n", encoding="utf-8")
    (OUTPUT / ".nojekyll").write_text("", encoding="utf-8")
    # Content versions keep phone browsers on the same markup, script and layout.
    versions = {
        name: sha256((OUTPUT / name).read_bytes()).hexdigest()[:12]
        for name in ("styles.css", "capability-videos.js")
    }
    for page in OUTPUT.rglob("*.html"):
        content = page.read_text(encoding="utf-8")
        for name, version in versions.items():
            content = content.replace(f'{name}"', f'{name}?v={version}"')
        page.write_text(content, encoding="utf-8")
    return OUTPUT


if __name__ == "__main__":
    print(f"Built Slate website: {build()}")
