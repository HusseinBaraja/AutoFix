using System.Windows.Input;

namespace AutoFix.SettingsUi.Commands;

public sealed class AsyncRelayCommand : ICommand
{
    private readonly Func<Task> execute;
    private readonly Func<bool>? canExecute;
    private readonly Action<Exception>? onError;
    private bool isRunning;

    public AsyncRelayCommand(Func<Task> execute, Func<bool>? canExecute = null, Action<Exception>? onError = null)
    {
        ArgumentNullException.ThrowIfNull(execute);

        this.execute = execute;
        this.canExecute = canExecute;
        this.onError = onError;
    }

    public event EventHandler? CanExecuteChanged;

    public bool CanExecute(object? parameter) => !isRunning && (canExecute?.Invoke() ?? true);

    public async void Execute(object? parameter)
    {
        if (!CanExecute(parameter))
        {
            return;
        }

        isRunning = true;
        RaiseCanExecuteChanged();
        try
        {
            await execute();
        }
        catch (Exception exception)
        {
            onError?.Invoke(exception);
        }
        finally
        {
            isRunning = false;
            RaiseCanExecuteChanged();
        }
    }

    public void RaiseCanExecuteChanged() => CanExecuteChanged?.Invoke(this, EventArgs.Empty);
}
