import fs from 'node:fs/promises';

// One watcher per link folder: the newest writes its pid to sidecar.pid, and an
// older one that finds another pid there stops. Slate names itself in
// ATLAS_PARENT_PID so a watcher does not outlive the window that started it.

export async function claimLink(pidPath, pid = process.pid) {
  await fs.writeFile(pidPath, String(pid));
}

// Remove the record only while it is still ours; a newer watcher owns it otherwise.
export async function releaseLink(pidPath, pid = process.pid) {
  const text = await fs.readFile(pidPath, 'utf8').catch(() => null);
  if (text !== null && text.trim() === String(pid)) await fs.rm(pidPath, { force: true }).catch(() => {});
}

export function processAlive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (err) {
    return err?.code !== 'ESRCH';
  }
}

// Why this watcher should stop, or null. A missing record means Slate stopped it.
export async function supervisionLost(pidPath, pid = process.pid, parent = process.env.ATLAS_PARENT_PID, alive = processAlive) {
  const text = await fs.readFile(pidPath, 'utf8').catch((err) => (err?.code === 'ENOENT' ? '' : null));
  if (text !== null && text.trim() !== String(pid)) return 'another sidecar owns this link folder';
  const parentPid = Number.parseInt(parent ?? '', 10);
  if (Number.isInteger(parentPid) && parentPid > 0 && !alive(parentPid)) return 'Slate exited';
  return null;
}
