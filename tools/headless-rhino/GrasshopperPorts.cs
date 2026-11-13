using System;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using Grasshopper;
using Grasshopper.Kernel;
using Grasshopper.Kernel.Data;
using Grasshopper.Kernel.Special;
using Grasshopper.Kernel.Types;
using Rhino.FileIO;
using Rhino.Geometry;
using Rhino.Runtime;

internal sealed partial class GrasshopperHost
{
    private sealed class PortTree
    {
        public PortBranch[] branches { get; set; }
    }

    private sealed class PortBranch
    {
        public int[] path { get; set; }
        public PortValue[] items { get; set; }
    }

    private sealed class PortValue
    {
        public string kind { get; set; }
        public object value { get; set; }
    }

    private sealed class Port
    {
        public readonly IGH_Param Parameter;
        public readonly string Name, Kind, Access;
        public readonly bool Input;
        public readonly MethodInfo Assign;
        public string Applied;

        public Port(IGH_Param parameter, string name, string kind, string access, bool input)
        {
            Parameter = parameter;
            Name = name;
            Kind = kind;
            Access = access;
            Input = input;
            if (input && parameter is IGH_ContextualParameter)
                Assign = parameter.GetType().GetMethod("AssignContextualDataTree");
        }

        public string Id { get { return Parameter.InstanceGuid.ToString("D"); } }

        public object Describe(string solvedKind)
        {
            return new { id = Id, name = Name, direction = Input ? "input" : "output", kind = solvedKind ?? Kind, access = Access };
        }
    }

    private static List<Port> DiscoverPorts(GH_Document document)
    {
        var ports = new List<Port>();
        var seen = new HashSet<string>();
        Action<Port> add = port =>
        {
            if (seen.Add((port.Input ? "in:" : "out:") + port.Id))
                ports.Add(port);
        };
        foreach (var obj in document.Objects.OrderBy(item => item.Attributes.Pivot.Y))
        {
            var parameter = obj as IGH_Param;
            var getter = obj as IGH_ContextualParameter;
            if (parameter != null && getter != null && !parameter.Locked)
            {
                var tree = obj.GetType().GetProperty("TreeAccess");
                string access = tree != null && true.Equals(tree.GetValue(obj, null))
                    ? "tree" : (getter.AtMost == 1 ? "item" : "list");
                add(new Port(parameter, parameter.NickName, ParameterKind(parameter), access, true));
                continue;
            }
            var component = obj as GH_Component;
            string className = obj.GetType().Name;
            if (component != null && !component.Locked && component.Params.Input.Count > 0 &&
                (className == "ContextBakeComponent" || className == "ContextPrintComponent"))
            {
                var output = component.Params.Input[0];
                add(new Port(output, output.NickName, className == "ContextPrintComponent" ? "text" : "data", "tree", false));
                continue;
            }
            var group = obj as GH_Group;
            if (group == null || String.IsNullOrEmpty(group.NickName))
                continue;
            var member = group.Objects().FirstOrDefault();
            var groupedParameter = member as IGH_Param;
            if (group.NickName.IndexOf("RH_IN", StringComparison.OrdinalIgnoreCase) >= 0 && groupedParameter != null && !groupedParameter.Locked)
            {
                add(new Port(groupedParameter, group.NickName, ParameterKind(groupedParameter), groupedParameter.Access.ToString(), true));
            }
            if (group.NickName.IndexOf("RH_OUT", StringComparison.OrdinalIgnoreCase) < 0)
                continue;
            if (groupedParameter != null && !groupedParameter.Locked)
                add(new Port(groupedParameter, group.NickName, ParameterKind(groupedParameter), "tree", false));
            var groupedComponent = member as GH_Component;
            if (groupedComponent == null || groupedComponent.Locked)
                continue;
            foreach (var output in groupedComponent.Params.Output)
            {
                string name = groupedComponent.Params.Output.Count == 1 ? group.NickName : group.NickName + " (" + output.NickName + ")";
                add(new Port(output, name, ParameterKind(output), "tree", false));
            }
        }
        return ports;
    }

    private static string ParameterKind(IGH_Param parameter)
    {
        switch (parameter.GetType().Name)
        {
            case "GetIntegerParameter": case "Param_Integer": return "integer";
            case "GetNumberParameter": case "Param_Number": return "number";
            case "GetBooleanParameter": case "Param_Boolean": return "boolean";
            case "GetStringParameter": case "GetFilePathParameter": case "Param_String": case "Param_FilePath": return "text";
            case "Param_Colour": return "color";
            case "GetPointParameter": case "Param_Point": return "point";
            case "GetGeometryParameter": case "Param_Geometry": case "Param_Curve": case "Param_Line":
            case "Param_Surface": case "Param_Brep": case "Param_Mesh": case "Param_SubD": return "geometry";
            case "GetDataParameter": case "Param_GenericObject": return "data";
            default: return "unsupported";
        }
    }

    private List<Action> PrepareInputs(Viewer viewer, Dictionary<string, PortTree> inputs)
    {
        var known = new HashSet<string>();
        var actions = new List<Action>();
        foreach (var port in viewer.Ports.Where(port => port.Input))
        {
            known.Add(port.Id);
            PortTree tree = null;
            if (inputs != null)
                inputs.TryGetValue(port.Id, out tree);
            string key = tree == null ? null : json.Serialize(tree);
            if (key == port.Applied)
                continue;
            if (tree != null && (port.Kind == "unsupported" || (port.Parameter is IGH_ContextualParameter && port.Assign == null)))
                throw new InvalidDataException("Unsupported Grasshopper input type for " + port.Name + ".");
            var branches = tree == null ? null : PrepareTree(port, tree);
            actions.Add(() =>
            {
                ApplyInput(port, branches);
                port.Applied = key;
            });
        }
        if (inputs != null && inputs.Keys.Any(id => !known.Contains(id)))
            throw new InvalidDataException("A wired Grasshopper input no longer exists in the definition.");
        return actions;
    }

    private static List<KeyValuePair<GH_Path, List<IGH_Goo>>> PrepareTree(Port port, PortTree tree)
    {
        if (tree.branches == null || tree.branches.Length > 10000)
            throw new InvalidDataException("Invalid Grasshopper branch count for " + port.Name + ".");
        var result = new List<KeyValuePair<GH_Path, List<IGH_Goo>>>();
        var paths = new HashSet<string>();
        int total = 0;
        foreach (var branch in tree.branches)
        {
            if (branch == null || branch.path == null || branch.path.Length == 0 || branch.path.Length > 64 || branch.path.Any(i => i < 0) || branch.items == null)
                throw new InvalidDataException("Invalid Grasshopper data branch for " + port.Name + ".");
            var path = new GH_Path(branch.path);
            total += branch.items.Length;
            if (total > 10000 || !paths.Add(path.ToString()))
                throw new InvalidDataException("Too many values or a duplicate branch for " + port.Name + ".");
            result.Add(new KeyValuePair<GH_Path, List<IGH_Goo>>(path, branch.items.Select(value => DecodeValue(value, port.Kind)).ToList()));
        }
        return result;
    }

    private static void ApplyInput(Port port, List<KeyValuePair<GH_Path, List<IGH_Goo>>> branches)
    {
        if (port.Assign != null)
        {
            if (branches == null)
            {
                var getter = port.Parameter as IGH_ContextualParameter2;
                if (getter == null)
                    throw new InvalidDataException("This Grasshopper getter cannot restore its defaults.");
                getter.ClearContextualData();
            }
            else
            {
                var treeType = port.Assign.GetParameters()[0].ParameterType;
                var tree = Activator.CreateInstance(treeType);
                var ensure = treeType.GetMethod("EnsurePath", new[] { typeof(GH_Path) });
                var add = treeType.GetMethod("Add", new[] { treeType.GetGenericArguments()[0], typeof(GH_Path) });
                foreach (var branch in branches)
                {
                    ensure.Invoke(tree, new object[] { branch.Key });
                    foreach (var item in branch.Value)
                        add.Invoke(tree, new object[] { item, branch.Key });
                }
                port.Assign.Invoke(port.Parameter, new[] { tree });
            }
            port.Parameter.ExpireSolution(false);
            return;
        }

        port.Parameter.ExpireSolution(false);
        if (branches == null)
            return;
        port.Parameter.VolatileData.Clear();
        var structure = port.Parameter.VolatileData;
        var ensurePath = structure.GetType().GetMethod("EnsurePath", new[] { typeof(GH_Path) });
        foreach (var branch in branches)
        {
            if (ensurePath != null)
                ensurePath.Invoke(structure, new object[] { branch.Key });
            for (int i = 0; i < branch.Value.Count; i++)
                port.Parameter.AddVolatileData(branch.Key, i, branch.Value[i]);
        }
    }

    private static IGH_Goo DecodeValue(PortValue value, string kind)
    {
        if (value == null || value.kind == "null")
            return null;
        if (!(kind == "data" || kind == value.kind || (kind == "number" && value.kind == "integer") || (kind == "geometry" && value.kind == "point")))
            throw new InvalidDataException("Incompatible Grasshopper value " + value.kind + " for " + kind + ".");
        var data = value.value;
        switch (value.kind)
        {
            case "integer":
                int integer = Convert.ToInt32(data, CultureInfo.InvariantCulture);
                if (Number(data) != integer)
                    throw new InvalidDataException("Grasshopper integer value must be whole.");
                return kind == "number" ? (IGH_Goo)new GH_Number(integer) : new GH_Integer(integer);
            case "number": return new GH_Number(Number(data));
            case "boolean": if (data is bool) return new GH_Boolean((bool)data); break;
            case "text": if (data is string) return new GH_String((string)data); break;
            case "color":
                var rgba = data as System.Collections.IList;
                if (rgba != null && rgba.Count == 4)
                    return new GH_Colour(Color.FromArgb(Convert.ToByte(rgba[3]), Convert.ToByte(rgba[0]), Convert.ToByte(rgba[1]), Convert.ToByte(rgba[2])));
                break;
            case "point":
                var xyz = data as System.Collections.IList;
                if (xyz != null && xyz.Count == 3)
                    return new GH_Point(new Point3d(Number(xyz[0]), Number(xyz[1]), Number(xyz[2])));
                break;
            case "geometry":
                var json = data as string;
                if (json != null)
                {
                    var geometry = CommonObject.FromJSON(json) as GeometryBase;
                    if (geometry != null)
                    {
                        var goo = GH_Convert.ToGeometricGoo(geometry);
                        if (goo != null) return goo;
                        geometry.Dispose();
                    }
                }
                break;
        }
        throw new InvalidDataException("Malformed Grasshopper " + value.kind + " value.");
    }

    private static double Number(object value)
    {
        if (!(value is int || value is long || value is decimal || value is double))
            throw new InvalidDataException("Expected a numeric Grasshopper value.");
        double number = Convert.ToDouble(value, CultureInfo.InvariantCulture);
        if (Double.IsNaN(number) || Double.IsInfinity(number))
            throw new InvalidDataException("Expected a finite Grasshopper value.");
        return number;
    }

    private static void ReadOutputs(Viewer viewer, Response response)
    {
        var descriptions = new List<object>();
        foreach (var port in viewer.Ports)
        {
            string solvedKind = null;
            if (!port.Input)
            {
                try
                {
                    var tree = port.Parameter.VolatileData;
                    if (tree.PathCount > 10000 || tree.DataCount > 10000)
                        throw new InvalidDataException("Grasshopper output exceeds 10,000 branches or values.");
                    var branches = tree.Paths.Select(path => new PortBranch
                    {
                        path = path.Indices,
                        items = tree.get_Branch(path).Cast<IGH_Goo>().Select(EncodeValue).ToArray(),
                    }).ToArray();
                    var kinds = branches.SelectMany(branch => branch.items).Select(value => value.kind)
                        .Where(kind => kind != "null").Distinct().ToArray();
                    if ((port.Kind == "data" || port.Kind == "unsupported") && kinds.Length != 0)
                        solvedKind = kinds.Length == 1 ? kinds[0] : "data";
                    response.outputs.Add(port.Id, new PortTree { branches = branches });
                }
                catch (Exception error)
                {
                    solvedKind = "unsupported";
                    response.diagnostics.Add("Output " + port.Name + ": " + error.GetBaseException().Message);
                }
            }
            descriptions.Add(port.Describe(solvedKind));
        }
        response.ports = descriptions.ToArray();
    }

    private static PortValue EncodeValue(IGH_Goo goo)
    {
        if (goo == null) return Leaf("null", null);
        var wrapped = goo as GH_ObjectWrapper;
        if (wrapped != null && wrapped.Value is IGH_Goo) return EncodeValue((IGH_Goo)wrapped.Value);
        var integer = goo as GH_Integer;
        if (integer != null) return Leaf("integer", integer.Value);
        var number = goo as GH_Number;
        if (number != null) return Leaf("number", Number(number.Value));
        var boolean = goo as GH_Boolean;
        if (boolean != null) return Leaf("boolean", boolean.Value);
        var text = goo as GH_String;
        if (text != null) return Leaf("text", text.Value ?? String.Empty);
        var color = goo as GH_Colour;
        if (color != null) return Leaf("color", new int[] { color.Value.R, color.Value.G, color.Value.B, color.Value.A });
        var point = goo as GH_Point;
        if (point != null) return Leaf("point", new[] { Number(point.Value.X), Number(point.Value.Y), Number(point.Value.Z) });
        if (goo is IGH_GeometricGoo)
        {
            var geometry = GH_Convert.ToGeometryBase(goo);
            if (geometry != null) return Leaf("geometry", geometry.ToJSON(new SerializationOptions()));
        }
        throw new InvalidDataException("Unsupported Grasshopper value type " + goo.TypeName + ".");
    }

    private static PortValue Leaf(string kind, object value)
    {
        return new PortValue { kind = kind, value = value };
    }
}
