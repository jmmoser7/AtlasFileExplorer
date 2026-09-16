// Read-only reference inventory: node reference-inventory.cjs MODEL GUARD RHINO3DM_PACKAGE
const fs = require('node:fs');
const { createHash } = require('node:crypto');
const { execFileSync } = require('node:child_process');
const [path, guard, packagePath] = process.argv.slice(2);

async function main() {
  const rhino = await require(packagePath)();
  // Reuse the app's cloud guard before this targeted source byte read.
  execFileSync(guard, [path], { stdio: 'pipe' });
  const start = performance.now();
  const bytes = fs.readFileSync(path);
  const doc = rhino.File3dm.fromByteArray(new Uint8Array(bytes));
  if (!doc) throw new Error('Reference reader could not open the model');
  const types = {}, cached = {}, visibility = {};
  const render = {vertices: 0, triangles: 0, bounds_min: [Infinity, Infinity, Infinity], bounds_max: [-Infinity, -Infinity, -Infinity]};
  function recordRender(mesh) {
    const vertices = mesh.vertices(), faces = mesh.faces();
    render.vertices += vertices.count;
    for (let i = 0; i < faces.count; i++) {
      const f = faces.get(i);
      render.triangles += f[2] === f[3] ? 1 : 2;
    }
    for (let i = 0; i < vertices.count; i++) {
      const v = vertices.get(i);
      for (let k = 0; k < 3; k++) {
        render.bounds_min[k] = Math.min(render.bounds_min[k], v[k]);
        render.bounds_max[k] = Math.max(render.bounds_max[k], v[k]);
      }
    }
  }
  let faceCount = 0;
  const add = (counts, key) => { counts[key] = (counts[key] || 0) + 1; };
  const objects = doc.objects();
  for (let i = 0; i < objects.count; i++) {
    const object = objects.get(i);
    const geometry = object.geometry();
    add(visibility, String(object.attributes().visible));
    const name = ['Brep', 'Mesh', 'Extrusion', 'SubD', 'InstanceReference', 'PolylineCurve', 'NurbsCurve', 'LineCurve', 'Point']
      .find(kind => rhino[kind] && geometry instanceof rhino[kind]) || geometry.constructor.name;
    add(types, name);
    if (name === 'Mesh') { add(cached, 'standalone_meshes'); recordRender(geometry); }
    if (name === 'Brep') {
      const faces = geometry.faces();
      faceCount += faces.count;
      for (let f = 0; f < faces.count; f++) {
        for (const [label, meshType] of [['brep_render_meshes', rhino.MeshType.Render], ['brep_analysis_meshes', rhino.MeshType.Analysis]]) {
          const mesh = faces.get(f).getMesh(meshType);
          if (mesh && mesh.faces().count > 0) {
            add(cached, label);
            if (label === 'brep_render_meshes') recordRender(mesh);
          }
        }
      }
    }
    if (name === 'Extrusion') {
      const mesh = geometry.getMesh(rhino.MeshType.Render);
      if (mesh && mesh.faces().count > 0) { add(cached, 'extrusion_render_meshes'); recordRender(mesh); }
    }
  }
  const bounds = objects.getBoundingBox();
  const unitValue = doc.settings().modelUnitSystem.value;
  console.log(JSON.stringify({ source: path, bytes: bytes.length,
    sha256: createHash('sha256').update(bytes).digest('hex'),
    reference_library: 'rhino3dm ' + require(packagePath + '/package.json').version + ' (WASM)',
    archive_version: doc.archiveVersion, objects: objects.count, geometry_types: types,
    brep_faces: faceCount, cached_mesh_counts: cached, render_mesh_geometry: render, layer_count: doc.layers().count,
    instance_definition_count: doc.instanceDefinitions().count,
    object_visibility: visibility,
    hidden_layers: Array.from({length: doc.layers().count}, (_, i) => doc.layers().get(i)).filter(layer => !layer.visible).length,
    bounds_min: bounds.min, bounds_max: bounds.max,
    units: Object.keys(rhino.UnitSystem).find(key => rhino.UnitSystem[key]?.value === unitValue) || unitValue,
    read_and_inventory_ms: +(performance.now() - start).toFixed(3)
  }, null, 2));
}
main().catch(error => { console.error(error.message); process.exitCode = 1; });
