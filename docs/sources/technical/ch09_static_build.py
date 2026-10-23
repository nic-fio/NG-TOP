from build import c, note, p, rif, sysf, table, term, warn

S1 = p("ngtop is meant to be copied to a server, a container or an old workstation and run there. A normal Linux executable "
       "depends on the machine's C library, and the version it was built against must not be newer than the one it finds. A <b>static</b> "
       "executable carries everything it needs: it asks the kernel for services, and nothing else.", lead=True) + \
    table(["Requirement", "How it is met"], [
        ["No dependency on the distribution's libc", "Built for " + c("x86_64-unknown-linux-musl") + ": the musl C library is linked into the file, not loaded at run time"],
        ["No C code in the build", "All dependencies are Rust crates that need no C compiler"],
        ["Proof, not hope", c("build.sh") + " fails if " + c("ldd") + " finds a dynamic dependency"],
        ["Tried on old and new systems", c("scripts/test-distro.sh") + " runs the file on Ubuntu 24.04, Debian 12 and 10, Fedora 41, Arch, Alpine 3.20, openSUSE Tumbleweed and CentOS 7"],
    ], "«TAB» — What “static” requires") + \
    term("""$ ldd dist/ngtop
	statically linked
$ bash build.sh
...
OK: dist/ngtop is static (3.5M)""", "The check")

S2 = p("The seven direct dependencies are all Rust. Each does one job.", lead=True) + \
    table(["Crate", "Version", "Used for", "Where"], [
        [c("ratatui"), "0.30", "Buffer, layout and test backend for the interface", c("src/ui/")],
        [c("crossterm"), "0.29", "Raw mode, alternate screen, keys, mouse, terminal size", c("src/ui/mod.rs")],
        [c("rustix"), "1.1", "System calls without a C library: " + c("pidfd_open") + ", " + c("pidfd_send_signal") + ", " + c("setpriority") + ", " + c("statvfs") + ", " + c("geteuid") + ", clock ticks, page size. On Linux x86_64 it issues the raw system calls itself",
         c("process.rs") + ", " + c("disk.rs") + ", " + c("processi.rs")],
        [c("zbus"), "5", "D-Bus in pure Rust, with the blocking API: systemd and login1", c("services/dbus.rs")],
        [c("serde_json"), "1.0", "Reads " + c("journalctl -o json") + " lines", c("services/mod.rs")],
        [c("signal-hook"), "0.3", "Sets flags on SIGTERM, SIGINT, SIGHUP without unsafe code in ngtop", c("src/ui/mod.rs")],
        [c("unicode-width"), "0.2", "Width of characters in terminal cells", c("widgets/mod.rs")],
    ], "«TAB» — The direct dependencies") + \
    p("The full tree for the musl target has about 150 crates (" + c("cargo tree --target x86_64-unknown-linux-musl") + "). The ordinary way for a crate "
      "to compile C is the " + c("cc") + " crate in a build script; " + c("cargo tree -e normal,build -i cc --target x86_64-unknown-linux-musl") + " finds "
      "nothing, which is the proof that none does. The crate " + c("libc") + " is in the tree, but it only declares function signatures; with musl, the "
      "functions come from the musl library that the toolchain links in. The licences of all these crates are listed by " + c("scripts/third-party.py") + ".") + \
    note("A new dependency is accepted only if it is Rust all the way down. Before adding one, run " + c("cargo tree -i cc") + " and " + c("build.sh") + ": if either shows C, "
         "reconsider (" + rif("Extending ngtop") + ").", "Rule for new crates.")

S3 = p("The default Linux toolchain links against glibc, and a program built that way cannot be copied to Alpine or to an old CentOS. For "
       + c("x86_64-unknown-linux-musl") + " Rust ships its own musl and links statically by default, so no C compiler or musl package is needed on the build machine: "
       "only the target, added once with " + c("rustup target add x86_64-unknown-linux-musl") + ".", lead=True) + \
    table(["Step of build.sh", "Why"], [
        [c("set -euo pipefail"), "Stop at the first error, including inside pipelines"],
        [c("cd \"$(dirname \"$0\")\""), "Works from any folder"],
        [c("export PATH=\"$HOME/.cargo/bin:$PATH\""), "Finds " + c("cargo") + " even in a shell where rustup is not initialised"],
        [c("cargo build --release --target x86_64-unknown-linux-musl"), "The static build, with LTO and strip (" + c("Cargo.toml") + ")"],
        [c("rm -f dist/ngtop"), "A running ngtop would give “text file busy” on overwrite"],
        [c("cp target/…/ngtop dist/ngtop"), "The finished file is " + c("dist/ngtop")],
        [c("ldd dist/ngtop | grep -qE \"not a dynamic executable|statically linked\""), "The verification; on failure, the dependencies are printed and the exit status is 1"],
    ], "«TAB» — build.sh, line by line") + \
    p("The result is about 3.5 MB: " + c("opt-level = 3") + ", " + c("lto = \"fat\"") + ", " + c("codegen-units = 1") + " and " + c("strip = true") + " (no debugging symbols).")

S4 = p("Static linking with musl removes some conveniences a dynamic program takes for granted. ngtop works around each of them in its own code.", lead=True) + \
    table(["Missing", "Consequence", "What ngtop does"], [
        ["The name service switch (NSS)", "A static musl program cannot ask sssd, LDAP or NIS for user names", "Reads " + sysf("/etc/passwd") + " itself (" + c("load_users()") + "). A user known only to a directory service is shown by its number"],
        ["Time zone from libc", "No " + c("localtime") + " configured by the distribution", "Reads " + c("TZ") + " and TZif files in Rust (" + rif("Time zone without trust") + ")"],
        ["Host name from libc", "—", "Reads " + c("/proc/sys/kernel/hostname")],
        ["Loading shared libraries", "No " + c("dlopen") + " of a driver's library", "Everything comes from kernel files. This is the reason for the next section"],
        ["Different libc behaviour", "musl differs from glibc in details (locale, resolver)", "ngtop uses no locale and does not resolve names"],
    ], "«TAB» — What a static musl binary cannot use, and the replacement")

S5 = p("Of the three GPU makers, AMD and Intel publish their counters in the kernel, in sysfs and in each process's " + c("fdinfo") + ". NVIDIA's proprietary driver "
       "does not; its numbers come from NVML, the NVIDIA management library, a closed-source shared library installed with the driver. A program uses it by loading it "
       "at run time. A static musl executable has no dynamic loader, so it cannot load a library built for glibc.", lead=True) + \
    table(["Choice", "Consequence"], [
        ["Do not use NVML in version 1.0", "NVIDIA cards are <b>detected</b> (name, driver) and appear with the state “unsupported”; no figure is shown"],
        ["Keep the single static file", "The promise “copy it anywhere, it runs” is worth more than one maker's counters in this version"],
        ["Open for the future", "NVIDIA support through NVML is on the list of future work. How to reconcile it with a static executable is still open"],
    ], "«TAB» — Why NVIDIA is not read") + \
    warn("Anything that would bring a dynamic dependency back — an NVML binding, a crate that links a system library, a call to a tool that is not installed everywhere — "
         "goes against the central promise. Discuss it before writing it.", "The promise.")

CHAPTER = ("The static build", [
    ("Why static", S1),
    ("Dependencies: Rust only", S2),
    ("musl and build.sh", S3),
    ("What static takes away", S4),
    ("Why not NVML", S5),
])
