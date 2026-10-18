"""Generate materials_v5/v8.3dm with official rhino3dm 8.35.0.

Two triangles share one layer: first inherits its material, second overrides
with an object material. A third uses a colored, unassigned layer. UVs are
stored to verify the compressed m_T array. Existing geometry fixtures remain
untouched. Run with rhino3dm importable: python generate_materials.py.
"""
from pathlib import Path
import rhino3dm as r


def triangle(x):
    mesh = r.Mesh()
    for p in [(x, 0, 0), (x+1, 0, 0), (x, 1, 0)]:
        mesh.Vertices.Add(*p)
    mesh.Faces.AddFace(0, 1, 2)
    mapping = r.TextureMapping.CreatePlaneMapping(
        r.Plane.WorldXY(), r.Interval(0, 1), r.Interval(0, 1), r.Interval(0, 1))
    mesh.SetTextureCoordinates(mapping, r.Transform.Identity(), False)
    return mesh


doc = r.File3dm()
red = r.Material()
red.Name = 'Red glass fallback'
red.DiffuseColor = (204, 51, 26, 255)
red.Transparency = 0.4
red.Shine = 64.0
red_index = doc.Materials.Add(red)
green = r.Material()
green.Name = 'Green object override'
green.DiffuseColor = (26, 179, 77, 255)
green_index = doc.Materials.Add(green)
layer = r.Layer()
layer.Name = 'Material layer'
layer.Color = (70, 80, 90, 255)
layer.RenderMaterialIndex = red_index
layer_index = doc.Layers.Add(layer)
blue = r.Layer()
blue.Name = 'Blue display layer'
blue.Color = (30, 60, 210, 255)
blue_index = doc.Layers.Add(blue)
attrs = r.ObjectAttributes()
attrs.LayerIndex = layer_index
doc.Objects.AddMesh(triangle(0), attrs)
attrs.MaterialSource = r.ObjectMaterialSource.MaterialFromObject
attrs.MaterialIndex = green_index
doc.Objects.AddMesh(triangle(2), attrs)
plain = r.ObjectAttributes()
plain.LayerIndex = blue_index
doc.Objects.AddMesh(triangle(4), plain)
for version in [5, 8]:
    out = Path(__file__).parent / f'materials_v{version}.3dm'
    assert doc.Write(str(out), version)
    print(out.name, out.stat().st_size)
