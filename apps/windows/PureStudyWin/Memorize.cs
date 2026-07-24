// The memorization UI (Tier 2 #15): the SM-2 review/drill window, the canon
// coverage map, and the activity heatmap — the WinUI mirror of the GTK shell's
// show_memorize / draw_mem_coverage / draw_mem_activity (apps/desktop M:3044,
// M:3282, M:3362). All study logic lives across the ABI; this file is
// orchestration + paint only. Unlike the view-only Popups (which close on
// defocus), these are plain windows — the review window takes typed input — so
// they close on Esc / the window chrome, matching the GTK memorize windows.

using System.Numerics;
using Microsoft.Graphics.Canvas.Text;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using PureStudy;
using Windows.UI;

namespace PureStudyWin;

public static class Memorize
{
    // ── shared frame ─────────────────────────────────────────────────────────

    /// A plain, theme-aware window (paper background, Esc closes). Not
    /// close-on-defocus like Popups: the review window is interactive, and GTK's
    /// memorize windows are all ordinary transient windows.
    private static Window Frame(string title, int w, int h, UIElement content)
    {
        var win = new Window { Title = title };
        win.AppWindow.Resize(new Windows.Graphics.SizeInt32(w, h));
        var root = new Grid
        {
            RequestedTheme = Palette.Dark ? ElementTheme.Dark : ElementTheme.Light,
            Background = new SolidColorBrush(Palette.Paper),
            // No key-tip tooltips lingering over the canvas / prompt.
            KeyboardAcceleratorPlacementMode = KeyboardAcceleratorPlacementMode.Hidden,
        };
        root.Children.Add(content);
        win.Content = root;
        var esc = new KeyboardAccelerator { Key = Windows.System.VirtualKey.Escape };
        esc.Invoked += (_, e) => { win.Close(); e.Handled = true; };
        root.KeyboardAccelerators.Add(esc);
        win.Activate();
        return win;
    }

    // ── (a) the SM-2 review / drill window (GTK show_memorize) ────────────────

    /// Step the verses due now, drilling each (first-letter · progressive
    /// blank-out · typed recall), then grade with SM-2 (Again/Hard/Good/Easy).
    /// `now` supplies the UTC timestamp (the shell's Now() helper).
    public static void Review(StudyEngine engine, Func<string> now)
    {
        var due = engine.MemoryDueJson(now()) is { } dj
            ? Wire.Parse<MemoryDue>(dj).Refs
            : new List<string>();

        if (due.Count == 0)
        {
            var empty = new TextBlock
            {
                TextWrapping = TextWrapping.Wrap,
                Margin = new Thickness(22, 18, 22, 18),
                VerticalAlignment = VerticalAlignment.Center,
                Foreground = new SolidColorBrush(Palette.Ink),
                Text = "Nothing due for review.\n\n"
                     + "Right-click a verse → “Memorize this verse” to start a card.",
            };
            Frame("Memorize", 720, 520, empty);
            return;
        }

        // Prompt modes mirror GTK's Prompt enum: 0 = first-letters skeleton,
        // 1 = progressively blanked at `level`, 2 = full text (Reveal).
        int idx = 0, level = 0, mode = 0;
        bool loading = false;
        string curRef = due[0];
        Window? win = null;

        var caption = new TextBlock
        {
            FontSize = 12,
            Foreground = new SolidColorBrush(Palette.InkFaded),
        };
        var refLabel = new TextBlock
        {
            FontSize = 20,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
            Foreground = new SolidColorBrush(Palette.Ink),
        };
        var prompt = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            IsTextSelectionEnabled = true,
            FontSize = 18,
            Foreground = new SolidColorBrush(Palette.Ink),
            VerticalAlignment = VerticalAlignment.Top,
        };
        var promptScroll = new ScrollViewer
        {
            Content = prompt,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
        };

        // Prompt-mode controls: first-letters · a blank-out slider · reveal.
        var flBtn = new Button { Content = "First letters" };
        var slider = new Slider
        {
            Minimum = 0,
            Maximum = 4,                 // updated from the drill's MaxLevel below
            StepFrequency = 1,
            SnapsTo = SliderSnapsTo.StepValues,
            VerticalAlignment = VerticalAlignment.Center,
        };
        ToolTipService.SetToolTip(slider, "Blank out words progressively");
        var revealBtn = new Button { Content = "Reveal" };
        var controls = new Grid { ColumnSpacing = 8, VerticalAlignment = VerticalAlignment.Center };
        controls.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        controls.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        controls.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(flBtn, 0);
        Grid.SetColumn(slider, 1);
        Grid.SetColumn(revealBtn, 2);
        controls.Children.Add(flBtn);
        controls.Children.Add(slider);
        controls.Children.Add(revealBtn);

        // Typed recall.
        var recallBox = new TextBox
        {
            PlaceholderText = "Type the verse from memory, then Check",
        };
        var checkBtn = new Button { Content = "Check" };
        var recall = new Grid { ColumnSpacing = 8 };
        recall.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        recall.ColumnDefinitions.Add(new ColumnDefinition { Width = GridLength.Auto });
        Grid.SetColumn(recallBox, 0);
        Grid.SetColumn(checkBtn, 1);
        recall.Children.Add(recallBox);
        recall.Children.Add(checkBtn);
        var result = new TextBlock
        {
            TextWrapping = TextWrapping.Wrap,
            Foreground = new SolidColorBrush(Palette.Ink),
        };

        // Grade buttons (SM-2's four): Again resets to relearning, the rest grow
        // the interval. Easy takes the accent style (GTK's suggested-action).
        var again = new Button { Content = "Again", HorizontalAlignment = HorizontalAlignment.Stretch };
        var hard = new Button { Content = "Hard", HorizontalAlignment = HorizontalAlignment.Stretch };
        var good = new Button { Content = "Good", HorizontalAlignment = HorizontalAlignment.Stretch };
        var easy = new Button { Content = "Easy", HorizontalAlignment = HorizontalAlignment.Stretch };
        if (Application.Current.Resources.TryGetValue("AccentButtonStyle", out var accent)
            && accent is Style accentStyle)
            easy.Style = accentStyle;
        var grades = new Grid { ColumnSpacing = 8 };
        for (int i = 0; i < 4; i++)
            grades.ColumnDefinitions.Add(new ColumnDefinition { Width = new GridLength(1, GridUnitType.Star) });
        var gradeBtns = new[] { again, hard, good, easy };
        for (int i = 0; i < gradeBtns.Length; i++)
        {
            Grid.SetColumn(gradeBtns[i], i);
            grades.Children.Add(gradeBtns[i]);
        }

        var grid = new Grid { Padding = new Thickness(22, 18, 22, 18), RowSpacing = 12 };
        foreach (var hgt in new[]
        {
            GridLength.Auto, GridLength.Auto, new GridLength(1, GridUnitType.Star),
            GridLength.Auto, GridLength.Auto, GridLength.Auto, GridLength.Auto,
        })
            grid.RowDefinitions.Add(new RowDefinition { Height = hgt });
        var rows = new FrameworkElement[] { caption, refLabel, promptScroll, controls, recall, result, grades };
        for (int i = 0; i < rows.Length; i++)
        {
            Grid.SetRow(rows[i], i);
            grid.Children.Add(rows[i]);
        }

        // Repaint the prompt for the current verse + mode. Only `Blanked`
        // depends on `level`, but the drill also carries FirstLetters/Text and
        // the (constant) MaxLevel, so one call feeds every mode.
        void RenderPrompt()
        {
            if (engine.MemoryDrillJson(curRef, (uint)level) is not { } drillJson)
            {
                prompt.Text = "";
                return;
            }
            var d = Wire.Parse<MemoryDrill>(drillJson);
            if (slider.Maximum != d.MaxLevel)
            {
                loading = true;
                slider.Maximum = d.MaxLevel;
                loading = false;
            }
            prompt.Text = mode switch
            {
                2 => d.Text,
                1 => d.Blanked,
                _ => d.FirstLetters,
            };
        }

        // Load the card at `idx`, or close when the queue is exhausted. Resets
        // to the first-letter prompt (GTK's advance sets Prompt::FirstLetters).
        void LoadCard()
        {
            if (idx >= due.Count) { win?.Close(); return; }
            curRef = due[idx];
            loading = true;
            mode = 0;
            level = 0;
            slider.Value = 0;
            caption.Text = $"Card {idx + 1} of {due.Count} due";
            refLabel.Text = engine.VerseJson(curRef) is { } vj
                ? Wire.Parse<VerseData>(vj).Display
                : curRef;
            recallBox.Text = "";
            result.Text = "";
            loading = false;
            RenderPrompt();
        }

        void Advance() { idx++; LoadCard(); }

        void Grade(string g)
        {
            engine.MemoryGrade(curRef, g, now());
            Advance();
        }

        flBtn.Click += (_, _) => { mode = 0; RenderPrompt(); };
        revealBtn.Click += (_, _) => { mode = 2; RenderPrompt(); };
        slider.ValueChanged += (_, _) =>
        {
            if (loading) return;
            mode = 1;
            level = (int)Math.Round(slider.Value);
            RenderPrompt();
        };
        checkBtn.Click += (_, _) =>
        {
            if (engine.MemoryScoreJson(curRef, recallBox.Text) is not { } sj) return;
            var score = Wire.Parse<RecallScore>(sj);
            int pct = (int)Math.Round(score.Accuracy * 100);
            var missed = score.Words.Where(wd => !wd.Ok).Select(wd => wd.Word).ToList();
            result.Text = missed.Count == 0
                ? $"✓ {pct}% — perfect"
                : $"{pct}% — missed: {string.Join(" ", missed)}";
        };
        again.Click += (_, _) => Grade("again");
        hard.Click += (_, _) => Grade("hard");
        good.Click += (_, _) => Grade("good");
        easy.Click += (_, _) => Grade("easy");

        win = Frame("Memorize", 720, 520, grid);
        LoadCard();
    }

    // ── (b) the coverage map (GTK draw_mem_coverage) ─────────────────────────

    /// The canon strip shaded by how much of each book is being memorized and
    /// how well (average mastery), OT/NT seam marked, section labels along the
    /// top — the dispersion visual language reused for memory work. `books` is
    /// the TOC (66 books, canon order); a verse maps to a book by its ref key.
    public static void Coverage(StudyEngine engine, List<TocBook> books, Func<string> now)
    {
        var canvas = new CanvasControl { ClearColor = Palette.Paper };
        canvas.Draw += (s, args) =>
        {
            var ds = args.DrawingSession;
            float w = (float)s.ActualWidth, h = (float)s.ActualHeight;
            ds.Clear(Palette.Paper);
            if (w < 10 || books.Count == 0) return;

            // Per-book: card count + summed mastery score → an average shade.
            var byBook = new Dictionary<string, (int count, double sum)>();
            if (engine.MemoryCoverageJson(now()) is { } cj)
                foreach (var v in Wire.Parse<MemoryCoverage>(cj).Verses)
                {
                    var book = BookOf(v.Ref);
                    if (book is null) continue;
                    double sc = v.Mastery switch
                    {
                        "new" => 0.15,
                        "learning" => 0.40,
                        "young" => 0.70,
                        "mature" => 1.0,
                        _ => 0.15,
                    };
                    var e = byBook.TryGetValue(book, out var cur) ? cur : (count: 0, sum: 0.0);
                    byBook[book] = (e.count + 1, e.sum + sc);
                }

            float nb = books.Count;
            var gold = Palette.Gold;
            const float Top = 26f;
            for (int i = 0; i < books.Count; i++)
            {
                float x0 = i / nb * w, x1 = (i + 1) / nb * w;
                float alpha = byBook.TryGetValue(books[i].Id, out var bb)
                    ? (float)(0.2 + 0.75 * (bb.sum / Math.Max(1, bb.count)))
                    : 0.05f;
                ds.FillRectangle(x0, Top, Math.Max(0.5f, x1 - x0 - 0.5f), h - Top,
                    Color.FromArgb((byte)(alpha * 255), gold.R, gold.G, gold.B));
            }
            // OT/NT seam.
            float dx = Canon.OtNtDivide / nb * w;
            ds.FillRectangle(dx - 0.75f, 0, 1.5f, h, Color.FromArgb(230, gold.R, gold.G, gold.B));
            // Section labels along the top.
            using var labelFmt = new CanvasTextFormat { FontSize = 11 };
            foreach (var seg in Canon.Segments)
            {
                float mid = (seg.First + seg.Last + 1) / 2f / nb * w;
                using var tl = new CanvasTextLayout(s, seg.Label, labelFmt, 1e6f, 1e6f);
                float tw = (float)tl.LayoutBounds.Width;
                ds.DrawTextLayout(tl, Math.Max(1f, mid - tw / 2), 6, Palette.Faded);
            }
        };
        Frame("Memory coverage", 1000, 220, canvas);
    }

    /// The book id (OSIS) of a compact ref key ("Gen 1:7" → "Gen",
    /// "1Cor 13:4" → "1Cor"). Book ids never contain spaces, so the last space
    /// bounds the book — the same split as core's VRef::parse_ref_key.
    private static string? BookOf(string refKey)
    {
        int i = refKey.LastIndexOf(' ');
        return i > 0 ? refKey[..i] : null;
    }

    // ── (c) the activity heatmap (GTK draw_mem_activity) ─────────────────────

    /// Reviews per calendar day, oldest → newest, as columns with the first
    /// and last day labelled — a glance at when the memory work happened.
    public static void Activity(StudyEngine engine)
    {
        var canvas = new CanvasControl { ClearColor = Palette.Paper };
        canvas.Draw += (s, args) =>
        {
            var ds = args.DrawingSession;
            float w = (float)s.ActualWidth, h = (float)s.ActualHeight;
            ds.Clear(Palette.Paper);

            var days = engine.MemoryActivityJson() is { } aj
                ? Wire.Parse<MemoryActivity>(aj).Days
                : new List<DayActivity>();
            var gold = Palette.Gold;
            var faded = Palette.Faded;

            if (days.Count == 0)
            {
                using var msgFmt = new CanvasTextFormat { FontSize = 13 };
                ds.DrawText("No reviews yet — grade some cards in Review due.",
                    new Vector2(24, h / 2 - 8), faded, msgFmt);
                return;
            }

            int max = Math.Max(1, days.Max(d => d.Reviews));
            float n = days.Count;
            float baseline = h - 28;
            float plotH = baseline - 24;
            float gap = (w - 48) / n;
            float barW = Math.Max(Math.Min(gap, 28f), 2f) - 2f;
            for (int i = 0; i < days.Count; i++)
            {
                float x = 24 + i * gap;
                float bh = (float)days[i].Reviews / max * plotH;
                ds.FillRectangle(x, baseline - bh, Math.Max(2f, barW), bh,
                    Color.FromArgb(217, gold.R, gold.G, gold.B));   // α 0.85
            }
            // First + last day labels.
            using var lblFmt = new CanvasTextFormat { FontSize = 10 };
            ds.DrawText(days[0].Day, new Vector2(24, baseline + 6), faded, lblFmt);
            if (days.Count > 1)
            {
                using var tl = new CanvasTextLayout(s, days[^1].Day, lblFmt, 1e6f, 1e6f);
                float tw = (float)tl.LayoutBounds.Width;
                ds.DrawTextLayout(tl, w - 24 - tw, baseline + 6, faded);
            }
        };
        Frame("Memory activity", 720, 280, canvas);
    }
}
