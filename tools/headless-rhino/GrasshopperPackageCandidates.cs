using System;
using System.Collections;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.IO.Compression;
using System.Linq;
using System.Reflection;
using System.Text.RegularExpressions;
using System.Threading.Tasks;

internal static class GrasshopperPackageCandidates
{
    // Rhino ships Yak.Core beside RhinoCommon. Keeping the dependency at runtime
    // lets the helper compile on machines with only the public RhinoCommon SDK.
    internal static Dictionary<string, object> Resolve(IList<string> libraryIds)
    {
        var configured = Rhino.ApplicationSettings.PackageManagerSettings.Sources;
        var sources = string.IsNullOrWhiteSpace(configured)
            ? new[] { "https://yak.rhino3d.com" }
            : configured.Split(';').Select(source => source.Trim())
                .Where(source => source.Length != 0).Distinct(StringComparer.Ordinal).ToArray();
        var system = Path.GetDirectoryName(typeof(Rhino.RhinoApp).Assembly.Location);
        return ResolveInSources(libraryIds, sources, system);
    }

    internal static Dictionary<string, object> ResolveInSources(
        IList<string> libraryIds, string[] sources, string rhinoSystem)
    {
        var candidates = new List<Dictionary<string, object>>();
        var errors = new List<string>();
        var result = new Dictionary<string, object>
        {
            { "trusted_sources", sources },
            { "candidates", candidates },
            { "errors", errors }
        };
        var clock = Stopwatch.StartNew();
        var deadline = TimeSpan.FromSeconds(20);
        try
        {
            var sdk = new YakLookup(Assembly.LoadFrom(Path.Combine(rhinoSystem, "Yak.Core.dll")));
            var clients = sources.Select(source => sdk.CreateClient(source)).ToArray();
            var seen = new HashSet<Guid>();
            foreach (var value in libraryIds)
            {
                Guid id;
                if (!Guid.TryParseExact(value, "D", out id) || id == Guid.Empty || !seen.Add(id))
                    continue;
                try
                {
                    var matching = new List<Dictionary<string, object>>();
                    for (var index = 0; index < clients.Length; index++)
                    {
                        var source = sources[index];
                        var client = clients[index];
                        matching.AddRange(ReadBeforeDeadline(
                            () => sdk.FindCompatible(client, source, id), deadline - clock.Elapsed));
                    }
                    candidates.AddRange(matching);
                }
                catch (TimeoutException)
                {
                    errors.Add("Rhino Package Manager lookup timed out for " + id);
                    break;
                }
                catch (Exception)
                {
                    errors.Add("A configured Rhino repository could not resolve " + id);
                }
            }
        }
        catch (Exception)
        {
            errors.Add("Rhino Package Manager SDK or repository configuration is unavailable");
        }
        return result;
    }

    internal static Dictionary<string, object> InspectLocal(
        string packagePath, IEnumerable<Guid> loadedLibraries)
    {
        var system = Path.GetDirectoryName(typeof(Rhino.RhinoApp).Assembly.Location);
        return InspectLocalInSystem(packagePath, loadedLibraries, system);
    }

    internal static Dictionary<string, object> InspectLocalInSystem(
        string packagePath, IEnumerable<Guid> loadedLibraries, string rhinoSystem)
    {
        if (String.IsNullOrEmpty(packagePath) || !Path.IsPathRooted(packagePath) ||
            !Path.GetExtension(packagePath).Equals(".yak", StringComparison.OrdinalIgnoreCase))
            throw new InvalidDataException("Drop an original Rhino Package Manager .yak file");

        var yak = Assembly.LoadFrom(Path.Combine(rhinoSystem, "Yak.Core.dll"));
        var manifestType = yak.GetType("Yak.Manifest", true);
        object manifest;
        using (var input = File.OpenRead(packagePath))
        using (var archive = new ZipArchive(input, ZipArchiveMode.Read))
        {
            var manifests = archive.Entries.Where(entry =>
                entry.FullName.Equals("manifest.yml", StringComparison.OrdinalIgnoreCase)).ToArray();
            if (manifests.Length != 1 || manifests[0].Length > 1024 * 1024)
                throw new InvalidDataException("The .yak file needs one valid manifest.yml");
            var hasLibrary = false;
            foreach (var entry in archive.Entries)
            {
                var entryPath = entry.FullName.Replace('\\', '/');
                var segments = entryPath.Split('/');
                if (entryPath.StartsWith("/", StringComparison.Ordinal) || entryPath.IndexOf(':') >= 0 ||
                    segments.Any(segment => segment == "." || segment == ".." ||
                        segment.EndsWith(" ", StringComparison.Ordinal) ||
                        segment.EndsWith(".", StringComparison.Ordinal)))
                    throw new InvalidDataException("The .yak file contains an invalid archive path");
                if (entryPath.EndsWith(".gha", StringComparison.OrdinalIgnoreCase) &&
                    (segments.Length == 1 || (segments.Length == 2 &&
                        segments[0].Equals("net48", StringComparison.OrdinalIgnoreCase))))
                    hasLibrary = true;
            }
            if (!hasLibrary)
                throw new InvalidDataException("The .yak file does not contain a Grasshopper library for this Rhino runtime");
            using (var metadata = manifests[0].Open())
            {
                manifest = manifestType.GetMethod("FromStream", new[] { typeof(Stream) })
                    .Invoke(null, new object[] { metadata });
            }
        }
        if (manifest == null || !(bool)manifestType.GetMethod("IsValid").Invoke(manifest, null))
            throw new InvalidDataException("Rhino could not validate the .yak manifest");
        var name = (string)manifestType.GetProperty("Name").GetValue(manifest, null);
        var version = (string)manifestType.GetProperty("Version").GetValue(manifest, null);
        CheckLocalDistribution(yak, packagePath, name, version);

        var ids = new HashSet<Guid>();
        var keywords = manifestType.GetProperty("Keywords").GetValue(manifest, null) as IEnumerable<string>;
        foreach (var keyword in keywords ?? Enumerable.Empty<string>())
        {
            Guid id;
            if (keyword != null && keyword.StartsWith("guid:", StringComparison.OrdinalIgnoreCase) &&
                Guid.TryParse(keyword.Substring(5), out id) && id != Guid.Empty)
                ids.Add(id);
        }
        var secretProperty = manifestType.GetProperty("Secret");
        var secret = secretProperty == null ? null : secretProperty.GetValue(manifest, null);
        if (secret != null)
        {
            var id = (Guid)secret.GetType().GetProperty("Id").GetValue(secret, null);
            if (id != Guid.Empty)
                ids.Add(id);
        }
        if (ids.Count == 0)
            throw new InvalidDataException("The .yak manifest does not declare a Grasshopper plug-in ID");
        if (ids.Overlaps(loadedLibraries))
            throw new InvalidDataException("Rhino already has a plug-in from this package loaded");
        return new Dictionary<string, object>
        {
            { "name", name },
            { "version", version },
            { "library_ids", ids.OrderBy(id => id).Select(id => id.ToString("D")).ToArray() }
        };
    }

    private static void CheckLocalDistribution(
        Assembly yak, string packagePath, string name, string version)
    {
        var expected = name + "-" + version;
        var filename = Path.GetFileNameWithoutExtension(packagePath);
        if (!filename.Equals(expected, StringComparison.OrdinalIgnoreCase))
        {
            if (!filename.StartsWith(expected + "-", StringComparison.OrdinalIgnoreCase) ||
                !Regex.IsMatch(filename.Substring(expected.Length + 1).ToLowerInvariant(),
                    @"\A(?:any|rh[0-9]+(?:_[0-9]+)?)-(?:any|win|mac)\z"))
                throw new InvalidDataException("Use the .yak file's original package and distribution filename");
        }
        var type = yak.GetType("Yak.Distribution", true);
        var distribution = type.GetMethod("FromFilename", new[] { typeof(string) })
            .Invoke(null, new object[] { Path.GetFileName(packagePath).ToLowerInvariant() });
        if (!(bool)type.GetMethod("IsCompatible", new[] { typeof(bool) })
            .Invoke(distribution, new object[] { true }))
            throw new InvalidDataException("This .yak distribution is incompatible with the installed Rhino version or Windows");
    }

    private static T ReadBeforeDeadline<T>(Func<T> lookup, TimeSpan remaining)
    {
        if (remaining <= TimeSpan.Zero)
            throw new TimeoutException();
        // The GH worker is STA. Yak queries must not capture its UI context.
        var task = Task.Run(lookup);
        if (!task.Wait(remaining))
            throw new TimeoutException();
        return task.Result;
    }

    private sealed class YakLookup
    {
        private readonly ConstructorInfo newHeader, newClient;
        private readonly MethodInfo createSource, search, getVersions, isCompatible, parseVersion, compareVersions;
        private readonly PropertyInfo versionsClient;
        private readonly FieldInfo packageName, versionNumber, prerelease, distributions;

        internal YakLookup(Assembly assembly)
        {
            var headerType = assembly.GetType("Yak.ProductHeaderValue", true);
            var sourceType = assembly.GetType("Yak.IPackageRepository", true);
            var clientType = assembly.GetType("Yak.YakClient", true);
            var packageType = assembly.GetType("Yak.Package", true);
            var versionType = assembly.GetType("Yak.Version", true);
            var numberType = assembly.GetType("Yak.Helpers.VersionNumber", true);
            var distributionType = assembly.GetType("Yak.Distribution", true);
            newHeader = headerType.GetConstructor(new[] { typeof(string) });
            newClient = clientType.GetConstructor(new[] { sourceType });
            createSource = assembly.GetType("Yak.PackageRepositoryFactory", true)
                .GetMethod("Create", new[] { typeof(string), headerType });
            search = clientType.GetMethod("Search", new[] { typeof(string) });
            versionsClient = clientType.GetProperty("Version");
            getVersions = versionsClient.PropertyType.GetMethod("GetAll", new[] { typeof(string) });
            isCompatible = distributionType.GetMethod("IsCompatible", new[] { typeof(bool) });
            parseVersion = numberType.GetMethod("TryParse", new[] { typeof(string), numberType.MakeByRefType() });
            compareVersions = numberType.GetMethod("CompareTo", new[] { numberType });
            packageName = packageType.GetField("Name");
            versionNumber = versionType.GetField("Number");
            prerelease = versionType.GetField("Prerelease");
            distributions = versionType.GetField("Distributions");
        }

        internal object CreateClient(string source)
        {
            var product = newHeader.Invoke(new object[] { "Slate" });
            var repository = createSource.Invoke(null, new[] { source, product });
            return newClient.Invoke(new[] { repository });
        }

        internal List<Dictionary<string, object>> FindCompatible(object client, string source, Guid id)
        {
            var candidates = new List<Dictionary<string, object>>();
            // GH1 Package Restore uses this same GUID field in Yak searches.
            foreach (var package in InvokeQuery(search, client, "guid:" + id.ToString("D")))
            {
                var name = (string)packageName.GetValue(package);
                if (string.IsNullOrWhiteSpace(name))
                    continue;
                var versions = InvokeQuery(getVersions, versionsClient.GetValue(client, null), name);
                var compatible = new List<KeyValuePair<object, Dictionary<string, object>>>();
                foreach (var version in versions)
                {
                    var available = distributions.GetValue(version) as IEnumerable;
                    if (available == null || !available.Cast<object>().Any(
                        item => (bool)isCompatible.Invoke(item, new object[] { true })))
                        continue;
                    var number = (string)versionNumber.GetValue(version);
                    var parts = new object[] { number, null };
                    if (!(bool)parseVersion.Invoke(null, parts))
                        continue;
                    compatible.Add(new KeyValuePair<object, Dictionary<string, object>>(
                        parts[1], new Dictionary<string, object>
                        {
                            { "library_id", id.ToString("D") },
                            { "name", name },
                            { "version", number },
                            { "source", source },
                            { "prerelease", prerelease.GetValue(version) }
                        }));
                }
                compatible.Sort((left, right) =>
                    (int)compareVersions.Invoke(right.Key, new[] { left.Key }));
                candidates.AddRange(compatible.Select(item => item.Value));
            }
            return candidates;
        }

        private static Array InvokeQuery(MethodInfo method, object client, string query)
        {
            var task = (Task)method.Invoke(client, new object[] { query });
            task.GetAwaiter().GetResult();
            return (Array)task.GetType().GetProperty("Result").GetValue(task, null);
        }
    }
}
