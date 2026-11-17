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
node website/check-release.cjs
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

The primary installer and portable ZIP follow the latest stable GitHub
Release. The preview installer uses the rolling `preview` tag. Both
installers include Slate and File Atlas.

### Update the stable release notes

1. Publish the versioned stable GitHub Release through the Windows release
   workflow in [distribution.md](../docs/distribution.md). Verify the
   installer downloads successfully from that release.
2. Add an entry to `website/release-history.json` in descending publication
   order. Include the stable tag, publication date, and the actual installer
   URL under `/releases/download/<tag>/`. Keep each older installer pinned
   to its own tag.
3. Summarize verified changes in the `patches`, `regression_fixes`, and
   `feature_additions` arrays. Use an empty list when a category has no notes.
4. Run the preview checks above. Verify the version label, its Release notes
   link, and the download beneath the new release heading. Merge to `main`,
   wait for Pages, then open the live page and download the package through
   its versioned link.

If a rollout is on hold and GitHub Latest points to a previous stable release,
add a `hold_notice` to the held entry. The entry keeps its pinned download and
displays the notice. The static build label skips held entries and the live
label follows GitHub Latest.

### Publish notes for a Windows QA build

1. Publish the tested portable ZIP on a separate dated prerelease, such as
   `slate-feedback-20261116`. Keep the stable installer and update feed on
   their existing stable release. Check the published ZIP against the tested
   package checksum.
2. Add the QA entry in publication order in `website/release-history.json`.
   Use the dated tag, asset URL under `/releases/download/<tag>/`,
   installed app version as `base_version`, and the same three note arrays.
   The page uses the newest stable entry for its build label.
3. Follow the same local and live Pages checks as the stable release. Verify
   the direct portable ZIP from the QA heading.

Set the repository Actions variable `SLATE_SUGGESTION_ENDPOINT` to the public
HTTPS `/suggestions` endpoint of the private intake relay. For a local site
build, the same environment variable supplies the endpoint. Allow the website
origin `https://jmmoser7.github.io` in the relay's CORS configuration. The URL
is public. GitHub credentials belong in the relay's secret store.

The form submits directly to the relay, which creates a private issue without
requiring the visitor to sign in. An unconfigured build shows the form with
sending disabled. Local report files saved in Slate are shared separately.

