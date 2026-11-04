/* The download links and their version label follow the published stable release. */
(async () => {
  const labels = document.querySelectorAll('[data-stable-build]');
  if (!labels.length) return;
  try {
    const response = await fetch('https://api.github.com/repos/jmmoser7/AtlasFileExplorer/releases/latest', {
      headers: { Accept: 'application/vnd.github+json' },
      credentials: 'omit',
      cache: 'no-cache'
    });
    if (!response.ok) return;
    const release = await response.json();
    const version = /^v?(\d+\.\d+\.\d+)$/.exec(release.tag_name || '');
    const installer = release.assets?.some(asset =>
      asset.name === 'AtlasSuite-stable-Setup.exe' && asset.size > 0 && asset.state === 'uploaded');
    if (!version || release.draft || release.prerelease || !release.published_at || !installer) return;
    for (const label of labels) label.textContent = `Build ${version[1]}`;
  } catch {
    // The existing stable download remains available when metadata cannot load.
  }
})();
