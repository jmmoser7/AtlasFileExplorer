#!/usr/bin/env python3
"""Generate the .3dm test fixtures for the rhino-mesh crate.

Run from this directory with the official Rhino library installed:

    pip install --user --break-system-packages rhino3dm
    python3 generate.py

The script prints the deterministic vertex/face counts that the Rust
integration tests in ../read_fixtures.rs hard-code. Re-run it whenever the
fixtures need to be regenerated; the geometry is fully deterministic.

Fixtures (all written with the default archive version, Rhino 8 / "80"):

  mesh_sphere.3dm          two ON_Mesh objects: a UV sphere (radius 2,
                           centered at origin, with a stored normals array)
                           and a small quad patch whose object attributes
                           carry an explicit display color (color source =
                           color-from-object).
  brep_with_render_mesh.3dm one box ON_Brep whose 6 faces each carry a
                           cached render mesh (face.SetMesh(Render)) — the
                           same data layout Rhino produces when saving a
                           shaded viewport without "Save Small".
  extrusion.3dm            one ON_Extrusion (box extrusion) with a render
                           mesh in its mesh cache (extrusion.SetMesh).
  quads.3dm                one ON_Mesh made purely of quads (no normals
                           array) to exercise quad triangulation and
                           computed normals.
  empty.3dm                a single line curve: valid 3dm, zero meshes.
"""

import math
import os
import sys

import rhino3dm

HERE = os.path.dirname(os.path.abspath(__file__))


def uv_sphere(radius=2.0, u_count=8, v_count=6):
    """UV sphere mesh: quads in the body, triangles at the poles.

    Vertices: 2 poles + (v_count - 1) rings of u_count.
    Faces: u_count triangles per pole cap + u_count * (v_count - 2) quads.
    """
    m = rhino3dm.Mesh()
    m.Vertices.Add(0.0, 0.0, radius)          # index 0: north pole
    for vi in range(1, v_count):
        phi = math.pi * vi / v_count
        for ui in range(u_count):
            theta = 2.0 * math.pi * ui / u_count
            m.Vertices.Add(
                radius * math.sin(phi) * math.cos(theta),
                radius * math.sin(phi) * math.sin(theta),
                radius * math.cos(phi),
            )
    south = 1 + (v_count - 1) * u_count
    m.Vertices.Add(0.0, 0.0, -radius)         # south pole

    def ring(vi, ui):
        return 1 + (vi - 1) * u_count + (ui % u_count)

    for ui in range(u_count):
        m.Faces.AddFace(0, ring(1, ui), ring(1, ui + 1))
    for vi in range(1, v_count - 1):
        for ui in range(u_count):
            m.Faces.AddFace(ring(vi, ui), ring(vi + 1, ui),
                            ring(vi + 1, ui + 1), ring(vi, ui + 1))
    for ui in range(u_count):
        m.Faces.AddFace(south, ring(v_count - 1, ui + 1), ring(v_count - 1, ui))
    return m


def quad_patch(nx=2, ny=2, z=5.0):
    """(nx x ny) grid of quads, (nx+1)*(ny+1) vertices, offset in +z."""
    m = rhino3dm.Mesh()
    for j in range(ny + 1):
        for i in range(nx + 1):
            m.Vertices.Add(float(i), float(j), z)
    for j in range(ny):
        for i in range(nx):
            a = j * (nx + 1) + i
            m.Faces.AddFace(a, a + 1, a + nx + 2, a + nx + 1)
    return m


def report(name, meshes):
    for label, m in meshes:
        quads = sum(1 for i in range(len(m.Faces)) if len(set(m.Faces[i])) == 4)
        tris = len(m.Faces) - quads
        print(f"  {name} [{label}]: vertices={len(m.Vertices)} "
              f"faces={len(m.Faces)} (quads={quads} tris={tris}) "
              f"triangles-after-split={tris + 2 * quads}")


def write(f3dm, name):
    path = os.path.join(HERE, name)
    assert f3dm.Write(path, 0), name
    print(f"wrote {name}: {os.path.getsize(path)} bytes")


def main():
    # --- mesh_sphere.3dm ---------------------------------------------------
    f = rhino3dm.File3dm()
    sphere = uv_sphere()
    sphere.Normals.ComputeNormals()  # persist an m_N normals array
    f.Objects.AddMesh(sphere)

    colored = quad_patch()
    attr = rhino3dm.ObjectAttributes()
    attr.ObjectColor = (210, 40, 20, 255)  # (r, g, b, a)
    attr.ColorSource = rhino3dm.ObjectColorSource.ColorFromObject
    f.Objects.AddMesh(colored, attr)
    report("mesh_sphere", [("sphere", sphere), ("colored patch", colored)])
    write(f, "mesh_sphere.3dm")

    # --- brep_with_render_mesh.3dm -----------------------------------------
    f = rhino3dm.File3dm()
    box = rhino3dm.Box(rhino3dm.BoundingBox(rhino3dm.Point3d(0, 0, 0),
                                            rhino3dm.Point3d(1, 2, 3)))
    brep = rhino3dm.Brep.CreateFromBox(box)
    face_meshes = []
    for i in range(len(brep.Faces)):
        # A 2x1 patch per face (2 quads -> 4 triangles). The geometry does
        # not have to match the surface for the reader test; only the layout
        # in the file matters.
        fm = quad_patch(nx=2, ny=1, z=float(i))
        face_meshes.append(fm)
        assert brep.Faces[i].SetMesh(fm, rhino3dm.MeshType.Render)
    f.Objects.AddBrep(brep)
    report("brep_with_render_mesh", [(f"face {i}", m)
                                     for i, m in enumerate(face_meshes)])
    write(f, "brep_with_render_mesh.3dm")

    # --- extrusion.3dm ------------------------------------------------------
    f = rhino3dm.File3dm()
    ext_box = rhino3dm.Box(rhino3dm.BoundingBox(rhino3dm.Point3d(0, 0, 0),
                                                rhino3dm.Point3d(1, 1, 4)))
    ext = rhino3dm.Extrusion.CreateBoxExtrusion(ext_box, True)
    ext_mesh = uv_sphere(radius=1.0, u_count=6, v_count=4)
    assert ext.SetMesh(ext_mesh, rhino3dm.MeshType.Render)
    f.Objects.AddExtrusion(ext)
    report("extrusion", [("render cache", ext_mesh)])
    write(f, "extrusion.3dm")

    # --- quads.3dm -----------------------------------------------------------
    f = rhino3dm.File3dm()
    quads = quad_patch(nx=3, ny=2, z=0.0)  # 12 verts, 6 quads, no normals
    f.Objects.AddMesh(quads)
    report("quads", [("patch", quads)])
    write(f, "quads.3dm")

    # --- empty.3dm ------------------------------------------------------------
    f = rhino3dm.File3dm()
    f.Objects.AddLine(rhino3dm.Point3d(0, 0, 0), rhino3dm.Point3d(1, 1, 1))
    write(f, "empty.3dm")

    sys.stdout.flush()
    # rhino3dm 8.17 crashes in C++ destructors once objects touched by
    # BrepFace.SetMesh/Extrusion.SetMesh are garbage collected. Every file
    # is already on disk here, so skip the destructors entirely.
    os._exit(0)


if __name__ == "__main__":
    main()
