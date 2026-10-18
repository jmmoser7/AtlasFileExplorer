// The provider catalog owns parameter identities, choices, and defaults.
function name(value, fallback) {
  return typeof value === 'string' && value.trim() ? value : fallback;
}

function generatorParameter(parameter) {
  const words = `${parameter.id ?? ''} ${parameter.displayName ?? ''}`
    .replace(/([a-z])([A-Z])/g, '$1 $2').replace(/[_-]/g, ' ');
  return !/\b(max|context)\b/i.test(words)
    && /\b(reasoning|thought|thinking|effort|speed|fast)\b/i.test(words);
}

function definitions(model) {
  const seen = new Set();
  return (Array.isArray(model.parameters) ? model.parameters : []).flatMap((parameter) => {
    if (!parameter || typeof parameter.id !== 'string' || !parameter.id || seen.has(parameter.id)) return [];
    seen.add(parameter.id);
    const values = (Array.isArray(parameter.values) ? parameter.values : [])
      .filter((choice) => choice && typeof choice.value === 'string')
      .map((choice) => ({ value: choice.value, name: name(choice.displayName, choice.value) }));
    return values.length ? [{ id: parameter.id, name: name(parameter.displayName, parameter.id), values }] : [];
  });
}

function generatorDefinitions(model) {
  return definitions(model).filter((definition) => generatorParameter({
    id: definition.id, displayName: definition.name,
  }));
}

export function projectModels(catalog) {
  return catalog.filter((model) => model && typeof model.id === 'string' && model.id).map((model) => {
    const defaults = (model.variants ?? []).filter((variant) => variant?.isDefault === true);
    const parameters = generatorDefinitions(model).map((definition) => {
      const selected = defaults.length === 1
        ? defaults[0].params?.find((parameter) => parameter.id === definition.id)?.value : undefined;
      return { ...definition, ...(definition.values.some((choice) => choice.value === selected) ? { default: selected } : {}) };
    });
    return { id: model.id, name: name(model.displayName, model.id), is_default: model.isDefault === true, parameters };
  });
}

export function modelSelection(modelId, parameters, catalog = []) {
  if (parameters === undefined) return { id: modelId };
  if (!parameters || typeof parameters !== 'object' || Array.isArray(parameters)) {
    throw new Error('Model parameters must be a map of parameter IDs to catalog values.');
  }
  const selected = Object.entries(parameters);
  if (!selected.length) return { id: modelId };
  const model = catalog.find((candidate) => candidate.id === modelId);
  if (!model) throw new Error(`Model ${modelId} is no longer available; select a current model.`);
  const available = generatorDefinitions(model);
  const params = selected.map(([id, value]) => {
    const definition = available.find((candidate) => candidate.id === id);
    if (typeof value !== 'string' || !definition?.values.some((choice) => choice.value === value)) {
      throw new Error(`Model ${modelId} does not support ${id}=${String(value)}; refresh its model options.`);
    }
    return { id, value };
  });
  return { id: modelId, params };
}
