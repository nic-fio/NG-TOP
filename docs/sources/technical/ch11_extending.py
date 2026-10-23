from build import c, code, note, p, rif, steps, sysf, table, tip, warn

COLLECTOR = '''//! Open file handles of the whole system, from /proc/sys/fs/file-nr.

use std::path::Path;

use super::read_text;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileNr {
    /// Handles in use.
    pub used: u64,
    /// System-wide limit.
    pub max: u64,
}

/// "allocated  unused  max": in use = allocated - unused.
pub fn parse_file_nr(text: &str) -> FileNr {
    let n: Vec<u64> = text
        .split_whitespace()
        .map(|x| x.parse().unwrap_or(0))
        .collect();
    let get = |i: usize| n.get(i).copied().unwrap_or(0);
    FileNr {
        used: get(0).saturating_sub(get(1)),
        max: get(2),
    }
}

pub fn read_file_nr(path: &Path) -> FileNr {
    read_text(path).map(|t| parse_file_nr(&t)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_nr() {
        let f = parse_file_nr("3456\\t0\\t9223372036854775807\\n");
        assert_eq!((f.used, f.max), (3456, 9223372036854775807));
        assert_eq!(parse_file_nr(""), FileNr::default());
    }
}'''

WIRING = '''// src/collect/mod.rs
pub mod filenr;                                  // 1. declare the module

pub struct System {
    // ...
    pub files: filenr::FileNr,                   // 2. the field
}

// in System::new():
    files: filenr::FileNr::default(),            // 3. its initial value

// in System::refresh_with_dt():
    self.files = filenr::read_file_nr(           // 4. read it, through the roots
        &self.roots.proc.join("sys/fs/file-nr"),
    );'''

S1 = p("This tutorial adds a small collector from start to finish: the number of file handles in use on the whole system, from "
       + c("/proc/sys/fs/file-nr") + ". The pattern is the one of every collector in " + c("src/collect/") + ".", lead=True) + \
    steps([
        "<b>Write the module</b> " + sysf("src/collect/filenr.rs") + ". A data struct; a " + c("parse_*()") + " that takes <i>text</i> and never panics; a " + c("read_*()") + " that takes a "
        "<i>path</i> and uses " + c("read_text()") + ". Put the test in the same file. Start the file with the two licence lines used everywhere (" + rif("Conventions") + ")." + code(COLLECTOR, "rust", "src/collect/filenr.rs"),
        "<b>Connect it to System.</b> Four lines in " + c("src/collect/mod.rs") + ". Always build the path from " + c("self.roots") + ", never write " + c("/proc") + " in the collector." + code(WIRING, "rust", "src/collect/mod.rs"),
        "<b>Decide how often.</b> One small file can be read every cycle. A costly one should be read every 5th cycle, like routes (" + c("refresh_count.is_multiple_of(5)") + "), or only when "
        "a section is visible, like connections (a " + c("collect_*") + " flag, set in " + c("event_loop()") + ", " + c("snapshot()") + " and " + c("bench()") + "). See " + rif("Caches and schedules") + ".",
        "<b>Add the file to the fixtures.</b> Put " + c("proc/sys/fs/file-nr") + " in " + c("tests/fixtures/snap_a/") + " and " + c("snap_b/") + ", with different numbers if it is a rate, then a test "
        "in " + c("tests/collect.rs") + " that asserts the value after the two samples (" + rif("Fixtures and two samples") + ").",
        "<b>Show it.</b> The quickest place is " + c("dump()") + " in " + c("src/main.rs") + ": one more " + c("outln!") + ". To show it on screen, see the next section.",
        "<b>Run</b> " + c("cargo test") + ", " + c("cargo clippy --all-targets -- -D warnings") + ", " + c("cargo fmt") + ", and " + c("ngtop --dump") + ".",
    ]) + \
    table(["Rule", "Why"], [
        ["Parse text, read paths", "A parser tested with strings needs no file; the reader is two lines"],
        ["Missing, empty, strange: a default", "Never " + c("unwrap()") + " or " + c("expect()") + " on data from the system; use " + c("unwrap_or") + ", " + c("?") + " on an " + c("Option") + ", " + c("saturating_sub") + ", " + c("checked_*")],
        ["A rate needs a previous sample", "Keep it in " + c("System") + " (like " + c("prev_cpu") + ") and use the " + c("dt") + " that " + c("refresh_with_dt()") + " is given, not the clock"],
        ["Keep what is slow out of the interface thread", "If a read can block (a network file system, a device that wakes up), do it in a thread, as " + c("FsMonitor") + " does"],
        ["Never wake hardware", "If the data comes from a device that sleeps, learn from " + rif("GPUs that must not be woken") + " first"],
    ], "«TAB» — Rules for a new collector")

S2 = p("A new panel inside a section, or a new section, is only drawing. The data is already in " + c("System") + " and " + c("App") + ".", lead=True) + \
    steps([
        "<b>Draw with a Panel.</b> " + c("Panel::new(title, colour).right(text).render(area, buf)") + " returns the inner rectangle. Inside, write with " + c("put()") + " and " + c("putn()") + ", never with ratatui's own "
        "functions (" + rif("Safe writing and widths") + "). Use " + c("Meter") + " for bars and " + c("BrailleGraph") + " for histories; take the colour from a gradient in " + c("theme.rs") + ".",
        "<b>Check the size first.</b> Return at once if " + c("inner.width") + " or " + c("inner.height") + " is too small for your content, as the existing panels do (" + c("if inner.width < 20 || inner.height < 3 { return; }") + ").",
        "<b>Place it in the layout</b> of the section's " + c("draw()") + " (a " + c("Layout::vertical") + " or " + c("horizontal") + " of the existing code). Decide what gives way on a small terminal; hide the panel rather than squeeze it.",
        "<b>If it needs history</b>, add a " + c("History") + " to " + c("App") + " and push to it in " + c("App::update()") + ".",
        "<b>If it has keys</b>, handle them in the section's " + c("on_key()") + ", and add them to the key table in " + c("src/ui/keys.rs") + ": the bottom bar, and the lines of the F1 help.",
        "<b>Test</b> that the section still draws at every size: the sweep in " + c("tests/ui.rs") + " covers all sections automatically.",
    ]) + \
    p("<b>A whole new section</b> is the same work plus a list of places that know there are five of them. Search the source for " + c("Section::ALL") + ", " + c("Section::Servizi") + " and the text “1-5”:") + \
    table(["Where", "What to change"], [
        [sysf("src/ui/sections/NEW.rs") + ", " + c("sections/mod.rs"), "The file with " + c("draw()") + " and, if needed, its state struct; a " + c("pub mod") + " line"],
        [c("Section") + " in " + c("src/ui/app.rs"), "A variant; add it to " + c("ALL") + " (the array length changes) and to " + c("title()")],
        [c("view::draw()") + " in " + c("src/ui/view.rs"), "A branch in the match, and the choice of key bar"],
        [c("src/ui/keys.rs"), "A " + c("KEYS_…") + " table; add it to " + c("section_bars()") + " (its array length) and one entry to " + c("SECTION_HELP") + ", in the same order as " + c("Section::ALL") + "; “1-5” becomes “1-6” everywhere"],
        [c("App::on_key()"), "The digit pattern " + c("'1'..='5'") + " that selects a section"],
        [c("src/config.rs"), "The range check " + c("(1..=5)") + " of " + c("--section") + ", the text of " + c("USAGE") + ", and the tests that assume the last section"],
        [c("App") + " fields and " + c("App::new()"), "The state of the new section"],
        [c("event_loop()") + ", " + c("snapshot()") + ", " + c("bench()"), "If the section needs a " + c("collect_*") + " flag in " + c("System") + ", set it in the three places"],
        [c("README.md") + " and the manuals", "The list of sections and keys"],
    ], "«TAB» — The places that know the number of sections") + \
    warn("The key table, the F1 help and the manuals' key reference come from the same source (" + c("src/ui/keys.rs") + ", used by " + c("--doc-json") + "). Change keys there and only there.", "One source for keys.")

S3 = p("A short guide to deciding where something belongs.", lead=True) + \
    table(["I want to…", "Where"], [
        ["Read a new kernel file", "A collector in " + c("src/collect/") + "; " + rif("How the collectors read")],
        ["Show a number differently", "The section's drawing function; formatting helpers are in " + c("src/ui/fmt.rs")],
        ["Add a colour or gradient", c("src/ui/theme.rs") + ". Check the result on 256 and 16 colours (" + rif("Colour depth") + ")"],
        ["Change a refresh rhythm", "The condition on " + c("refresh_count") + " in the collector or in " + c("System::refresh_with_dt()") + "; update " + rif("Caches and schedules")],
        ["Add an option or a setting", c("src/config.rs") + ": " + c("Cli") + ", " + c("Config") + ", " + c("parse_full()") + ", " + c("merge()") + ", " + c("USAGE") + "; it appears in the manuals through " + c("--doc-json")],
        ["Support another GPU driver", c("src/collect/gpu.rs") + ": a branch in " + c("read_dynamic()") + ", and its engines' names in " + c("engine_kind()")],
        ["Support another init system than systemd", "A new implementation of the " + c("Backend") + " trait in " + c("src/collect/services/") + " (the fake one in " + c("fake.rs") + " shows the minimum) and a choice in " + c("Servizi::start()")],
        ["Add a new dependency", "Only a pure Rust crate (" + rif("Dependencies: Rust only") + ")"],
    ], "«TAB» — Where does it go?") + \
    tip("Before adding anything that acts on the system (a signal, an action, a write), read " + rif("Safety") + ". The three habits: identify the target twice, ask before doing, never wait in the interface thread.",
        "Actions.")

CHAPTER = ("Extending ngtop", [
    ("Add a collector", S1),
    ("Add a panel or a section", S2),
    ("Where things go", S3),
])
