// Internal SDK bridge only. Slate's existing connector remains the wire protocol.
using System;
using System.IO;
using System.Linq;
using System.Collections.Generic;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Web.Script.Serialization;
using Rhino.Geometry;

class HeadlessRhino {
    static string system, job;
    static int meshCount, vertices, faces;
    static readonly JavaScriptSerializer Json = new JavaScriptSerializer { MaxJsonLength = 262144 };
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)] static extern bool SetDllDirectory(string path);
    [STAThread] static int Main(string[] args) {
        if(args.Length != 2) return 2;
        system=Path.GetFullPath(args[0]); job=Path.GetFullPath(args[1]);
        SetDllDirectory(system);
        Environment.SetEnvironmentVariable("PATH",system+";"+Environment.GetEnvironmentVariable("PATH"));
        AppDomain.CurrentDomain.AssemblyResolve+=(s,e)=> { var p=Path.Combine(system,new AssemblyName(e.Name).Name+".dll"); return File.Exists(p)?Assembly.LoadFrom(p):null; };
        try { return Run(); } catch(OperationCanceledException) { Progress("cancelled"); return 3; }
        catch(Exception e) { File.WriteAllText(Path.Combine(job,"error.txt"),e.ToString()); return 1; }
    }
    static void CheckStop() { if(File.Exists(Path.Combine(job,"stop"))) throw new OperationCanceledException(); }
    static void Progress(string value) { File.AppendAllText(Path.Combine(job,"progress.txt"),value+Environment.NewLine); }
    static double Number(Dictionary<string,object> o,string k) { var v=Convert.ToDouble(o[k]); if(Double.IsNaN(v)||Double.IsInfinity(v)||Math.Abs(v)>1000000) throw new Exception("Invalid numeric "+k); return v; }
    static double Positive(Dictionary<string,object> o,string k) { var v=Number(o,k); if(v<0.000001) throw new Exception("Degenerate "+k); return v; }
    static double[] Numbers(Dictionary<string,object> o,string k) { var a=((System.Collections.IEnumerable)o[k]).Cast<object>().Select(Convert.ToDouble).ToArray(); if(a.Length!=3||a.Any(v=>Double.IsNaN(v)||Double.IsInfinity(v)||Math.Abs(v)>1000000)) throw new Exception("Invalid vector "+k); return a; }
    static Point3d Point(Dictionary<string,object> o,string k) { var a=Numbers(o,k); return new Point3d(a[0],a[1],a[2]); }
    static Vector3d Vector(Dictionary<string,object> o,string k) { var a=Numbers(o,k); return new Vector3d(a[0],a[1],a[2]); }
    static string Text(Dictionary<string,object> o,string k) { return (string)o[k]; }
    static Mesh IndexedMesh(Dictionary<string,object> op) {
        var positions=((System.Collections.IEnumerable)op["vertices"]).Cast<System.Collections.IEnumerable>().Select(a=>a.Cast<object>().Select(Convert.ToDouble).ToArray()).ToArray();
        var triangles=((System.Collections.IEnumerable)op["triangles"]).Cast<System.Collections.IEnumerable>().Select(a=>a.Cast<object>().Select(v=>Convert.ToDouble(v)).ToArray()).ToArray();
        if(positions.Length<3||positions.Length>4096||triangles.Length<1||triangles.Length>8192)throw new Exception("Mesh vertex/triangle budget exceeded");
        var mesh=new Mesh(); mesh.Vertices.UseDoublePrecisionVertices=true;
        foreach(var p in positions) {
            if(p.Length!=3||p.Any(v=>Double.IsNaN(v)||Double.IsInfinity(v)||Math.Abs(v)>1000000))throw new Exception("Invalid mesh vertex");
            mesh.Vertices.Add(p[0],p[1],p[2]);
        }
        foreach(var t in triangles) {
            if(t.Length!=3||t.Any(v=>Double.IsNaN(v)||Double.IsInfinity(v)||v!=Math.Floor(v)||v<0||v>=positions.Length))throw new Exception("Invalid mesh triangle index");
            int a=(int)t[0],b=(int)t[1],c=(int)t[2];
            var ab=mesh.Vertices.Point3dAt(b)-mesh.Vertices.Point3dAt(a);var ac=mesh.Vertices.Point3dAt(c)-mesh.Vertices.Point3dAt(a);
            if(Vector3d.CrossProduct(ab,ac).SquareLength<=0)throw new Exception("Degenerate mesh triangle");
            mesh.Faces.AddFace(a,b,c);
        }
        mesh.Normals.ComputeNormals(); mesh.Compact();
        if(!mesh.IsValid)throw new Exception("Native mesh validation failed");
        if(op.ContainsKey("closed") && op["closed"]!=null && (bool)op["closed"] && !mesh.IsSolid)throw new Exception("Mesh closed:true requires a closed oriented manifold");
        return mesh;
    }
    static Brep[] BooleanBreps(GeometryBase[] geometry) {
        return geometry.Select(g=> {
            var brep=g as Brep;var extrusion=g as Extrusion;
            if(brep==null && extrusion!=null)brep=extrusion.ToBrep();
            if(brep==null)throw new NotSupportedException("union/difference require Brep or extrusion operands; mesh Booleans are not supported");
            return brep;
        }).ToArray();
    }
    static double Fraction(Dictionary<string,object> op,string key) {
        var value=Number(op,key);if(value<0||value>1)throw new Exception(key+" must be 0..1");return value;
    }
    static Rhino.DocObjects.Material Appearance(Dictionary<string,object> op,string id) {
        var rgb=((System.Collections.IEnumerable)op["diffuse"]).Cast<object>().Select(Convert.ToDouble).ToArray();
        if(rgb.Length!=3||rgb.Any(v=>Double.IsNaN(v)||Double.IsInfinity(v)||v!=Math.Floor(v)||v<0||v>255))throw new Exception("Invalid diffuse RGB");
        return new Rhino.DocObjects.Material { Name="slate-"+id, DiffuseColor=System.Drawing.Color.FromArgb((int)rgb[0],(int)rgb[1],(int)rgb[2]), Shine=Fraction(op,"gloss")*Rhino.DocObjects.Material.MaxShine, Transparency=Fraction(op,"transparency") };
    }
    static void CountMesh(Mesh mesh) {
        vertices+=mesh.Vertices.Count;faces+=mesh.Faces.Count;meshCount++;
        if(vertices>1000000||faces>1000000)throw new Exception("Preview mesh budget exceeded");
    }
    static void CacheBrep(Brep brep) {
        for(int face=0;face<brep.Faces.Count;face++) {
            CheckStop();
            using(var part=brep.Faces[face].DuplicateFace(false)) {
                var meshes=Mesh.CreateFromBrep(part,MeshingParameters.FastRenderMesh);
                if(meshes==null||meshes.Length==0)throw new Exception("Meshing failed");
                var joined=new Mesh();foreach(var mesh in meshes){joined.Append(mesh);mesh.Dispose();}
                CountMesh(joined);
                if(!brep.Faces[face].SetMesh(MeshType.Render,joined))throw new Exception("Mesh cache failed");
            }
        }
    }
    [MethodImpl(MethodImplOptions.NoInlining)] static int Run() {
        var input=Path.Combine(job,"plan.json");
        if(new FileInfo(input).Length>262144) throw new Exception("Plan byte limit exceeded");
        var plan=Json.Deserialize<Dictionary<string,object>>(File.ReadAllText(input));
        var ops=((System.Collections.IEnumerable)plan["operations"]).Cast<Dictionary<string,object>>().ToArray();
        if(ops.Length>64) throw new Exception("Operation limit exceeded");
        Rhino.UnitSystem units;
        switch(Text(plan,"units")) { case "source": units=Rhino.UnitSystem.None;break;case "meters": units=Rhino.UnitSystem.Meters; break; case "millimeters": units=Rhino.UnitSystem.Millimeters; break; default: throw new Exception("Unsupported units"); }
        CheckStop(); Progress("starting Rhino (headless)");
        using(var core=new Rhino.Runtime.InProcess.RhinoCore())
        using(var doc=Rhino.RhinoDoc.CreateHeadless(null))
        using(var model=File.Exists(Path.Combine(job,"input.3dm"))?Rhino.FileIO.File3dm.Read(Path.Combine(job,"input.3dm")):new Rhino.FileIO.File3dm()) {
            if(model==null)throw new Exception("Bound source .3dm could not be read");
            bool imported=File.Exists(Path.Combine(job,"input.3dm"));
            // Resolve existing relative texture locators without opening/copying
            // texture files (including dehydrated cloud placeholders).
            var originFile=Path.Combine(job,"source-root.txt");
            if(imported && File.Exists(originFile)) {
                var origin=File.ReadAllText(originFile);
                foreach(var material in model.AllMaterials)foreach(var texture in material.GetTextures()) {
                    var locator=texture.FileName;Uri uri;
                    if(!String.IsNullOrWhiteSpace(locator) && !Path.IsPathRooted(locator) && !Uri.TryCreate(locator,UriKind.Absolute,out uri)) {
                        texture.FileReference=Rhino.FileIO.FileReference.CreateFromFullPath(Path.GetFullPath(Path.Combine(origin,locator)));
                        if(!material.SetTexture(texture,texture.TextureType))throw new Exception("Could not preserve relative material texture path");
                    }
                }
            }
            if(units==Rhino.UnitSystem.None) {if(!imported)throw new Exception("Source units require an existing model");units=model.Settings.ModelUnitSystem;}
            if(imported && model.Settings.ModelUnitSystem!=units)throw new Exception("Plan units differ from the source; use the source units (unit conversion is not offered)");
            var sourceObjects=model.Objects.Select(o=>new {object_id=o.Attributes.ObjectId.ToString(),name=o.Attributes.Name,geometry=o.Geometry.ObjectType.ToString(),layer=o.Attributes.LayerIndex,material=o.Attributes.MaterialIndex}).ToArray();
            var objects=new Dictionary<string,GeometryBase[]>();
            var attributesById=new Dictionary<string,Rhino.DocObjects.ObjectAttributes>();
            var references=new Dictionary<string,Guid>();
            var consumed=new HashSet<string>();
            var authoredMaterials=new Dictionary<int,Rhino.DocObjects.Material>();
            doc.ModelUnitSystem=units; if(!imported)model.Settings.ModelUnitSystem=units;
            foreach(var op in ops) {
                CheckStop(); var id=Text(op,"id"); var kind=Text(op,"kind");
                if(id.Length<1||id.Length>64||objects.ContainsKey(id)) throw new Exception("Invalid/duplicate operation ID");
                Progress("creating "+id+" ("+kind+")"); GeometryBase[] result;
                switch(kind) {
                    case "reference": {
                        Guid objectId; if(!Guid.TryParse(Text(op,"object_id"),out objectId))throw new Exception("Invalid source object ID");
                        var item=model.Objects.FirstOrDefault(o=>o.Attributes.ObjectId==objectId);
                        if(item==null)throw new Exception("Source object not found: "+objectId);
                        result=new[]{item.Geometry.Duplicate()}; references[id]=objectId;attributesById[id]=item.Attributes.Duplicate();break;
                    }
                    case "box": { var p=Point(op,"origin"); var s=Numbers(op,"size"); if(s.Any(v=>v<0.000001))throw new Exception("Degenerate box"); result=new[]{new Box(new Plane(p,Vector3d.ZAxis),new Interval(0,s[0]),new Interval(0,s[1]),new Interval(0,s[2])).ToBrep()}; break; }
                    case "sphere": result=new[]{new Sphere(Point(op,"origin"),Positive(op,"radius")).ToBrep()}; break;
                    case "cylinder": result=new[]{new Cylinder(new Circle(new Plane(Point(op,"origin"),Vector3d.ZAxis),Positive(op,"radius")),Positive(op,"height")).ToBrep(true,true)}; break;
                    case "mesh": result=new GeometryBase[]{IndexedMesh(op)};break;
                    case "appearance": {
                        var target=Text(op,"target");result=objects[target].Select(g=>g.Duplicate()).ToArray();consumed.Add(target);
                        var attributes=attributesById.ContainsKey(target)?attributesById[target].Duplicate():new Rhino.DocObjects.ObjectAttributes { Name=id };
                        using(var material=Appearance(op,id)) {
                            int index=model.AllMaterials.AddMaterial(material);if(index<0)throw new Exception("Material assignment failed");
                            attributes.MaterialIndex=index;attributes.MaterialSource=Rhino.DocObjects.ObjectMaterialSource.MaterialFromObject;
                            authoredMaterials[index]=new Rhino.DocObjects.Material(material);
                        }
                        attributesById[id]=attributes;break;
                    }
                    case "move": case "rotate": case "scale": {
                        var target=Text(op,"target"); result=objects[target].Select(b=>b.Duplicate()).ToArray(); consumed.Add(target);
                        if(attributesById.ContainsKey(target))attributesById[id]=attributesById[target].Duplicate();
                        Transform transform;
                        if(kind=="move") transform=Transform.Translation(Vector(op,"vector"));
                        else if(kind=="scale") transform=Transform.Scale(Point(op,"origin"),Positive(op,"factor"));
                        else { var axis=Vector(op,"axis"); if(!axis.Unitize())throw new Exception("Degenerate rotation axis"); transform=Transform.Rotation(Number(op,"angle"),axis,Point(op,"origin")); }
                        foreach(var b in result) if(!b.Transform(transform))throw new Exception("Transform failed"); break;
                    }
                    case "union": case "difference": {
                        var left=Text(op,"left"); var right=Text(op,"right");
                        if(attributesById.ContainsKey(left))attributesById[id]=attributesById[left].Duplicate();
                        var leftBreps=BooleanBreps(objects[left]);var rightBreps=BooleanBreps(objects[right]);
                        result=kind=="union"?Brep.CreateBooleanUnion(leftBreps.Concat(rightBreps),0.000001):Brep.CreateBooleanDifference(leftBreps,rightBreps,0.000001);
                        consumed.Add(left); consumed.Add(right); break;
                    }
                    case "extrude": {
                        var points=((System.Collections.IEnumerable)op["points"]).Cast<System.Collections.IEnumerable>().Select(a=>a.Cast<object>().Select(Convert.ToDouble).ToArray()).ToArray();
                        if(points.Length<3||points.Length>128||points.Any(a=>a.Length!=2||a.Any(v=>Double.IsNaN(v)||Double.IsInfinity(v)||Math.Abs(v)>1000000)))throw new Exception("Invalid profile");
                        var poly=new Polyline(points.Select(a=>new Point3d(a[0],a[1],0))); if(poly[0]!=poly[poly.Count-1])poly.Add(poly[0]);
                        using(var curve=poly.ToNurbsCurve()) { var e=Extrusion.Create(curve,Positive(op,"height"),true); result=e==null?null:new[]{e.ToBrep()}; } break;
                    }
                    default: throw new NotSupportedException("Unsupported operation "+kind+"; use reference, box, sphere, cylinder, extrude, mesh, appearance, move, rotate, scale, union or difference");
                }
                if(result==null||result.Length==0||result.Length>64||result.Any(b=>b==null||!b.IsValid)) throw new Exception("Geometry operation failed: "+id);
                objects.Add(id,result);
            }
            Progress("applying explicit source targets");
            foreach(var reference in references.Where(p=>consumed.Contains(p.Key)))model.Objects.Delete(reference.Value);
            int preserved=model.Objects.Count;
            var bounds=BoundingBox.Empty; var ids=new List<string>();
            foreach(var pair in objects.Where(p=>!consumed.Contains(p.Key) && !references.ContainsKey(p.Key))) foreach(var geometry in pair.Value) {
                Progress("retaining result "+pair.Key);
                bounds.Union(geometry.GetBoundingBox(true));
                var attributes=attributesById.ContainsKey(pair.Key)?attributesById[pair.Key].Duplicate():new Rhino.DocObjects.ObjectAttributes { Name=pair.Key };
                if(pair.Value.Length>1)attributes.ObjectId=Guid.NewGuid();
                attributes.SetUserString("slate.operation",pair.Key); attributes.SetUserString("slate.preview",geometry is Mesh?"mesh":"cached-render-mesh");
                if(model.Objects.Any(o=>o.Attributes.ObjectId==attributes.ObjectId))attributes.ObjectId=Guid.NewGuid();
                if(model.Objects.Add(geometry,attributes)==Guid.Empty)throw new Exception("Could not retain geometry"); ids.Add(pair.Key);
                if(model.Objects.Count>256)throw new Exception("Output object budget exceeded");
            }
            var finalObjects=model.Objects.Select(o=>new {Geometry=o.Geometry.Duplicate(),Attributes=o.Attributes.Duplicate()}).ToArray();
            foreach(var item in finalObjects) {
                bounds.Union(item.Geometry.GetBoundingBox(true));
                var attributes=item.Attributes.Duplicate();
                var nativeMesh=item.Geometry as Mesh;
                if(nativeMesh!=null) {if(!nativeMesh.IsValid)throw new Exception("Invalid output mesh");CountMesh(nativeMesh);continue;}
                var originalBrep=item.Geometry as Brep;
                if(originalBrep!=null) {
                    var brep=originalBrep.DuplicateBrep();Progress("meshing "+attributes.ObjectId);CacheBrep(brep);
                    model.Objects.Delete(attributes.ObjectId);model.Objects.AddBrep(brep,attributes);continue;
                }
                var extrusion=item.Geometry as Extrusion;
                if(extrusion!=null) {
                    extrusion=(Extrusion)extrusion.Duplicate();
                    var pieces=Mesh.CreateFromBrep(extrusion.ToBrep(),MeshingParameters.FastRenderMesh);var joined=new Mesh();foreach(var piece in pieces)joined.Append(piece);
                    CountMesh(joined);
                    if(!extrusion.SetMesh(joined,MeshType.Render))throw new Exception("Extrusion mesh cache failed");
                    model.Objects.Delete(attributes.ObjectId);model.Objects.AddExtrusion(extrusion,attributes);
                }
            }
            if(!bounds.IsValid)bounds=new BoundingBox(Point3d.Origin,Point3d.Origin);
            CheckStop();
            Progress("writing model.3dm"); var output=Path.Combine(job,"model.3dm");
            if(!model.Write(output,8))throw new Exception("3dm save failed");
            CheckStop(); Progress("verifying model.3dm");
            using(var verify=Rhino.FileIO.File3dm.Read(output)) {
                if(verify==null||verify.Objects.Count!=model.Objects.Count||verify.Settings.ModelUnitSystem!=units)throw new Exception("3dm verification failed");
                int cached=0;
                foreach(var item in verify.Objects) {
                    var nativeMesh=item.Geometry as Mesh;
                    if(nativeMesh!=null) {if(!nativeMesh.IsValid||nativeMesh.Vertices.Count==0||nativeMesh.Faces.Count==0)throw new Exception("Output lost native mesh");
                        var expected=finalObjects.First(o=>o.Attributes.ObjectId==item.Attributes.ObjectId).Geometry as Mesh;
                        if(expected==null||expected.Vertices.Count!=nativeMesh.Vertices.Count||expected.Faces.Count!=nativeMesh.Faces.Count||expected.IsSolid!=nativeMesh.IsSolid)throw new Exception("Native mesh changed while saving");cached++;continue;}
                    var solid=item.Geometry as Brep;if(solid==null) {if(item.Geometry is Extrusion)cached++;continue;}
                    foreach(var face in solid.Faces) { var mesh=face.GetMesh(MeshType.Render); if(mesh==null||mesh.Vertices.Count==0)throw new Exception("Output lost cached preview mesh"); cached++; }
                }
                if(cached<meshCount)throw new Exception("Cached preview count changed while saving");
                foreach(var pair in authoredMaterials) {
                    var actual=verify.AllMaterials.FindIndex(pair.Key);var expected=pair.Value;
                    if(actual==null||actual.DiffuseColor.ToArgb()!=expected.DiffuseColor.ToArgb()||Math.Abs(actual.Shine-expected.Shine)>1e-9||Math.Abs(actual.Transparency-expected.Transparency)>1e-9)throw new Exception("Output lost standard material appearance");
                }
                foreach(var expected in finalObjects.Where(o=>o.Attributes.MaterialSource==Rhino.DocObjects.ObjectMaterialSource.MaterialFromObject)) {
                    var actual=verify.Objects.FirstOrDefault(o=>o.Attributes.ObjectId==expected.Attributes.ObjectId);
                    if(actual==null||actual.Attributes.MaterialIndex!=expected.Attributes.MaterialIndex||actual.Attributes.MaterialSource!=expected.Attributes.MaterialSource)throw new Exception("Output lost material assignment");
                }
            }
            File.WriteAllText(Path.Combine(job,"result.json"),Json.Serialize(new { units=Text(plan,"units"),actual_units=units.ToString(),source_objects=sourceObjects, objects=model.Objects.Count, preserved_objects=preserved, meshes=meshCount, vertices=vertices, faces=faces, operation_ids=ids.ToArray(), min=new[]{bounds.Min.X,bounds.Min.Y,bounds.Min.Z}, max=new[]{bounds.Max.X,bounds.Max.Y,bounds.Max.Z} }));
            Progress("verified");
        }
        return 0;
    }
}
