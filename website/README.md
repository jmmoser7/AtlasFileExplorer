# Slate website

A small static GitHub Pages site for downloads, documentation and suggestions.
No npm dependencies, third-party fonts, tracking or server-side services.

## Preview and check

From the repository root:

```powershell
python website/build.py
python website/check.py
node --check website/suggestions/suggestions.js
node website/check-suggestions.cjs
python -m http.server 4173 --bind 127.0.0.1 --directory target/slate-site
```

Open http://127.0.0.1:4173/. The public bundle contains only explicitly listed
files, the native application icon, and theme CSS generated from
`crates/atlas-shell/ui-tokens.toml`. Edit colors there, not in a second palette.

## Publish

In repository **Settings → Pages → Build and deployment**, set **Source** to
**GitHub Actions**. Merge the website and `.github/workflows/pages.yml` into
`main`; the workflow checks and deploys the site. It can also be run manually
from Actions → Slate website.

Expected address: https://jmmoser7.github.io/AtlasFileExplorer/

Download URLs follow the latest stable release, so no version needs to be
changed when publishing a new Windows build. Preview uses the existing
`preview` release tag. Both installers include Slate and File Atlas.

Suggestions open a prefilled GitHub issue for the visitor to review and submit
with their own account. The form does not automatically publish or send anything.
The no-JavaScript form also works. Existing in-app reports remain local bundles.

