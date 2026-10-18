import assert from 'node:assert/strict';
import test from 'node:test';
import { modelSelection, projectModels } from './models.mjs';

const catalog = [{ id: 'actual-model', displayName: 'Actual Model', isDefault: true, parameters: [
  { id: 'reasoning_effort', displayName: 'Thought level', values: [{ value: 'brief', displayName: 'Brief' }, { value: 'deep', displayName: 'Deep' }] },
  { id: 'speed', displayName: 'Speed', values: [{ value: 'standard' }, { value: 'quick', displayName: 'Quick' }] },
  { id: 'max', displayName: 'MAX reasoning', values: [{ value: 'on' }] },
  { id: 'context', displayName: 'Full reasoning context', values: [{ value: 'large' }] },
], variants: [{ isDefault: true, params: [{ id: 'reasoning_effort', value: 'brief' }, { id: 'speed', value: 'standard' }] }] }];

test('model projection exposes actual reasoning and speed choices and declared defaults only', () => {
  assert.deepEqual(projectModels(catalog), [{ id: 'actual-model', name: 'Actual Model', is_default: true, parameters: [
    { id: 'reasoning_effort', name: 'Thought level', values: [{ value: 'brief', name: 'Brief' }, { value: 'deep', name: 'Deep' }], default: 'brief' },
    { id: 'speed', name: 'Speed', values: [{ value: 'standard', name: 'standard' }, { value: 'quick', name: 'Quick' }], default: 'standard' },
  ] }]);
});

test('missing or ambiguous provider defaults never fabricate defaults', () => {
  assert.deepEqual(projectModels([{ id: 'plain', displayName: 'Plain' }]), [{ id: 'plain', name: 'Plain', is_default: false, parameters: [] }]);
  const ambiguous = { ...catalog[0], isDefault: undefined, variants: [...catalog[0].variants, ...catalog[0].variants] };
  const model = projectModels([ambiguous])[0];
  assert.equal(model.is_default, false);
  assert.ok(model.parameters.every((parameter) => !Object.hasOwn(parameter, 'default')));
  const invalid = { ...catalog[0], variants: [{ isDefault: true, params: [{ id: 'speed', value: 'invented' }] }] };
  assert.ok(projectModels([invalid])[0].parameters.every((parameter) => !Object.hasOwn(parameter, 'default')));
});

test('empty selections preserve legacy SDK shape without adding guessed defaults', () => {
  assert.deepEqual(modelSelection('auto', undefined), { id: 'auto' });
  assert.deepEqual(modelSelection('auto', {}), { id: 'auto' });
});

test('actual parameter selections use exact SDK shape and are checked against current catalog', () => {
  assert.deepEqual(modelSelection('actual-model', { reasoning_effort: 'deep', speed: 'quick' }, catalog), {
    id: 'actual-model', params: [{ id: 'reasoning_effort', value: 'deep' }, { id: 'speed', value: 'quick' }],
  });
  assert.throws(() => modelSelection('actual-model', { speed: 'quick' }, []), /no longer available/);
  assert.throws(() => modelSelection('actual-model', { speed: 'fast' }, catalog), /does not support speed=fast/);
  assert.throws(() => modelSelection('actual-model', { reasoning: 'deep' }, catalog), /does not support reasoning=deep/);
  assert.throws(() => modelSelection('actual-model', { max: 'on' }, catalog), /does not support max=on/);
  assert.throws(() => modelSelection('actual-model', { context: 'large' }, catalog), /does not support context=large/);
  for (const invalid of [null, [], 'deep', { speed: 1 }]) {
    assert.throws(() => modelSelection('actual-model', invalid, catalog));
  }
});
