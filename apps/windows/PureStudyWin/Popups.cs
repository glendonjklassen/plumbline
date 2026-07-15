// View-only popup windows: the chord/arc "Map" (book-to-book weave density),
// the constellation (weave-library overview), and the concept map (radial
// neighbours + dispersion strip). All close on Esc or on losing focus, like
// the GTK popups (close_on_defocus). Geometry per docs/FEATURE-MANIFEST.md.

using System.Numerics;
using Microsoft.Graphics.Canvas;
using Microsoft.Graphics.Canvas.Geometry;
using Microsoft.Graphics.Canvas.Text;
using Microsoft.Graphics.Canvas.UI.Xaml;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Input;
using PureStudy;
using Windows.UI;

namespace PureStudyWin;

public static class Popups
{
    private static readonly Color PopupPaper = Color.FromArgb(255, 242, 238, 230);

    private static Window Shell(string title, int w, int h, UIElement content)
    {
        var win = new Window { Title = title };
        win.AppWindow.Resize(new Windows.Graphics.SizeInt32(w, h));
        var root = new Grid
        {
            RequestedTheme = ElementTheme.Light,
            // No auto "Esc" key-tip tooltips over the canvas.
            KeyboardAcceleratorPlacementMode = KeyboardAcceleratorPlacementMode.Hidden,
        };
        root.Children.Add(content);
        win.Content = root;
        var esc = new KeyboardAccelerator { Key = Windows.System.VirtualKey.Escape };
        esc.Invoked += (_, e) => { win.Close(); e.Handled = true; };
        root.KeyboardAccelerators.Add(esc);
        bool shown = false;
        win.Activated += (_, e) =>
        {
            if (e.WindowActivationState == WindowActivationState.Deactivated)
            {
                if (shown) win.Close();
            }
            else shown = true;
        };
        win.Activate();
        return win;
    }

    private static (string book, uint ch, uint v)? ParseFull(string refKey)
    {
        int sp = refKey.LastIndexOf(' ');
        if (sp < 0) return null;
        var cv = refKey[(sp + 1)..].Split(':');
        if (cv.Length < 2 || !uint.TryParse(cv[0], out var ch) || !uint.TryParse(cv[1], out var v))
            return null;
        return (refKey[..sp], ch, v);
    }

    // ── chord / arc map ────────────────────────────────────────────────────

    public static void ChordMap(List<LinkPair> links, List<TocBook> books, Action<string> pick)
    {
        var canvas = new CanvasControl { ClearColor = PopupPaper };
        Window? win = null;
        Windows.Foundation.Point pointer = default;
        (int ia, int ib)? hovered = null;

        int Order(string book) => books.FindIndex(b => b.Id == book);

        // Canon-ordered book-pair counts over the deduped pairs.
        var counts = new Dictionary<(int, int), uint>();
        foreach (var l in links)
        {
            int ia = Order(l.BookA), ib = Order(l.BookB);
            if (ia < 0 || ib < 0) continue;
            var key = ia <= ib ? (ia, ib) : (ib, ia);
            counts[key] = counts.GetValueOrDefault(key) + 1;
        }
        uint max = counts.Count > 0 ? counts.Values.Max() : 1;

        (float x1, float x2, float apex, float y0) Arc((int ia, int ib) k, float w, float h)
        {
            float y0 = h - 26;
            float x1 = (k.ia + 0.5f) / 66f * w, x2 = (k.ib + 0.5f) / 66f * w;
            // Use the whole canvas at any window size: the longest span arcs
            // to ~the top; short spans stay low. (A fixed cap bottom-aligns
            // everything in a tall window.)
            float span = Math.Abs(x2 - x1) / Math.Max(1, w);
            float apex = 24 + (y0 - 74) * (float)Math.Pow(span, 0.75);
            return (x1, x2, apex, y0);
        }

        (int, int)? HitArc(float w, float h)
        {
            var pt = new Vector2((float)pointer.X, (float)pointer.Y);
            (int, int)? best = null;
            float bestD = 7f;
            foreach (var (k, _) in counts)
            {
                var (x1, x2, apex, y0) = Arc(k, w, h);
                var p0 = new Vector2(x1, y0);
                var p1 = new Vector2(x1, y0 - apex);
                var p2 = new Vector2(x2, y0 - apex);
                var p3 = new Vector2(x2, y0);
                for (int i = 0; i <= 18; i++)
                {
                    float t = i / 18f, u = 1 - t;
                    var q = u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3;
                    float d = Vector2.Distance(q, pt);
                    if (d < bestD) { bestD = d; best = k; }
                }
            }
            return best;
        }

        canvas.Draw += (s, args) =>
        {
            var ds = args.DrawingSession;
            float w = (float)s.ActualWidth, h = (float)s.ActualHeight;
            float y0 = h - 26;

            using var labelFmt = new CanvasTextFormat { FontSize = 10 };
            for (int i = 0; i < CanonStrip.Segments.Length; i++)
            {
                var seg = CanonStrip.Segments[i];
                float x0 = seg.First / 66f * w, x1 = (seg.Last + 1) / 66f * w;
                if (i % 2 == 1) ds.FillRectangle(x0, 0, x1 - x0, y0, Color.FromArgb(10, 0, 0, 0));
                ds.DrawText(seg.Name, new Vector2(x0 + 3, y0 + 6),
                    Color.FromArgb(230, 89, 77, 56), labelFmt);
            }
            // Book ticks anchor the ribbon feet.
            for (int b = 0; b <= 66; b++)
            {
                float tx = b / 66f * w;
                ds.DrawLine(tx, y0, tx, y0 - 4, Color.FromArgb(60, 89, 77, 56), 1f);
            }
            ds.DrawLine(0, y0, w, y0, Color.FromArgb(128, 158, 125, 56), 1.5f);
            float seam = CanonStrip.OtNtDivide / 66f * w;
            ds.DrawLine(seam, 0, seam, y0, Color.FromArgb(128, 102, 77, 51), 1f);

            Color ArcColor((int ia, int ib) k, float frac, bool hot)
            {
                bool aOt = k.ia < CanonStrip.OtNtDivide, bOt = k.ib < CanonStrip.OtNtDivide;
                var (r, g, b2) = aOt && bOt ? (0.72f, 0.57f, 0.24f)
                    : !aOt && !bOt ? (0.30f, 0.53f, 0.78f)
                    : (0.58f, 0.38f, 0.70f);
                float alpha = hot ? 0.95f : Math.Min(0.25f + 0.45f * frac + (aOt != bOt ? 0.06f : 0f), 0.75f);
                return Color.FromArgb((byte)(alpha * 255),
                    (byte)(r * 255), (byte)(g * 255), (byte)(b2 * 255));
            }

            foreach (var (key, cnt) in counts.OrderBy(kv => kv.Value))
            {
                (int ia, int ib) k = key;
                float frac = (float)cnt / max;
                var (x1, x2, apex, _) = Arc(k, w, h);
                var color = ArcColor(k, frac, hovered == k);
                float width = 1.5f + 8f * frac + (hovered == k ? 1f : 0f);

                if (k.ia == k.ib)
                {
                    ds.DrawCircle(x1, y0 - 8, 8, color, Math.Max(1.5f, width * 0.6f));
                    continue;
                }
                using var path = new CanvasPathBuilder(s);
                path.BeginFigure(x1, y0);
                path.AddCubicBezier(new Vector2(x1, y0 - apex),
                    new Vector2(x2, y0 - apex), new Vector2(x2, y0));
                path.EndFigure(CanvasFigureLoop.Open);
                using var geo = CanvasGeometry.CreatePath(path);
                ds.DrawGeometry(geo, color, width);
            }

            // Legend.
            using var legendFmt = new CanvasTextFormat { FontSize = 11 };
            void LegendDot(float x, Color c, string label)
            {
                ds.FillCircle(x, 14, 4, c);
                ds.DrawText(label, new Vector2(x + 8, 7), Color.FromArgb(220, 89, 77, 56), legendFmt);
            }
            LegendDot(12, Color.FromArgb(220, 184, 145, 61), "OT ↔ OT");
            LegendDot(92, Color.FromArgb(220, 77, 135, 199), "NT ↔ NT");
            LegendDot(172, Color.FromArgb(220, 148, 97, 179), "OT ↔ NT");
            ds.DrawText("heavier = more links · click a book to open it",
                new Vector2(262, 7), Color.FromArgb(160, 89, 77, 56), legendFmt);

            // Hover: name the pair + count.
            if (hovered is { } hk && counts.TryGetValue(hk, out var hc))
            {
                var text = $"{books[hk.ia].Name} ↔ {books[hk.ib].Name} · {hc} link{(hc == 1 ? "" : "s")}";
                using var fmt = new CanvasTextFormat { FontSize = 11.5f };
                using var tl = new CanvasTextLayout(s, text, fmt, 1e6f, 1e6f);
                float tw = (float)tl.LayoutBounds.Width + 12, th = (float)tl.LayoutBounds.Height + 8;
                float tx = Math.Clamp((float)pointer.X + 10, 0, w - tw);
                float ty = Math.Max(0, (float)pointer.Y - th - 8);
                ds.FillRectangle(tx, ty, tw, th, Color.FromArgb(245, 23, 26, 28));
                ds.DrawTextLayout(tl, tx + 6, ty + 4, Color.FromArgb(255, 235, 230, 222));
            }
        };
        canvas.PointerMoved += (_, e) =>
        {
            pointer = e.GetCurrentPoint(canvas).Position;
            var hit = HitArc((float)canvas.ActualWidth, (float)canvas.ActualHeight);
            if (hit != hovered) { hovered = hit; }
            canvas.Invalidate();
        };
        canvas.PointerReleased += (s, e) =>
        {
            var x = e.GetCurrentPoint(canvas).Position.X;
            int idx = Math.Clamp((int)(x / Math.Max(1, canvas.ActualWidth) * 66), 0, 65);
            pick(books[idx].Id);
            win?.Close();
        };

        win = Shell("Weave map — how strongly each pair of books is woven together",
            1000, 380, canvas);
    }

    // ── constellation ──────────────────────────────────────────────────────

    private const int Lanes = 18;
    private const float ConstGutter = 150f, PlotLeft = 162f, ConstTopPad = 18f;

    private static readonly (float r, float g, float b)[] LaneColors =
    {
        (210, 180, 110), (127, 180, 230), (143, 184, 138), (217, 140, 140),
        (184, 156, 214), (150, 194, 190), (214, 170, 128),
    };

    public static void Constellation(StudyEngine engine, WeaveLib lib, List<TocBook> books,
        Action<string, uint, string?> navigate, Action<int> compare)
    {
        Window? win = null;
        var canvas = new CanvasControl { ClearColor = PopupPaper };
        var caption = new TextBlock
        {
            FontSize = 12,
            Foreground = new Microsoft.UI.Xaml.Media.SolidColorBrush(Palette.InkFaded),
            VerticalAlignment = VerticalAlignment.Center,
        };
        int page = 0;
        var pins = new HashSet<int>();          // weave library indices
        (int lane, int link, bool aEnd)? hoverNode = null;
        (int lane, int link)? hoverEdge = null;
        Windows.Foundation.Point pointer = default;

        int Order(string book) => books.FindIndex(b => b.Id == book);
        int ChapterCount(string book) => books.FirstOrDefault(b => b.Id == book)?.Chapters ?? 1;

        var usable = lib.Weaves
            .Select(w => (weave: w, links: w.Links.Where(l => l.Resolved).ToList()))
            .Where(t => t.links.Count > 0)
            .OrderByDescending(t => t.links.Count)
            .ToList();

        // Degree of every verse across the whole library (node sizing).
        var degrees = new Dictionary<string, int>();
        foreach (var (_, ls) in usable)
            foreach (var l in ls)
            {
                degrees[l.A] = degrees.GetValueOrDefault(l.A) + 1;
                degrees[l.B] = degrees.GetValueOrDefault(l.B) + 1;
            }
        int maxDeg = degrees.Count > 0 ? degrees.Values.Max() : 1;

        List<(Weave1 weave, List<WeaveLink1> links, bool pinned)> Visible()
        {
            var pinned = usable.Where(t => pins.Contains(t.weave.Index)).ToList();
            var free = usable.Where(t => !pins.Contains(t.weave.Index)).ToList();
            int freeLanes = Math.Max(0, Lanes - pinned.Count);
            int maxPage = freeLanes > 0 ? Math.Max(0, (free.Count - 1) / freeLanes) : 0;
            page = Math.Clamp(page, 0, maxPage);
            var shown = pinned.Select(t => (t.weave, t.links, true)).ToList();
            if (freeLanes > 0)
                shown.AddRange(free.Skip(page * freeLanes).Take(freeLanes)
                    .Select(t => (t.weave, t.links, false)));

            int lo = freeLanes > 0 ? page * freeLanes + 1 : 0;
            int hi = Math.Min(free.Count, (page + 1) * freeLanes);
            caption.Text =
                (pinned.Count > 0 ? $"{pinned.Count} pinned · " : "") +
                (freeLanes == 0 ? "all lanes pinned — unpin one to page"
                    : free.Count == 0 ? "no free weaves"
                    : $"weaves {lo}–{hi} of {free.Count} · largest first · click the ▪ to pin a lane");
            return shown;
        }

        // Reserve a little bottom margin so the 18th lane never clips at the
        // canvas edge.
        float LaneH(float h) => (h - ConstTopPad - 10) / Lanes;

        (float x, float y)? NodePos(string refKey, int lane, float w, float h)
        {
            if (ParseFull(refKey) is not { } r) return null;
            int order = Order(r.book);
            if (order < 0) return null;
            float frac = (order + (r.ch - 1f) / Math.Max(1, ChapterCount(r.book))) / 66f;
            float laneH = LaneH(h);
            float laneTop = ConstTopPad + lane * laneH;
            float cy = laneTop + laneH / 2;
            float jitter = ((r.ch * 3 + r.v) % 7 - 3) * laneH * 0.12f;
            float y = Math.Clamp(cy + jitter, laneTop + 5, laneTop + laneH - 5);
            return (PlotLeft + frac * (w - PlotLeft), y);
        }

        canvas.Draw += (s, args) =>
        {
            var ds = args.DrawingSession;
            float w = (float)s.ActualWidth, h = (float)s.ActualHeight;
            var shown = Visible();
            float laneH = LaneH(h);

            using var nameFmt = new CanvasTextFormat { FontSize = 10.5f };
            using var rulerFmt = new CanvasTextFormat { FontSize = 10f };

            for (int i = 0; i < Lanes; i++)
                if (i % 2 == 0)
                    ds.FillRectangle(0, ConstTopPad + i * laneH, w, laneH, Color.FromArgb(8, 0, 0, 0));

            foreach (var seg in CanonStrip.Segments)
            {
                float x = PlotLeft + seg.First / 66f * (w - PlotLeft);
                ds.DrawLine(x, ConstTopPad, x, h, Color.FromArgb(26, 0, 0, 0), 1f);
                ds.DrawText(seg.Name, new Vector2(x + 2, 2), Color.FromArgb(180, 89, 77, 56), rulerFmt);
            }
            float seam = PlotLeft + CanonStrip.OtNtDivide / 66f * (w - PlotLeft);
            ds.DrawLine(seam, ConstTopPad, seam, h, Color.FromArgb(153, 158, 125, 56), 1f);

            for (int lane = 0; lane < shown.Count; lane++)
            {
                var (weave, links, pinned) = shown[lane];
                var lc = LaneColors[lane % LaneColors.Length];
                var edge = Color.FromArgb(128,
                    (byte)(lc.r * 0.72f), (byte)(lc.g * 0.72f), (byte)(lc.b * 0.72f));
                var node = Color.FromArgb(230,
                    (byte)(lc.r * 0.72f), (byte)(lc.g * 0.72f), (byte)(lc.b * 0.72f));

                float cy = ConstTopPad + lane * laneH + laneH / 2;
                if (pinned) ds.FillRectangle(6, cy - 4, 8, 8, Palette.Gold);
                else ds.DrawRectangle(6.5f, cy - 3.5f, 7, 7, Color.FromArgb(153, 100, 100, 100), 1f);
                var name = weave.Name.Length > 22 ? weave.Name[..22] : weave.Name;
                ds.DrawText(name, new Vector2(18, cy - 7),
                    pinned ? Color.FromArgb(255, 140, 107, 38) : Color.FromArgb(255, 89, 84, 77),
                    nameFmt);

                foreach (var l in links)
                {
                    var pa = NodePos(l.A, lane, w, h);
                    var pb = NodePos(l.B, lane, w, h);
                    if (pa is null || pb is null) continue;
                    float dx = pb.Value.x - pa.Value.x;
                    using var path = new CanvasPathBuilder(s);
                    path.BeginFigure(pa.Value.x, pa.Value.y);
                    path.AddCubicBezier(
                        new Vector2(pa.Value.x + dx * 0.4f, pa.Value.y),
                        new Vector2(pb.Value.x - dx * 0.4f, pb.Value.y),
                        new Vector2(pb.Value.x, pb.Value.y));
                    path.EndFigure(CanvasFigureLoop.Open);
                    using var geo = CanvasGeometry.CreatePath(path);
                    ds.DrawGeometry(geo, edge, 1f);
                }
                foreach (var l in links)
                    foreach (var (refKey, _) in new[] { (l.A, 0), (l.B, 1) })
                    {
                        var p = NodePos(refKey, lane, w, h);
                        if (p is null) continue;
                        float half = 1.4f + 2.4f * degrees.GetValueOrDefault(refKey) / (float)maxDeg;
                        ds.FillRectangle(p.Value.x - half, p.Value.y - half, half * 2, half * 2, node);
                    }
            }

            // Hover tooltip: "verse · weave" in a dark box.
            if (HitNode(shown, w, h) is { } hn)
            {
                var l = shown[hn.lane].links[hn.link];
                var refKey = hn.aEnd ? l.A : l.B;
                var disp = hn.aEnd ? l.ADisplay : l.BDisplay;
                var text = $"{disp} · {shown[hn.lane].weave.Name}";
                using var fmt = new CanvasTextFormat { FontSize = 11 };
                using var tl = new CanvasTextLayout(s, text, fmt, 1e6f, 1e6f);
                float tw = (float)tl.LayoutBounds.Width + 12, th = (float)tl.LayoutBounds.Height + 8;
                float tx = Math.Clamp((float)pointer.X + 8, 0, w - tw);
                float ty = Math.Max(0, (float)pointer.Y - th - 6);
                ds.FillRectangle(tx, ty, tw, th, Color.FromArgb(245, 23, 26, 28));
                ds.DrawTextLayout(tl, tx + 6, ty + 4, Color.FromArgb(255, 235, 230, 222));
            }
        };

        (int lane, int link, bool aEnd)? HitNode(
            List<(Weave1 weave, List<WeaveLink1> links, bool pinned)> shown, float w, float h)
        {
            (int, int, bool)? best = null;
            float bestD = float.MaxValue;
            for (int lane = 0; lane < shown.Count; lane++)
                for (int li = 0; li < shown[lane].links.Count; li++)
                {
                    var l = shown[lane].links[li];
                    foreach (var (refKey, isA) in new[] { (l.A, true), (l.B, false) })
                    {
                        if (NodePos(refKey, lane, w, h) is not { } p) continue;
                        float half = 1.4f + 2.4f * degrees.GetValueOrDefault(refKey) / (float)maxDeg;
                        float d = Vector2.Distance(new Vector2(p.x, p.y),
                            new Vector2((float)pointer.X, (float)pointer.Y));
                        if (d <= half + 4 && d < bestD) { bestD = d; best = (lane, li, isA); }
                    }
                }
            return best;
        }

        (int lane, int link)? HitEdge(
            List<(Weave1 weave, List<WeaveLink1> links, bool pinned)> shown, float w, float h)
        {
            var pt = new Vector2((float)pointer.X, (float)pointer.Y);
            for (int lane = 0; lane < shown.Count; lane++)
                for (int li = 0; li < shown[lane].links.Count; li++)
                {
                    var l = shown[lane].links[li];
                    if (NodePos(l.A, lane, w, h) is not { } pa ||
                        NodePos(l.B, lane, w, h) is not { } pb) continue;
                    float dx = pb.x - pa.x;
                    var p0 = new Vector2(pa.x, pa.y);
                    var p1 = new Vector2(pa.x + dx * 0.4f, pa.y);
                    var p2 = new Vector2(pb.x - dx * 0.4f, pb.y);
                    var p3 = new Vector2(pb.x, pb.y);
                    for (int i = 0; i <= 18; i++)
                    {
                        float t = i / 18f, u = 1 - t;
                        var q = u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3;
                        if (Vector2.Distance(q, pt) <= 5f) return (lane, li);
                    }
                }
            return null;
        }

        canvas.PointerMoved += (_, e) =>
        {
            pointer = e.GetCurrentPoint(canvas).Position;
            canvas.Invalidate();
        };
        canvas.PointerReleased += (_, e) =>
        {
            pointer = e.GetCurrentPoint(canvas).Position;
            float w = (float)canvas.ActualWidth, h = (float)canvas.ActualHeight;
            var shown = Visible();
            // Priority: node > edge > pin gutter.
            if (HitNode(shown, w, h) is { } hn)
            {
                var l = shown[hn.lane].links[hn.link];
                var refKey = hn.aEnd ? l.A : l.B;
                if (ParseFull(refKey) is { } r) navigate(r.book, r.ch, refKey);
                return;
            }
            if (HitEdge(shown, w, h) is { } he)
            {
                compare(shown[he.lane].weave.Index);
                win?.Close();
                return;
            }
            if (pointer.X < ConstGutter)
            {
                int lane = (int)((pointer.Y - ConstTopPad) / LaneH(h));
                if (lane >= 0 && lane < shown.Count)
                {
                    int idx = shown[lane].weave.Index;
                    if (!pins.Remove(idx)) pins.Add(idx);
                    canvas.Invalidate();
                }
            }
        };

        var prev = new Button { Content = "‹ prev" };
        var next = new Button { Content = "next ›" };
        prev.Click += (_, _) => { page--; canvas.Invalidate(); };
        next.Click += (_, _) => { page++; canvas.Invalidate(); };

        var bar = new StackPanel
        {
            Orientation = Orientation.Horizontal, Spacing = 8,
            Padding = new Thickness(8, 6, 8, 6),
        };
        bar.Children.Add(prev);
        bar.Children.Add(next);
        bar.Children.Add(caption);

        var root = new Grid();
        root.RowDefinitions.Add(new RowDefinition { Height = GridLength.Auto });
        root.RowDefinitions.Add(new RowDefinition { Height = new GridLength(1, GridUnitType.Star) });
        Grid.SetRow(bar, 0);
        Grid.SetRow(canvas, 1);
        root.Children.Add(bar);
        root.Children.Add(canvas);

        win = Shell("Constellation — the weave library", 1200, 640, root);
        var left = new KeyboardAccelerator { Key = Windows.System.VirtualKey.Left };
        left.Invoked += (_, e) => { page--; canvas.Invalidate(); e.Handled = true; };
        var right = new KeyboardAccelerator { Key = Windows.System.VirtualKey.Right };
        right.Invoked += (_, e) => { page++; canvas.Invalidate(); e.Handled = true; };
        ((Grid)win.Content).KeyboardAccelerators.Add(left);
        ((Grid)win.Content).KeyboardAccelerators.Add(right);
    }

    // ── concept map ────────────────────────────────────────────────────────

    public static void ConceptMap(StudyEngine engine, string code, bool full)
    {
        var canvas = new CanvasControl { ClearColor = PopupPaper };

        // Neighbours: semantic (embedding) in gold, collocation (community) in
        // green; deduped, semantic wins.
        var spokes = new List<(string code, bool semantic)>();
        if (engine.ConceptNeighboursJson(code, 6) is { } nj)
            foreach (var s in Wire.Parse<ConceptNeighbours>(nj).Near)
                spokes.Add((s.Code, true));
        Concept1? concept = engine.ConceptJson(code) is { } cj ? Wire.Parse<Concept1>(cj) : null;
        if (concept is not null)
            foreach (var c in concept.Community.Take(6))
                if (spokes.All(s => s.code != c))
                    spokes.Add((c, false));

        string Label(string c)
        {
            var gloss = engine.Gloss(c);
            var lemma = engine.StrongsJson(c) is { } sj ? Wire.Parse<StrongsEntry>(sj).Lemma : null;
            return (gloss, lemma) switch
            {
                (not null, not null) => $"{gloss}\n{lemma}",
                (not null, null) => gloss!,
                (null, not null) => lemma!,
                _ => c,
            };
        }

        canvas.Draw += (s, args) =>
        {
            var ds = args.DrawingSession;
            float w = (float)s.ActualWidth, h = (float)s.ActualHeight;
            float stripH = 40, mapH = h - stripH;
            float cx = w / 2, cy = mapH / 2;
            float radius = Math.Max(Math.Min(w, mapH) / 2 - 95, 40);

            // Alignment-specific formats: a CanvasTextLayout positions text
            // inside its requested box, so the draw origin must be the BOX
            // corner, not the measured text edge — mixing those shifts labels
            // by half the box width.
            const float Box = 220f;
            using var fmtLeft = new CanvasTextFormat { FontSize = 12 };
            using var fmtRight = new CanvasTextFormat
            {
                FontSize = 12, HorizontalAlignment = CanvasHorizontalAlignment.Right,
            };
            using var fmtCentre = new CanvasTextFormat
            {
                FontSize = 12, HorizontalAlignment = CanvasHorizontalAlignment.Center,
            };
            using var centreFmt = new CanvasTextFormat
            {
                FontSize = 15, HorizontalAlignment = CanvasHorizontalAlignment.Center,
            };

            for (int i = 0; i < spokes.Count; i++)
            {
                double angle = 2 * Math.PI * i / Math.Max(1, spokes.Count) - Math.PI / 2;
                float ca = (float)Math.Cos(angle), sa = (float)Math.Sin(angle);
                float nx = cx + radius * ca;
                float ny = cy + radius * sa;
                var stroke = spokes[i].semantic
                    ? Color.FromArgb(128, 158, 125, 56)
                    : Color.FromArgb(128, 107, 140, 102);
                ds.DrawLine(cx, cy, nx, ny, stroke, 1.4f);
                ds.FillCircle(nx, ny, 3, Color.FromArgb(230, 158, 125, 56));

                // Sides hang off the node left/right; top and bottom sit
                // above/below it, per-line centred.
                var text = Label(spokes[i].code);
                var fmt2 = ca > 0.35f ? fmtLeft : ca < -0.35f ? fmtRight : fmtCentre;
                using var tl = new CanvasTextLayout(s, text, fmt2, Box, 100);
                float th = (float)tl.LayoutBounds.Height;
                float lx, ly;
                if (ca > 0.35f) { lx = nx + 9; ly = ny - th / 2; }
                else if (ca < -0.35f) { lx = nx - 9 - Box; ly = ny - th / 2; }
                else if (sa < 0) { lx = nx - Box / 2; ly = ny - 10 - th; }
                else { lx = nx - Box / 2; ly = ny + 9; }
                ly = Math.Clamp(ly, 2, mapH - th - 2);
                ds.DrawTextLayout(tl, lx, ly, Palette.Ink);
            }

            ds.FillCircle(cx, cy, 5, Palette.Gold);
            using (var ctl = new CanvasTextLayout(s, Label(code), centreFmt, 260, 100))
            {
                float th = (float)ctl.LayoutBounds.Height;
                ds.DrawTextLayout(ctl, cx - 130, cy - 14 - th, Palette.Ink);
            }

            // Dispersion strip: where across the 66 books this concept occurs.
            if (concept is not null && concept.ByBook.Count > 0)
            {
                float y0 = h - stripH;
                uint bmax = concept.ByBook.Values.Max();
                ds.FillRectangle(0, y0, w, stripH, Color.FromArgb(10, 0, 0, 0));
                int bi = 0;
                foreach (var book in BookOrder)
                {
                    if (concept.ByBook.TryGetValue(book, out var cnt) && cnt > 0)
                    {
                        float alpha = 0.15f + 0.75f * cnt / bmax;
                        float x0 = bi / 66f * w, x1 = (bi + 1) / 66f * w;
                        ds.FillRectangle(x0, y0, x1 - x0, stripH,
                            Color.FromArgb((byte)(alpha * 255), 158, 125, 56));
                    }
                    bi++;
                }
                float seam = CanonStrip.OtNtDivide / 66f * w;
                ds.DrawLine(seam, y0, seam, h, Color.FromArgb(128, 102, 77, 51), 1f);
            }
        };

        Shell($"Concept map — {code}", 720, 560, canvas);
    }

    /// OSIS ids in canon order (66) — for the dispersion strip cells.
    public static readonly string[] BookOrder =
    {
        "Gen","Exod","Lev","Num","Deut","Josh","Judg","Ruth","1Sam","2Sam","1Kgs","2Kgs",
        "1Chr","2Chr","Ezra","Neh","Esth","Job","Ps","Prov","Eccl","Song","Isa","Jer",
        "Lam","Ezek","Dan","Hos","Joel","Amos","Obad","Jonah","Mic","Nah","Hab","Zeph",
        "Hag","Zech","Mal","Matt","Mark","Luke","John","Acts","Rom","1Cor","2Cor","Gal",
        "Eph","Phil","Col","1Thess","2Thess","1Tim","2Tim","Titus","Phlm","Heb","Jas",
        "1Pet","2Pet","1John","2John","3John","Jude","Rev",
    };
}
