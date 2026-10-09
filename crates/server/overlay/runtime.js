// Moteur des overlays PaceWall : flux de données (WebSocket ou démo), rendu des widgets, glisser-déposer.
// Utilisé par /overlay/{player,obs} (rendu seul) et /editor (édition). Aucune dépendance.
(() => {
  const PW = window.PW;
  const REF_H = 1080; // les tailles des widgets sont écrites pour un écran de 1080 px de haut

  /* ---------- Flux de données ---------- */
  const Feed = PW.feed = {
    store: new Map(), game: false, mode: 'live',
    listeners: new Set(), layoutListeners: new Set(),
    want: { prefixes: [], hz: 5 },
    ws: null, retry: null, demoTimer: null,

    get(k, d) { const v = this.store.get(k); return v === undefined ? d : v; },
    on(cb) { this.listeners.add(cb); return () => this.listeners.delete(cb); },
    onLayout(cb) { this.layoutListeners.add(cb); return () => this.layoutListeners.delete(cb); },
    emit(changed) { for (const cb of this.listeners) cb(changed); },

    // Les vues indiquent ce qu'elles veulent : le serveur ne nous envoie que ça, et ne lit le jeu que pour ça.
    use(prefixes, hz) {
      this.want = { prefixes, hz };
      if (this.ws && this.ws.readyState === 1) this.ws.send(JSON.stringify({ sub: prefixes, hz }));
    },

    setMode(mode) {
      this.mode = mode;
      this.store.clear();
      clearInterval(this.demoTimer); this.demoTimer = null;
      if (mode === 'demo') {
        this.close();
        for (const [k, v] of Object.entries(PW.demo)) this.store.set(k, v);
        this.game = true;
        this.demoTimer = setInterval(() => this.tickDemo(), 100);
        this.emit(new Set(this.store.keys()));
      } else {
        this.game = false;
        this.connect();
        this.emit(new Set());
      }
    },

    tickDemo() {
      const t = performance.now() / 1000, s = this.store, ch = new Set();
      const put = (k, v) => { if (s.get(k) !== v) { s.set(k, v); ch.add(k); } };
      const wave = Math.sin(t * 1.3);
      put('player.speed', Math.round(200 + 60 * wave));
      put('player.rpm', Math.round(6800 + 1100 * wave));
      put('player.throttle', Math.round(Math.max(0, Math.min(100, 55 + 60 * wave))));
      put('player.brake', Math.round(Math.max(0, Math.min(100, -20 - 80 * wave))));
      put('player.steering', Math.round(40 * Math.sin(t * 0.7)));
      put('player.gear', 4 + Math.round(1.5 * wave));
      put('lap.delta_best', +(0.35 * Math.sin(t * 0.4)).toFixed(2));
      this.emit(ch);
    },

    connect() {
      if (this.ws || this.mode !== 'live') return;
      const ws = this.ws = new WebSocket('ws://' + location.host + '/ws');
      ws.onopen = () => ws.send(JSON.stringify({ sub: this.want.prefixes, hz: this.want.hz }));
      ws.onmessage = e => {
        const m = JSON.parse(e.data);
        if (m.t === 'layout') { for (const cb of this.layoutListeners) cb(); return; }
        if (m.t !== 'u') return;
        const ch = new Set();
        for (const k of m.x || []) { this.store.delete(k); ch.add(k); }
        for (const [k, v] of Object.entries(m.d || {})) { this.store.set(k, v); ch.add(k); }
        const g = !!m.game;
        if (g !== this.game) { this.game = g; ch.add('game.'); }
        this.emit(ch);
      };
      ws.onclose = () => { this.ws = null; this.game = false; this.emit(new Set(['game.'])); if (this.mode === 'live') this.retry = setTimeout(() => this.connect(), 1500); };
    },
    close() { clearTimeout(this.retry); if (this.ws) { this.ws.onclose = null; this.ws.close(); this.ws = null; } },
  };

  /* ---------- Mise en page ---------- */
  const spec = (id, type, x, y, scale = 1) => ({ id, type, x, y, scale, opacity: 1, opts: {} });
  PW.defaultLayout = () => ({
    widgets: [
      spec('standings', 'standings', 0.012, 0.05), spec('timing', 'timing', 0.012, 0.42),
      spec('fuel', 'fuel', 0.012, 0.66), spec('tyres', 'tyres', 0.18, 0.66),
      spec('session', 'session', 0.42, 0.02), spec('inputs', 'inputs', 0.40, 0.76),
    ],
  });

  PW.loadLayouts = async () => {
    try { const r = await fetch('/api/layouts'); const j = await r.json(); return j && j.targets ? j : { targets: {} }; }
    catch { return { targets: {} }; }
  };
  PW.saveLayouts = async layouts => (await fetch('/api/layouts', { method: 'PUT', body: JSON.stringify(layouts) })).ok;

  PW.optionDefs = def => [...(def.options || []), ...PW.commonOptions];
  PW.ACCENTS = { cyan: '#19d3f3', white: '#e8edf4', orange: '#ff9628', green: '#38e07b', red: '#ff4d5e', violet: '#a870ff', pink: '#ff6abe', yellow: '#ffd633' };
  PW.accentHex = id => PW.ACCENTS[id] || PW.ACCENTS.cyan;
  PW.resolveOpts = (def, saved) => {
    const o = {};
    for (const d of PW.optionDefs(def)) o[d.id] = saved && saved[d.id] !== undefined ? saved[d.id] : d.def;
    return o;
  };

  /* ---------- Vue : rend une mise en page dans un conteneur ---------- */
  PW.createView = (container, cfg = {}) => {
    const root = document.createElement('div');
    root.className = 'pw-root' + (cfg.editing ? ' editing' : '');
    container.appendChild(root);

    let layout = cfg.layout || PW.defaultLayout();
    let items = [];       // { spec, def, el, inst }
    let selected = null;
    let dirty = new Set(), raf = 0;

    const unit = () => root.clientHeight / REF_H || 1;
    const place = it => {
      const s = it.spec, el = it.el;
      el.style.left = (s.x * 100) + '%';
      el.style.top = (s.y * 100) + '%';
      el.style.transform = 'scale(' + (s.scale * unit()).toFixed(4) + ')';
      el.style.opacity = s.opacity;
      el.style.setProperty('--bg', (it.opts.bg ?? 55) / 100);
    };
    const g = (k, d) => Feed.get(k, d);

    function mount() {
      for (const it of items) { it.inst.destroy && it.inst.destroy(); it.el.remove(); }
      items = [];
      for (const s of layout.widgets) {
        if (cfg.only && !cfg.only.includes(s.id) && !cfg.only.includes(s.type)) continue;
        const def = PW.widgets[s.type];
        if (!def) continue;
        const el = document.createElement('div');
        el.className = 'pw-w' + (selected === s.id ? ' sel' : '');
        el.dataset.id = s.id;
        const opts = PW.resolveOpts(def, s.opts);
        el.style.setProperty('--pw-accent', PW.accentHex(s.opts && s.opts.accent));
        // Affichage épuré par défaut : ni barre d'accent ni titres, sauf demande explicite.
        if (!(s.opts && s.opts.bar === true)) el.classList.add('nobar');
        if (!(s.opts && s.opts.titles === true)) el.classList.add('notitles');
        const inst = def.mount(el, opts);
        const it = { spec: s, def, el, inst, opts };
        place(it);
        root.appendChild(el);
        items.push(it);
        if (cfg.editing) bindDrag(it);
        inst.update(g);
      }
      subscribe();
      visibility();
    }

    function subscribe() {
      const prefixes = [...new Set(items.flatMap(it => it.def.keys))];
      const hz = Math.max(1, ...items.map(hzOf));
      Feed.use(prefixes, hz);
    }

    function visibility() {
      // Sans jeu, les overlays disparaissent (OBS reste propre) ; l'éditeur montre toujours tout.
      root.classList.toggle('pw-hidden', !(cfg.editing || Feed.game));
    }

    // Chaque widget a sa cadence (`def.hz`) : la page peut recevoir des données à 20 Hz pour le graphe des
    // inputs sans que le carburant ou le classement se redessinent à 20 Hz.
    const hzOf = it => (typeof it.def.hz === 'function' ? it.def.hz(it.opts) : it.def.hz) || 5;
    const flush = () => {
      raf = 0;
      const keys = dirty; dirty = new Set();
      const now = performance.now();
      let wait = Infinity;
      for (const it of items) {
        const hit = it.pending || keys.has('*') || it.def.keys.some(p => { for (const k of keys) if (k.startsWith(p)) return true; return false; });
        if (!hit) continue;
        const left = (it.last || 0) + 1000 / hzOf(it) - now;
        if (left <= 4) { it.pending = false; it.last = now; it.inst.update(g); }
        else { it.pending = true; wait = Math.min(wait, left); }
      }
      if (wait < Infinity) setTimeout(() => { if (!raf) raf = requestAnimationFrame(flush); }, wait);
    };
    const off = Feed.on(changed => {
      if (changed.has('game.')) visibility();
      for (const k of changed) dirty.add(k);
      if (!raf) raf = requestAnimationFrame(flush);
    });
    const ro = new ResizeObserver(() => items.forEach(place));
    ro.observe(root);

    /* Glisser-déposer (édition) */
    function bindDrag(it) {
      it.el.addEventListener('pointerdown', e => {
        if (e.button !== 0) return;
        api.select(it.spec.id);
        const rect = root.getBoundingClientRect();
        const sx = e.clientX, sy = e.clientY, x0 = it.spec.x, y0 = it.spec.y;
        it.el.setPointerCapture(e.pointerId);
        const move = ev => {
          const snap = v => Math.round(v * 400) / 400;
          it.spec.x = Math.max(0, Math.min(0.98, snap(x0 + (ev.clientX - sx) / rect.width)));
          it.spec.y = Math.max(0, Math.min(0.98, snap(y0 + (ev.clientY - sy) / rect.height)));
          place(it);
        };
        const up = () => { it.el.removeEventListener('pointermove', move); it.el.removeEventListener('pointerup', up); cfg.onChange && cfg.onChange(); };
        it.el.addEventListener('pointermove', move); it.el.addEventListener('pointerup', up);
      });
    }

    const api = {
      root,
      get layout() { return layout; },
      setLayout(l) { layout = l; mount(); },
      select(id) {
        selected = id;
        items.forEach(it => it.el.classList.toggle('sel', it.spec.id === id));
        cfg.onSelect && cfg.onSelect(id);
      },
      // Position, échelle, opacité : sans reconstruire.
      refresh(id) { const it = items.find(i => i.spec.id === id); if (it) { it.opts = PW.resolveOpts(it.def, it.spec.opts); place(it); } },
      // Options : le widget doit être reconstruit.
      remount() { mount(); },
      destroy() { off(); ro.disconnect(); items.forEach(it => it.inst.destroy && it.inst.destroy()); root.remove(); },
    };
    mount();
    return api;
  };
})();
