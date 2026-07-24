// One reading pane: the per-pane nav strip (book / chapter / prev / next /
// add / close) above a ReaderView. Mirrors the GTK pane exactly: 1–3 panes,
// nav strip on #efeae1, and the active pane gets a 2-px gold top border when
// more than one pane is open.

using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using PureStudy;

namespace PureStudyWin;

public sealed class PaneView : UserControl, IDisposable
{
    public readonly ReaderView Reader = new();

    private readonly ComboBox _bookBox = new() { MinWidth = 150 };
    private readonly NumberBox _chapterBox = new()
    {
        Minimum = 1, Value = 1, MinWidth = 110,
        SpinButtonPlacementMode = NumberBoxSpinButtonPlacementMode.Inline,
    };
    private readonly Button _prev = new() { Content = "‹" };
    private readonly Button _next = new() { Content = "›" };
    private readonly Button _add = new() { Content = "+" };
    private readonly Button _close = new() { Content = "✕" };
    private readonly Border _accent = new()
    {
        Height = 2,
        Background = new SolidColorBrush(Palette.Gold),
        Visibility = Visibility.Collapsed,
    };
    private readonly StackPanel _nav = new()
    {
        Orientation = Orientation.Horizontal,
        Spacing = 6,
        Padding = new Thickness(6, 3, 6, 3),
    };

    private List<TocBook> _books = new();
    private bool _guard;

    /// This pane was interacted with (GTK: becomes the active pane).
    public event Action? Touched;
    public event Action? AddRequested;
    public event Action? CloseRequested;

    public PaneView()
    {
        _nav.Background = new SolidColorBrush(Palette.PaneNavBg);
        _nav.Children.Add(_bookBox);
        _nav.Children.Add(_chapterBox);
        _nav.Children.Add(_prev);
        _nav.Children.Add(_next);
        _nav.Children.Add(_add);
        _nav.Children.Add(_close);

        var root = new Grid();
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(_accent, 0);
        Grid.SetRow(_nav, 1);
        Grid.SetRow(Reader, 2);
        root.Children.Add(_accent);
        root.Children.Add(_nav);
        root.Children.Add(Reader);
        Content = root;

        _bookBox.SelectionChanged += (_, _) =>
        {
            if (_guard || _bookBox.SelectedIndex < 0) return;
            Touched?.Invoke();
            Reader.ShowChapter(_books[_bookBox.SelectedIndex].Id, 1);
        };
        _chapterBox.ValueChanged += (_, _) =>
        {
            if (_guard || double.IsNaN(_chapterBox.Value)) return;
            Touched?.Invoke();
            Reader.ShowChapter(Reader.Book, (uint)_chapterBox.Value);
        };
        _prev.Click += (_, _) => { Touched?.Invoke(); Step(-1); };
        _next.Click += (_, _) => { Touched?.Invoke(); Step(+1); };
        _add.Click += (_, _) => AddRequested?.Invoke();
        _close.Click += (_, _) => CloseRequested?.Invoke();
        Reader.ChapterShown += (_, _) => SyncNav();
        Reader.Activated += () => Touched?.Invoke();
    }

    public void SetBooks(List<TocBook> books)
    {
        _books = books;
        _guard = true;
        _bookBox.ItemsSource = books.Select(b => b.Name).ToList();
        _guard = false;
        SyncNav();
    }

    /// GTK step_pane: clamp within this book.
    private void Step(int dir)
    {
        int idx = _books.FindIndex(b => b.Id == Reader.Book);
        if (idx < 0) return;
        var ch = (int)Reader.ChapterNumber + dir;
        if (ch >= 1 && ch <= _books[idx].Chapters)
            Reader.ShowChapter(Reader.Book, (uint)ch);
    }

    private void SyncNav()
    {
        int idx = _books.FindIndex(b => b.Id == Reader.Book);
        _guard = true;
        if (idx >= 0)
        {
            _bookBox.SelectedIndex = idx;
            _chapterBox.Maximum = _books[idx].Chapters;
        }
        _chapterBox.Value = Reader.ChapterNumber;
        _guard = false;
    }

    /// Active accent + add/close visibility for the current pane count.
    public void SetChrome(bool active, int paneCount)
    {
        _accent.Visibility = active && paneCount > 1 ? Visibility.Visible : Visibility.Collapsed;
        _add.Visibility = paneCount < 3 ? Visibility.Visible : Visibility.Collapsed;
        _close.Visibility = paneCount > 1 ? Visibility.Visible : Visibility.Collapsed;
    }

    /// Re-theme the pane chrome after a palette change (Tier 0 #5).
    public void ApplyTheme()
    {
        _nav.Background = new SolidColorBrush(Palette.PaneNavBg);
        _accent.Background = new SolidColorBrush(Palette.Gold);
    }

    public void Dispose() => Reader.Dispose();
}
