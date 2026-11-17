"""Validate release metadata and render static notes on the download page."""
from datetime import date
from html import escape
import json
import re
from urllib.parse import urlsplit


RELEASES = "https://github.com/jmmoser7/AtlasFileExplorer/releases"
CATEGORIES = (
    ("patches", "Patches"),
    ("regression_fixes", "Regression fixes"),
    ("feature_additions", "Feature additions"),
)


def release_notes(source):
    """Return the newest stable version available for rollout and the release history markup."""
    entries = json.loads(source.read_text(encoding="utf-8"))
    if not isinstance(entries, list) or not entries:
        raise ValueError("release-history.json must contain a nonempty list")
    latest_stable = None
    previous_stable = None
    previous_date = None
    seen_tags = set()
    html = []
    for entry in entries:
        expected = {"tag", "date", "download", *(key for key, _ in CATEGORIES)}
        if not isinstance(entry, dict) or set(entry) not in (
            expected, expected | {"base_version"}, expected | {"hold_notice"}
        ):
            raise ValueError("Each release needs a tag, date, download and all three note lists")
        tag = entry["tag"]
        if not isinstance(tag, str) or tag in seen_tags:
            raise ValueError(f"Expected a unique release tag, got {tag!r}")
        seen_tags.add(tag)
        published = date.fromisoformat(entry["date"])
        if previous_date is not None and previous_date < published:
            raise ValueError("Release history must be ordered newest first")
        previous_date = published
        base_version = entry.get("base_version")
        held = "hold_notice" in entry
        hold_status = ""
        hold_notice = ""
        if held:
            notice = entry["hold_notice"]
            if not isinstance(notice, str) or not notice.strip():
                raise ValueError(f"{tag} needs a nonempty hold notice")
            hold_status = '<strong class="release-hold-status">On hold</strong>'
            hold_notice = f'<p class="release-package">{escape(notice)}</p>\n'
        if "base_version" in entry:
            if not re.fullmatch(r"slate-feedback-\d{8}", tag) or not isinstance(base_version, str) or not re.fullmatch(r"\d+\.\d+\.\d+", base_version):
                raise ValueError(f"{tag} needs a dated QA tag and a numeric base version")
            heading = "Slate Windows QA build"
            download_label = "Download Windows x64 QA ZIP"
            extension = "zip"
            package = (
                f'<p class="release-package">Development build based on Slate {base_version}. '
                'Extract the portable ZIP, then open <code>slate.exe</code>.</p>\n'
            )
        else:
            if not re.fullmatch(r"v\d+\.\d+\.\d+", tag):
                raise ValueError(f"Expected a stable GitHub tag, got {tag!r}")
            version = tag[1:]
            stable = tuple(map(int, version.split(".")))
            if previous_stable is not None and previous_stable <= stable:
                raise ValueError("Stable releases must be ordered newest first")
            previous_stable = stable
            if latest_stable is None and not held:
                latest_stable = version
            heading = f"Slate {version}"
            download_label = f"Download {version}"
            extension = "exe"
            package = ""

        download = entry["download"]
        url = urlsplit(download)
        prefix = f"{RELEASES}/download/{tag}/"
        if (not download.startswith(prefix) or url.query or url.fragment
                or not re.fullmatch(rf"[A-Za-z0-9_.-]+\.{extension}", download[len(prefix):])):
            raise ValueError(f"{tag} needs a versioned .{extension} download URL pinned to its tag")

        groups = []
        for key, title in CATEGORIES:
            notes = entry[key]
            if not isinstance(notes, list) or any(not isinstance(note, str) or not note.strip() for note in notes):
                raise ValueError(f"{tag} {title} must be a list of nonempty text notes")
            if notes:
                details = "<ul>" + "".join(f"<li>{escape(note)}</li>" for note in notes) + "</ul>"
            else:
                details = '<p class="release-empty">None listed.</p>'
            groups.append(f"<div><h5>{title}</h5>{details}</div>")
        html.append(
            f'<article class="release-entry" id="release-{tag}">\n'
            '  <div class="release-entry-header">'
            f'<h4><a href="{RELEASES}/tag/{tag}">{heading}</a></h4>'
            '<span aria-hidden="true">·</span>'
            f'<time datetime="{published.isoformat()}">{published:%B} {published.day}, {published.year}</time>'
            f'{hold_status}'
            '</div>\n'
            f'  <a class="text-link" href="{escape(download, quote=True)}">{download_label} <span aria-hidden="true">↓</span></a>\n'
            f'  {package}'
            f'  {hold_notice}'
            '  <div class="release-categories">' + "".join(groups) + '</div>\n'
            '</article>'
        )
    if latest_stable is None:
        raise ValueError("Release history needs at least one stable release available for rollout")
    return latest_stable, "\n".join(html)
