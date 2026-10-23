from build import ROOT, c, code, fig, key, note, ol, p, rif, sysf, table, term, tip


def _count(pattern):
    n = 0
    for f in pattern:
        n += f.read_text(errors="replace").count("#[test]")
    return n


INLINE = _count((ROOT / "src").rglob("*.rs"))
INTEG = _count((ROOT / "tests").glob("*.rs"))

S1 = p("There are two kinds of test, and a few tools to look at the screen without a terminal. Nothing needs root, a GPU, systemd or a network: "
       "everything runs from fake trees and a fake services backend.", lead=True) + \
    table(["Kind", "Where", "How many", "What it checks"], [
        ["Unit tests", "Inside each source file, in " + c("#[cfg(test)] mod tests"), str(INLINE) + " test functions",
         "One parser, one formula, one widget. They feed text to a " + c("parse_*()") + " function and compare numbers"],
        ["Integration tests", c("tests/*.rs"), str(INTEG) + " test functions",
         "The collectors over a fake tree, and the views drawn from them, through the public library"],
        ["Distribution tests", c("scripts/test-distro.sh"), "8 distributions × 4 cases", "The finished static binary on real systems"],
        ["Remote end-to-end tests", c("scripts/test-remote.sh"), "3 containers with sshd", "The finished binary monitoring another machine over real ssh (" + rif("Testing remote monitoring") + ")"],
    ], "«TAB» — The kinds of test") + \
    table(["File", "Subject"], [
        [c("tests/collect.rs"), "CPU, memory, load, processes (names with parentheses, tree, a reused pid), network, disks, sensors, on the two snapshots"],
        [c("tests/gpu.rs"), "AMD, Intel (i915, xe), integrated and discrete, suspended, NVIDIA (with a fake command runner), no GPU; the GPU views"],
        [c("tests/retedischi.rs"), "Network, disks and connections: collectors, 'off when not visible', drawing at every size, keys"],
        [c("tests/servizi.rs"), "Services with a fake backend: drawing, filters, search, an action with its confirmation, a refusal, no systemd, no blocking"],
        [c("tests/ui.rs"), "The Overview large and compact; every section at 81 sizes; machines with 1 to 128 cores"],
        [c("tests/remote.rs"), "The remote protocol through a local " + c("sh") + ": the same machine read directly and “remotely” must give the same results"],
        [c("tests/hostile.rs"), "Security tests against a hostile server, fake " + c("sudo") + " programs and hostile " + c("nvidia-smi") + " output"],
        [c("tests/source_only.rs"), "The collectors read only through " + c("Source") + " (" + rif("Sources and runners") + ")"],
        [c("tests/cli.rs"), "The program as a process: it refuses to start without a terminal (exit 2); " + c("--help") + " and " + c("--version") + " work without one"],
    ], "«TAB» — The integration test files")

S2 = p("A rate needs two samples, so the fixtures come in pairs: <b>A</b> and <b>B</b>, as if taken one second apart. A test loads A, refreshes "
       "with " + c("dt = 0") + ", swaps the roots to B and refreshes with " + c("dt = 1.0") + ". Then every number is exact and nothing depends on the real clock.",
       lead=True) + \
    code("""fn sistema() -> System {
    let mut s = System::new(roots("snap_a"));
    s.clock_ticks = 100;
    s.page_size = 4096;
    s.refresh_with_dt(0.0);      // first sample: rates are zero
    s.roots = roots("snap_b");
    s.refresh_with_dt(1.0);      // one second later
    s
}""", "rust", "tests/collect.rs") + \
    p("Take the CPU. The " + c("cpu") + " line of snapshot A is " + c("1000 0 500 8000 500 0 0 0 0 0") + ", of B " + c("1100 0 550 8050 500 0 0 0 0 0") + ". "
      "Totals: 10 000 and 10 200, so Δtotal = 200. Idle (idle + iowait): 8 500 and 8 550, so Δidle = 50. Usage = (200 − 50) × 100 ÷ 200 = <b>75 %</b>, "
      "which is exactly what the test asserts. For a process, 80 ticks at 100 Hz in 1 s on 2 cores is 40 %.") + \
    table(["Folder in " + c("tests/fixtures/"), "Contents"], [
        [c("snap_a") + ", " + c("snap_b"), "A reduced " + c("proc") + ", " + c("sys") + " and " + c("etc") + ": the files ngtop reads, with up to seven processes (one named “a) b” to test the parser; one without permission on "
         + c("io") + "; one owned by a user known only by number), two cores, a zram device, a mount table, network counters"],
        [c("gpu/*"), "One tree per case: " + c("amd_a") + " and " + c("amd_b") + " (two samples), " + c("i915_a") + ", " + c("i915_b") + ", " + c("i915_idle") + ", " + c("i915_arc_a") + " and "
         + c("i915_arc_b") + " (a discrete Intel), " + c("xe_a") + ", " + c("xe_b") + ", " + c("susp") + " and " + c("susp_rpm") + " (suspended: D3cold and runtime), " + c("nvidia") + ", " + c("nvidia_procs") + ", " + c("nvidia_susp") + ", " + c("nvidia_two") + ", " + c("none")],
        [c("nvidia/*"), "Sample " + c("nvidia-smi") + " outputs: full and basic card queries (3090, laptop without fan, two cards, " + c("[Not Supported]") + " fields, unknown-field error, truncated lines), " + c("pmon") + " (old and new drivers, unsupported, header-only, corrupted) and " + c("--query-compute-apps")],
        [c("rete_dischi/a") + ", " + c("b"), "Interfaces, routes, connections with their sockets, disk counters and mounts"],
        [c("cgroup/a") + ", " + c("b"), "cgroup files for the Services CPU% and memory"],
    ], "«TAB» — The fixtures") + \
    note("The fixtures are plain text files copied from a real machine and then reduced. To add a case, copy the files of a real system into a new folder, "
         "cut it to what ngtop reads, and write the expected numbers by hand from the formulas in " + rif("Data sources") + ".", "Making a new one.")

S3 = p("Drawing is tested in two ways: by looking at its text, and by trying to break it.", lead=True) + \
    table(["Tool", "What it does"], [
        [c("ngtop --snapshot WxH [--section N] [--ansi]"), "Collects about 20 samples 100 ms apart; with " + c("--roots") + " (fixtures) the graph histories are then replaced by about 300 synthetic samples around the last values (" + c("synthesize_history()") + "), so the figures of the manuals have full graphs that are drawn, not measured. It draws <i>one</i> frame at the given size on a test backend and prints it, "
         "as text or, with " + c("--ansi") + ", with truecolor escape codes. " + c("N") + " is 1 to 5. This is how the screens in these manuals are made"],
        [c("--roots DIR"), "Hidden option: read " + c("DIR/proc") + ", " + c("DIR/sys") + " and " + c("DIR/etc") + " instead of the machine. With a fake tree, the screenshot contains no real names, addresses or processes. With it, the Services section of a snapshot uses the fake backend (demo units, no D-Bus)"],
        [c("--doc-json"), "Hidden option: the keys and the command-line options as JSON, produced from the same tables and the same help text the program uses, so the key and option reference of the manuals cannot differ from the program"],
        [c("--dump [--tree]"), "A text summary and no interface: CPU and cores, memory, swap, load, uptime, the 15 busiest processes, optionally as a tree. Useful over a bad link, in a bug report, and in CI"],
        [c("--bench"), "Times a cycle per section (" + rif("Budget and measurements") + ")"],
        [c("--bench-start"), "Times the start up to the first frame"],
    ], "«TAB» — Looking at the screen without a terminal") + \
    term("""$ ngtop --snapshot 80x24 --section 2
$ ngtop --roots tests/fixtures/gpu/amd_a --snapshot 120x36 --section 2 --ansi""", "Two ways to draw one frame") + \
    p("<b>The sweep.</b> " + c("tutte_le_misure_e_sezioni_senza_panic") + " draws all five sections at every combination of widths 1, 5, 12, 40, 80, 99, 100, 101, 182 and heights "
      "1, 3, 5, 12, 24, 29, 30, 31, 52, and converts each frame to ANSI. The values straddle the thresholds (99 and 100, 29 and 30) and include terminals so small nothing fits. The "
      "same is done in other files for narrower sets of sizes: the GPU and Overview views over six GPU fixtures (" + c("tasto_d_cambia_dispositivo_e_sweep") + "), Network &amp; Disks (" + c("rendering_a_ogni_misura") + "), Services (" + c("rendering_a_quattro_misure_senza_panic") + ") and every Processes tab (" + c("disegno_senza_panic_alle_misure_previste") + "). "
      + c("molti_core_si_adattano") + " builds machines with 1, 4, 24, 64 and 128 cores and checks that the frame has exactly the requested number of lines and that the last core is "
      "present — or, at 128 cores in a small window, that “not shown” is.")

S4 = p("Most of ngtop's safety is a promise about <i>when</i> something happens. These tests make the promise checkable.", lead=True) + \
    table(["Promise", "Test"], [
        ["The interface never waits for the services thread", c("l_interfaccia_non_attende_il_backend") + " and " + c("tenendo_premuto_giu_non_partono_richieste_a_raffica") + " in " + c("tests/servizi.rs") + ": a backend that sleeps; a held key; the number of requests is counted"],
        ["Only one service action at a time", c("un_azione_lenta_non_blocca_la_coda_e_ce_ne_e_una_sola") + " (" + c("worker.rs") + ")"],
        ["A reused pid never receives a signal", c("pid_riusato_non_riceve_nulla") + ", " + c("radice_riusata_i_figli_del_nuovo_non_ricevono") + ", " + c("nodo_intermedio_riusato_salta_il_ramo") + " (" + c("processi.rs") + ")"],
        ["PID 1 and ngtop itself", c("pid_uno_e_se_stessi_protetti") + ", " + c("sottoalbero_di_init_vietato")],
        ["A large subtree needs a second Enter", c("sottoalbero_grande_chiede_seconda_conferma")],
        ["A sleeping GPU is not read", c("gpu_sospesa_non_viene_interrogata") + ", " + c("integrata_con_runtime_pm_si_legge_comunque") + ", " + c("nvidia_sospesa_non_lancia_nvidia_smi") + ", " + c("nvidia_suspending_conta_come_sospesa") + ", " + c("nvidia_due_schede_una_sospesa_interroga_solo_la_sveglia") + ", " + c("nvidia_scheda_che_si_sospende_non_viene_piu_interrogata") + ", " + c("nvidia_guardia_addormentata_non_e_un_errore_e_non_fa_raffiche") + " (" + c("tests/gpu.rs") + "), " + c("guardia_lancia_solo_con_schede_sveglie") + " (" + c("src/collect/nvidia_worker.rs") + ")"],
        ["Hostile nvidia-smi output is bounded and cleaned", c("hostile_nvidia_smi_output_never_panics_blocks_or_dirties_the_name") + " (" + c("tests/hostile.rs") + "), " + c("nvidia_uscita_oltre_il_limite_non_si_legge") + " (" + c("tests/gpu.rs") + "), " + c("lossy_taglia_a_max_output") + " (" + c("src/collect/nvidia_worker.rs") + "), " + c("uscita_enorme_limitata") + ", " + c("limiti_di_schede_e_nome") + ", " + c("fuzz_byte_casuali") + ", " + c("fuzz_righe_casuali") + ", " + c("fuzz_mutazioni_fixture") + " (" + c("src/collect/nvidia.rs") + ")"],
        ["Connections are collected only when shown", c("la_raccolta_connessioni_e_spenta_se_la_sezione_non_e_visibile") + " (" + c("tests/retedischi.rs") + ")"],
        ["No terminal, no interface", c("senza_terminale_non_apre_la_tui") + " (" + c("tests/cli.rs") + ")"],
        ["Hostile time zone data", c("nessun_panic_su_prefissi_e_file_troncati") + ", " + c("tzif_inventato_e_rotto") + " (" + c("tz.rs") + ")"],
        ["Stuck mounts do not stop the others", c("statvfs_in_thread_non_blocca_e_segnala_non_risponde") + " (" + c("disk.rs") + "): a fake probe that blocks"],
        ["Collectors never read the disk directly", c("nessuna_lettura_diretta_fuori_da_source") + " (" + c("tests/source_only.rs") + ")"],
        ["Remote and local give the same numbers", c("remote_local_sh_matches_local_on_real_machine") + ", " + c("remote_local_sh_matches_local_on_fixture") + " (" + c("tests/remote.rs") + ")"],
        ["Remote actions stay disabled", c("azioni_disattivate_in_remoto") + " (" + c("app.rs") + ")"],
        ["A slow link widens the interval, a single slow update does not", c("intervallo_si_allarga_in_remoto") + " (" + c("app.rs") + ")"],
        ["A real signal works", c("segnale_reale_a_un_figlio") + " starts a child process, signals it through the whole path and checks it died"],
    ], "«TAB» — Promises and the tests that guard them") + \
    table(["File", "Tests"], [
        [c("tests/gpu.rs"), c("nvidia_attiva_con_dati_e_processi") + ", " + c("nvidia_senza_nvidia_smi") + ", " + c("nvidia_comando_assente_in_remoto_rc_127") + ", "
         + c("nvidia_sospesa_non_lancia_nvidia_smi") + ", " + c("nvidia_suspending_conta_come_sospesa") + ", " + c("nvidia_due_schede_una_sospesa_interroga_solo_la_sveglia") + ", "
         + c("nvidia_scheda_che_si_sospende_non_viene_piu_interrogata") + ", " + c("nvidia_guardia_addormentata_non_e_un_errore_e_non_fa_raffiche") + ", "
         + c("nvidia_campo_rifiutato_si_ripassa_ai_campi_base") + ", " + c("nvidia_senza_pmon_usa_compute_apps") + ", " + c("nvidia_timeout_e_not_responding") + ", "
         + c("nvidia_timeout_dei_comandi_al_massimo_3_secondi") + ", " + c("nvidia_uscita_oltre_il_limite_non_si_legge") + ", " + c("viste_gpu_sospesa_nvidia_e_nessuna")],
        [c("src/collect/nvidia.rs"), c("argomenti") + ", " + c("query_completa_3090") + ", " + c("portatile_senza_ventola") + ", " + c("due_schede") + ", "
         + c("ripiego_base_driver_vecchio") + ", " + c("errore_campo_non_valido") + ", " + c("valori_non_supportati") + ", " + c("query_troncata_e_rovinata") + ", "
         + c("limiti_di_schede_e_nome") + ", " + c("pmon_misto") + ", " + c("pmon_driver_nuovo") + ", " + c("pmon_non_supportato_o_vuoto") + ", " + c("pmon_rovinato") + ", "
         + c("pmon_solo_intestazione") + ", " + c("compute_apps") + ", " + c("bus_id_normalizzato") + ", " + c("fuzz_byte_casuali") + ", " + c("fuzz_righe_casuali") + ", "
         + c("fuzz_mutazioni_fixture") + ", " + c("uscita_enorme_limitata")],
        [c("src/collect/nvidia_worker.rs"), c("guardia_lancia_solo_con_schede_sveglie") + ", " + c("bus_id_per_nvidia_smi") + ", " + c("lossy_taglia_a_max_output")],
        [c("src/ui/sections/gpu.rs"), c("nota_cuda_solo_sulla_scheda_selezionata")],
        [c("tests/hostile.rs"), c("hostile_nvidia_smi_output_never_panics_blocks_or_dirties_the_name")],
    ], "«TAB» — The NVIDIA unit, integration and security tests")

S5 = p(c("scripts/test-distro.sh") + " runs the finished binary " + c("dist/ngtop") + " in containers, with rootless podman, on the images below. It needs only podman and the binary; "
       "the containers have no network (" + c("--network=none") + ").", lead=True) + \
    table(["Image", "Why it is in the list"], [
        ["Ubuntu 24.04, Debian 12, Fedora 41", "Current mainstream distributions"],
        ["Arch Linux, openSUSE Tumbleweed", "Rolling releases: a new kernel and new files"],
        ["Alpine 3.20", "musl-based, no systemd, tiny"],
        ["Debian 10, CentOS 7", "Old kernels and old libraries: the strongest test of “static”"],
    ], "«TAB» — The images tried by default") + \
    table(["Case", "Command in the container", "Pass if"], [
        [c("dump"), c("ngtop --dump"), "exit 0 and output not empty"],
        [c("snap120"), c("ngtop --snapshot 120x40"), "the same"],
        [c("snap80gpu"), c("ngtop --snapshot 80x24 --section 2"), "the same"],
        ["interactive", "ngtop under a pseudo-terminal (" + c("script") + "), TERM=xterm-256color, 120 × 40, for 3 s, then SIGTERM", "the output has the sequences that leave the alternate screen and show the cursor; "
         "no “panicked”; exit status 0, 124 or 143; if " + c("script") + " is missing in the image: n/a"],
    ], "«TAB» — The four cases of each image") + \
    p("The script prints a table with OK or ERROR for each image and keeps a log per case in the folder given as first argument (default " + c("/tmp") + "). "
      "Other images can be passed as further arguments.") + \
    tip("Run " + c("build.sh") + " first: the test uses the binary in " + c("dist/") + ", not a debug build.", "Order.")

S6 = table(["Command", "When"], [
    [c("cargo test"), "Always, before a change is considered finished: unit and integration tests"],
    [c("cargo clippy --all-targets -- -D warnings"), "The linter; warnings are errors"],
    [c("cargo fmt"), "Formatting"],
    [c("build.sh") + " then " + c("scripts/test-distro.sh"), "Before a release, or after a change that touches system calls, files or the terminal"],
    [c("scripts/test-remote.sh"), "After a change in " + c("src/remote/") + ", " + c("agent.sh") + " or the remote services backend"],
    [c("ngtop --bench"), "After a change in a collector or in drawing (" + rif("Performance") + ")"],
    [c("python3 docs/sources/build.py --check"), "After a change in code that the manuals describe: it verifies the cited functions and files and that the numbers are up to date"],
], "«TAB» — What to run, and when")

S7 = p("Remote monitoring is tested at three levels, from cheap to realistic.", lead=True) + \
    table(["Level", "How", "What it proves"], [
        ["Protocol, no network", c("src/remote/tests.rs") + " (unit) and " + c("tests/remote.rs") + " run the real agent through a local " + c("sh") + " instead of ssh: " + c("NGTOP_SSH=local-sh") + " (or " + c("connect_local_sh()") + "). No sshd, no container",
         "Quoting, records, the manifest, the bulk loaders, hostile input, the double measure, reconnection after the session is killed; and that a remote read gives the same results as a direct read"],
        ["Interface, no network", "A fake " + c("Source") + " with a given " + c("RemoteStatus") + " in " + c("view.rs") + " tests; " + c("App") + " tests in " + c("app.rs"), "The tab bar, the reconnecting and failed lines, disabled actions, the widening interval"],
        ["Real ssh, containers", c("scripts/test-remote.sh") + " builds three images from " + c("tests/fixtures/remote/Containerfile.*") + " (Alpine with busybox, Debian, Debian with systemd), starts them with rootless podman, "
         "and runs " + c("ngtop") + " against each over real ssh", "Busybox and GNU tools, the fallbacks, Services on a real systemd, reconnection, a login that is refused"],
    ], "«TAB» — The three levels of remote testing") + \
    p("<b>Security tests.</b> " + c("tests/hostile.rs") + " runs the real client against a <i>hostile server</i> built from shell scripts: garbage and oversized replies, a fake READY, a missing end marker, "
      "a host name or error text with escape sequences, fuzzed unit names and process inputs (only the expected argv is ever run, only a matching start time is hit), a stale start time under fork load, fake " + c("sudo") + " programs "
      "(wrong password, unknown prompt, requiretty, a prompt that is not ours), drops during an action, and hostile " + c("nvidia-smi") + " output (" + c("hostile_nvidia_smi_output_never_panics_blocks_or_dirties_the_name") + "). " + c("scripts/test-remote.sh --actions") + " goes through real ssh into containers with sudo users (NOPASSWD and with a password) and a systemd test service: "
      "signals and nice on a " + c("sleep") + ", service start, stop, restart, enable and disable, a wrong password, and that no script line reaches the server's logs. Actions are never run against real servers.") + \
    p("<b>The script</b> creates a throwaway ed25519 key and an " + c("ssh_config") + " in a temporary folder and sets " + c("NGTOP_SSH") + " to use it, so your " + sysf("~/.ssh") + " is never read or changed. Options: " + c("NGTOP_BIN") + " (the binary, default "
      + c("dist/ngtop") + "), " + c("CONTAINERS") + " (default “alpine debian systemd”), " + c("MEASURE_SECS") + " (30). Logs go to the folder given as argument (default " + c("/tmp/ngtop-remote-logs") + "). Without options it prints a table of OK and ERROR. "
      "With " + c("--measure") + " (or " + c("scripts/measure-remote.sh") + ") it measures bytes per update, latency, the CPU of ngtop and the CPU of the server at a 1 s interval, 30 s per container: the figures of " + rif("Remote measurements") + ".") + \
    tip("Run " + c("build.sh") + " first: like " + c("test-distro.sh") + ", the script tests the binary in " + c("dist/") + ".", "Order.")

S8 = p("Version 1.7.0-rc1 was written and tested only on sample outputs of " + c("nvidia-smi") + ". Before the final 1.7.0, this list must be done on real hardware.", lead=True) + \
    ol([
        "<b>Values.</b> Run ngtop and, by hand, " + c("nvidia-smi --query-gpu=utilization.gpu,memory.used,memory.total,temperature.gpu,power.draw,fan.speed,clocks.gr,clocks.mem --format=csv") + ". Compare usage, VRAM, temperature, power, fan, clocks, PCIe and encoder/decoder, under load and idle.",
        "<b>Time of each call.</b> Time " + c("nvidia-smi --query-gpu=... ") + " and " + c("nvidia-smi pmon -c 1 -s um") + " locally, then through " + c("ssh") + " (with " + c("time") + "). Check that the load of the ~2 s loop is small and that on a remote machine the connection is not held too long.",
        "<b>pmon on GeForce and on a professional card.</b> See whether " + c("pmon") + " works on both; on GeForce check the fallback to " + c("--query-compute-apps") + " (retried every 30 rounds) and the note “only CUDA processes are listed on this GPU” in the title of the process table when that card is selected. Run a graphics app (a game, a browser with GPU) and a CUDA program, and see which appear.",
        "<b>Hybrid laptop with the NVIDIA card asleep.</b> Read " + c("/sys/class/drm/card*/device/power/runtime_status") + " before and after starting ngtop and after a minute: it must stay " + c("suspended") + ", and " + c("nvidia-smi") + " must not be started (the pre-check and the " + c("sh") + " guard prevent it; check with " + c("ps") + " or " + c("strace -f -e execve") + "). Then wake the card with a program and see the numbers appear.",
        "<b>Hybrid laptop, runtime suspend.</b> Check that polling " + c("nvidia-smi") + " every 2 s with " + c("-i <bus id>") + " does not keep the discrete GPU from entering runtime suspend (" + c("power/runtime_status") + " staying " + c("active") + " while idle). If it does, poll less often or only while the GPU section is visible.",
        "<b>Old driver.</b> On a driver that refuses some fields, check the fallback to the basic list and that the other values are right.",
        "<b>nvidia-smi missing.</b> Rename or remove it (or use a machine with the driver but not the utilities): the panel must say “detected — install nvidia-smi for metrics”. Also try with nouveau: “not supported”.",
        "<b>nvidia-smi not responding.</b> Replace it with a script that sleeps 10 s: after 3 s the panel says “not responding”, the interface stays smooth, the retry pause grows, and when the script is removed the numbers come back.",
        "<b>Two cards.</b> Check the match by PCI address (each panel with its own values and processes) and the key " + key("d") + ".",
        "<b>Graphics processes.</b> Check type (Graphics/Compute), VRAM per process and the sort by GPU and VRAM in the GPU section.",
        "<b>Security suite.</b> " + c("cargo test --test hostile") + ", the container script with " + c("--actions") + " and the sabotage review, as for every release. Remote: monitor a machine with an NVIDIA card through " + c("ssh") + " and, with " + c("--sudo") + ", check the processes of other users.",
    ])

CHAPTER = ("Testing", [
    ("Kinds of test", S1),
    ("Fixtures and two samples", S2),
    ("Seeing the screen: snapshot, dump, sweep", S3),
    ("Tests that guard the promises", S4),
    ("Distributions", S5),
    ("Testing remote monitoring", S7),
    ("What to run", S6),
    ("NVIDIA: checklist for tests on real hardware", S8),
])
