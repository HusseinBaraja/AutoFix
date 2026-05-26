using System.IO.Pipes;
using System.Text;
using System.IO;

namespace AutoFix.SettingsUi.Lifetime;

public sealed class SingleInstance : IDisposable
{
    private const string MutexName = "Global\\AutoFix.Autofix.SingleInstance";
    private const string PipeName = "AutoFix.Autofix.Activate";
    private readonly Mutex mutex;
    private readonly CancellationTokenSource cancellation = new();
    private readonly Action activated;
    private readonly bool ownsInstance;
    private Task? listenerTask;
    private bool disposed;

    internal SingleInstance(Mutex mutex, bool ownsInstance, Action activated)
    {
        this.mutex = mutex;
        this.ownsInstance = ownsInstance;
        this.activated = activated;
    }

    public bool OwnsInstance => ownsInstance;

    public static SingleInstance Create(Action activated)
    {
        var mutex = new Mutex(true, MutexName, out var createdNew);
        var ownsInstance = createdNew || TryAcquireExistingMutex(mutex);
        return new SingleInstance(mutex, ownsInstance, activated);
    }

    internal static bool TryAcquireExistingMutex(Mutex mutex)
    {
        try
        {
            return mutex.WaitOne(0);
        }
        catch (AbandonedMutexException)
        {
            return true;
        }
    }

    public async Task SignalExistingAsync()
    {
        if (disposed)
        {
            throw new ObjectDisposedException(nameof(SingleInstance));
        }

        await using var client = new NamedPipeClientStream(".", PipeName, PipeDirection.Out, PipeOptions.Asynchronous);
        await client.ConnectAsync(500).ConfigureAwait(false);
        await client.WriteAsync(Encoding.UTF8.GetBytes("activate"), cancellation.Token).ConfigureAwait(false);
    }

    public void StartListening()
    {
        if (!ownsInstance)
        {
            return;
        }

        listenerTask = Task.Run(ListenAsync);
        _ = ObserveListenerTaskAsync(listenerTask);
    }

    internal static async Task ObserveListenerTaskAsync(Task task)
    {
        try
        {
            await task.ConfigureAwait(false);
        }
        catch (OperationCanceledException)
        {
        }
        catch (Exception)
        {
        }
    }

    private async Task ListenAsync()
    {
        while (!cancellation.IsCancellationRequested)
        {
            await using var server = new NamedPipeServerStream(PipeName, PipeDirection.In, 1, PipeTransmissionMode.Byte, PipeOptions.Asynchronous);
            try
            {
                await server.WaitForConnectionAsync(cancellation.Token).ConfigureAwait(false);
                TryActivate(activated);
            }
            catch (OperationCanceledException)
            {
                return;
            }
            catch (IOException)
            {
            }
        }
    }

    internal static bool TryActivate(Action activated)
    {
        try
        {
            activated();
            return true;
        }
        catch (Exception)
        {
            return false;
        }
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }

        disposed = true;
        cancellation.Cancel();
        if (listenerTask?.IsFaulted == true)
        {
            _ = listenerTask.Exception;
        }

        cancellation.Dispose();
        if (ownsInstance)
        {
            try
            {
                mutex.ReleaseMutex();
            }
            catch (ApplicationException)
            {
            }
        }

        mutex.Dispose();
    }
}
