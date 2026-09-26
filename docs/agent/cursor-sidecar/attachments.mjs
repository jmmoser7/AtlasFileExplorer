import fs from 'node:fs/promises';
import path from 'node:path';

// The SDK sends pictures inline. A larger file stays a path in the wired data.
export const MAX_IMAGE_BYTES = 15 * 1024 * 1024;

const MIME = {
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.webp': 'image/webp',
  '.gif': 'image/gif',
};

// What agent.send receives: the prompt, plus every wired picture as an inline
// image in wire order. Wired words already travel in the prompt's wired data.
export async function userMessage(text, wired = [], read = fs.readFile) {
  const images = [];
  for (const item of wired ?? []) {
    for (const file of item?.images ?? []) {
      const mimeType = MIME[path.extname(String(file)).toLowerCase()];
      if (!mimeType) continue;
      const bytes = await read(file).catch(() => null);
      if (!bytes || bytes.length > MAX_IMAGE_BYTES) continue;
      images.push({ data: Buffer.from(bytes).toString('base64'), mimeType });
    }
  }
  return images.length ? { text, images } : text;
}
