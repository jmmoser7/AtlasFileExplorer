# Client feedback bundles

Both Slate and File Atlas can write **Suggestion box** reports here (dev checkouts
default to `feedback/client/`). Each submit creates a folder:

```
feedback/client/2026-10-09_153045-bug-short-slug/
  report.json    — metadata, description, links, recorded steps
  steps.txt      — human-readable step timeline
  00_*.png       — image attachments (captures, drops)
```

`report.json` fields:

| Field | Meaning |
|-------|---------|
| `app` | `slate` or `file-atlas` |
| `version` | Installed app version |
| `git_hash` | Short hash when running from a git checkout |
| `os` | Host OS id |
| `kind` | `bug` or `feature` |
| `description` | User text |
| `links` | URLs or references, one per line in the UI |
| `steps` | `{ at_ms, label }` entries from optional Reproduce recording |
| `reproduce` | Whether the user asked for step capture |
| `created_at` | RFC 3339 timestamp |
| `bundle_dir` | Absolute path to this folder (filled on write) |

Installed builds without a repo root use
`%LOCALAPPDATA%\NativeFileAtlas\feedback\` unless overridden in
**Advanced → Suggestion box**.

No network or account is required. Use **Email…** to open a `mailto:` summary;
attach files from the bundle folder manually.
