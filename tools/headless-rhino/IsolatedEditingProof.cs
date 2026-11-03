// Disposable SDK fixture/proof, compiled separately from the production helper.
using System;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using Rhino.Geometry;
class IsolatedEditingProof {
    static readonly Guid Target=new Guid("11111111-1111-4111-8111-111111111111");
    static readonly Guid Untouched=new Guid("22222222-2222-4222-8222-222222222222");
    static string system;
    [DllImport("kernel32.dll",CharSet=CharSet.Unicode)] static extern bool SetDllDirectory(string path);
    [STAThread] static int Main(string[] args) {
        system=args[0];SetDllDirectory(system);
        Environment.SetEnvironmentVariable("PATH",system+";"+Environment.GetEnvironmentVariable("PATH"));
        AppDomain.CurrentDomain.AssemblyResolve+=(s,e)=>{var p=Path.Combine(system,new AssemblyName(e.Name).Name+".dll");return File.Exists(p)?Assembly.LoadFrom(p):null;};
        try{return Run(args);}catch(Exception e){Console.Error.WriteLine(e);return 1;}
    }
    [MethodImpl(MethodImplOptions.NoInlining)] static int Run(string[] args) {
        using(var core=new Rhino.Runtime.InProcess.RhinoCore()) {
            if(args[1]=="seed")using(var model=new Rhino.FileIO.File3dm()) {
                model.Settings.ModelUnitSystem=Rhino.UnitSystem.Millimeters;
                int layer=model.AllLayers.AddLayer("Retained layer",System.Drawing.Color.Coral);
                var material=new Rhino.DocObjects.Material {Name="Retained material",DiffuseColor=System.Drawing.Color.CadetBlue};
                material.SetBitmapTexture("textures/missing-cloud-texture.png");
                int mat=model.AllMaterials.AddMaterial(material);
                var target=new Rhino.DocObjects.ObjectAttributes {ObjectId=Target,Name="Target",LayerIndex=layer,MaterialIndex=mat,MaterialSource=Rhino.DocObjects.ObjectMaterialSource.MaterialFromObject};
                var untouched=target.Duplicate();untouched.ObjectId=Untouched;untouched.Name="Untouched";
                model.Objects.AddBrep(new Box(Plane.WorldXY,new Interval(0,2),new Interval(0,3),new Interval(0,4)).ToBrep(),target);
                model.Objects.AddBrep(new Sphere(new Point3d(20,0,0),2).ToBrep(),untouched);
                if(!model.Write(args[2],8))throw new Exception("seed save failed");
                Console.WriteLine("Seeded two disposable solids, named layer/material, relative texture, mm");
            } else using(var model=Rhino.FileIO.File3dm.Read(args[2])) {
                if(model.Settings.ModelUnitSystem!=Rhino.UnitSystem.Millimeters || model.Objects.Count!=2)throw new Exception("units/count lost");
                var target=model.Objects.Single(o=>o.Attributes.ObjectId==Target);
                var untouched=model.Objects.Single(o=>o.Attributes.ObjectId==Untouched);
                if(untouched.Attributes.Name!="Untouched" || Math.Abs(untouched.Geometry.GetBoundingBox(true).Center.X-20)>1e-6)throw new Exception("unrelated object changed");
                if(Math.Abs(target.Geometry.GetBoundingBox(true).Min.X-Double.Parse(args[3],System.Globalization.CultureInfo.InvariantCulture))>1e-6)throw new Exception("target did not move expected distance");
                if(!model.AllLayers.Any(l=>l.Name=="Retained layer")||!model.AllMaterials.Any(m=>m.Name=="Retained material"))throw new Exception("layer/material lost");
                var texture=model.AllMaterials.Single(m=>m.Name=="Retained material").GetBitmapTexture();
                if(texture==null || !Path.IsPathRooted(texture.FileName) || !String.Equals(Path.GetFullPath(texture.FileName),Path.GetFullPath(Path.Combine(args[4],"textures/missing-cloud-texture.png")),StringComparison.OrdinalIgnoreCase))throw new Exception("relative texture locator was not preserved at source origin: "+(texture==null?"null":texture.FileName));
                if(target.Attributes.LayerIndex!=untouched.Attributes.LayerIndex || target.Attributes.MaterialIndex!=untouched.Attributes.MaterialIndex)throw new Exception("target attributes lost");
                Console.WriteLine("VERIFIED target ID/transform, unrelated ID/geometry, mm, named layer/material, rebased texture locator");
            }
        }
        return 0;
    }
}
