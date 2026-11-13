using System;
using System.Diagnostics;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
using System.Threading;

// Grasshopper's vendor crash tracker is shared by all Rhino sessions of a user.
internal sealed class GrasshopperStartup : IDisposable
{
    private const string CrashDialog = "Potential Plugin Crash Protection";
    private readonly Mutex gate;
    private readonly ManualResetEvent finished = new ManualResetEvent(false);
    private readonly Thread watch;
    private bool completed;

    private GrasshopperStartup(string root, bool startup)
    {
        string sid;
        using (var identity = WindowsIdentity.GetCurrent())
            sid = identity.User.Value;
        gate = new Mutex(false, @"Global\Slate.Grasshopper.Components." + sid);
        bool held = false;
        try
        {
            held = gate.WaitOne(0);
            if (!held)
            {
                if (startup)
                    File.WriteAllText(Path.Combine(root, "progress.txt"), "Waiting for another Slate Grasshopper worker to finish loading components");
                held = gate.WaitOne();
            }
        }
        catch (AbandonedMutexException)
        {
            // The previous helper exited while loading. The vendor crash
            // tracker remains intact and Rhino can handle that recovery.
            held = true;
        }
        catch
        {
            gate.Dispose();
            finished.Dispose();
            throw;
        }
        if (!held)
        {
            gate.Dispose();
            finished.Dispose();
            throw new InvalidOperationException("Could not acquire the Grasshopper component-loading lock.");
        }
        watch = new Thread(() => Watch(root)) { IsBackground = true, Name = "Grasshopper crash-protection monitor" };
        watch.Start();
    }

    internal static GrasshopperStartup Enter(string root, bool startup)
    {
        return new GrasshopperStartup(root, startup);
    }

    public void Dispose()
    {
        Complete();
    }

    internal void Complete()
    {
        if (completed)
            return;
        completed = true;
        finished.Set();
        watch.Join();
        finished.Dispose();
        gate.ReleaseMutex();
        gate.Dispose();
    }

    private void Watch(string root)
    {
        using (var process = Process.GetCurrentProcess())
        {
            while (!finished.WaitOne(200))
            {
                if (!HasCrashDialog(process.Id))
                    continue;
                string message = "Rhino paused because Grasshopper detected a possible plug-in crash. " +
                    "Open Grasshopper in Rhino and resolve Potential Plugin Crash Protection. " +
                    "Choose Try Again Next Time to skip the plug-in for that Rhino launch. " +
                    "Wait for Grasshopper to finish opening before retrying in Slate.";
                string tracked = TrackedPluginNames();
                if (!String.IsNullOrEmpty(tracked))
                    message += " Grasshopper recorded: " + tracked + ".";
                try
                {
                    string path = Path.Combine(root, "error.txt");
                    string temporary = Path.Combine(root, "crash-protection.tmp");
                    File.WriteAllText(temporary, message);
                    File.WriteAllText(Path.Combine(root, "plugin-crash-protection"), "1");
                    File.Move(temporary, path);
                }
                catch (IOException) { }
                catch (UnauthorizedAccessException) { }
                return;
            }
        }
    }

    private static string TrackedPluginNames()
    {
        var root = Environment.GetFolderPath(Environment.SpecialFolder.ApplicationData);
        var path = Path.Combine(root, "Grasshopper", "PluginLoading.txt");
        try
        {
            if (new FileInfo(path).Length > 16384)
                return null;
            return String.Join(", ", File.ReadLines(path).Take(8).Select(line => Path.GetFileName(line.Trim()))
                .Where(name => !String.IsNullOrEmpty(name)).Distinct(StringComparer.OrdinalIgnoreCase));
        }
        catch (IOException) { return null; }
        catch (UnauthorizedAccessException) { return null; }
        catch (ArgumentException) { return null; }
    }

    private static bool HasCrashDialog(int processId)
    {
        bool found = false;
        EnumWindows((window, _) =>
        {
            uint owner;
            GetWindowThreadProcessId(window, out owner);
            if (owner != processId)
                return true;
            var title = new StringBuilder(128);
            GetWindowText(window, title, title.Capacity);
            found = String.Equals(title.ToString(), CrashDialog, StringComparison.Ordinal);
            return !found;
        }, IntPtr.Zero);
        return found;
    }

    private delegate bool EnumWindowsCallback(IntPtr window, IntPtr state);

    [DllImport("user32.dll")]
    private static extern bool EnumWindows(EnumWindowsCallback callback, IntPtr state);

    [DllImport("user32.dll")]
    private static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    private static extern int GetWindowText(IntPtr window, StringBuilder text, int maximum);
}
