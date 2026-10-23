'use strict';
// ngtop – motore comune dei mockup: dati finti, grafici braille, barre, tabelle, animazione.
const NG = (() => {
  const PAGES = [
    ['01-classico.html', 'Classico', 'btop + nvtop in una schermata'],
    ['02-prestazioni.html', 'Prestazioni', 'come la scheda Prestazioni di Windows'],
    ['03-processi.html', 'Processi', 'come la scheda Processi di Windows'],
    ['04-gpu.html', 'GPU', 'vista dedicata alle schede video (nvtop)'],
    ['05-compatto.html', 'Compatto', '80×24 e barra a riga singola'],
    ['06-cpu-sensori.html', 'CPU e sensori', 'mappa di calore dei core, temperature, energia'],
    ['07-albero.html', 'Albero processi', 'albero, dettaglio del processo, invio segnali'],
    ['08-rete-disco.html', 'Rete e disco', 'interfacce, connessioni, I/O, filesystem'],
    ['09-servizi.html', 'Servizi', 'systemd, avvio, utenti e registro'],
    ['10-cronologia.html', 'Cronologia', 'storico, eventi e consumi per app'],
  ];

  // ---------- colori ----------
  const G = {
    cpu: ['#3fb950', '#d29922', '#f85149'], mem: ['#1b7c83', '#39c5cf', '#a5f3fc'],
    used: ['#56d364', '#e3b341', '#f85149'], gpu: ['#8957e5', '#bc8cff', '#f778ba'],
    net: ['#1f6feb', '#58a6ff', '#a5d6ff'], up: ['#8250df', '#d2a8ff', '#f0b6ff'],
    disk: ['#9e6a03', '#e3b341', '#f0883e'], wr: ['#bd561d', '#f0883e', '#ffa657'],
    temp: ['#58a6ff', '#3fb950', '#d29922', '#f85149'], heat: ['#0d1f12', '#1a4d24', '#7a5c00', '#a33a1f', '#d1242f'],
    proc: ['#9a4b10', '#f0883e', '#ffc680'], white: ['#6e7681', '#c9d1d9', '#ffffff'],
  };
  const hex = c => [1, 3, 5].map(i => parseInt(c.substr(i, 2), 16));
  function grad(name, t) {
    const g = Array.isArray(name) ? name : (G[name] || G.cpu);
    t = Math.max(0, Math.min(1, t));
    const p = t * (g.length - 1), i = Math.min(g.length - 2, Math.floor(p)), f = p - i;
    const a = hex(g[i]), b = hex(g[i + 1]);
    return `rgb(${a.map((v, k) => Math.round(v + (b[k] - v) * f)).join(',')})`;
  }

  // ---------- serie temporali finte (random walk) ----------
  const S = {};
  const randn = () => (Math.random() + Math.random() + Math.random() - 1.5) * 1.4;
  function step(s) {
    s.x += (s.base - s.x) * 0.12 + randn() * s.vol;
    if (Math.random() < 0.02) s.x += s.vol * 4;
    s.x = Math.max(0.005, Math.min(0.995, s.x));
    s.v.push(s.x); if (s.v.length > 700) s.v.shift();
  }
  function series(key, base = 0.3, vol = 0.08) {
    if (S[key]) return S[key];
    const s = S[key] = { v: [], base, vol, x: base };
    for (let i = 0; i < 600; i++) step(s);
    return s;
  }
  const def = o => Object.entries(o).forEach(([k, [b, v]]) => series(k, b, v));
  const cur = k => series(k).x;
  const ser = el => series(el.dataset.key, +(el.dataset.base ?? 0.3), +(el.dataset.vol ?? 0.08));

  // ---------- grafico braille (2×4 punti per cella) ----------
  const UP = [[0x40, 0x04, 0x02, 0x01], [0x80, 0x20, 0x10, 0x08]];   // dal basso verso l'alto
  const DN = [[0x01, 0x02, 0x04, 0x40], [0x08, 0x10, 0x20, 0x80]];   // dall'alto verso il basso
  function brailleLines(vals, w, h, down) {
    const out = [];
    for (let r = 0; r < h; r++) {
      const rb = down ? r : h - 1 - r;
      let line = '';
      for (let c = 0; c < w; c++) {
        let bits = 0;
        for (let side = 0; side < 2; side++) {
          const L = Math.round((vals[c * 2 + side] ?? 0) * h * 4);
          for (let k = 0; k < 4; k++) if (L > rb * 4 + k) bits |= (down ? DN : UP)[side][k];
        }
        line += String.fromCharCode(0x2800 + bits);
      }
      out.push([line, rb]);
    }
    return out;
  }
  // nel mockup i punti braille sono disegnati in SVG su una griglia esatta di celle (1ch × 1 riga),
  // così non dipendono dal font del browser; nel terminale vero saranno caratteri U+2800–U+28FF
  function braille(el) {
    const s = ser(el), w = +el.dataset.w, h = +(el.dataset.h || 4), down = el.hasAttribute('data-down');
    const g = el.dataset.grad || 'cpu', vals = s.v.slice(-w * 2);
    const rows = Array.from({ length: h }, () => '');
    for (let x = 0; x < w * 2; x++) {
      const L = Math.round((vals[x] ?? 0) * h * 4);
      for (let k = 0; k < L; k++) {
        const y = down ? k : h * 4 - 1 - k, rb = Math.floor(k / 4);
        rows[rb] += `M${x + 0.2} ${y + 0.15}h.62v.62h-.62z`;
      }
    }
    el.innerHTML = `<svg viewBox="0 0 ${w * 2} ${h * 4}" preserveAspectRatio="none" style="display:block;width:${w}ch;height:${h * 1.25}em">` +
      rows.map((d, rb) => d && `<path fill="${grad(g, (rb + 0.5) / h)}" d="${d}"/>`).join('') + '</svg>';
  }

  // ---------- barre e sparkline ----------
  function bar(frac, w, g = 'cpu', ch = '■', empty = '#21262d') {
    const n = Math.round(frac * w); let s = '';
    for (let i = 0; i < w; i++) s += i < n ? `<span style="color:${grad(g, i / Math.max(1, w - 1))}">${ch}</span>` : `<span style="color:${empty}">${ch}</span>`;
    return s;
  }
  function meter(el) {
    const f = el.dataset.key ? ser(el).x : +el.dataset.v / 100;
    el.innerHTML = bar(f, +el.dataset.w || 10, el.dataset.grad || 'cpu', el.dataset.ch || '■');
  }
  const SP = '▁▂▃▄▅▆▇█';
  function spark(el) {
    const s = ser(el), w = +el.dataset.w || 20, g = el.dataset.grad || 'cpu';
    el.innerHTML = s.v.slice(-w).map(v => `<span style="color:${grad(g, v)}">${SP[Math.min(7, Math.floor(v * 8))]}</span>`).join('');
  }
  function num(el) {
    const v = cur(el.dataset.num) * +(el.dataset.scale ?? 100) + +(el.dataset.off ?? 0);
    el.textContent = v.toFixed(+(el.dataset.dec ?? 0)).padStart(+(el.dataset.pad ?? 0)) + (el.dataset.unit ?? '');
  }

  // ---------- numeri grandi su 3 righe (mezzi blocchi, realizzabili in un terminale) ----------
  const BIG = {
    0: ['█▀█', '█ █', '▀▀▀'], 1: ['▀█ ', ' █ ', '▀▀▀'], 2: ['▀▀█', '█▀▀', '▀▀▀'], 3: ['▀▀█', ' ▀█', '▀▀▀'],
    4: ['█ █', '▀▀█', '  ▀'], 5: ['█▀▀', '▀▀█', '▀▀▀'], 6: ['█▀▀', '█▀█', '▀▀▀'], 7: ['▀▀█', '  █', '  ▀'],
    8: ['█▀█', '█▀█', '▀▀▀'], 9: ['█▀█', '▀▀█', '▀▀▀'], '.': [' ', ' ', '▀'], ',': [' ', ' ', '▀'],
    ':': ['▄', ' ', '▀'], ' ': [' ', ' ', ' '], '%': ['▀ █', ' █ ', '█ ▄'], '/': ['  █', ' █ ', '█  '],
  };
  function bigtext(str, unit = '') {
    const ch = [...String(str)].map(c => BIG[c] || BIG[' ']);
    return [0, 1, 2].map(r => ch.map(c => c[r]).join(' ') + (r === 2 && unit ? ` <span class="dim">${unit}</span>` : '')).join('\n');
  }
  function big(el) {
    const txt = el.dataset.big !== undefined && el.dataset.big !== ''
      ? (cur(el.dataset.big) * +(el.dataset.scale ?? 100) + +(el.dataset.off ?? 0)).toFixed(+(el.dataset.dec ?? 0))
      : el.dataset.txt;
    el.innerHTML = bigtext(txt, el.dataset.unit);
  }

  // ---------- core della CPU ----------
  const NCORE = 24;
  for (let i = 0; i < NCORE; i++) series('c' + i, 0.08 + Math.random() * 0.4, 0.07);
  function cores(el) {
    const n = +(el.dataset.n || NCORE), w = +(el.dataset.w || 8), sp = el.hasAttribute('data-spark');
    const a = +(el.dataset.from || 0); let h = '';
    for (let i = a; i < a + n; i++) {
      const v = cur('c' + i), ghz = (3.0 + v * 2.6).toFixed(1);
      const vis = sp ? S['c' + i].v.slice(-w).map(x => `<span style="color:${grad('cpu', x)}">${SP[Math.min(7, Math.floor(x * 8))]}</span>`).join('') : bar(v, w, 'cpu');
      h += `<div class="pre"><span class="dim">C${String(i).padEnd(2)}</span> ${vis} ${(v * 100).toFixed(0).padStart(3)}%${el.hasAttribute('data-ghz') ? ` <span class="dim">${ghz}</span>` : ''}</div>`;
    }
    el.innerHTML = h;
  }

  // ---------- processi finti ----------
  const P = (pid, ppid, user, name, cpu, mem, thr, o = {}) => ({ pid, ppid, user, name, cpu, mem, thr, gpu: 0, vram: 0, disk: 0, net: 0, grp: 'bg', ...o });
  const procs = [
    P(1, 0, 'root', 'systemd', 0.1, 14, 1, { cmd: '/sbin/init splash', grp: 'sys' }),
    P(402, 1, 'root', 'systemd-journald', 0.2, 52, 1, { grp: 'sys', disk: 0.1 }),
    P(431, 1, 'root', 'systemd-udevd', 0, 9, 1, { grp: 'sys' }),
    P(688, 1, 'root', 'NetworkManager', 0.1, 21, 4, { grp: 'sys', cmd: '/usr/sbin/NetworkManager --no-daemon' }),
    P(702, 1, 'root', 'irqbalance', 0, 4, 2, { grp: 'sys' }),
    P(901, 1, 'root', 'sshd', 0, 8, 1, { grp: 'sys', cmd: 'sshd: /usr/sbin/sshd -D' }),
    P(1022, 1, 'root', 'dockerd', 0.4, 96, 22, { grp: 'sys', cmd: '/usr/bin/dockerd -H fd://' }),
    P(1290, 1022, 'root', 'containerd', 0.2, 48, 14, { grp: 'sys' }),
    P(1305, 1290, '999', 'postgres', 0.6, 186, 1, { cmd: 'postgres: checkpointer', disk: 0.8 }),
    P(1402, 1, 'alex', 'systemd', 0, 12, 1, { cmd: '/usr/lib/systemd/systemd --user' }),
    P(1420, 1402, 'alex', 'pipewire', 0.8, 18, 3, {}),
    P(1421, 1402, 'alex', 'wireplumber', 0.1, 22, 5, {}),
    P(1500, 1402, 'alex', 'kwin_wayland', 2.4, 312, 18, { gpu: 6, vram: 380, grp: 'sys', cmd: '/usr/bin/kwin_wayland --wayland-fd 7' }),
    P(1544, 1402, 'alex', 'plasmashell', 1.1, 486, 31, { gpu: 2, vram: 210 }),
    P(2001, 1544, 'alex', 'firefox', 6.2, 1840, 96, { gpu: 4, vram: 420, net: 2.4, grp: 'app', cmd: '/usr/lib/firefox/firefox' }),
    P(2044, 2001, 'alex', 'Isolated Web Co', 3.8, 620, 28, { cmd: 'firefox -contentproc -isForBrowser 3', grp: 'child' }),
    P(2051, 2001, 'alex', 'Isolated Web Co', 1.2, 410, 26, { cmd: 'firefox -contentproc -isForBrowser 5', grp: 'child' }),
    P(2057, 2001, 'alex', 'RDD Process', 0.9, 88, 9, { cmd: 'firefox -contentproc rdd', grp: 'child', gpu: 1 }),
    P(2301, 1544, 'alex', 'konsole', 0.4, 92, 7, { grp: 'app' }),
    P(2320, 2301, 'alex', 'bash', 0, 6, 1, { grp: 'child' }),
    P(2410, 2320, 'alex', 'ngtop', 0.6, 9, 2, { grp: 'child', cmd: './ngtop' }),
    P(2333, 2301, 'alex', 'zsh', 0, 7, 1, { grp: 'child' }),
    P(2602, 2333, 'alex', 'claude', 1.9, 344, 14, { grp: 'child', net: 0.3, cmd: 'node /usr/bin/claude' }),
    P(3100, 1544, 'alex', 'code', 2.1, 980, 44, { grp: 'app', gpu: 1, vram: 120, cmd: '/usr/share/code/code' }),
    P(3122, 3100, 'alex', 'code', 1.3, 610, 21, { grp: 'child', cmd: 'code --type=renderer' }),
    P(3140, 3100, 'alex', 'rust-analyzer', 4.4, 1320, 18, { grp: 'child' }),
    P(3200, 1544, 'alex', 'steam', 1.0, 540, 64, { grp: 'app', net: 0.6, cmd: '/home/alex/.steam/steam.sh' }),
    P(3290, 3200, 'alex', 'Cyberpunk2077.ex', 31.5, 9240, 118, { grp: 'child', gpu: 88, vram: 11420, disk: 14, cmd: 'Z:\\…\\bin\\x64\\Cyberpunk2077.exe' }),
    P(3301, 3200, 'alex', 'wineserver', 2.2, 64, 1, { grp: 'child' }),
    P(3500, 1544, 'alex', 'obs', 3.4, 720, 38, { grp: 'app', gpu: 5, vram: 640, disk: 6.2, cmd: 'obs --startreplaybuffer', enc: 34 }),
    P(3610, 1544, 'alex', 'dolphin', 0.1, 140, 9, { grp: 'app' }),
    P(3702, 1544, 'alex', 'telegram-deskto', 0.5, 380, 30, { grp: 'app', net: 0.1 }),
    P(4011, 2320, 'alex', 'cargo', 0.3, 160, 6, { grp: 'child', cmd: 'cargo build --release' }),
    P(4019, 4011, 'alex', 'rustc', 12.8, 1460, 24, { grp: 'child', disk: 3.1, cmd: 'rustc --crate-name ngtop --edition=2021' }),
    P(4090, 1402, 'alex', 'syncthing', 0.7, 110, 19, { net: 1.1, disk: 0.4 }),
    P(4120, 1, 'root', 'cupsd', 0, 11, 1, { grp: 'sys' }),
    P(4188, 1, 'root', 'smartd', 0, 5, 1, { grp: 'sys' }),
    P(4301, 1402, 'alex', 'baloo_file', 0.2, 74, 3, { disk: 1.4 }),
    P(4402, 1402, 'alex', 'kdeconnectd', 0.1, 58, 8, { net: 0.05 }),
  ];
  const byPid = Object.fromEntries(procs.map(p => [p.pid, p]));
  function live() {
    const j = x => x * Math.max(0, 1 + randn() * 0.28);
    for (const p of procs) {
      p.c = j(p.cpu); p.m = p.mem * (1 + randn() * 0.004); p.g = p.gpu ? Math.min(99, j(p.gpu)) : 0;
      p.d = j(p.disk); p.n = j(p.net); p.e = p.enc ? j(p.enc) : 0;
    }
  }
  live();
  const fmtMB = mb => mb >= 1024 ? (mb / 1024).toFixed(1) + 'G' : mb.toFixed(0) + 'M';
  const fmtRate = (v, u = 'MB/s') => v < 0.05 ? '0' : v.toFixed(1) + (u ? ' ' + u : '');
  const t0 = {}; let tick = 0;
  const ptime = p => { const s = Math.round(p.cpu * 3600 + p.pid * 7) + tick; return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`; };
  const COLS = {
    pid: { h: 'PID', r: 1, f: p => p.pid, v: p => p.pid },
    ppid: { h: 'PPID', r: 1, f: p => p.ppid },
    user: { h: 'Utente', f: p => p.user },
    name: { h: 'Nome', f: p => p.name },
    cmd: { h: 'Comando', f: p => `<span class="dim clip">${p.cmd || p.name}</span>` },
    cpu: { h: 'CPU%', r: 1, f: p => p.c.toFixed(1), v: p => p.c, max: 40 },
    cbar: { h: '', f: p => bar(Math.min(1, p.c / 40), 5, 'cpu') },
    mem: { h: 'Mem', r: 1, f: p => fmtMB(p.m), v: p => p.m, max: 4000 },
    memp: { h: 'Mem%', r: 1, f: p => (p.m / 327.68).toFixed(1), v: p => p.m },
    thr: { h: 'Thr', r: 1, f: p => p.thr },
    gpu: { h: 'GPU%', r: 1, f: p => p.gpu ? p.g.toFixed(0) : '<span class="faint">-</span>', v: p => p.g, max: 100 },
    vram: { h: 'VRAM', r: 1, f: p => p.vram ? fmtMB(p.vram) : '<span class="faint">-</span>', v: p => p.vram },
    disk: { h: 'Disco', r: 1, f: p => fmtRate(p.d), v: p => p.d, max: 15 },
    net: { h: 'Rete', r: 1, f: p => fmtRate(p.n, 'Mb/s'), v: p => p.n, max: 5 },
    st: { h: 'S', f: p => p.c > 5 ? '<span class="c-grn">R</span>' : 'S' },
    ni: { h: 'NI', r: 1, f: p => p.pid === 4019 ? '<span class="c-yel">10</span>' : 0 },
    time: { h: 'Tempo+', r: 1, f: ptime },
    dev: { h: 'DEV', r: 1, f: p => p.pid === 2057 ? '1' : '0' },
    gtype: { h: 'Tipo', f: p => p.enc ? '<span class="c-yel">Grafica+Codif.</span>' : p.name === 'rust-analyzer' ? 'Calcolo' : '<span class="c-gpu">Grafica</span>' },
    enc: { h: 'ENC%', r: 1, f: p => p.enc ? p.e.toFixed(0) : '<span class="faint">-</span>' },
    gbar: { h: '', f: p => bar(Math.min(1, p.g / 100), 8, 'gpu') },
  };
  function treeOrder() {
    const kids = {}; procs.forEach(p => (kids[p.ppid] ||= []).push(p));
    const out = [];
    (function walk(pid, pre) {
      const ch = (kids[pid] || []).sort((a, b) => a.pid - b.pid);
      ch.forEach((p, i) => {
        const last = i === ch.length - 1;
        p._pre = pid === 0 ? '' : pre + (last ? '└─ ' : '├─ ');
        out.push(p);
        walk(p.pid, pid === 0 ? '' : pre + (last ? '   ' : '│  '));
      });
    })(0, '');
    return out;
  }
  function table(el) {
    const cols = el.dataset.cols.split(','), n = +(el.dataset.rows || 20), tree = el.hasAttribute('data-tree');
    const sort = el.dataset.sort || 'cpu', heat = el.hasAttribute('data-heat');
    let list = tree ? treeOrder() : [...procs].sort((a, b) => (COLS[sort].v(b) - COLS[sort].v(a)));
    if (el.dataset.filter === 'gpu') list = list.filter(p => p.vram > 0);
    list = list.slice(0, n);
    const selPid = +(el.dataset.sel || 0) || (el.dataset.selrow ? list[+el.dataset.selrow]?.pid : 0);
    let h = '<tr>' + cols.map(c => `<th class="${COLS[c].r ? 'r' : ''} ${c === sort && !tree ? 's' : ''}">${COLS[c].h}${c === sort && !tree ? '▼' : ''}</th>`).join('') + '</tr>';
    for (const p of list) {
      h += `<tr class="${p.pid === selPid ? 'sel' : ''}">` + cols.map(c => {
        const d = COLS[c]; let v = d.f(p);
        if (c === 'name' && tree) v = `<span class="dim">${p._pre}</span>${v}`;
        let st = '';
        if (heat && d.max && p.pid !== selPid) st = ` style="background:${grad('heat', Math.min(1, d.v(p) / d.max))}"`;
        return `<td class="${d.r ? 'r' : ''}"${st}>${v}</td>`;
      }).join('') + '</tr>';
    }
    el.innerHTML = h;
  }

  // ---------- ciclo ----------
  const hooks = [];
  function render() {
    document.querySelectorAll('.gr[data-key]').forEach(braille);
    document.querySelectorAll('.mt').forEach(meter);
    document.querySelectorAll('.sp[data-key]').forEach(spark);
    document.querySelectorAll('[data-num]').forEach(num);
    document.querySelectorAll('.cores').forEach(cores);
    document.querySelectorAll('.bg').forEach(big);
    document.querySelectorAll('table.pt[data-cols]').forEach(table);
    document.querySelectorAll('[data-clock]').forEach(e => e.textContent = new Date().toLocaleTimeString('en-GB'));
    hooks.forEach(f => f());
  }
  function nav() {
    const el = document.getElementById('nav'); if (!el || !el.dataset.i) return;
    const i = +el.dataset.i - 1, [, name, sub] = PAGES[i];
    const prev = PAGES[(i + 9) % 10][0], next = PAGES[(i + 1) % 10][0];
    el.innerHTML = `<a href="index.html">☰ Indice</a><h1>${String(i + 1).padStart(2, '0')} · ${name}</h1><span>${sub}</span><span class="grow"></span><a href="${prev}">◀ precedente</a><a href="${next}">successivo ▶</a>`;
  }
  function start(f) {
    if (f) hooks.push(f);
    nav();
    // registra prima le serie dichiarate dai grafici (che portano base/vol)
    document.querySelectorAll('[data-key]').forEach(ser);
    render();
    setInterval(() => { Object.values(S).forEach(s => s.frozen || step(s)); live(); tick++; render(); }, 1000);
  }
  return { redraw: render, start, def, bigtext, series, cur, bar, grad, brailleLines, procs, byPid, COLS, fmtMB, fmtRate, PAGES, SP, S };
})();
