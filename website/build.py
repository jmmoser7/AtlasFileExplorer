"""Build the public site using Slate's actual theme and icon. No dependencies."""
from pathlib import Path
import shutil
import tomllib

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "website"
OUTPUT = ROOT / "target" / "slate-site"
PUBLIC_FILES = (
    "index.html", "styles.css", "documentation/index.html",
    "suggestions/index.html", "suggestions/suggestions.js",
)


def build():
    OUTPUT.mkdir(parents=True, exist_ok=True)
    for relative in PUBLIC_FILES:
        destination = OUTPUT / relative
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(SOURCE / relative, destination)
    assets = OUTPUT / "assets"
    assets.mkdir(exist_ok=True)
    shutil.copyfile(ROOT / "apps/slate/assets/slate.ico", assets / "slate.ico")
    tokens = tomllib.loads((ROOT / "crates/atlas-shell/ui-tokens.toml").read_text(encoding="utf-8"))
    dark = tokens["theme"]["dark"]
    names = ("bg", "grid_dot", "card", "border", "border_strong", "ink", "sub",
             "accent", "select", "select_fill", "panel")
    declarations = [f"  --{key.replace('_', '-')}: {dark[key]};" for key in names]
    declarations.append("  --muted: color-mix(in srgb, var(--ink) 65%, var(--sub));")
    (assets / "theme.css").write_text(
        "/* Generated from atlas-shell/ui-tokens.toml. Do not edit. */\n:root {\n"
        + "\n".join(declarations) + "\n}\n", encoding="utf-8")
    (OUTPUT / ".nojekyll").write_text("", encoding="utf-8")
    return OUTPUT


if __name__ == "__main__":
    print(f"Built Slate website: {build()}")
