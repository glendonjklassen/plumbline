// Code-only WinUI 3 entry point (no XAML files, so no XAML compiler in the
// build). The generated Main is disabled in the csproj; this replicates it.
//
// A XAML-less app must do two things the App.xaml codegen normally does:
//   1. implement IXamlMetadataProvider (forwarding to the WinUI controls
//      provider) — without it the XAML runtime dies at startup with
//      0xc000027b, and
//   2. merge XamlControlsResources so the standard controls have a theme.

using Microsoft.UI.Dispatching;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Markup;
using Microsoft.UI.Xaml.XamlTypeInfo;

namespace PureStudyWin;

public partial class App : Application, IXamlMetadataProvider
{
    private readonly XamlControlsXamlMetaDataProvider _xamlMeta = new();

    public App()
    {
        UnhandledException += (_, e) =>
        {
            try
            {
                File.WriteAllText(
                    Path.Combine(AppContext.BaseDirectory, "crash.log"),
                    $"{DateTime.Now:O}\n{e.Message}\n{e.Exception}");
            }
            catch { /* best-effort */ }
        };
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        Resources.MergedDictionaries.Add(new XamlControlsResources());
        _window = new MainWindow();
        _window.Activate();
    }

    private Window? _window;

    public IXamlType GetXamlType(Type type) => _xamlMeta.GetXamlType(type);
    public IXamlType GetXamlType(string fullName) => _xamlMeta.GetXamlType(fullName);
    public XmlnsDefinition[] GetXmlnsDefinitions() => _xamlMeta.GetXmlnsDefinitions();
}

public static class Program
{
    [STAThread]
    public static void Main(string[] args)
    {
        WinRT.ComWrappersSupport.InitializeComWrappers();
        Application.Start(p =>
        {
            var ctx = new DispatcherQueueSynchronizationContext(
                DispatcherQueue.GetForCurrentThread());
            SynchronizationContext.SetSynchronizationContext(ctx);
            _ = new App();
        });
    }
}
