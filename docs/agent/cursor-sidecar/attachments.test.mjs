import test from 'node:test';
import assert from 'node:assert/strict';
import { MAX_IMAGE_BYTES, userMessage } from './attachments.mjs';

const png = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 1, 2, 3]);

test('a wired picture reaches agent.send as an inline image, not a label', async () => {
  const read = async (file) => {
    assert.equal(file, 'C:/photos/hall.png');
    return png;
  };
  const wired = [{ node: 3, text: '', images: ['C:/photos/hall.png'], slot: 'media' }];
  const message = await userMessage('Describe the style', wired, read);
  assert.deepEqual(message, {
    text: 'Describe the style',
    images: [{ data: png.toString('base64'), mimeType: 'image/png' }],
  });
});

test('pictures wired to a chat card without ports are attached in wire order', async () => {
  const files = { 'a.jpg': Buffer.from('jpeg'), 'b.webp': Buffer.from('webp') };
  const wired = [
    { node: 1, text: '', images: ['a.jpg'] },
    { node: 2, text: 'soft light', images: [] },
    { node: 3, text: '', images: ['b.webp'] },
  ];
  const message = await userMessage('Compare', wired, async (f) => files[f]);
  assert.deepEqual(
    message.images.map((i) => i.mimeType),
    ['image/jpeg', 'image/webp'],
  );
});

test('text-only inputs keep the plain prompt', async () => {
  const wired = [{ node: 2, text: 'soft light', images: [] }];
  assert.equal(await userMessage('Use the note', wired, async () => png), 'Use the note');
  assert.equal(await userMessage('No wires'), 'No wires');
});

test('an unreadable, oversized or non-picture file stays a path in the wired data', async () => {
  const wired = [{ node: 4, text: '', images: ['gone.png', 'huge.png', 'model.obj'] }];
  const read = async (file) => {
    if (file === 'huge.png') return Buffer.alloc(MAX_IMAGE_BYTES + 1);
    throw new Error('ENOENT');
  };
  assert.equal(await userMessage('Describe', wired, read), 'Describe');
});
