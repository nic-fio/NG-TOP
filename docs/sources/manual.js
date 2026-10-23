/* Script comune dei manuali dei 7 progetti: ricerca, indice analitico, pulsante Copy. Uguale in tutti i manuali. */
/* Ricerca nella barra laterale e indice analitico. Senza dipendenze; senza
   JavaScript il manuale resta completo (la casella di ricerca resta nascosta). */
(function () {
  "use strict";
  var main = document.querySelector(".main");
  if (!main) return;

  // Minuscole e senza accenti, carattere per carattere: la lunghezza resta
  // uguale, così le posizioni trovate valgono anche sul testo originale.
  function fold(s) {
    var o = "";
    for (var i = 0; i < s.length; i++) {
      var c = s.charAt(i);
      var f = c.normalize ? c.normalize("NFD").replace(/[̀-ͯ]/g, "").toLowerCase() : c.toLowerCase();
      o += f.length === 1 ? f : c;
    }
    return o;
  }
  function esc(s) {
    return s.replace(/[&<>"]/g, function (c) {
      return { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c];
    });
  }
  function secTitle(sec) {
    var h = sec && sec.querySelector("h3.h-sec, h2.h-ch");
    return h ? h.textContent.replace(/\s+/g, " ").trim() : "";
  }
  function splitTitle(t) { // "5.3 · Interruzioni e ripresa" -> ["5.3", "Interruzioni e ripresa"]
    var m = /^([\d.]+) · (.*)$/.exec(t);
    return m ? [m[1], m[2]] : ["", t];
  }

  /* ---------------- indice analitico ---------------- */
  var list = document.getElementById("index-list");
  if (list) {
    var terms = {};
    var els = main.querySelectorAll("[data-index]");
    for (var i = 0; i < els.length; i++) {
      var el = els[i];
      var target = el.id ? el : el.closest("[id]");
      if (!target || target.id === "index") continue;
      var sec = el.closest(".sec") || el.closest(".chapter");
      var label = splitTitle(secTitle(sec));
      var names = el.getAttribute("data-index").split(";");
      for (var j = 0; j < names.length; j++) {
        var name = names[j].trim();
        if (!name) continue;
        var key = fold(name);
        var t = terms[key] || (terms[key] = { name: name, refs: [], seen: {} });
        if (t.seen[target.id]) continue;
        t.seen[target.id] = true;
        t.refs.push({ id: target.id, n: label[0], title: label[1] });
      }
    }
    var sortKey = function (k) { return k.replace(/^[-.]+/, ""); };
    var keys = Object.keys(terms).sort(function (a, b) {
      return sortKey(a).localeCompare(sortKey(b), "en") || a.localeCompare(b, "en");
    });
    var groups = [], cur = null;
    keys.forEach(function (k) {
      var ch = sortKey(k).charAt(0).toUpperCase();
      if (!/[A-Z]/.test(ch)) ch = "#";
      if (!cur || cur.ch !== ch) { cur = { ch: ch, items: [] }; groups.push(cur); }
      cur.items.push(terms[k]);
    });
    var out = "";
    groups.forEach(function (g) {
      out += '<div class="idx-g"><h3 class="idx-l">' + g.ch + '</h3><ul class="idx-ul">';
      g.items.forEach(function (t) {
        // Voci da mostrare come codice: nomi di file e opzioni, piu' le parole
        // che il manuale elenca in window.MANUAL_CODE_TERMS (prima di questo script).
        var isCode = /^[-.]|_|\.(txt|sh|example|conf|toml|json|md|py|efi|nsb)$/.test(t.name) ||
          (window.MANUAL_CODE_TERMS || []).indexOf(t.name) >= 0;
        var nm = isCode ? "<code>" + esc(t.name) + "</code>" : esc(t.name);
        out += '<li><span class="idx-t">' + nm + "</span> — " + t.refs.map(function (r) {
          return '<a href="' + '#' + r.id + '" title="' + esc(r.title) + '">' + (r.n ? r.n + " " : "") + esc(r.title) + "</a>";
        }).join(", ") + "</li>";
      });
      out += "</ul></div>";
    });
    list.innerHTML = out;
  }

  /* ---------------- ricerca ---------------- */
  var box = document.getElementById("srch");
  var q = document.getElementById("srch-q");
  var info = document.getElementById("srch-info");
  var res = document.getElementById("srch-res");
  var toc = document.getElementById("toc");
  if (!box || !q) return;
  box.hidden = false;

  var secs = [];
  var nodes = main.querySelectorAll(".sec");
  for (var k = 0; k < nodes.length; k++) {
    var s = nodes[k];
    if (s.closest("#index")) continue;
    var parts = [], w = document.createTreeWalker(s, NodeFilter.SHOW_TEXT, null), tn;
    while ((tn = w.nextNode())) if (!tn.parentNode.closest("h3.h-sec")) parts.push(tn.data);
    var txt = parts.join(" ").replace(/\s+/g, " ").replace(/ ([,.;:)!?»])/g, "$1").replace(/([(«]) /g, "$1").trim();
    var tt = secTitle(s);
    secs.push({ el: s, id: s.id, title: tt, ftitle: fold(tt), text: txt, ftext: fold(txt) });
  }
  var tocLinks = {};
  if (toc) {
    var as = toc.querySelectorAll("a");
    for (var a = 0; a < as.length; a++) tocLinks[as[a].getAttribute("href").slice(1)] = as[a].parentNode;
  }

  function count(hay, w) {
    var n = 0, p = hay.indexOf(w);
    while (p >= 0) { n++; p = hay.indexOf(w, p + w.length); }
    return n;
  }
  function markWords(s, fs, words) { // testo -> HTML con <mark> sulle parole cercate
    var ranges = [];
    words.forEach(function (w) {
      var p = fs.indexOf(w);
      while (p >= 0) { ranges.push([p, p + w.length]); p = fs.indexOf(w, p + w.length); }
    });
    ranges.sort(function (x, y) { return x[0] - y[0]; });
    var o = "", last = 0;
    ranges.forEach(function (r) {
      if (r[0] < last) return;
      o += esc(s.slice(last, r[0])) + "<mark>" + esc(s.slice(r[0], r[1])) + "</mark>";
      last = r[1];
    });
    return o + esc(s.slice(last));
  }

  function clearMarks() {
    var ms = main.querySelectorAll("mark.hit");
    for (var i = 0; i < ms.length; i++) {
      var m = ms[i], p = m.parentNode;
      p.replaceChild(document.createTextNode(m.textContent), m);
      p.normalize();
    }
  }
  function highlight(root, words) {
    var walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT, {
      acceptNode: function (n) {
        return n.parentNode.closest("svg, script, style, mark") ? NodeFilter.FILTER_REJECT : NodeFilter.FILTER_ACCEPT;
      }
    });
    var list = [], n;
    while ((n = walker.nextNode())) list.push(n);
    var first = null;
    list.forEach(function (node) {
      var s = node.data, fs = fold(s), ranges = [];
      words.forEach(function (w) {
        var p = fs.indexOf(w);
        while (p >= 0) { ranges.push([p, p + w.length]); p = fs.indexOf(w, p + w.length); }
      });
      if (!ranges.length) return;
      ranges.sort(function (x, y) { return x[0] - y[0]; });
      var frag = document.createDocumentFragment(), last = 0;
      ranges.forEach(function (r) {
        if (r[0] < last) return;
        if (r[0] > last) frag.appendChild(document.createTextNode(s.slice(last, r[0])));
        var m = document.createElement("mark");
        m.className = "hit";
        m.textContent = s.slice(r[0], r[1]);
        frag.appendChild(m);
        if (!first) first = m;
        last = r[1];
      });
      if (last < s.length) frag.appendChild(document.createTextNode(s.slice(last)));
      node.parentNode.replaceChild(frag, node);
    });
    return first;
  }

  var found = [], sel = -1, words = [];
  function search() {
    var v = fold(q.value.trim());
    words = v.split(/\s+/).filter(function (w) { return w.length > 0; });
    found = [];
    sel = -1;
    Object.keys(tocLinks).forEach(function (id) { tocLinks[id].classList.remove("toc-hit"); });
    if (v.length < 2) {
      res.innerHTML = "";
      info.textContent = v.length ? "Type at least two letters." : "";
      if (toc) toc.classList.remove("filtering");
      clearMarks();
      return;
    }
    secs.forEach(function (s) {
      var score = 0;
      for (var i = 0; i < words.length; i++) {
        var c = count(s.ftext, words[i]);
        var inTitle = s.ftitle.indexOf(words[i]) >= 0;
        if (!c && !inTitle) return;
        score += c + (inTitle ? 20 : 0);
      }
      found.push({ s: s, score: score });
    });
    found.sort(function (x, y) { return y.score - x.score; });
    info.textContent = found.length
      ? (found.length === 1 ? "1 section found" : found.length + " sections found") + " · Enter to open the first"
      : "No results.";
    var html = "";
    found.slice(0, 15).forEach(function (f, i) {
      var s = f.s, p = Math.max(0, s.ftext.indexOf(words[0]));
      var a = Math.max(0, p - 50), b = Math.min(s.text.length, p + words[0].length + 70);
      var snip = (a > 0 ? "…" : "") + markWords(s.text.slice(a, b), s.ftext.slice(a, b), words) + (b < s.text.length ? "…" : "");
      var t = splitTitle(s.title);
      html += '<li><a href="' + '#' + s.id + '" data-i="' + i + '"><span class="r-t"><span class="toc-n">' + t[0] +
        "</span><span>" + esc(t[1]) + '</span></span><span class="r-s">' + snip + "</span></a></li>";
    });
    res.innerHTML = html;
    if (toc) {
      toc.classList.add("filtering");
      found.forEach(function (f) {
        var li = tocLinks[f.s.id];
        if (li) li.classList.add("toc-hit");
        var ch = f.s.el.closest(".chapter");
        if (ch && tocLinks[ch.id]) tocLinks[ch.id].classList.add("toc-hit");
      });
    }
  }
  function go(i) {
    var f = found[i];
    if (!f) return;
    clearMarks();
    var first = highlight(f.s.el, words);
    if (history.replaceState) history.replaceState(null, "", "#" + f.s.id);
    (first || f.s.el).scrollIntoView({ block: first ? "center" : "start" });
    f.s.el.classList.remove("flash");
    void f.s.el.offsetWidth;
    f.s.el.classList.add("flash");
  }
  function setSel(i) {
    var links = res.querySelectorAll("a");
    if (!links.length) return;
    sel = (i + links.length) % links.length;
    for (var k = 0; k < links.length; k++) links[k].classList.toggle("on", k === sel);
    links[sel].scrollIntoView({ block: "nearest" });
  }

  var timer = null;
  q.addEventListener("input", function () {
    clearTimeout(timer);
    timer = setTimeout(search, 120);
  });
  q.addEventListener("keydown", function (e) {
    if (e.key === "Enter") { e.preventDefault(); clearTimeout(timer); search(); go(sel >= 0 ? sel : 0); }
    else if (e.key === "ArrowDown") { e.preventDefault(); setSel(sel + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); setSel(sel - 1); }
    else if (e.key === "Escape") { q.value = ""; search(); q.blur(); }
  });
  res.addEventListener("click", function (e) {
    var a = e.target.closest("a[data-i]");
    if (!a) return;
    e.preventDefault();
    go(+a.getAttribute("data-i"));
  });
  document.addEventListener("keydown", function (e) {
    if (e.key !== "/" || e.ctrlKey || e.metaKey || e.altKey) return;
    var t = e.target;
    if (t && (t.tagName === "INPUT" || t.tagName === "TEXTAREA" || t.isContentEditable)) return;
    e.preventDefault();
    q.focus();
    q.select();
  });
})();
/* Pulsante Copy sui blocchi di comandi: copia le righe che iniziano con un
   prompt, senza il prompt. Prompt riconosciuti: "$ ", "utente@macchina:percorso$ "
   (e "#" di root in quella forma), "fs0:\> " e "Shell> " di UEFI. Un prompt senza
   comando non conta; le righe che continuano un comando ("\" in fondo, o "... "
   del BASIC di NESH) si copiano con lui. L'etichetta sta in CSS (data-l), cosi' non
   entra nel testo della pagina ne' nella ricerca. */
(function () {
  "use strict";
  var pres = document.querySelectorAll(".main pre.code");
  for (var i = 0; i < pres.length; i++) (function (pre) {
    var lines = pre.textContent.replace(/\s+$/, "").split("\n"), cmds = [];
    for (var k = 0; k < lines.length; k++) {
      var m = /^(?:\$|[\w.-]+@[\w.-]+:[^\s$#]*[$#]|fs\d+:[^>\s]*>|Shell>) (.*\S.*)$/.exec(lines[k]);
      if (!m) continue;
      var cmd = m[1];
      // Il comando continua: riga che finisce con "\" (shell) o righe "... " (BASIC di NESH)
      while (k + 1 < lines.length) {
        if (/\\\s*$/.test(cmd)) { cmd += "\n" + lines[++k]; continue; }
        var c = /^\.\.\. ?(.*)$/.exec(lines[k + 1]);
        if (c) { cmd += "\n" + c[1]; k++; continue; }
        break;
      }
      cmds.push(cmd);
    }
    if (!cmds.length) return;              // non e' un blocco di comandi
    var text = cmds.join("\n");
    var wrap = document.createElement("div");
    wrap.className = "code-w";
    pre.parentNode.insertBefore(wrap, pre);
    wrap.appendChild(pre);
    var btn = document.createElement("button");
    btn.type = "button";
    btn.className = "copy";
    btn.setAttribute("data-l", "Copy");
    btn.setAttribute("aria-label", "Copy the command" + (cmds.length > 1 ? "s" : ""));
    btn.title = "Copy the command" + (cmds.length > 1 ? "s" : "");
    var t = null;
    function done() {
      btn.setAttribute("data-l", "Copied");
      btn.classList.add("ok");
      clearTimeout(t);
      t = setTimeout(function () { btn.setAttribute("data-l", "Copy"); btn.classList.remove("ok"); }, 1400);
    }
    function fallback() {
      var ta = document.createElement("textarea");
      ta.value = text;
      ta.setAttribute("readonly", "");
      ta.style.position = "fixed"; ta.style.top = "-1000px"; ta.style.opacity = "0";
      document.body.appendChild(ta);
      ta.select();
      try { if (document.execCommand("copy")) done(); } catch (e) {}
      document.body.removeChild(ta);
    }
    btn.addEventListener("click", function () {
      if (navigator.clipboard && window.isSecureContext) navigator.clipboard.writeText(text).then(done, fallback);
      else fallback();
    });
    wrap.appendChild(btn);
  })(pres[i]);
})();
