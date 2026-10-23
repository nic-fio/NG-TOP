'use strict';
// ngtop – terminale dimostrativo per le pagine del sito: schermate in inglese animate dal motore NG (tui.js).
// Uso:  NGT.mount(elemento, 'overview' | 'gpu' | 'processes' | 'network' | 'services' | 'compact', {title, tabs})
//       NGT.start()   (una volta, dopo i mount)
const NGT = (() => {
  // testi delle colonne in inglese e dati anonimi (niente nomi o percorsi reali)
  const H = { pid: 'PID', ppid: 'PPID', user: 'User', name: 'Name', cmd: 'Command', cpu: 'CPU%', mem: 'Mem', memp: 'Mem%',
    thr: 'Thr', gpu: 'GPU%', vram: 'VRAM', disk: 'Disk', net: 'Net', st: 'S', ni: 'NI', time: 'Time+', dev: 'DEV',
    gtype: 'Type', enc: 'ENC%', cbar: '', gbar: '' };
  for (const [k, v] of Object.entries(H)) if (NG.COLS[k]) NG.COLS[k].h = v;
  NG.COLS.gtype.f = p => p.enc ? '<span class="c-yel">Graphics+Enc</span>' : p.name === 'rust-analyzer' ? 'Compute' : '<span class="c-gpu">Graphics</span>';

  const SECTIONS = ['Overview', 'GPU', 'Processes', 'Network & Disks', 'Services'];
  const IDX = { overview: 0, compact: 0, gpu: 1, processes: 2, network: 3, services: 4 };
  const KEYS = {
    overview: [['F1', 'Help'], ['F3', 'Search'], ['F4', 'Filter'], ['F5', 'Tree'], ['F6', 'Sort'], ['F9', 'Signal'], ['+/-', 'Interval'], ['1-5', 'Sections'], ['q', 'Quit']],
    gpu: [['d', 'Device'], ['F6', 'Sort'], ['F9', 'Signal'], ['+/-', 'Interval'], ['1-5', 'Sections'], ['q', 'Quit']],
    processes: [['F3', 'Search'], ['F4', 'Filter'], ['F5', 'List/Tree'], ['Space', 'Fold'], ['←→', 'Tabs'], ['F7/F8', 'Nice'], ['F9', 'Signal'], ['1-5', 'Sections'], ['q', 'Quit']],
    network: [['i', 'Interface'], ['c', 'TCP/UDP'], ['l', 'LISTEN'], ['Enter', 'Go to process'], ['F6', 'Sort'], ['z', 'Reset peaks/totals'], ['1-5', 'Sections'], ['q', 'Quit']],
    services: [['/', 'Search'], ['f', 'Filter'], ['u', 'System/User'], ['s', 'Start'], ['t', 'Stop'], ['r', 'Restart'], ['e', 'Enable/Disable'], ['j', 'Journal'], ['1-5', 'Sections'], ['q', 'Quit']],
    compact: [['F1', 'Help'], ['/', 'Filter'], ['F5', 'Tree'], ['F6', 'Sort'], ['F9', 'Sig'], ['q', 'Quit']],
  };
  const keys = n => '<div class="keys">' + KEYS[n].map(([k, l]) => `<b>${k}</b><span>${l}</span>`).join('') + '</div>';
  const top = (n) => `<div class="top"><span class="b c-net">ngtop</span><span class="dim"> 1.6 │ </span><div class="tabs">${SECTIONS.map((s, i) => `<span data-s="${i}" class="${i === IDX[n] ? 'on' : ''}">${i + 1} ${s}</span>`).join('')}</div><span class="grow"></span><span class="dim"> workstation │ load </span><span class="c-yel">2.41</span><span class="dim"> │ </span><span data-clock></span></div>`;
  const P = (c, t, body, extra = '') => `<div class="panel" style="--c:${c}"><span class="t">${t}</span>${extra}${body}</div>`;
  const tr = t => `<span class="t tr">${t}</span>`;

  const screens = {
    overview: () => `${top('overview')}
${P('var(--cpu)', 'cpu', `<div class="g" style="grid-template-columns:72ch 1fr">
  <div class="gr" data-key="cpu" data-w="72" data-h="7" data-base=".32" data-vol=".05"></div>
  <div><div class="pre"><span class="b">CPU</span> <span class="mt" data-key="cpu" data-w="18"></span> <span class="b" data-num="cpu" data-pad="3" data-unit="%"></span> <span class="c-yel">62°C</span> <span class="dim">4.71GHz</span></div>
  <div class="g" style="grid-template-columns:repeat(3,1fr);margin-top:.2em"><div class="cores" data-n="6" data-w="4"></div><div class="cores" data-from="6" data-n="6" data-w="4"></div><div class="cores" data-from="12" data-n="6" data-w="4"></div></div></div></div>`, tr('Ryzen 9 7900X · 24 threads'))}
<div class="g" style="grid-template-columns:46ch 1fr">
 <div>
  ${P('var(--mem)', 'mem', `<div class="pre">Used   <span class="mt" data-key="mem" data-base=".45" data-vol=".01" data-w="22" data-grad="used"></span> <span data-num="mem" data-scale="31.3" data-dec="1" data-pad="5" data-unit="G"></span></div>
  <div class="pre">Cache  <span class="mt" data-v="28" data-w="22" data-grad="mem"></span>   8.8G</div>
  <div class="pre">Swap   <span class="mt" data-v="6" data-w="22" data-grad="used"></span>   0.5G</div>`, tr('31.3 GiB'))}
  ${P('var(--gpu)', 'gpu', `<div class="gr" data-key="gpu" data-w="42" data-h="3" data-base=".84" data-vol=".06" data-grad="gpu"></div>
  <div class="pre">Use <span class="mt" data-key="gpu" data-w="12" data-grad="gpu"></span> <span data-num="gpu" data-pad="3" data-unit="%"></span>  VRAM 11.8<span class="dim">/16G</span></div>
  <div class="pre">Temp <span class="c-yel">71°C</span>  Power <span data-num="gpu" data-scale="220" data-pad="3" data-unit="W"></span>  Fan 48%</div>`, tr('Radeon RX 6800'))}
  ${P('var(--net)', 'net', `<div class="g" style="grid-template-columns:28ch 1fr"><div><div class="gr" data-key="rx" data-w="28" data-h="2" data-base=".35" data-vol=".1" data-grad="net"></div><div class="gr" data-key="tx" data-w="28" data-h="2" data-base=".15" data-vol=".06" data-grad="up" data-down></div></div>
  <div><div class="pre c-net">▼ <span data-num="rx" data-scale="12" data-dec="1" data-unit=" MB/s"></span></div><div class="pre dim">  total 12.4G</div><div class="pre" style="color:#d2a8ff">▲ <span data-num="tx" data-scale="4" data-dec="1" data-unit=" MB/s"></span></div><div class="pre dim">  total 1.9G</div></div></div>`, tr('enp5s0'))}
  ${P('var(--disk)', 'disks', `<div class="pre">/      <span class="mt" data-v="44" data-w="20" data-grad="used"></span> 412G<span class="dim">/931G</span></div>
  <div class="pre">/home  <span class="mt" data-v="71" data-w="20" data-grad="used"></span> 1.3T<span class="dim">/1.8T</span></div>`)}
 </div>
 ${P('var(--proc)', 'processes', `<table class="pt" data-cols="pid,name,user,thr,mem,cpu,cbar,gpu,vram" data-rows="18" data-selrow="2"></table>`, tr('filter: _  ‹ cpu ›') + '<span class="t tb">412 processes · 5321 threads</span>')}
</div>${keys('overview')}`,

    gpu: () => `${top('gpu')}
<div class="g" style="grid-template-columns:1fr 1fr">
 ${P('var(--gpu)', 'GPU 0 · AMD Radeon RX 6800', `<div class="pre"><span class="dim">Clock</span> 2405 MHz <span class="dim">Mem</span> 1000 MHz  <span class="dim">PCIe</span> 4.0 ×16</div>
 <div class="pre"><span class="dim">Temp</span> <span class="c-yel">71°C</span> <span class="dim">junction</span> <span class="c-red">84°C</span>  <span class="dim">Fan</span> 48%  <span class="dim">Power</span> <span data-num="gpu" data-scale="220" data-pad="3"></span>/203 W</div>
 <div class="pre">GPU  [<span class="mt" data-key="gpu" data-base=".86" data-vol=".05" data-w="14" data-grad="gpu"></span>] <span data-num="gpu" data-pad="3" data-unit="%"></span>  VRAM [<span class="mt" data-v="74" data-w="10" data-grad="gpu"></span>] 11.8/16G</div>`, tr('Navi 21 · 16 GiB'))}
 ${P('#7d8590', 'GPU 1 · AMD Raphael (integrated)', `<div class="pre"><span class="dim">Clock</span> 400 MHz <span class="dim">Mem</span> 2400 MHz  <span class="dim">state</span> idle</div>
 <div class="pre"><span class="dim">Temp</span> 48°C   <span class="dim">Power</span> 3 W</div>
 <div class="pre">GPU  [<span class="mt" data-key="igpu" data-base=".03" data-vol=".02" data-w="14" data-grad="gpu"></span>] <span data-num="igpu" data-pad="3" data-unit="%"></span>  VRAM [<span class="mt" data-v="9" data-w="10" data-grad="gpu"></span>] 0.05/0.5G</div>`, tr('512 MiB UMA'))}
</div>
<div class="g" style="grid-template-columns:1fr 34ch">
 ${P('var(--gpu)', 'GPU 0 history', `<div class="gr" data-key="gpu" data-w="80" data-h="6" data-grad="gpu"></div><div class="gr" data-key="vram" data-w="80" data-h="2" data-base=".74" data-vol=".01" data-grad="mem"></div>`, tr('use · VRAM'))}
 ${P('var(--gpu)', 'engines', `<div class="pre">3D/gfx    <span class="mt" data-key="gpu" data-w="12" data-grad="gpu"></span> <span data-num="gpu" data-pad="3" data-unit="%"></span></div>
 <div class="pre">Compute   <span class="mt" data-key="comp" data-base=".12" data-vol=".05" data-w="12" data-grad="gpu"></span> <span data-num="comp" data-pad="3" data-unit="%"></span></div>
 <div class="pre">Copy      <span class="mt" data-key="dma" data-base=".08" data-vol=".04" data-w="12" data-grad="gpu"></span> <span data-num="dma" data-pad="3" data-unit="%"></span></div>
 <div class="pre">Encode    <span class="mt" data-key="enc" data-base=".34" data-vol=".04" data-w="12" data-grad="up"></span> <span data-num="enc" data-pad="3" data-unit="%"></span></div>
 <div class="pre">Decode    <span class="mt" data-v="0" data-w="12" data-grad="up"></span>   0%</div>`)}
</div>
${P('var(--proc)', 'GPU processes', `<table class="pt" data-cols="pid,user,dev,gtype,gpu,gbar,enc,vram,cpu,name" data-rows="6" data-sort="vram" data-filter="gpu" data-selrow="0"></table>`, tr('sort ‹ VRAM ›'))}
${keys('gpu')}`,

    processes: () => `${top('processes')}
<div class="g" style="grid-template-columns:62ch 1fr">
 ${P('var(--proc)', 'process tree', `<table class="pt" data-cols="pid,user,cpu,mem,name" data-rows="27" data-tree data-sel="2001"></table>`, tr('filter: user=alex ✕'))}
 <div>
  ${P('var(--net)', 'firefox · PID 2001', `<div class="tabs" style="--c:var(--net);margin-bottom:.4em"><span class="on">General</span><span>Threads 96</span><span>Files 412</span><span>Net 38</span><span>Env</span></div>
  <div class="pre"><span class="dim">Command  </span>/usr/lib/firefox/firefox</div>
  <div class="pre"><span class="dim">Parent   </span>1544 plasmashell</div>
  <div class="pre"><span class="dim">Started  </span>today 07:41 <span class="dim">(1h 21m ago)</span></div>
  <div class="pre"><span class="dim">State    </span><span class="c-grn">S (sleeping)</span>  <span class="dim">nice</span> 0  <span class="dim">prio</span> 20</div>
  <div class="pre"><span class="dim">cgroup   </span>app-firefox@a41f.scope</div>`, tr('alex'))}
  ${P('var(--cpu)', 'CPU', `<div class="gr" data-key="ff" data-base=".15" data-vol=".06" data-w="52" data-h="3"></div>`, tr('subtree 13.3%'))}
  ${P('var(--mem)', 'memory & I/O', `<div class="pre"><span class="dim">RSS </span> 1.84 GiB   <span class="dim">Read </span> 2.1 GiB</div>
  <div class="pre"><span class="dim">PSS </span> 1.52 GiB   <span class="dim">Write</span> 640 MiB</div>
  <div class="pre"><span class="dim">VRAM</span> 420 MiB    <span class="dim">Net  </span> ▼2.4 ▲0.3 Mb/s</div>`)}
 </div>
</div>
<div class="modal" style="left:40ch;top:9em;width:52ch"><span style="position:absolute;top:-.72em;left:1.5ch;background:var(--bg);padding:0 .5ch;color:var(--red);font-weight:700">Send signal</span>
 <div class="pre">Process: <span class="w b">2001 firefox</span> <span class="dim">and 3 children</span></div>
 <div class="pre" style="margin-top:.3em">  ( ) 15 SIGTERM   ask to quit (recommended)</div>
 <div class="pre" style="background:#f8514933;color:#fff"> ▸(•)  9 SIGKILL   stop now, unsaved work lost</div>
 <div class="pre">  ( ) 19 SIGSTOP   pause</div>
 <div class="pre">  ( ) 18 SIGCONT   resume</div>
 <div class="pre" style="margin-top:.3em">  [x] whole subtree (4 processes)</div>
 <div class="pre" style="margin-top:.5em;text-align:right"><span class="btn">Esc: cancel</span>  <span class="btn on">Enter: send SIGKILL</span></div>
</div>
${keys('processes')}`,

    network: () => `${top('network')}
<div class="g" style="grid-template-columns:1fr 44ch">
 ${P('var(--net)', 'enp5s0', `<div class="g" style="grid-template-columns:52ch 1fr"><div><div class="gr" data-key="rx" data-base=".35" data-vol=".1" data-w="52" data-h="3" data-grad="net"></div><div class="gr" data-key="tx" data-base=".15" data-vol=".06" data-w="52" data-h="2" data-grad="up" data-down></div></div>
 <div><div class="pre c-net b">▼ <span data-num="rx" data-scale="12" data-dec="1" data-unit=" MB/s"></span></div><div class="pre dim">peak 98.1 MB/s</div><div class="pre b" style="color:#d2a8ff">▲ <span data-num="tx" data-scale="4" data-dec="1" data-unit=" MB/s"></span></div><div class="pre dim">total 1.9 GiB</div></div></div>`, tr('1 Gbit/s · 192.168.1.20'))}
 ${P('var(--net)', 'interfaces', `<div class="pre dim">Name        Address          State</div>
 <div class="pre hl">enp5s0      192.168.1.20     <span class="c-grn">● up</span></div>
 <div class="pre">wlp6s0      —                <span class="dim">○ down</span></div>
 <div class="pre">tailscale0  100.81.4.22      <span class="c-grn">● up</span></div>
 <div class="pre">docker0     172.17.0.1       <span class="c-grn">● up</span></div>`)}
</div>
${P('var(--proc)', 'connections', `<div class="pre dim">Proto Local                 Remote                 State   PID   Process</div>
<div class="pre hl">TCP   192.168.1.20:51842    151.101.1.140:443      <span class="c-grn">ESTAB</span>   2001  firefox</div>
<div class="pre">TCP   192.168.1.20:44120    160.79.104.10:443      <span class="c-grn">ESTAB</span>   2602  claude</div>
<div class="pre">TCP   192.168.1.20:22000    192.168.1.2:22000      <span class="c-grn">ESTAB</span>   4090  syncthing</div>
<div class="pre">UDP   0.0.0.0:27036         185.25.182.77:27017    —       3200  steam</div>
<div class="pre">TCP   0.0.0.0:22            *                      <span class="c-yel">LISTEN</span>   901  sshd</div>`, tr('TCP+UDP · 38 open'))}
<div class="g" style="grid-template-columns:1fr 1fr">
 ${P('var(--disk)', 'nvme0n1', `<div class="g" style="grid-template-columns:32ch 1fr"><div><div class="gr" data-key="dr" data-base=".2" data-vol=".1" data-w="32" data-h="2" data-grad="disk"></div><div class="gr" data-key="dw" data-base=".15" data-vol=".1" data-w="32" data-h="2" data-grad="wr" data-down></div></div>
 <div><div class="pre"><span class="c-disk">R</span> <span data-num="dr" data-scale="40" data-dec="1" data-unit=" MB/s"></span></div><div class="pre"><span class="c-proc">W</span> <span data-num="dw" data-scale="25" data-dec="1" data-unit=" MB/s"></span></div><div class="pre dim">lat 0.12 ms</div></div></div>`, tr('NVMe · 41°C'))}
 ${P('var(--disk)', 'filesystems', `<div class="pre">/          <span class="mt" data-v="44" data-w="18" data-grad="used"></span>  44%  412G</div>
 <div class="pre">/home      <span class="mt" data-v="71" data-w="18" data-grad="used"></span>  71%  1.3T</div>
 <div class="pre">/mnt/data  <span class="mt" data-v="93" data-w="18" data-grad="used"></span>  <span class="c-red">93%</span>  3.4T</div>`)}
</div>${keys('network')}`,

    services: () => `${top('services')}
<div class="g" style="grid-template-columns:70ch 1fr">
 ${P('var(--net)', 'systemd units', `<div class="pre" style="margin-bottom:.3em"><span class="btn">All 312</span> <span class="btn" style="background:var(--net);color:#0b0f14">Active 148</span> <span class="btn" style="color:var(--red)">Failed 2</span> <span class="btn">Timers 18</span></div>
 <div class="pre dim">Unit                     State        Startup     PID   Mem</div>
 <div class="pre">bluetooth.service        <span class="c-grn">● active</span>     enabled     688    6M</div>
 <div class="pre">containerd.service       <span class="c-grn">● active</span>     enabled    1290   48M</div>
 <div class="pre hl">docker.service           <span class="c-grn">● active</span>     enabled    1022   96M</div>
 <div class="pre">fwupd.service            <span class="dim">○ inactive</span>   static        —     —</div>
 <div class="pre">llama-swap.service       <span class="c-red">✖ failed</span>     enabled       —     —</div>
 <div class="pre">NetworkManager.service   <span class="c-grn">● active</span>     enabled     688   21M</div>
 <div class="pre">postgresql.service       <span class="c-grn">● active</span>     disabled   1305  186M</div>
 <div class="pre">snapper-timeline.timer   <span class="c-yel">◷ waiting</span>    enabled       —     —</div>
 <div class="pre">sshd.service             <span class="c-grn">● active</span>     enabled     901    8M</div>`)}
 ${P('var(--grn)', 'docker.service', `<div class="pre"><span class="dim">State   </span><span class="c-grn">● active (running)</span> since 07:38</div>
 <div class="pre"><span class="dim">Memory  </span>96.4M <span class="dim">(peak 212M)</span></div>
 <div class="pre dim" style="margin-top:.3em">── journal ─────────────────────────</div>
 <div class="pre"><span class="dim">07:38:12</span> <span class="c-grn">Started docker.service</span></div>
 <div class="pre"><span class="dim">07:38:14</span> <span class="c-yel">WARNING: bridge-nf-call disabled</span></div>
 <div class="pre"><span class="dim">08:51:03</span> <span class="c-red">error pulling image: timeout</span></div>
 <div class="pre"><span class="dim">08:51:09</span> postgres: health_status=healthy</div>`)}
</div>
${P('var(--yel)', 'boot', `<div class="pre"><span style="color:#8b949e">█████████████</span><span style="color:#58a6ff">██</span><span style="color:#bc8cff">████</span><span style="color:#39c5cf">███</span><span style="color:#e3b341">██████████</span>  <span class="dim">firmware 8.1s · loader 1.2s · kernel 2.3s · initrd 1.9s · services 6.4s</span></div>`, tr('last boot 19.9 s'))}
${keys('services')}`,

    compact: () => `<div class="top"><span class="inv b"> ngtop </span><span> workstation  up 3d 04:12  load 2.41</span><span class="grow"></span><span data-clock class="dim"></span></div>
<div class="pre"><span class="b c-cpu">CPU</span> <span class="mt" data-key="cpu" data-base=".3" data-vol=".05" data-w="30"></span> <span data-num="cpu" data-pad="3" data-unit="%"></span> <span class="c-yel">62°</span> 4.7GHz</div>
<div class="pre"><span class="b c-mem">MEM</span> <span class="mt" data-v="45" data-w="30" data-grad="used"></span> 14.2/31.3G  <span class="dim">swp</span> 0.5/8G</div>
<div class="pre"><span class="b c-gpu">GPU</span> <span class="mt" data-key="gpu" data-base=".86" data-vol=".05" data-w="30" data-grad="gpu"></span> <span data-num="gpu" data-pad="3" data-unit="%"></span> <span class="c-yel">71°</span> <span class="dim">vram</span> 11.8/16G</div>
<div class="pre"><span class="b c-net">NET</span> <span class="c-net">▼</span><span data-num="rx" data-scale="12" data-dec="1" data-pad="5" data-unit="M"></span> <span class="sp" data-key="rx" data-base=".35" data-vol=".1" data-w="12" data-grad="net"></span> <span style="color:#d2a8ff">▲</span><span data-num="tx" data-scale="4" data-dec="1" data-pad="5" data-unit="M"></span> <span class="sp" data-key="tx" data-base=".15" data-vol=".06" data-w="12" data-grad="up"></span></div>
<div class="pre dim">────────────────────────────────────────────────────────────────────────────────</div>
<table class="pt" data-cols="pid,user,cpu,mem,gpu,name" data-rows="13" data-selrow="0"></table>
${keys('compact')}`,
  };
  const SIZE = { overview: [120, 37], gpu: [120, 30], processes: [120, 37], network: [120, 33], services: [120, 26], compact: [80, 24] };

  // Monta una finestra di terminale con la schermata richiesta. opts: {title, bar: false, tabs: true}
  function mount(el, name = 'overview', opts = {}) {
    if (typeof el === 'string') el = document.querySelector(el);
    const [w, h] = SIZE[name];
    const bar = opts.bar === false ? '' : `<div class="ngt-bar"><i></i><i></i><i></i><span>${opts.title || 'ngtop'}</span><span class="r">${w}×${h}</span></div>`;
    el.innerHTML = `<div class="ngt-win">${bar}<div class="ngt" style="width:${w + 2}ch;position:relative">${screens[name]()}</div></div>`;
    el.dataset.ngtScreen = name;
    // schede cliccabili: cambiano schermata (tranne nel formato compatto)
    if (opts.tabs !== false && name !== 'compact') {
      el.querySelector('.tabs')?.addEventListener('click', e => {
        const s = e.target.dataset?.s; if (s === undefined) return;
        mount(el, ['overview', 'gpu', 'processes', 'network', 'services'][+s], opts);
        NG.redraw();
        el.dispatchEvent(new CustomEvent('ngt-screen', { detail: el.dataset.ngtScreen, bubbles: true }));
      });
    }
    return el;
  }
  // Rimpicciolisce la finestra (transform) perché stia nella larghezza del contenitore; mai ingrandisce.
  function fit(el, max = 1) {
    if (typeof el === 'string') el = document.querySelector(el);
    const win = el.querySelector('.ngt-win'); if (!win) return;
    win.style.transform = ''; win.style.transformOrigin = 'top left';
    const avail = el.clientWidth, w = win.offsetWidth, h = win.offsetHeight;
    const k = Math.min(max, avail / w);
    win.style.transform = k < 1 ? `scale(${k})` : '';
    el.style.height = k < 1 ? Math.ceil(h * k) + 'px' : '';
  }
  function autofit(el, max = 1) {
    if (typeof el === 'string') el = document.querySelector(el);
    const run = () => fit(el, max);
    addEventListener('resize', run); el.addEventListener('ngt-screen', run);
    if (document.fonts) document.fonts.ready.then(run);
    run();
  }
  let started = false;
  function start() { if (!started) { started = true; NG.start(); } }
  return { mount, start, fit, autofit, screens: Object.keys(screens), SIZE };
})();
