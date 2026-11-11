# Slate website

A small static GitHub Pages site for downloads, documentation and suggestions.
It has no npm dependencies, third-party fonts, or tracking.

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

Set the repository Actions variable `SLATE_SUGGESTION_ENDPOINT` to the public
HTTPS `/suggestions` endpoint of the private intake relay. For a local site
build, the same environment variable supplies the endpoint. Allow the website
origin `https://jmmoser7.github.io` in the relay's CORS configuration. The URL
is public. GitHub credentials belong in the relay's secret store.

The form submits directly to the relay, which creates a private issue without
requiring the visitor to sign in. An unconfigured build shows the form with
sending disabled. Local report files saved in Slate are shared separately.

