// Performance monitoring: one switch.
//
// Flip PERF to true and the app measures itself — the boot trace, the per-turn
// cost split, the counter in the text-measurement hot path, the stall meter on
// the engine thread, and the "Boot diagnostics" tables in Settings. It exists so
// on-device numbers can be read off a real phone during a performance push, not
// so every reader carries a stopwatch forever, which is why it ships OFF.
//
// WHAT IT MUST NOT GATE: the pasteable bug report (Settings → "Report a
// problem"). Version, build id, engine, data pack and the device facts are not
// measurements, and while they sat inside the PERF block the only two options
// were to ship a measuring build or to ship with nothing actionable to paste
// (D-20). `reportHeader()` in shell/SettingsDialog.svelte must read identically
// with this flag either way — anything it prints has to come from a value the
// app computes regardless of PERF.
//
// Deliberately a plain constant, not an env var or a build flag: the bundler
// folds it away, and turning it on means editing one line here.

export const PERF = false;
