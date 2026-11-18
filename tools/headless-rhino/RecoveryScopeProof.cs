// Inspect the packaged worker's effective Grasshopper assembly scope without starting Rhino.
using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Reflection;

internal static class RecoveryScopeProof
{
    private static int Main(string[] args)
    {
        try
        {
            Verify(args);
            return 0;
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine(ex.GetBaseException().Message);
            return 1;
        }
    }

    private static void Verify(string[] args)
    {
        if (args.Length < 2 || args.Length > 3 ||
            (args.Length == 3 && args[2] != "--require-tracked-stock"))
            throw new ArgumentException("Expected the Rhino System folder, packaged HeadlessRhino.exe and optional --require-tracked-stock.");

        var system = Path.GetFullPath(args[0]);
        var grasshopper = Path.Combine(Directory.GetParent(system).FullName, "Plug-ins", "Grasshopper");
        AppDomain.CurrentDomain.AssemblyResolve += (sender, e) =>
        {
            var name = new AssemblyName(e.Name).Name + ".dll";
            foreach (var folder in new[] { system, grasshopper })
            {
                var file = Path.Combine(folder, name);
                if (File.Exists(file))
                    return Assembly.LoadFrom(file);
            }
            return null;
        };

        var marker = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData),
            "Grasshopper", "PluginLoading.txt");
        var original = File.Exists(marker) ? File.ReadAllBytes(marker) : null;
        var tracked = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        if (original != null)
            foreach (var line in File.ReadAllLines(marker))
                if (Path.IsPathRooted(line.Trim()))
                    tracked.Add(Path.GetFullPath(line.Trim()));

        var flags = BindingFlags.Static | BindingFlags.NonPublic;
        var worker = Assembly.LoadFrom(Path.GetFullPath(args[1]));
        worker.GetType("GrasshopperHost", true).GetMethod("ScopeBuiltInComponents", flags).Invoke(null, null);
        var sdk = Assembly.LoadFrom(Path.Combine(grasshopper, "Grasshopper.dll"));
        var server = sdk.GetType("Grasshopper.Kernel.GH_ComponentServer", true);
        var allowed = new HashSet<string>((IEnumerable<string>)server.GetField("_externalGHAPaths", flags)
            .GetValue(null), StringComparer.OrdinalIgnoreCase);
        var stock = Directory.GetFiles(Path.Combine(grasshopper, "Components"), "*.gha");
        var eligible = stock.Where(p => !Path.GetFileName(p).Equals("GhPython.gha", StringComparison.OrdinalIgnoreCase));
        var recordedStock = eligible.Where(tracked.Contains).ToArray();
        if (args.Length == 3 && recordedStock.Length == 0)
            throw new InvalidOperationException("This installation has no vendor-recorded stock assemblies for this case.");
        if (!allowed.SetEquals(eligible.Where(p => !tracked.Contains(p))))
            throw new InvalidOperationException("The actual SDK component scope differs from the eligible factory assemblies.");
        if (original == null ? File.Exists(marker) : !original.SequenceEqual(File.ReadAllBytes(marker)))
            throw new InvalidOperationException("The vendor's crash history changed.");
        Console.WriteLine("Admitted {0} of {1} installed factory assemblies. Recorded stock exclusions: {2}.",
            allowed.Count, stock.Length, String.Join(", ", recordedStock.Select(Path.GetFileName)));
    }
}
