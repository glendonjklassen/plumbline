// Performance monitoring: one switch.
//
// Flip PERF to false and the app stops measuring itself — no boot trace, no
// per-turn cost split, no counter in the text-measurement hot path, and no
// "Boot diagnostics" section in Settings. It exists so on-device numbers can
// be read off a real phone during a performance push, not so every reader
// carries a stopwatch forever.
//
// Deliberately a plain constant, not an env var or a build flag: the bundler
// folds it away, and turning it on means editing one line here.

export const PERF = true;
