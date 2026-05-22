using AutoFix.SettingsUi.Commands;

namespace AutoFix.SettingsUi.Tests;

[TestClass]
public sealed class AsyncRelayCommandTests
{
    [TestMethod]
    public async Task ExecuteReportsExceptionAndAllowsLaterExecution()
    {
        var expected = new InvalidOperationException("Command failed.");
        var reported = new TaskCompletionSource<Exception>();
        var secondReported = new TaskCompletionSource<Exception>();
        var executions = 0;
        var command = new AsyncRelayCommand(
            () =>
            {
                executions++;
                throw expected;
            },
            onError: exception =>
            {
                if (executions == 1)
                {
                    reported.TrySetResult(exception);
                }
                else
                {
                    secondReported.TrySetResult(exception);
                }
            });

        command.Execute(null);

        Assert.AreSame(expected, await reported.Task.WaitAsync(TimeSpan.FromSeconds(1)));
        Assert.IsTrue(command.CanExecute(null));

        command.Execute(null);

        Assert.AreSame(expected, await secondReported.Task.WaitAsync(TimeSpan.FromSeconds(1)));
        Assert.AreEqual(2, executions);
    }
}
