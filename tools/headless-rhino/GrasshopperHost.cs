using System;
using System.Collections;
using System.Collections.Generic;
using System.Drawing;
using System.Globalization;
using System.IO;
using System.Linq;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Security.Cryptography;
using System.Text;
using System.Web.Script.Serialization;
using GH_IO.Serialization;
using Grasshopper.GUI.Base;
using Grasshopper.Kernel;
using Grasshopper.Kernel.Special;
using Rhino;
using Rhino.DocObjects;
using Rhino.FileIO;
using Rhino.Geometry;
using Rhino.PlugIns;

// This process stays on one STA thread. Grasshopper and installed components can
// have process-wide state, so do not overlap document solutions.
internal sealed partial class GrasshopperHost : IDisposable
{
    // A solved tree may itself become the next definition's request.
    private const int MessageLimit = 8 * 1024 * 1024;
    private const int ViewerLimit = 8;
    private static readonly Guid RhinoCommands = new Guid("02bf604d-799c-4cc2-830e-8d72f21b14b7");
    private static readonly Guid RhinoScripting = new Guid("c9cba87a-23ce-4f15-a918-97645c05cde7");
    private readonly JavaScriptSerializer json = new JavaScriptSerializer { MaxJsonLength = MessageLimit };
    private readonly Dictionary<string, Viewer> viewers = new Dictionary<string, Viewer>();
    private readonly Dictionary<string, string> localPluginHashes = new Dictionary<string, string>(StringComparer.OrdinalIgnoreCase);
    private long useCounter;

    [MethodImpl(MethodImplOptions.NoInlining)]
    internal static int Run(string root, bool builtInsOnly)
    {
        File.WriteAllText(Path.Combine(root, "progress.txt"), "Starting Rhino (headless)");
        using (var startup = GrasshopperStartup.Enter(root, true))
        using (var core = new Rhino.Runtime.InProcess.RhinoCore(null, Rhino.Runtime.InProcess.WindowStyle.NoWindow))
        {
            // Match Rhino.Compute's order so GH can load component libraries
            // that depend on Rhino scripting services during initialization.
            File.WriteAllText(Path.Combine(root, "progress.txt"), "Loading Rhino commands and scripting");
            PlugIn.LoadPlugIn(RhinoCommands);
            if (PlugIn.LoadPlugIn(RhinoScripting))
            {
                var scripting = RhinoApp.GetPlugInObject(RhinoScripting);
                var reports = scripting == null ? null : scripting.GetType().GetProperty("SendReportsToConsole");
                if (reports != null && reports.CanWrite && reports.PropertyType == typeof(bool))
                    reports.SetValue(scripting, true, null);
            }
            File.WriteAllText(Path.Combine(root, "progress.txt"), builtInsOnly
                ? "Loading Grasshopper's built-in components after plug-in crash protection"
                : "Loading Grasshopper and installed components");
            var plugin = RhinoApp.GetPlugInObject("Grasshopper");
            var runHeadless = plugin == null ? null : plugin.GetType().GetMethod("RunHeadless");
            if (runHeadless == null)
                throw new InvalidOperationException("The installed Rhino does not provide the Grasshopper headless runtime.");
            if (builtInsOnly)
                ScopeBuiltInComponents();
            runHeadless.Invoke(plugin, null);
            startup.Complete();
            return Serve(root, builtInsOnly);
        }
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static void ScopeBuiltInComponents()
    {
        // This SDK hook scopes assembly discovery to the Rhino installation for
        // this process. The normal profile and the vendor crash tracker stay intact.
        var filter = typeof(GH_ComponentServer).GetMethod("SetExternalGHAs", BindingFlags.Static | BindingFlags.Public | BindingFlags.NonPublic,
            null, new[] { typeof(IEnumerable<string>) }, null);
        if (filter == null)
            throw new InvalidOperationException("This Grasshopper version cannot start with built-in components only.");
        var components = Path.Combine(Path.GetDirectoryName(typeof(GH_ComponentServer).Assembly.Location), "Components");
        var files = Directory.GetFiles(components, "*.gha")
            .Where(path => !Path.GetFileName(path).Equals("GhPython.gha", StringComparison.OrdinalIgnoreCase)).ToArray();
        if (files.Length == 0)
            throw new InvalidOperationException("The Rhino installation does not contain Grasshopper components.");
        System.Environment.SetEnvironmentVariable("GRASSHOPPER_PLUGINS", null);
        filter.Invoke(null, new object[] { files });
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    private static int Serve(string root, bool builtInsOnly)
    {
        GH_Document.EnableSolutions = true;
        using (var host = new GrasshopperHost())
        {
            WriteAtomic(Path.Combine(root, "ready"), "ready");
            string token;
            while ((token = Console.ReadLine()) != null)
            {
                if (!IsToken(token))
                    throw new InvalidDataException("Invalid Grasshopper worker job token.");
                host.ProcessJob(Path.Combine(root, "jobs", token), builtInsOnly);
            }
        }
        return 0;
    }

    private void ProcessJob(string job, bool builtInsOnly)
    {
        var response = new Response();
        try
        {
            string input = Path.Combine(job, "request.json");
            if (new FileInfo(input).Length > MessageLimit)
                throw new InvalidDataException("Grasshopper request exceeds the worker size limit.");
            var request = json.Deserialize<Request>(File.ReadAllText(input));
            if (builtInsOnly && request != null && request.action != "solve" && request.action != "bake" && request.action != "close")
                throw new InvalidOperationException("Plug-in installation and loading require the normal Grasshopper profile.");
            Execute(job, request, response);
        }
        catch (Exception error)
        {
            response.status = "error";
            response.error = error.GetBaseException().Message;
        }
        string serialized;
        try
        {
            serialized = json.Serialize(response);
            if (Encoding.UTF8.GetByteCount(serialized) > MessageLimit)
                throw new InvalidDataException("Grasshopper response exceeds the worker size limit.");
        }
        catch (Exception error)
        {
            serialized = json.Serialize(new Response { status = "error", error = error.GetBaseException().Message });
        }
        WriteAtomic(Path.Combine(job, "response.json"), serialized);
    }

    private void Execute(string job, Request request, Response response)
    {
        if (request == null)
            throw new InvalidDataException("Missing Grasshopper request.");
        if (request.action == "resolve_packages")
        {
            ResolvePackages(request.library_ids, response);
            return;
        }
        if (request.action == "load_local_plugin")
        {
            LoadLocalPlugin(Path.GetDirectoryName(Path.GetDirectoryName(job)), request, response);
            return;
        }
        if (request.action == "inspect_local_package")
        {
            var loadedLibraries = Grasshopper.Instances.ComponentServer.Libraries.Select(library => library.Id);
            response.local_package = GrasshopperPackageCandidates.InspectLocal(request.package_path, loadedLibraries);
            return;
        }
        if (!IsToken(request.viewer_id))
            throw new InvalidDataException("Invalid Grasshopper viewer identifier.");
        if (request.action == "close")
        {
            CloseViewer(request.viewer_id);
            return;
        }
        if (request.action != "solve" && request.action != "bake")
            throw new InvalidDataException("Unknown Grasshopper worker action.");

        var viewer = OpenViewer(request, response);
        if (viewer == null)
            return;
        var previous = RhinoDoc.ActiveDoc;
        try
        {
            RhinoDoc.ActiveDoc = viewer.Context;
            response.ports = viewer.Ports.Select(port => port.Describe(null)).ToArray();
            var actions = PrepareOverrides(viewer, request.overrides, response.diagnostics);
            actions.AddRange(PrepareInputs(viewer, request.inputs));
            if (request.action == "bake" && (viewer.NeedsSolve || actions.Count != 0))
                throw new InvalidOperationException("Refresh the Grasshopper preview before baking.");
            foreach (var action in actions)
                action();
            viewer.NeedsSolve |= actions.Count != 0;
            viewer.Document.Enabled = true;
            if (viewer.NeedsSolve)
            {
                viewer.Document.NewSolution(false, GH_SolutionMode.CommandLine);
                var snapshot = Capture(viewer);
                if (viewer.Snapshot != null)
                    viewer.Snapshot.Dispose();
                viewer.Snapshot = snapshot;
                viewer.NeedsSolve = false;
            }
            response.controls = viewer.Controls.Select(control => control.Read()).ToArray();
            AddSolutionMessages(viewer.Document, response.diagnostics);
            ReadOutputs(viewer, response);
            WriteSnapshot(viewer, job, request.action == "bake", response);
        }
        catch
        {
            // A component can leave partially mutated state when a solve fails.
            CloseViewer(request.viewer_id);
            throw;
        }
        finally
        {
            if (!viewer.Disposed)
                viewer.Document.Enabled = false;
            if (previous != null && RhinoDoc.FromRuntimeSerialNumber(previous.RuntimeSerialNumber) != null)
                RhinoDoc.ActiveDoc = previous;
        }
    }

    private Viewer OpenViewer(Request request, Response response)
    {
        string path = Path.GetFullPath(request.source_path);
        string extension = Path.GetExtension(path);
        if (!extension.Equals(".gh", StringComparison.OrdinalIgnoreCase) &&
            !extension.Equals(".ghx", StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Open a Grasshopper .gh or .ghx definition.");
        var file = new FileInfo(path);
        if (!file.Exists)
            throw new FileNotFoundException("The Grasshopper definition is unavailable.", path);
        if (file.Length > 128L * 1024 * 1024)
            throw new InvalidDataException("The Grasshopper definition exceeds the 128 MiB worker limit.");

        Viewer cached;
        if (viewers.TryGetValue(request.viewer_id, out cached) &&
            cached.Path.Equals(path, StringComparison.OrdinalIgnoreCase) &&
            cached.SourceRevision == request.source_revision && cached.Length == file.Length &&
            cached.LastWrite == file.LastWriteTimeUtc)
        {
            cached.LastUse = ++useCounter;
            return cached;
        }

        if (request.action == "bake")
            throw new InvalidOperationException("Refresh the Grasshopper preview before baking.");
        CloseViewer(request.viewer_id);
        var archive = new GH_Archive();
        if (!archive.ReadFromFile(path))
            throw new InvalidDataException("Grasshopper could not read the source archive.");
        var definition = archive.GetRootNode.FindChunk("Definition");
        if (definition == null)
            throw new InvalidDataException("The archive has no Grasshopper definition.");
        response.missing_packages = FindMissingPackages(definition);
        if (response.missing_packages.Length != 0)
        {
            response.status = "missing_packages";
            return null;
        }

        var doc = new GH_Document();
        RhinoDoc context = null;
        try
        {
            if (!archive.ExtractObject(doc, "Definition"))
                throw new InvalidDataException("Grasshopper could not load the source definition.");
            doc.FilePath = path;
            // A separate document supplies model settings without reading or mutating the user's Rhino document.
            context = RhinoDoc.CreateHeadless(null);
            if (context == null)
                throw new InvalidOperationException("Could not create a headless Rhino context for this definition.");
            doc.Enabled = false;
            var viewer = new Viewer(doc, context, path, request.source_revision, file, ++useCounter);
            if (viewers.Count >= ViewerLimit)
                CloseViewer(viewers.OrderBy(pair => pair.Value.LastUse).First().Key);
            Grasshopper.Instances.DocumentServer.AddDocument(doc);
            viewers.Add(request.viewer_id, viewer);
            return viewer;
        }
        catch
        {
            doc.Dispose();
            if (context != null)
                context.Dispose();
            throw;
        }
    }

    private static MissingPackage[] FindMissingPackages(GH_IReader definition)
    {
        var libraries = new Dictionary<Guid, GH_IReader>();
        var chunk = definition.FindChunk("GHALibraries");
        int count = 0;
        if (chunk != null && chunk.TryGetInt32("Count", ref count))
        {
            for (int index = 0; index < count; index++)
            {
                var entry = chunk.FindChunk("Library", index);
                Guid id = Guid.Empty;
                if (entry != null && entry.TryGetGuid("Id", ref id))
                    libraries[id] = entry;
            }
        }

        var server = Grasshopper.Instances.ComponentServer;
        var loaded = new HashSet<Guid>(server.Libraries.Select(library => library.Id));
        var missing = new Dictionary<Guid, MissingPackage>();
        var objects = definition.FindChunk("DefinitionObjects");
        count = 0;
        if (objects == null || !objects.TryGetInt32("ObjectCount", ref count))
            return new MissingPackage[0];
        for (int index = 0; index < count; index++)
        {
            var obj = objects.FindChunk("Object", index);
            Guid componentId = Guid.Empty;
            if (obj == null || !obj.TryGetGuid("GUID", ref componentId) || server.IsObjectCached(componentId))
                continue;
            Guid libraryId = Guid.Empty;
            obj.TryGetGuid("Lib", ref libraryId);
            Guid key = libraryId == Guid.Empty ? componentId : libraryId;
            MissingPackage package;
            if (!missing.TryGetValue(key, out package))
            {
                string objectName = "Unknown Grasshopper component";
                obj.TryGetString("Name", ref objectName);
                string version = null;
                obj.TryGetString("LibVer", ref version);
                GH_IReader library;
                if (libraries.TryGetValue(libraryId, out library))
                {
                    library.TryGetString("Name", ref objectName);
                    if (String.IsNullOrEmpty(version))
                        library.TryGetString("Version", ref version);
                }
                package = new MissingPackage
                {
                    library_id = libraryId == Guid.Empty ? null : libraryId.ToString("D"),
                    name = objectName,
                    version = version,
                    already_loaded = loaded.Contains(libraryId),
                };
                missing.Add(key, package);
            }
            string id = componentId.ToString("D");
            if (!package.component_ids.Contains(id))
                package.component_ids.Add(id);
        }
        return missing.Values.ToArray();
    }

    private static void ResolvePackages(string[] libraryIds, Response response)
    {
        if (libraryIds == null || libraryIds.Length > 64)
            throw new InvalidDataException("Invalid list of missing Grasshopper libraries.");
        var candidates = GrasshopperPackageCandidates.Resolve(libraryIds);
        object value;
        if (candidates.TryGetValue("trusted_sources", out value))
            response.trusted_sources = value;
        if (candidates.TryGetValue("candidates", out value))
            response.candidates = value;
        if (candidates.TryGetValue("errors", out value))
            response.errors = value;
    }

    private void LoadLocalPlugin(string root, Request request, Response response)
    {
        var source = request.source_path;
        var pathRoot = String.IsNullOrEmpty(source) ? null : Path.GetPathRoot(source);
        bool absolute = pathRoot != null && pathRoot.Length >= 3 &&
            (pathRoot.StartsWith("\\\\", StringComparison.Ordinal) || pathRoot[2] == '\\' || pathRoot[2] == '/');
        if (!absolute ||
            !String.Equals(Path.GetExtension(source), ".gha", StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Drop a Grasshopper plug-in with an absolute .gha path.");
        string path = Path.GetFullPath(source);
        if (!File.Exists(path))
            throw new FileNotFoundException("The dropped Grasshopper plug-in is unavailable.", path);
        if (request.component_ids == null || request.component_ids.Length == 0 || request.component_ids.Length > 4096)
            throw new InvalidDataException("Provide the missing Grasshopper component identifiers.");

        var ids = request.component_ids.Select(id => ParsePluginId(id, "component")).Distinct().ToArray();
        Guid? libraryId = String.IsNullOrEmpty(request.library_id) ? (Guid?)null : ParsePluginId(request.library_id, "library");
        var server = Grasshopper.Instances.ComponentServer;

        // Keep the file open for reading throughout the verification and load.
        // A concurrent writer cannot replace its bytes after the hash check.
        using (var stream = File.OpenRead(path))
        using (var sha = SHA256.Create())
        {
            response.sha256 = BitConverter.ToString(sha.ComputeHash(stream)).Replace("-", "").ToLowerInvariant();
            if (request.expected_sha256 != null && !String.Equals(request.expected_sha256, response.sha256, StringComparison.OrdinalIgnoreCase))
                throw new InvalidOperationException("The local Grasshopper plug-in changed since it was added. Drop the updated file to load it.");
            string loadedHash;
            bool alreadyLoaded = localPluginHashes.TryGetValue(path, out loadedHash);
            if (alreadyLoaded && loadedHash != response.sha256)
                throw new InvalidOperationException("The local Grasshopper plug-in changed while it was loaded. Restart the worker before using the updated file.");

            UpdatePluginResponse(server, ids, libraryId, path, response);
            if (response.loaded_component_ids.Length != 0)
            {
                if (!alreadyLoaded)
                    localPluginHashes.Add(path, response.sha256);
                return;
            }
            if (alreadyLoaded)
                throw new InvalidOperationException("The dropped plug-in does not provide any of the remaining Grasshopper components.");

            var existing = libraryId.HasValue ? server.FindAssembly(libraryId.Value) : null;
            if (existing != null && !PluginLocationMatches(existing, path))
                throw new InvalidOperationException("A different file already provides this Grasshopper library in the current worker.");

            // Keep the native single-file load path, including Grasshopper's load
            // protection and memory-load settings. The public SDK offers a full scan.
            var load = typeof(GH_ComponentServer).GetMethod("LoadExternalFile", BindingFlags.Instance | BindingFlags.Public | BindingFlags.NonPublic,
                null, new[] { typeof(GH_ExternalFile), typeof(bool) }, null);
            if (load == null || load.ReturnType != typeof(bool))
                throw new InvalidOperationException("The installed Grasshopper version does not support loading a single plug-in file in this worker.");
            Rhino.Runtime.AssemblyResolver.AddSearchFolder(Path.GetDirectoryName(path));
            int errorsBefore = server.LoadingExceptions.Count;
            bool accepted;
            using (GrasshopperStartup.Enter(root, false))
                accepted = (bool)load.Invoke(server, new object[] { new GH_ExternalFile(path), false });
            foreach (var error in server.LoadingExceptions.Skip(errorsBefore).Take(8))
                response.diagnostics.Add(error.Name + ": " + error.Message);
            UpdatePluginResponse(server, ids, libraryId, path, response);
            if (!accepted || response.loaded_component_ids.Length == 0)
                throw new InvalidOperationException("The dropped plug-in did not provide any of the missing Grasshopper components. Check that the file and its dependencies match this Rhino installation.");
            localPluginHashes.Add(path, response.sha256);
        }
    }

    private static Guid ParsePluginId(string text, string name)
    {
        Guid id;
        if (!Guid.TryParseExact(text, "D", out id) || id == Guid.Empty)
            throw new InvalidDataException("Invalid Grasshopper " + name + " identifier.");
        return id;
    }

    private static void UpdatePluginResponse(GH_ComponentServer server, Guid[] ids, Guid? libraryId, string path, Response response)
    {
        var loaded = new List<string>();
        var remaining = new List<string>();
        foreach (var id in ids)
        {
            var library = server.FindAssemblyByObject(id);
            bool matches = library != null && (!libraryId.HasValue || library.Id == libraryId.Value) && PluginLocationMatches(library, path);
            (matches ? loaded : remaining).Add(id.ToString("D"));
        }
        response.loaded_component_ids = loaded.ToArray();
        response.remaining_component_ids = remaining.ToArray();
    }

    private static bool PluginLocationMatches(GH_AssemblyInfo library, string path)
    {
        return !String.IsNullOrEmpty(library.Location) &&
            String.Equals(Path.GetFullPath(library.Location), path, StringComparison.OrdinalIgnoreCase);
    }

    private void CloseViewer(string key)
    {
        Viewer viewer;
        if (!viewers.TryGetValue(key, out viewer))
            return;
        viewers.Remove(key);
        viewer.Dispose();
    }

    public void Dispose()
    {
        foreach (var key in viewers.Keys.ToArray())
            CloseViewer(key);
    }

    private static bool IsToken(string value)
    {
        return value != null && value.Length == 32 &&
            value.All(ch => (ch >= '0' && ch <= '9') || (ch >= 'a' && ch <= 'f'));
    }

    private static void WriteAtomic(string path, string content)
    {
        string temporary = path + ".tmp";
        File.WriteAllText(temporary, content);
        File.Move(temporary, path);
    }

    private List<Action> PrepareOverrides(Viewer viewer, Dictionary<string, Value> values, List<string> diagnostics)
    {
        var actions = new List<Action>();
        var known = new HashSet<string>();
        foreach (var control in viewer.Controls)
        {
            known.Add(control.Id);
            Value value = null;
            if (values != null)
                values.TryGetValue(control.Id, out value);
            var action = control.Prepare(value, json);
            if (action != null)
                actions.Add(action);
        }
        if (values != null)
        {
            int stale = values.Keys.Count(id => !known.Contains(id));
            if (stale > 0)
                diagnostics.Add(stale + " saved Grasshopper controls no longer exist in the definition.");
        }
        return actions;
    }

    private static void AddSolutionMessages(GH_Document doc, List<string> diagnostics)
    {
        foreach (var component in doc.Objects.OfType<IGH_ActiveObject>())
        {
            foreach (var level in new[] { GH_RuntimeMessageLevel.Error, GH_RuntimeMessageLevel.Warning })
            {
                foreach (var message in component.RuntimeMessages(level))
                {
                    if (diagnostics.Count >= 64)
                        return;
                    diagnostics.Add(component.NickName + ": " + message.Substring(0, Math.Min(message.Length, 512)));
                }
            }
        }
    }

    private static Snapshot Capture(Viewer viewer)
    {
        var snapshot = new Snapshot();
        if (viewer.Document.PreviewMode == GH_PreviewMode.Disabled)
        {
            snapshot.Diagnostics.Add("Preview is disabled in this Grasshopper definition.");
            return snapshot;
        }

        try
        {
            var bake = RhinoDoc.CreateHeadless(null);
            if (bake == null)
                throw new InvalidOperationException("Could not open the headless bake document.");
            snapshot.Document = bake;
            bake.ModelUnitSystem = viewer.Context.ModelUnitSystem;
            bake.ModelAbsoluteTolerance = viewer.Context.ModelAbsoluteTolerance;
            var ids = new List<Guid>();
            var boundary = viewer.Document.PreviewBoundary;
            int drawOnly = 0;
            foreach (var obj in viewer.Document.Objects)
            {
                var preview = obj as IGH_PreviewObject;
                var active = obj as IGH_ActiveObject;
                if (preview == null || preview.Hidden || !preview.IsPreviewCapable || (active != null && active.Locked) ||
                    (boundary != null && !boundary.IsVisible(obj)) ||
                    (viewer.Document.PreviewFilter == GH_PreviewFilter.Selected && !obj.Attributes.Selected))
                    continue;
                var baker = obj as IGH_BakeAwareObject;
                if (baker == null || !baker.IsBakeCapable)
                {
                    drawOnly++;
                    continue;
                }
                baker.BakeGeometry(bake, null, ids);
                if (ids.Count > 50000)
                    throw new InvalidOperationException("Grasshopper preview exceeds the 50,000-object worker limit.");
            }
            if (drawOnly > 0)
                snapshot.Diagnostics.Add(drawOnly + " visible Grasshopper components draw directly in Rhino and could not be transferred.");
            if (ids.Count == 0)
            {
                snapshot.Diagnostics.Add("No visible Grasshopper geometry to preview or bake.");
            }
            snapshot.Objects = ids.Distinct().Select(id => bake.Objects.FindId(id)).Where(obj => obj != null).ToArray();
            return snapshot;
        }
        catch
        {
            snapshot.Dispose();
            throw;
        }
    }

    private static void WriteSnapshot(Viewer viewer, string job, bool writeBake, Response response)
    {
        var snapshot = viewer.Snapshot;
        response.diagnostics.AddRange(snapshot.Diagnostics);
        if (snapshot.Objects.Length == 0)
            return;
        response.preview = WritePreview(viewer.Document, snapshot.Document, snapshot.Objects,
            Path.Combine(job, "preview.3dm"), response.diagnostics);
        if (!writeBake)
            return;

        foreach (var obj in snapshot.Objects)
            obj.CreateMeshes(MeshType.Render, MeshingParameters.FastRenderMesh, true);
        var options = new FileWriteOptions
        {
            IncludeRenderMeshes = true,
            SuppressDialogBoxes = true,
            SuppressAllInput = true,
        };
        string path = Path.Combine(job, "bake.3dm");
        if (!snapshot.Document.Write3dmFile(path, options))
            throw new IOException("Could not write the baked Rhino model.");
        var types = snapshot.Objects.ToDictionary(obj => obj.Id, obj => obj.Geometry.ObjectType);
        VerifyModel(path, snapshot.Objects.Length, types);
        response.baked = true;
    }

    private static bool WritePreview(GH_Document doc, RhinoDoc bake, RhinoObject[] objects, string path, List<string> diagnostics)
    {
        using (var preview = new File3dm())
        {
            preview.Settings.ModelUnitSystem = bake.ModelUnitSystem;
            var bounds = BoundingBox.Empty;
            foreach (var obj in objects)
                bounds.Union(obj.Geometry.GetBoundingBox(false));
            double span = bounds.IsValid ? bounds.Diagonal.Length : 0;
            double radius = span > 0 ? Math.Max(0.0001, span * 0.0015) : 0.05;
            int count = 0, omitted = 0, vertices = 0, faces = 0;
            var materials = new Dictionary<int, int>();
            foreach (var obj in objects)
            {
                using (var mesh = PreviewMesh(obj.Geometry, radius))
                {
                    if (mesh == null || mesh.Faces.Count == 0)
                    {
                        omitted++;
                        continue;
                    }
                    vertices += mesh.Vertices.Count;
                    faces += mesh.Faces.Count;
                    if (vertices > 1000000 || faces > 1000000)
                        throw new InvalidOperationException("Grasshopper preview exceeds the one-million-vertex or face worker limit.");
                    var attributes = new ObjectAttributes
                    {
                        Name = obj.Attributes.Name,
                        ColorSource = ObjectColorSource.ColorFromObject,
                        ObjectColor = obj.Attributes.ColorSource == ObjectColorSource.ColorFromLayer
                            ? doc.PreviewColour : obj.Attributes.DrawColor(bake),
                    };
                    var material = obj.GetMaterial(true);
                    if (material != null && material.MaterialIndex >= 0)
                    {
                        int index;
                        if (!materials.TryGetValue(material.MaterialIndex, out index))
                        {
                            index = preview.AllMaterials.AddMaterial(material);
                            materials.Add(material.MaterialIndex, index);
                        }
                        attributes.MaterialSource = ObjectMaterialSource.MaterialFromObject;
                        attributes.MaterialIndex = index;
                    }
                    if (preview.Objects.AddMesh(mesh, attributes) == Guid.Empty)
                        throw new IOException("Could not add geometry to the Grasshopper preview.");
                    count++;
                }
            }
            if (omitted > 0)
                diagnostics.Add(omitted + " visible objects use geometry Slate cannot show in this preview.");
            if (count == 0)
                return false;
            if (!preview.Write(path, 8))
                throw new IOException("Could not write the Grasshopper preview meshes.");
            VerifyModel(path, count, null);
            return true;
        }
    }

    private static void VerifyModel(string path, int count, Dictionary<Guid, ObjectType> types)
    {
        using (var model = File3dm.Read(path))
        {
            if (model == null || count == 0 || model.Objects.Count != count)
                throw new IOException("The saved Grasshopper geometry failed to round-trip.");
            foreach (var item in model.Objects)
            {
                ObjectType expected = ObjectType.Mesh;
                if ((types != null && !types.TryGetValue(item.Attributes.ObjectId, out expected)) ||
                    item.Geometry == null || item.Geometry.ObjectType != expected)
                    throw new IOException("The saved Grasshopper model changed a geometry type during export.");
            }
        }
    }

    private static Mesh PreviewMesh(GeometryBase geometry, double radius)
    {
        var mesh = geometry as Mesh;
        if (mesh != null)
            return mesh.DuplicateMesh();
        var curve = geometry as Curve;
        if (curve != null)
            return Mesh.CreateFromCurvePipe(curve, radius, 6, 8, MeshPipeCapStyle.Flat, false, null);
        var point = geometry as Rhino.Geometry.Point;
        if (point != null)
            return Mesh.CreateFromSphere(new Sphere(point.Location, radius * 2), 6, 4);
        var cloud = geometry as PointCloud;
        if (cloud != null)
        {
            if (cloud.Count > 10000)
                throw new InvalidOperationException("Grasshopper preview exceeds the 10,000-point worker limit.");
            var joined = new Mesh();
            foreach (var location in cloud.GetPoints())
            {
                using (var sphere = Mesh.CreateFromSphere(new Sphere(location, radius * 2), 6, 4))
                    joined.Append(sphere);
            }
            return joined;
        }
        var subd = geometry as SubD;
        if (subd != null)
            return Mesh.CreateFromSubD(subd, 2);
        var brep = geometry as Brep;
        if (brep != null)
            return MeshBrep(brep);
        var extrusion = geometry as Extrusion;
        if (extrusion != null)
        {
            using (var shape = extrusion.ToBrep())
                return MeshBrep(shape);
        }
        var surface = geometry as Surface;
        if (surface != null)
        {
            using (var shape = surface.ToBrep())
                return MeshBrep(shape);
        }
        return null;
    }

    private static Mesh MeshBrep(Brep brep)
    {
        if (brep == null)
            return null;
        var pieces = Mesh.CreateFromBrep(brep, MeshingParameters.FastRenderMesh);
        if (pieces == null || pieces.Length == 0)
            return null;
        var result = new Mesh();
        foreach (var mesh in pieces)
        {
            using (mesh)
                result.Append(mesh);
        }
        return result;
    }

    private sealed class Control
    {
        private readonly IGH_DocumentObject obj;
        private readonly string kind;
        private readonly Func<Value> read;
        private readonly Func<Value, Action> prepare;
        private readonly Value initial;
        private readonly Dictionary<string, object> details;
        private string applied;

        private Control(IGH_DocumentObject obj, string kind, Func<Value> read, Func<Value, Action> prepare,
            Dictionary<string, object> details = null)
        {
            this.obj = obj;
            this.kind = kind;
            this.read = read;
            this.prepare = prepare;
            this.details = details ?? new Dictionary<string, object>();
            initial = read == null ? null : read();
        }

        public string Id { get { return obj.InstanceGuid.ToString("D"); } }

        public object Read()
        {
            return new Dictionary<string, object>(details)
            {
                { "id", Id }, { "name", obj.NickName }, { "kind", kind },
                { "value", read == null ? null : read() },
            };
        }

        public Action Prepare(Value value, JavaScriptSerializer serializer)
        {
            if (prepare == null)
                return null;
            if (value == null && applied == null)
                return null;
            if (value != null && (value.type != initial.type || value.value == null))
                throw new InvalidDataException("Invalid value for Grasshopper control " + obj.NickName + ".");
            string key = value == null ? null : serializer.Serialize(value);
            if (key == applied)
                return null;
            var update = prepare(value ?? initial);
            return () =>
            {
                update();
                ((IGH_ActiveObject)obj).ExpireSolution(false);
                applied = key;
            };
        }

        public static Control From(IGH_DocumentObject obj)
        {
            if (String.IsNullOrWhiteSpace(obj.NickName))
                return null;
            var slider = obj as GH_NumberSlider;
            if (slider != null)
            {
                var accuracy = slider.Slider.Type;
                int precision = accuracy == GH_SliderAccuracy.Float ? slider.Slider.DecimalPlaces : 0;
                double step = accuracy == GH_SliderAccuracy.Even || accuracy == GH_SliderAccuracy.Odd
                    ? 2 : Math.Pow(10, -precision);
                return new Control(obj, "slider",
                    () => new Value { type = "number", value = slider.Slider.Value }, value =>
                    {
                        decimal number = Convert.ToDecimal(value.value, CultureInfo.InvariantCulture);
                        number = Math.Max(slider.Slider.Minimum, Math.Min(slider.Slider.Maximum, number));
                        return () => slider.SetSliderValue(number);
                    }, new Dictionary<string, object>
                    {
                        { "min", slider.Slider.Minimum }, { "max", slider.Slider.Maximum },
                        { "step", step }, { "precision", precision },
                    });
            }

            var toggle = obj as GH_BooleanToggle;
            if (toggle != null)
                return new Control(obj, "toggle", () => new Value { type = "bool", value = toggle.Value }, value =>
                {
                    if (!(value.value is bool))
                        throw new InvalidDataException("A Grasshopper toggle requires a boolean value.");
                    return () => toggle.Value = (bool)value.value;
                });

            var list = obj as GH_ValueList;
            if (list != null)
                return FromList(list);

            var panel = obj as GH_Panel;
            if (panel != null && panel.SourceCount == 0)
                return new Control(obj, "text", () => new Value { type = "text", value = panel.UserText }, value =>
                {
                    var text = value.value as string;
                    if (text == null || text.Length > 65536)
                        throw new InvalidDataException("A Grasshopper text control requires up to 65,536 characters.");
                    return () => panel.UserText = text;
                });

            var swatch = obj as GH_ColourSwatch;
            if (swatch != null)
                return new Control(obj, "color", () =>
                {
                    var color = swatch.SwatchColour;
                    return new Value { type = "color", value = new[] { (int)color.R, color.G, color.B, color.A } };
                }, value =>
                {
                    var channels = AsArray(value.value).Select(channel => Convert.ToInt32(channel, CultureInfo.InvariantCulture)).ToArray();
                    if (channels.Length != 4 || channels.Any(channel => channel < 0 || channel > 255))
                        throw new InvalidDataException("A Grasshopper color requires four RGBA byte values.");
                    var color = Color.FromArgb(channels[3], channels[0], channels[1], channels[2]);
                    return () => swatch.SwatchColour = color;
                });

            var param = obj as IGH_Param;
            if (obj is IRcpAwareObject || (param != null && param.SourceCount == 0 && obj.NickName != obj.Name))
                return new Control(obj, "readonly", null, null, new Dictionary<string, object>
                {
                    { "status", "This Grasshopper control type is read-only in Slate." },
                });
            return null;
        }

        private static Control FromList(GH_ValueList list)
        {
            bool multiple = list.ListMode == GH_ValueListMode.CheckList;
            var occurrences = new Dictionary<string, int>();
            var options = new List<Choice>();
            using (var sha = SHA256.Create())
            {
                foreach (var item in list.ListItems)
                {
                    string label = item.Name ?? "";
                    string identity = label.Length.ToString(CultureInfo.InvariantCulture) + ":" + label + item.Expression;
                    string hash = BitConverter.ToString(sha.ComputeHash(Encoding.UTF8.GetBytes(identity))).Replace("-", "").ToLowerInvariant();
                    int occurrence;
                    occurrences.TryGetValue(hash, out occurrence);
                    occurrences[hash] = occurrence + 1;
                    string id = occurrence == 0 ? hash : hash + "-" + (occurrence + 1).ToString(CultureInfo.InvariantCulture);
                    options.Add(new Choice { id = id, label = label });
                }
            }
            return new Control(list, "list", () => new Value
            {
                type = "choice",
                value = options.Where((option, index) => list.ListItems[index].Selected).Select(option => option.id).ToArray(),
            }, value =>
            {
                var selected = new HashSet<string>(AsArray(value.value).Cast<string>());
                if ((!multiple && selected.Count > 1) || selected.Any(id => !options.Any(option => option.id == id)))
                    throw new InvalidDataException("Invalid Grasshopper list selection.");
                return () =>
                {
                    for (int index = 0; index < options.Count; index++)
                        list.ListItems[index].Selected = selected.Contains(options[index].id);
                };
            }, new Dictionary<string, object> { { "options", options }, { "multiple", multiple } });
        }

        private static IEnumerable<object> AsArray(object value)
        {
            var array = value as IList;
            if (array == null)
                throw new InvalidDataException("Expected a list of Grasshopper control values.");
            return array.Cast<object>();
        }
    }

    private sealed class Choice
    {
        public string id;
        public string label;
    }

    private sealed class Request
    {
        public string action { get; set; }
        public string viewer_id { get; set; }
        public string source_path { get; set; }
        public string source_revision { get; set; }
        public Dictionary<string, Value> overrides { get; set; }
        public Dictionary<string, PortTree> inputs { get; set; }
        public string[] library_ids { get; set; }
        public string library_id { get; set; }
        public string[] component_ids { get; set; }
        public string expected_sha256 { get; set; }
        public string package_path { get; set; }
    }

    private sealed class Response
    {
        public string status = "ready";
        public object[] controls = new object[0];
        public object[] ports = new object[0];
        public Dictionary<string, PortTree> outputs = new Dictionary<string, PortTree>();
        public MissingPackage[] missing_packages = new MissingPackage[0];
        public List<string> diagnostics = new List<string>();
        public bool preview;
        public bool baked;
        public string error;
        public object trusted_sources = new string[0];
        public object candidates = new object[0];
        public object errors = new string[0];
        public string[] loaded_component_ids = new string[0];
        public string[] remaining_component_ids = new string[0];
        public string sha256;
        public object local_package;
    }

    private sealed class MissingPackage
    {
        public string library_id;
        public string name;
        public string version;
        public bool already_loaded;
        public List<string> component_ids = new List<string>();
    }

    private sealed class Value
    {
        public string type { get; set; }
        public object value { get; set; }
    }

    private sealed class Snapshot : IDisposable
    {
        public RhinoDoc Document;
        public RhinoObject[] Objects = new RhinoObject[0];
        public readonly List<string> Diagnostics = new List<string>();

        public void Dispose()
        {
            if (Document != null)
                Document.Dispose();
        }
    }

    private sealed class Viewer : IDisposable
    {
        public readonly GH_Document Document;
        public readonly RhinoDoc Context;
        public readonly string Path, SourceRevision;
        public readonly long Length;
        public readonly DateTime LastWrite;
        public readonly List<Control> Controls;
        public readonly List<Port> Ports;
        public long LastUse;
        public bool NeedsSolve = true;
        public bool Disposed;
        public Snapshot Snapshot;

        public Viewer(GH_Document document, RhinoDoc context, string path, string revision, FileInfo file, long lastUse)
        {
            Document = document;
            Context = context;
            Path = path;
            SourceRevision = revision;
            Length = file.Length;
            LastWrite = file.LastWriteTimeUtc;
            LastUse = lastUse;
            Controls = document.Objects.Select(Control.From).Where(control => control != null).ToList();
            Ports = DiscoverPorts(document);
        }

        public void Dispose()
        {
            Disposed = true;
            Document.Enabled = false;
            Grasshopper.Instances.DocumentServer.RemoveDocument(Document);
            Document.Dispose();
            if (Snapshot != null)
                Snapshot.Dispose();
            Context.Dispose();
        }
    }
}
