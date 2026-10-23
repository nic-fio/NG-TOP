# Copyright (c) 2026 Nicola Fiorillo
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
"""User Manual, chapter 9 — monitoring a remote machine over ssh.
Sources: README.md (Remote machines), src/config.rs (USAGE, parse_args, host key), src/main.rs (connect_remote),
src/remote/ (mod.rs: NGTOP_SSH, target.rs: Target::parse, session.rs: ssh options, source.rs: reconnection),
src/ui/view.rs (tab bar, status line, latency_color), src/ui/app.rs (adapt_interval, remote_blocked, REMOTE_NA),
src/collect/services/remote.rs (REMOTE_ACTION_MSG, REMOTE_USER_MSG)."""
from build import c, code, key, note, p, rif, steps, table, term, tip, ui, ul, warn

S_USE = p("ngtop can show <b>another machine</b> instead of the one you sit at. It uses the <code>ssh</code> command you already have, "
          "and it installs <b>nothing</b> on the other machine.", lead=True) + \
    term("""$ ngtop alex@server
$ ngtop --host ssh://alex@10.0.0.5:2222 -s 3
$ ngtop --dump alex@server""") + \
    p("The machine you want to watch is called the <b>target</b>. You can write it in three ways:") + \
    table(["Form", "Example", "Meaning"], [
        [c("host"), c("ngtop server"), "A name or an address. The user and the port come from your ssh configuration, or are the usual ones."],
        [c("user@host"), c("ngtop alex@server"), "The same, with the user name."],
        [c("ssh://user@host:port"), c("ngtop ssh://alex@10.0.0.5:2222"), "A URL, the only form that can carry a port. An IPv6 address goes in brackets."],
    ], "«TAB» — How to write a target") + \
    p("The target can be given as the last argument, or with " + c("--host TARGET") + ", or once and for all in the configuration file "
      "with the key " + c("host") + " (" + rif("Every setting") + "). The command line wins over the file. To look at your own machine even if "
      "the file names a host, use " + c("--local") + ".") + \
    p("Everything else works as usual: the same five sections, the same keys, the same options (" + c("-d") + ", " + c("-s") + ", " + c("--256") + "…).")

S_NEED = p("Remote monitoring asks very little of both machines.", lead=True) + \
    table(["Where", "What is needed"], [
        ["Your PC", "The <code>ssh</code> command, as you use it every day. ngtop uses your keys, your ssh agent, your <code>~/.ssh/config</code> "
         "(including <code>ProxyJump</code>) and your <code>known_hosts</code>."],
        ["The server", "An ssh server and a POSIX shell with <code>cat</code>. The server must run <b>Linux</b>, because ngtop reads the same files "
         "as on the local machine. No ngtop, no agent, no extra package."],
    ], "«TAB» — What remote monitoring needs") + \
    note("<b>Nothing is installed and nothing is written on the server.</b> ngtop sends a small shell script through the ssh connection. "
         "It lives only in the memory of the remote shell and goes away when the connection ends. No file is created on the server's disk.",
         "Nothing is left behind.")

S_CONNECT = p("ngtop opens the connection <b>before</b> it draws the interface, in the same terminal. So ssh can ask you what it usually asks:", lead=True) + \
    term("""$ ngtop alex@server
Connecting to alex@server…
alex@server's password:""") + \
    ul(["a <b>password</b> or the <b>passphrase</b> of your key;",
        "the confirmation of the <b>host key</b>, the first time you meet that server (<code>Are you sure you want to continue connecting?</code>)."]) + \
    p("When the connection is ready, the interface appears. If it cannot connect (host unreachable, wrong password, host key refused), ngtop prints "
      "the reason and exits with code 2, without ever opening the screen:") + \
    term("""ngtop: cannot connect to alex@server: ssh: connect to host server port 22: Connection refused""") + \
    p("ngtop starts ssh without a terminal of its own (option <code>-T</code>), asks it to compress the data (<code>-C</code>) and to check "
      "that the link is alive (<code>ServerAliveInterval=15</code>, three misses and the link is declared dead). A single connection is kept open for the "
      "whole session.")

S_BAR = p("On a remote machine the left side of the tab bar looks like the usual one, and the right side changes:", lead=True) + \
    term("""⇄ server 23 ms │ up 41d 02:10 │ load 0.20 0.31 0.29 │ every 1s │ 14:52:07""") + \
    table(["Part", "Meaning"], [
        [ui("⇄ server"), "The symbol of a remote session and the <b>host name of the server</b> (as the server calls itself; if it is not known yet, the target "
         "you typed). The tab bar always carries this mark, so you never take a remote screen for your own."],
        [ui("23 ms"), "The <b>latency</b>: the time of one round trip to the server. <b>Green</b> below 50 ms, <b>yellow</b> below 200 ms, <b>red</b> above."],
        [ui("offline"), "In red, instead of the latency: the connection is down (see below)."],
        ["The rest", "Uptime, load average, refresh interval and clock are those <b>of the server</b>."],
    ], "«TAB» — The tab bar of a remote session") + \
    p("The help window (" + key("F1") + ") says <code>Help — server</code> in its title.")

S_LOST = p("Networks fail. When the link to the server drops, ngtop does not quit and does not freeze. It shows, in yellow on the "
           "last line, a message that stays until the link is back:", lead=True) + \
    term("""connection lost — reconnecting… (broken pipe)""", "Status line") + \
    p("The tab bar shows " + ui("offline") + " in place of the latency, and ngtop tries again by itself. The pause between the tries grows, up to ten seconds. "
      "When the server answers, the message disappears and the data flow again.") + \
    warn("The automatic reconnection runs in the background, where ssh <b>cannot ask questions</b> (it runs in batch mode). So it works only if ssh "
         "can log in <b>by itself</b>: with a <b>key without passphrase</b> or with a key loaded in an <b>ssh agent</b>. If the server needs a password every "
         "time, the first connection works, because you type the password, but after a drop the reconnection fails and ngtop stays offline: quit with " +
         key("q") + " and start it again.", "Passwords and reconnection.") + \
    p("Some failures are final. If an automatic reconnection is refused with <code>Permission denied</code>, <code>Host key verification failed</code> or "
      "<code>REMOTE HOST IDENTIFICATION HAS CHANGED</code>, trying again would be useless: ngtop stops and the last line says in red "
      "<code>connection failed: REASON — press q to quit</code>. Press " + key("q") + " to leave. In all other cases (network down, server off) ngtop "
      "keeps trying, with waits that grow up to ten seconds.")

S_NOT = p("In this version a remote machine is for <b>looking</b>. The actions that change something on the server are not available. If you try, the "
          "status line says so in red and nothing is sent.", lead=True) + \
    table(["What you try", "What happens"], [
        [key("F9") + " (signal), and in " + ui("Processes") + " " + key("k") + ", " + key("F7") + ", " + key("F8") + " (signal, nice)", ui("Not available on remote machines in this version")],
        ["The keys " + key("s") + " " + key("t") + " " + key("r") + " " + key("e") + " of " + ui("Services") + " (start, stop, restart, enable, disable)", ui("Not available on remote machines in this version")],
        ["The " + ui("user") + " units of " + ui("Services") + " (key " + key("u") + ")", ui("User units are not available on remote machines")],
    ], "«TAB» — What does not work on a remote machine") + \
    p("Everything else is available: all five sections, the details of a process, the filter, the sorting, the connections, the journal.")

S_SERVICES = p("The " + ui("Services") + " section works on a remote machine too, but it does not use D-Bus: it runs the commands " +
               c("systemctl") + ", " + c("journalctl") + ", " + c("loginctl") + " and " + c("systemd-analyze") + " on the server, through the connection, "
               "and reads their answers. The server must therefore use systemd.", lead=True) + \
    ul(["The list, the details, the boot time and the sessions look as in the local case (" + rif("The Services screen") + ").",
        "The <b>journal</b> is read with the rights of the <b>user you log in as</b>. If that user is not in the group <code>systemd-journal</code>, "
        "the panel is empty (" + rif("The journal") + "). Log in as a user who can read it, or as root.",
        "Starting, stopping and enabling are not available (see above), and neither are the units of the user."])

S_SLOW = p("On a slow link a full update can take longer than the refresh interval. ngtop does not pile up requests: if three updates in a row are "
           "too slow, it <b>widens the interval by one step</b> (250, 500, 1000, 2000, 5000 ms) and tells you:", lead=True) + \
    term("""remote update takes 1350 ms: interval widened to 2s""", "Status line") + \
    p("One slow update (for example just after a reconnection) is not enough to trigger it. The interval never goes above 5 seconds. You can shorten it "
      "again with " + key("-") + " if the link has improved. Remember that " + key("W") + " saves the <b>current</b> interval, widened or not.")

S_COST = p("A remote session is light on both sides.", lead=True) + \
    ul(["<b>One request for each update.</b> ngtop asks for everything it needs in a single exchange, so one round trip is enough, whatever the "
        "number of files read.",
        "<b>A few kilobytes.</b> With the compression of ssh, an update takes 2 to 3 KB on the network (measured with a refresh interval of one second).",
        "<b>Low load on the server.</b> The script uses <code>cat</code> and little else: in the tests, 0.4 to 0.7% of a CPU for the shell process plus 0.1 to 0.3% for <code>sshd</code>."]) + \
    note("These figures are measures, not guarantees: they grow with the number of processes and connections of the server, and shrink when you lengthen "
         "the interval.")

S_DUMP = p("For a quick look, without the interface, " + c("--dump") + " accepts a target too:", lead=True) + \
    term("""$ ngtop --dump alex@server
CPU: Intel(R) Xeon(R) Silver 4210 (20 logical, 10 physical)
  total   4.2%
...""", "Console") + \
    p("It is the same text summary as in " + rif("A text summary with --dump") + ", but of the server. It connects, prints once and exits; "
      "against a remote server it took about 1.7 seconds in the tests.")

S_SSHCMD = p("By default ngtop runs the command <code>ssh</code>. The variable " + c("NGTOP_SSH") + " tells it to run another command, with its own "
             "arguments. This is how you use another configuration file, a special key, or a wrapper.", lead=True) + \
    term("""$ NGTOP_SSH="ssh -F ~/.ssh/other_config" ngtop alex@server
$ NGTOP_SSH="ssh -i ~/.ssh/monitor_key" ngtop server""") + \
    ul(["The value is split at the spaces. A path that contains a space cannot be used directly: put the options in <code>~/.ssh/config</code> instead.",
        "ngtop adds its own options after yours (<code>-T</code>, the keep-alive, <code>-C</code>, the port if the target has one), then the target.",
        "If you can do it with a <code>Host</code> block in <code>~/.ssh/config</code> (user, port, key, ProxyJump), that is the simplest way, and you "
        "do not need the variable."])

S_FILE = p("To always watch the same server, put it in the configuration file:", lead=True) + \
    code('host = "alex@server"', "toml", "config.toml") + \
    p("Then " + c("ngtop") + " alone shows that server, and " + c("ngtop --local") + " shows your own machine. A target on the command line "
      "replaces the one in the file. Pressing " + key("W") + " saves only the interval and the section: it leaves the " + c("host") + " line as it is.")

CHAPTER = ("Remote machines", [
    ("Monitoring another machine", S_USE),
    ("What you need", S_NEED),
    ("Connecting", S_CONNECT),
    ("The tab bar of a remote session", S_BAR),
    ("Lost connection and reconnection", S_LOST),
    ("What does not work remotely", S_NOT),
    ("Services on a remote machine", S_SERVICES),
    ("Slow networks", S_SLOW),
    ("How much it costs", S_COST),
    ("A remote text summary", S_DUMP),
    ("Choosing the ssh command", S_SSHCMD),
    ("A fixed host in the configuration file", S_FILE),
])
