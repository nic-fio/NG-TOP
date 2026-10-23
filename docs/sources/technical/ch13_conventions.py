from build import c, code, note, p, rif, sysf, table, tip

S1 = p("The rules below are the ones the code already follows. Where a rule can be checked by a tool, the tool is named.", lead=True) + \
    table(["Rule", "Detail", "Checked by"], [
        ["Text on screen is English", "Panel titles, key labels, dialogs, status lines, messages, " + c("--help"), "reading; the manuals check their own text"],
        ["Comments are short and in Italian", "One or two lines above the function or the odd line, saying <i>why</i>, not what", "—"],
        ["Documentation is English", "These manuals, " + c("README.md") + ", " + c("decisions-and-history.md"), c("build.py --check") + " flags Italian sentences in the manuals"],
        ["Names", "Types, functions and variables are English; test names are Italian sentences with underscores; the modules and variants of the sections keep their Italian names (see the glossary)", "—"],
        ["Licence header", "Every source file starts with two lines: " + c("// Copyright (c) 2026 Nicola Fiorillo") + " and " + c("// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0")
         + ". In scripts they follow the shebang line", "reading"],
        ["Formatting", c("cargo fmt") + ", default rustfmt settings", c("cargo fmt --check")],
        ["Lints", "No clippy warning; the few " + c("#[allow(...)]") + " are local and explained", c("cargo clippy --all-targets -- -D warnings")],
    ], "«TAB» — Language, names and headers") + \
    code("""// Copyright (c) 2026 Nicola Fiorillo
// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0

//! Memoria e swap da /proc/meminfo (valori in byte).""", "rust", "The start of every .rs file")

S2 = p("A monitor runs for weeks beside everything else on a machine. It must not be the thing that crashes. These rules are applied everywhere in " + c("src/") + ".", lead=True) + \
    table(["Rule", "In practice"], [
        ["No panic on system data", "Outside tests, the sources contain no " + c("unwrap()") + ", no " + c("expect(…)") + ", no " + c("panic!") + ", no " + c("unreachable!") + ", no " + c("todo!") + " and no "
         + c("unsafe") + ". (Tests may use them.) A grep of " + c("src/") + " for them, ignoring test modules, should find nothing"],
        ["Missing or unreadable is not an error", "A file that is missing, or a field that cannot be parsed, gives " + c("None") + ", a default or a dash on screen. Permission denied is the same: the value is “—”"],
        ["Arithmetic cannot overflow or go negative", c("saturating_sub") + " for counters, " + c("saturating_mul") + " for sizes, " + c("checked_*") + " in the time zone parser, " + c("u32") + " for products of screen sizes, and " + c("clamp") + " for every shown percentage"],
        ["Division is guarded", "A rate is computed only when " + c("dt > 0") + "; a ratio only when the denominator is non-zero; " + c("max(1)") + " on core counts and tick rates"],
        ["Draw inside the buffer", "Only " + c("put()") + " and " + c("putn()") + " write cells; they ignore anything outside the buffer"],
        ["Reads through the source", "A collector reads only through " + c("Source") + " (" + c("roots.src") + ") and runs commands only through " + c("CommandRunner") + " (" + c("roots.run") + "), so the same code works on a remote machine; " + c("tests/source_only.rs") + " enforces it"],
        ["Paths from the roots", "Collectors take their paths from " + c("Roots") + " (the literals are the defaults in " + c("Roots::default()") + "). The Services code has two parameters of its own, " + c("run_dir") + " and " + c("cgroup_root") + ". Still literal: the zoneinfo folders, and one read of " + c("/proc/PID/comm") + " in the services thread"],
        ["Bound the data", "Lists from the system are capped (200 open files, 500 environment variables, 50 journal lines), and the configuration file is read only if it is a small regular file"],
        ["Never block the interface thread", "Slow or possibly blocking work goes to a thread with a channel (" + rif("Threads") + ")"],
        ["Do not run anything through a shell", "External programs get a fixed list of arguments. On a remote machine the arguments are quoted one by one (" + c("sh_quote()") + ") and the data from the server are checked before use (" + rif("Remote security") + ")"],
        ["Pure Rust dependencies", rif("Dependencies: Rust only")],
    ], "«TAB» — Robustness rules")

S3 = p("Habits that keep the code uniform.", lead=True) + \
    table(["Habit", "Detail"], [
        ["Parse and read apart", "A function that takes text and returns a value, and one that reads the file; every parser has a unit test with a malformed input"],
        ["Tests next to the code", "A " + c("#[cfg(test)] mod tests") + " at the end of the file; integration tests only for what needs a fake tree or a whole frame"],
        ["Tests are deterministic", "No real clock (" + c("refresh_with_dt") + "), no real network, no real systemd (" + c("FakeBackend") + "), no dependence on the machine's files (only " + c("tests/fixtures/") + ", and " + c("/usr/share/zoneinfo") + " with a fallback)"],
        ["Replaceable edges", "What is dangerous or slow is behind a function pointer or a trait so a test can substitute it: " + c("KillFn") + ", " + c("StartFn") + ", " + c("FsProbe") + ", " + c("Backend")],
        ["One source for each fact", "Keys: " + c("keys.rs") + ". Gradients: " + c("theme.rs") + ". Options: " + c("USAGE") + ". The manuals read them"],
        ["Hidden diagnostics", c("--roots") + ", " + c("--bench") + ", " + c("--bench-start") + ", " + c("--doc-json") + " exist for developers and are not in " + c("--help")],
        ["Simple state", "State is plain structs with public fields and functions; no global mutable state except the temperature unit (a single atomic in " + c("fmt.rs") + ")"],
    ], "«TAB» — Habits") + \
    note("Before a change is finished: " + c("cargo fmt") + ", " + c("cargo clippy --all-targets -- -D warnings") + ", " + c("cargo test"), "Checklist.") + \
    tip("If you change a thing the manuals describe — a key, an option, a formula, a cache rhythm, the file layout — change the manual in the same step. The file map and the numbers update "
        "themselves; the sentences do not.", "Keep the manuals true.")

CHAPTER = ("Conventions", [
    ("Language, names and headers", S1),
    ("Rules for robust code", S2),
    ("Habits of the code base", S3),
])
