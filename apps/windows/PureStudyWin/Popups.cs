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

    // ── chord / arc map ────────────────────────────────────────────────────

    public static void ChordMap(ChordMapData map, List<TocBook> books, Action<string> pick)
    {
        var canvas = new CanvasControl { ClearColor = PopupPaper };
        Window? win = null;
        Windows.Foundation.Point pointer = default;
        (int ia, int ib)? hovered = null;

        // Canon-ordered book-pair counts + max come folded from the core
        // view-model (pure_engine_chord_map_json) — the shell no longer folds
        // link pairs or re-derives the max (weave::chord_pairs owns it).
        var counts = new Dictionary<(int, int), uint>();
        foreach (var p in map.Pairs) counts[(p.A, p.B)] = p.Count;
        uint max = Math.Max(1u, map.Max);

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
            for (int i = 0; i < Canon.Segments.Length; i++)
            {
                var seg = Canon.Segments[i];
                float x0 = seg.First / 66f * w, x1 = (seg.Last + 1) / 66f * w;
                if (i % 2 == 1) ds.FillRectangle(x0, 0, x1 - x0, y0, Color.FromArgb(10, 0, 0, 0));
                ds.DrawText(seg.Label, new Vector2(x0 + 3, y0 + 6),
                    Color.FromArgb(230, 89, 77, 56), labelFmt);
            }
            // Book ticks anchor the ribbon feet.
            for (int b = 0; b <= 66; b++)
            {
                float tx = b / 66f * w;
                ds.DrawLine(tx, y0, tx, y0 - 4, Color.FromArgb(60, 89, 77, 56), 1f);
            }
            ds.DrawLine(0, y0, w, y0, Color.FromArgb(128, 158, 125, 56), 1.5f);
            float seam = Canon.OtNtDivide / 66f * w;
            ds.DrawLine(seam, 0, seam, y0, Color.FromArgb(128, 102, 77, 51), 1f);

            Color ArcColor((int ia, int ib) k, float frac, bool hot)
            {
                bool aOt = k.ia < Canon.OtNtDivide, bOt = k.ib < Canon.OtNtDivide;
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
    // Lane count (18) lives in the core view-model (lane_capacity); these are
    // the shell's paint-only geometry: the pin gutter width, the plot's left
    // margin, and the top pad.
    private const float ConstGutter = 150f, PlotLeft = 162f, ConstTopPad = 18f;

    private static readonly (float r, float g, float b)[] LaneColors =
    {
        (210, 180, 110), (127, 180, 230), (143, 184, 138), (217, 140, 140),
        (184, 156, 214), (150, 194, 190), (214, 170, 128),
    };

    public static void Constellation(StudyEngine engine,
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
        Windows.Foundation.Point pointer = default;

        // The whole layout — usable filter, largest-first order, per-verse
        // degree, jitter, lane assignment, paging, pins — comes from the core
        // view-model (pure_engine_constellation_json). The shell holds only the
        // transient page + pin set and paints the returned fractions (item 3).
        var model = Wire.Parse<ConstellationData>(engine.ConstellationJson((uint)page, pins)!);

        void Refresh()
        {
            model = Wire.Parse<ConstellationData>(engine.ConstellationJson((uint)page, pins)!);
            page = model.Page;                  // the core clamps it into range
            caption.Text = model.Caption;
            canvas.Invalidate();
        }

        // The only geometry left in the shell: the lane band height (fixed
        // capacity, small bottom margin so the last lane never clips) and the
        // fraction→pixel map for a node/edge endpoint on lane `lane`.
        float LaneH(float h) => (h - ConstTopPad - 10) / Math.Max(1, model.LaneCapacity);
        (float x, float y) NodeXY(float xFrac, float laneFrac, int lane, float w, float h)
        {
            float laneH = LaneH(h);
            return (PlotLeft + xFrac * (w - PlotLeft), ConstTopPad + (lane + laneFrac) * laneH);
        }

        canvas.Draw += (s, args) =>
        {
            var ds = args.DrawingSession;
            float w = (float)s.ActualWidth, h = (float)s.ActualHeight;
            float laneH = LaneH(h);

            using var nameFmt = new CanvasTextFormat { FontSize = 10.5f };
            using var rulerFmt = new CanvasTextFormat { FontSize = 10f };

            for (int i = 0; i < model.LaneCapacity; i++)
                if (i % 2 == 0)
                    ds.FillRectangle(0, ConstTopPad + i * laneH, w, laneH, Color.FromArgb(8, 0, 0, 0));

            foreach (var seg in Canon.Segments)
            {
                float x = PlotLeft + seg.First / 66f * (w - PlotLeft);
                ds.DrawLine(x, ConstTopPad, x, h, Color.FromArgb(26, 0, 0, 0), 1f);
                ds.DrawText(seg.Label, new Vector2(x + 2, 2), Color.FromArgb(180, 89, 77, 56), rulerFmt);
            }
            float seam = PlotLeft + Canon.OtNtDivide / 66f * (w - PlotLeft);
            ds.DrawLine(seam, ConstTopPad, seam, h, Color.FromArgb(153, 158, 125, 56), 1f);

            for (int lane = 0; lane < model.Lanes.Count; lane++)
            {
                var laneData = model.Lanes[lane];
                var lc = LaneColors[lane % LaneColors.Length];
                var edge = Color.FromArgb(128,
                    (byte)(lc.r * 0.72f), (byte)(lc.g * 0.72f), (byte)(lc.b * 0.72f));
                var node = Color.FromArgb(230,
                    (byte)(lc.r * 0.72f), (byte)(lc.g * 0.72f), (byte)(lc.b * 0.72f));

                float cy = ConstTopPad + lane * laneH + laneH / 2;
                if (laneData.Pinned) ds.FillRectangle(6, cy - 4, 8, 8, Palette.Gold);
                else ds.DrawRectangle(6.5f, cy - 3.5f, 7, 7, Color.FromArgb(153, 100, 100, 100), 1f);
                var name = laneData.Name.Length > 22 ? laneData.Name[..22] : laneData.Name;
                ds.DrawText(name, new Vector2(18, cy - 7),
                    laneData.Pinned ? Color.FromArgb(255, 140, 107, 38) : Color.FromArgb(255, 89, 84, 77),
                    nameFmt);

                foreach (var ed in laneData.Edges)
                {
                    var (ax, ay) = NodeXY(ed.AX, ed.ALaneFrac, lane, w, h);
                    var (bx, by) = NodeXY(ed.BX, ed.BLaneFrac, lane, w, h);
                    float dx = bx - ax;
                    using var path = new CanvasPathBuilder(s);
                    path.BeginFigure(ax, ay);
                    path.AddCubicBezier(
                        new Vector2(ax + dx * 0.4f, ay),
                        new Vector2(bx - dx * 0.4f, by),
                        new Vector2(bx, by));
                    path.EndFigure(CanvasFigureLoop.Open);
                    using var geo = CanvasGeometry.CreatePath(path);
                    ds.DrawGeometry(geo, edge, 1f);
                }
                foreach (var n in laneData.Nodes)
                {
                    var (px, py) = NodeXY(n.X, n.LaneFrac, lane, w, h);
                    float half = 1.4f + 2.4f * n.Size;
                    ds.FillRectangle(px - half, py - half, half * 2, half * 2, node);
                }
            }

            // Hover tooltip: "verse · weave" in a dark box.
            if (HitNode(w, h) is { } hn)
            {
                var n = model.Lanes[hn.lane].Nodes[hn.node];
                var text = $"{n.Display} · {model.Lanes[hn.lane].Name}";
                using var fmt = new CanvasTextFormat { FontSize = 11 };
                using var tl = new CanvasTextLayout(s, text, fmt, 1e6f, 1e6f);
                float tw = (float)tl.LayoutBounds.Width + 12, th = (float)tl.LayoutBounds.Height + 8;
                float tx = Math.Clamp((float)pointer.X + 8, 0, w - tw);
                float ty = Math.Max(0, (float)pointer.Y - th - 6);
                ds.FillRectangle(tx, ty, tw, th, Color.FromArgb(245, 23, 26, 28));
                ds.DrawTextLayout(tl, tx + 6, ty + 4, Color.FromArgb(255, 235, 230, 222));
            }
        };

        (int lane, int node)? HitNode(float w, float h)
        {
            (int, int)? best = null;
            float bestD = float.MaxValue;
            for (int lane = 0; lane < model.Lanes.Count; lane++)
            {
                var nodes = model.Lanes[lane].Nodes;
                for (int ni = 0; ni < nodes.Count; ni++)
                {
                    var n = nodes[ni];
                    var (px, py) = NodeXY(n.X, n.LaneFrac, lane, w, h);
                    float half = 1.4f + 2.4f * n.Size;
                    float d = Vector2.Distance(new Vector2(px, py),
                        new Vector2((float)pointer.X, (float)pointer.Y));
                    if (d <= half + 4 && d < bestD) { bestD = d; best = (lane, ni); }
                }
            }
            return best;
        }

        (int lane, int edge)? HitEdge(float w, float h)
        {
            var pt = new Vector2((float)pointer.X, (float)pointer.Y);
            for (int lane = 0; lane < model.Lanes.Count; lane++)
            {
                var edges = model.Lanes[lane].Edges;
                for (int ei = 0; ei < edges.Count; ei++)
                {
                    var ed = edges[ei];
                    var (ax, ay) = NodeXY(ed.AX, ed.ALaneFrac, lane, w, h);
                    var (bx, by) = NodeXY(ed.BX, ed.BLaneFrac, lane, w, h);
                    float dx = bx - ax;
                    var p0 = new Vector2(ax, ay);
                    var p1 = new Vector2(ax + dx * 0.4f, ay);
                    var p2 = new Vector2(bx - dx * 0.4f, by);
                    var p3 = new Vector2(bx, by);
                    for (int i = 0; i <= 18; i++)
                    {
                        float t = i / 18f, u = 1 - t;
                        var q = u * u * u * p0 + 3 * u * u * t * p1 + 3 * u * t * t * p2 + t * t * t * p3;
                        if (Vector2.Distance(q, pt) <= 5f) return (lane, ei);
                    }
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
            // Priority: node > edge > pin gutter.
            if (HitNode(w, h) is { } hn)
            {
                var n = model.Lanes[hn.lane].Nodes[hn.node];
                navigate(n.Book, n.Chapter, n.RefKey);
                return;
            }
            if (HitEdge(w, h) is { } he)
            {
                compare(model.Lanes[he.lane].WeaveIndex);
                win?.Close();
                return;
            }
            if (pointer.X < ConstGutter)
            {
                int lane = (int)((pointer.Y - ConstTopPad) / LaneH(h));
                if (lane >= 0 && lane < model.Lanes.Count)
                {
                    int idx = model.Lanes[lane].WeaveIndex;
                    if (!pins.Remove(idx)) pins.Add(idx);
                    Refresh();
                }
            }
        };

        var prev = new Button { Content = "‹ prev" };
        var next = new Button { Content = "next ›" };
        prev.Click += (_, _) => { page--; Refresh(); };
        next.Click += (_, _) => { page++; Refresh(); };

        var bar = new StackPanel
        {
            Orientation = Orientation.Horizontal, Spacing = 8,
            Padding = new Thickness(8, 6, 8, 6),
        };
        bar.Children.Add(prev);
        bar.Children.Add(next);
        caption.Text = model.Caption;
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
        left.Invoked += (_, e) => { page--; Refresh(); e.Handled = true; };
        var right = new KeyboardAccelerator { Key = Windows.System.VirtualKey.Right };
        right.Invoked += (_, e) => { page++; Refresh(); e.Handled = true; };
        ((Grid)win.Content).KeyboardAccelerators.Add(left);
        ((Grid)win.Content).KeyboardAccelerators.Add(right);
    }

    // ── concept map ────────────────────────────────────────────────────────

    public static void ConceptMap(StudyEngine engine, string code, bool full)
    {
        var canvas = new CanvasControl { ClearColor = PopupPaper };

        // The whole popup comes from one core view-model
        // (pure_engine_concept_map_json): spokes (near ∪ community, deduped,
        // labels pre-baked) + canon-ordered dispersion — no shell-side assembly,
        // gloss/lemma lookups, or book-order table (review item 4).
        if (engine.ConceptMapJson(code) is not { } cmj) return;
        var map = Wire.Parse<ConceptMapData>(cmj);
        var spokes = map.Spokes;

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
                var stroke = spokes[i].Semantic
                    ? Color.FromArgb(128, 158, 125, 56)
                    : Color.FromArgb(128, 107, 140, 102);
                ds.DrawLine(cx, cy, nx, ny, stroke, 1.4f);
                ds.FillCircle(nx, ny, 3, Color.FromArgb(230, 158, 125, 56));

                // Sides hang off the node left/right; top and bottom sit
                // above/below it, per-line centred.
                var text = spokes[i].Label;
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
            using (var ctl = new CanvasTextLayout(s, map.CenterLabel, centreFmt, 260, 100))
            {
                float th = (float)ctl.LayoutBounds.Height;
                ds.DrawTextLayout(ctl, cx - 130, cy - 14 - th, Palette.Ink);
            }

            // Dispersion strip: where across the books this concept occurs.
            // ByBook is canon-ordered (cell bi at bi/bookCount); the divide + book
            // count come from the same view-model, so no shell book table.
            if (map.ByBook.Any(v => v > 0))
            {
                float y0 = h - stripH;
                uint bmax = Math.Max(1u, map.ByBook.Max());
                float bc = Math.Max(1, map.BookCount);
                ds.FillRectangle(0, y0, w, stripH, Color.FromArgb(10, 0, 0, 0));
                for (int bi = 0; bi < map.ByBook.Count; bi++)
                {
                    var cnt = map.ByBook[bi];
                    if (cnt == 0) continue;
                    float alpha = 0.15f + 0.75f * cnt / bmax;
                    float x0 = bi / bc * w, x1 = (bi + 1) / bc * w;
                    ds.FillRectangle(x0, y0, x1 - x0, stripH,
                        Color.FromArgb((byte)(alpha * 255), 158, 125, 56));
                }
                float seam = map.OtNtDivide / bc * w;
                ds.DrawLine(seam, y0, seam, h, Color.FromArgb(128, 102, 77, 51), 1f);
            }
        };

        Shell($"Concept map — {code}", 720, 560, canvas);
    }
}
