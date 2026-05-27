using AutoFix.SettingsUi;
using AutoFix.SettingsUi.Lifetime;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class EngineProcessLauncherTests
{
    [TestMethod]
    public void StartInfoRunsEngineRoleWithoutCreatingAWindow()
    {
        var startInfo = EngineProcessLauncher.CreateStartInfo(@"C:\AutoFix\Autofix.exe");

        Assert.AreEqual(@"C:\AutoFix\Autofix.exe", startInfo.FileName);
        Assert.AreEqual(Program.EngineArgument, startInfo.Arguments);
        Assert.IsFalse(startInfo.UseShellExecute);
        Assert.IsTrue(startInfo.CreateNoWindow);
        Assert.AreEqual(@"C:\AutoFix", startInfo.WorkingDirectory);
    }

    [TestMethod]
    public void ProcessPathGuardRejectsExistingNonAutofixExecutable()
    {
        using var temp = new TemporaryExecutable("testhost.exe");

        Assert.IsFalse(AutofixHostPath.IsAutofixHostPath(temp.Path));
    }

    [TestMethod]
    public void ProcessPathGuardAcceptsExistingAutofixExecutable()
    {
        using var temp = new TemporaryExecutable("Autofix.exe");

        Assert.IsTrue(AutofixHostPath.IsAutofixHostPath(temp.Path));
    }

    private sealed class TemporaryExecutable : IDisposable
    {
        private readonly string directory;

        public TemporaryExecutable(string fileName)
        {
            directory = System.IO.Path.Combine(System.IO.Path.GetTempPath(), System.IO.Path.GetRandomFileName());
            Directory.CreateDirectory(directory);
            Path = System.IO.Path.Combine(directory, fileName);
            File.WriteAllText(Path, string.Empty);
        }

        public string Path { get; }

        public void Dispose() => Directory.Delete(directory, recursive: true);
    }
}
