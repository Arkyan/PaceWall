// Catalogue des widgets PaceWall.
// Un widget = { name, keys (préfixes de clés suivies), hz, size, options, mount(root, opts) }.
// `mount` construit le DOM une fois et renvoie { update(g) } ; `g(clé, défaut)` lit la dernière valeur.
// Les unités arrivent déjà converties du serveur (km/h, °C, %, secondes).
window.PW = window.PW || {};
PW.widgets = {};

/* ---------- Outils ---------- */
PW.commonOptions = [
  { id: 'bg', label: 'Opacité du fond (%)', type: 'number', min: 0, max: 100, step: 5, def: 78 },
];

PW.tpl = html => { const d = document.createElement('div'); d.innerHTML = html; return d; };

// Liaison des éléments [data-k] ; n'écrit dans le DOM que si la valeur change.
PW.bind = root => {
  const el = {}, cache = {};
  root.querySelectorAll('[data-k]').forEach(e => { el[e.dataset.k] = e; });
  return {
    el,
    text(k, v) { if (cache[k] !== v) { cache[k] = v; el[k].textContent = v; } },
    cls(k, v) { const key = k + '#c'; if (cache[key] !== v) { cache[key] = v; el[k].className = (el[k].dataset.base || '') + (v ? ' ' + v : ''); } },
    css(k, prop, v) { const key = k + '#' + prop; if (cache[key] !== v) { cache[key] = v; el[k].style.setProperty(prop, v); } },
    scaleX(k, f) { this.css(k, 'transform', 'scaleX(' + Math.max(0, Math.min(1, f)).toFixed(3) + ')'); },
    scaleY(k, f) { this.css(k, 'transform', 'scaleY(' + Math.max(0, Math.min(1, f)).toFixed(3) + ')'); },
  };
};

const F = {
  n: (v, d = 1) => (v == null || !isFinite(v)) ? '—' : v.toFixed(d),
  signed: (v, d = 2) => (v == null || !isFinite(v)) ? '—' : (v > 0 ? '+' : v < 0 ? '−' : '') + Math.abs(v).toFixed(d),
  lap: s => { if (s == null || !(s > 0)) return '—'; const m = Math.floor(s / 60); return m + ':' + (s - m * 60).toFixed(3).padStart(6, '0'); },
  hms: s => { if (s == null || !(s >= 0) || s > 1e7) return '—'; s = Math.floor(s); const h = Math.floor(s / 3600), m = Math.floor(s % 3600 / 60); return (h ? h + ':' + String(m).padStart(2, '0') : m) + ':' + String(s % 60).padStart(2, '0'); },
};
PW.fmt = F;

const lapsClass = v => v == null ? '' : v < 3 ? 'pw-bad' : v < 6 ? 'pw-warn' : '';
const barColor = f => f < 0.15 ? 'var(--pw-bad)' : f < 0.3 ? 'var(--pw-warn)' : 'var(--pw-ok)';

/* ---------- Carburant et énergie ---------- */
function resourceBlock(id, title, unit, decimals) {
  return `<div class="pw-title"><span>${title}</span><em data-k="${id}lim"></em></div>
    <div class="pw-row"><span class="pw-big" data-k="${id}">—</span><span class="pw-unit">${unit}</span><span class="pw-sub" data-k="${id}pct"></span></div>
    <div class="pw-bar"><i data-k="${id}bar"></i></div>
    <div class="pw-stats">
      <div><small>Tours</small><b data-k="${id}laps" data-base="">—</b></div>
      <div><small>Conso</small><b data-k="${id}per">—</b></div>
      <div><small>À ajouter</small><b data-k="${id}add">—</b></div>
    </div>`;
}

PW.widgets.fuel = {
  name: 'Carburant et énergie',
  keys: ['fuel.', 'energy.virtual', 'strategy.'],
  hz: 4, size: [300, 150],
  options: [
    { id: 'energy', label: 'Afficher l’énergie virtuelle', type: 'bool', def: true },
    { id: 'margin', label: 'Marge de sécurité (tours)', type: 'number', min: 0, max: 3, step: 0.5, def: 1 },
  ],
  mount(root, o) {
    const card = PW.tpl(`<div class="pw-card">${resourceBlock('f', 'Carburant', 'L')}${o.energy ? '<div class="pw-sep"></div>' + resourceBlock('e', 'Énergie virtuelle', '%') : ''}</div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card);
    const fill = (id, level, pct, per, laps, add, limiting, decimals) => {
      if (!b.el[id]) return;
      b.text(id, F.n(level, decimals));
      b.text(id + 'pct', pct == null ? '' : F.n(pct, 0) + ' %');
      b.scaleX(id + 'bar', pct == null ? 0 : pct / 100);
      b.css(id + 'bar', 'background', barColor((pct ?? 0) / 100));
      b.text(id + 'laps', F.n(laps, 1));
      b.cls(id + 'laps', lapsClass(laps));
      b.text(id + 'per', per == null ? '—' : F.n(per, decimals === 1 ? 2 : 1));
      b.text(id + 'add', add == null ? '—' : '+' + F.n(add + o.margin * (per ?? 0), decimals));
      b.text(id + 'lim', limiting ? 'limitant' : '');
    };
    return {
      update(g) {
        const lim = g('strategy.limiting');
        fill('f', g('fuel.level'), g('fuel.percent'), g('strategy.fuel_per_lap'), g('strategy.fuel_laps_left'), g('strategy.fuel_to_add'), lim === 'fuel', 1);
        fill('e', g('energy.virtual'), g('energy.virtual'), g('strategy.energy_per_lap'), g('strategy.energy_laps_left'), g('strategy.energy_to_add'), lim === 'energy', 1);
      },
    };
  },
};

/* ---------- Pneus ---------- */
PW.widgets.tyres = {
  name: 'Pneus',
  keys: ['tyre.', 'brake.'],
  hz: 3, size: [260, 190],
  options: [
    { id: 'pressure', label: 'Pression', type: 'select', choices: [['kpa', 'kPa'], ['bar', 'bar'], ['psi', 'psi'], ['off', 'Masquer']], def: 'kpa' },
    { id: 'brakes', label: 'Températures des freins', type: 'bool', def: false },
    { id: 'tempsrc', label: 'Température affichée', type: 'select', choices: [['carcass', 'Carcasse'], ['inner', 'Couche interne'], ['surface', 'Surface (moyenne)']], def: 'carcass' },
    { id: 'tempoffset', label: 'Décalage de température (°C)', type: 'number', min: -10, max: 10, step: 0.5, def: 0 },
    { id: 'wearmode', label: "Affichage de l'usure", type: 'select', choices: [['state', 'État du pneu (100 → 0 %)'], ['used', 'Usure (0 → 100 %)']], def: 'state' },
    { id: 'unit', label: 'Unité de température', type: 'select', choices: [['c', '°C'], ['f', '°F']], def: 'c' },
  ],
  mount(root, o) {
    const names = ['fl', 'fr', 'rl', 'rr'];
    const tyre = n => `<div class="pw-tyre" data-k="${n}" data-base="pw-tyre"><b data-k="${n}t">—</b><div class="wear"><i data-k="${n}w"></i></div><small data-k="${n}i">—</small></div>`;
    const card = PW.tpl(`<div class="pw-card"><div class="pw-title"><span>Pneus</span><em data-k="cmp"></em></div>
      <div class="pw-tyres">${names.map(tyre).join('')}</div>
      ${o.brakes ? '<div class="pw-brakes">' + names.map(n => `<div>${n.toUpperCase()}<b data-k="${n}b">—</b></div>`).join('') + '</div>' : ''}</div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card);
    const temp = c => o.unit === 'f' ? c * 9 / 5 + 32 : c;
    const press = kpa => o.pressure === 'bar' ? (kpa / 100).toFixed(2) + ' bar' : o.pressure === 'psi' ? (kpa * 0.145038).toFixed(1) + ' psi' : kpa.toFixed(0) + ' kPa';
    // Bleu = froid, vert = dans la fenêtre idéale, rouge = trop chaud (écart à la température optimale).
    const hue = d => d < 0 ? 130 + Math.min(1, -d / 25) * 80 : 130 - Math.min(1, d / 25) * 130;
    return {
      update(g) {
        const c = (g('tyre.front.compound', '') || g('tyre.rear.compound', ''));
        b.text('cmp', c);
        for (const n of names) {
          const t = (+o.tempoffset || 0) + (o.tempsrc === 'surface' ? (g(`tyre.${n}.temp_left`) + g(`tyre.${n}.temp_center`) + g(`tyre.${n}.temp_right`)) / 3 : o.tempsrc === 'inner' ? g(`tyre.${n}.inner`, NaN) : g(`tyre.${n}.carcass`, NaN));
          const opt = g(`tyre.${n}.optimal`, 0) || 88;
          const wear = g(`tyre.${n}.wear`, 0);
          b.text(n + 't', isFinite(t) ? Math.round(temp(t)) + '°' : '—');
          b.css(n, '--c', isFinite(t) ? `hsl(${hue(t - opt)} 70% 52%)` : '#666');
          const shown = o.wearmode === 'used' ? 100 - wear : wear;
          b.scaleX(n + 'w', shown / 100);
          b.css(n + 'w', 'background', wear < 30 ? 'var(--pw-bad)' : wear < 55 ? 'var(--pw-warn)' : 'var(--pw-ok)');
          const p = g(`tyre.${n}.pressure`);
          b.text(n + 'i', (o.pressure === 'off' ? '' : (p ? press(p) + ' · ' : '')) + F.n(shown, 0) + ' %');
          if (b.el[n + 'b']) b.text(n + 'b', Math.round(temp(g(`brake.${n}.temp`, 0))) + '°');
        }
      },
    };
  },
};

/* ---------- Chrono et écarts ---------- */
PW.widgets.timing = {
  name: 'Chrono et écarts',
  keys: ['position.', 'lap.', 'gap.'],
  hz: 5, size: [280, 170],
  options: [
    { id: 'gaps', label: 'Écarts devant / derrière', type: 'bool', def: true },
  ],
  mount(root, o) {
    const card = PW.tpl(`<div class="pw-card"><div class="pw-timing">
        <div class="pw-pos" data-k="pos">P—</div><div class="pw-delta" data-k="delta" data-base="pw-delta">—</div></div>
      <div class="pw-sep"></div>
      <div class="pw-kv"><span>Tour</span><b data-k="lap">—</b></div>
      <div class="pw-kv"><span>Dernier</span><b data-k="last">—</b></div>
      <div class="pw-kv"><span>Meilleur</span><b data-k="best">—</b></div>
      ${o.gaps ? '<div class="pw-kv"><span>Devant</span><b data-k="ahead">—</b></div><div class="pw-kv"><span>Derrière</span><b data-k="behind">—</b></div>' : ''}</div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card);
    return {
      update(g) {
        b.text('pos', 'P' + g('position.place', '—'));
        const d = g('lap.delta_best');
        b.text('delta', F.signed(d, 2));
        b.cls('delta', d == null ? '' : d <= 0 ? 'pw-ok' : 'pw-bad');
        b.text('lap', String(g('lap.number', '—')));
        b.text('last', F.lap(g('position.last_lap')));
        b.text('best', F.lap(g('position.best_lap')));
        if (b.el.ahead) {
          const a = g('gap.car_ahead'), r = g('gap.car_behind');
          b.text('ahead', a > 0 ? F.n(a, 1) + ' s' : '—');
          b.text('behind', r > 0 ? F.n(r, 1) + ' s' : '—');
        }
      },
    };
  },
};

/* ---------- Pilotage ---------- */
PW.widgets.inputs = {
  name: 'Pilotage',
  keys: ['player.speed', 'player.gear', 'player.rpm', 'player.throttle', 'player.brake', 'player.clutch', 'player.steering'],
  hz: o => +o.fps, size: [260, 180],
  options: [
    { id: 'fps', label: 'Fluidité (images/s)', type: 'select', choices: [['10', '10'], ['15', '15'], ['20', '20'], ['30', '30']], def: '20' },
    { id: 'graph', label: 'Graphe des inputs', type: 'bool', def: true },
    { id: 'window', label: 'Durée du graphe', type: 'select', choices: [['5', '5 s'], ['10', '10 s'], ['20', '20 s']], def: '10' },
    { id: 'steering', label: 'Volant dans le graphe', type: 'bool', def: true },
    { id: 'clutch', label: 'Embrayage', type: 'bool', def: false },
    { id: 'mph', label: 'Vitesse en mph', type: 'bool', def: false },
  ],
  mount(root, o) {
    const card = PW.tpl(`<div class="pw-card"><div class="pw-rpm"><i data-k="rpm"></i></div>
      <div class="pw-inputs">
        <div class="pw-pedals"><div class="pw-pedal b"><i data-k="brake"></i></div><div class="pw-pedal t"><i data-k="thr"></i></div>${o.clutch ? '<div class="pw-pedal c"><i data-k="clu"></i></div>' : ''}</div>
        <div class="pw-gear" data-k="gear">N</div>
        <div><div class="pw-speed"><span data-k="speed">0</span><small>${o.mph ? 'mph' : 'km/h'}</small></div></div>
      </div>${o.graph ? '<canvas class="pw-graph" width="460" height="112"></canvas>' : ''}</div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card);
    let get = null, timer = 0;

    // Graphe : défilement continu, échantillonné à 25 Hz (la courbe avance même si les inputs ne bougent pas).
    const canvas = card.querySelector('canvas');
    if (canvas) {
      const ctx = canvas.getContext('2d'), W = canvas.width, H = canvas.height, win = (+o.window || 10) * 1000;
      const buf = [];
      const line = (now, i, color, width, map) => {
        ctx.beginPath(); ctx.strokeStyle = color; ctx.lineWidth = width; ctx.lineJoin = 'round';
        buf.forEach((p, n) => { const x = W - (now - p[0]) / win * W, y = map(p[i]); n ? ctx.lineTo(x, y) : ctx.moveTo(x, y); });
        ctx.stroke();
      };
      timer = setInterval(() => {
        if (!get || !PW.feed.game) return;
        const now = performance.now();
        buf.push([now, get('player.throttle', 0), get('player.brake', 0), get('player.steering', 0), get('player.clutch', 0)]);
        while (buf.length && buf[0][0] < now - win - 100) buf.shift();
        ctx.clearRect(0, 0, W, H);
        ctx.strokeStyle = 'rgba(255,255,255,.09)'; ctx.lineWidth = 1;
        for (const f of [0, .5, 1]) { const y = Math.round(2 + f * (H - 4)) + .5; ctx.beginPath(); ctx.moveTo(0, y); ctx.lineTo(W, y); ctx.stroke(); }
        const pct = v => H - 3 - Math.max(0, Math.min(100, v)) / 100 * (H - 6);
        if (o.steering) line(now, 3, 'rgba(232,237,244,.55)', 2, v => H / 2 - Math.max(-100, Math.min(100, v)) / 100 * (H / 2 - 3));
        if (o.clutch) line(now, 4, '#4f9dff', 2, pct);
        line(now, 1, '#38e07b', 3, pct);
        line(now, 2, '#ff4d5e', 3, pct);
      }, Math.round(1000 / (+o.fps || 20)));
    }

    return {
      update(g) {
        get = g;
        const rpm = g('player.rpm', 0), max = g('player.rpm_max', 8000) || 8000, f = rpm / max;
        b.scaleX('rpm', f);
        b.css('rpm', 'background', f > 0.95 ? 'var(--pw-bad)' : f > 0.85 ? 'var(--pw-warn)' : 'var(--pw-ok)');
        b.scaleY('brake', g('player.brake', 0) / 100);
        b.scaleY('thr', g('player.throttle', 0) / 100);
        if (b.el.clu) b.scaleY('clu', g('player.clutch', 0) / 100);
        const gear = g('player.gear', 0);
        b.text('gear', gear < 0 ? 'R' : gear === 0 ? 'N' : String(gear));
        const kmh = g('player.speed', 0);
        b.text('speed', String(Math.round(o.mph ? kmh * 0.621371 : kmh)));
      },
      destroy() { clearInterval(timer); },
    };
  },
};

/* ---------- Classement ---------- */
const CLASS_COLORS = [[/hyper/i, '#e5393b'], [/lmp2/i, '#2d7ff9'], [/gt3/i, '#2fbf71'], [/lmp3/i, '#a85cf2'], [/gte/i, '#f2a93b']];
const classColor = c => (CLASS_COLORS.find(([re]) => re.test(c)) || [0, '#7b8494'])[1];

PW.widgets.standings = {
  name: 'Classement',
  keys: ['grid.', 'session.vehicles', 'session.type', 'session.track_length', 'player.speed'],
  hz: 2, size: [300, 230],
  options: [
    { id: 'ahead', label: 'Voitures devant', type: 'number', min: 0, max: 8, step: 1, def: 3 },
    { id: 'behind', label: 'Voitures derrière', type: 'number', min: 0, max: 8, step: 1, def: 3 },
    { id: 'columns', label: 'Colonne de droite', type: 'select', choices: [['auto', 'Auto (écarts en course, chronos sinon)'], ['gaps', 'Toujours les écarts'], ['laps', 'Toujours les chronos']], def: 'auto' },
    { id: 'scope', label: 'Classement', type: 'select', choices: [['overall', 'Général'], ['class', 'Par classe (ma classe)']], def: 'overall' },
  ],
  mount(root, o) {
    const rows = o.ahead + o.behind + 1;
    const classOnly = o.scope ? o.scope === 'class' : !!o.classOnly;
    const card = PW.tpl(`<div class="pw-card"><div class="pw-title"><span>Classement</span><em data-k="cls"></em></div><table class="pw-board"><tbody data-k="body"></tbody></table></div>`).firstChild;
    root.appendChild(card);
    const body = card.querySelector('tbody');
    const trs = [];
    for (let i = 0; i < rows; i++) {
      const tr = document.createElement('tr');
      tr.innerHTML = '<td class="c"></td><td class="p"></td><td class="n"></td><td class="g"></td>';
      body.appendChild(tr); trs.push({ tr, c: tr.children[0], p: tr.children[1], n: tr.children[2], g: tr.children[3], key: '' });
    }
    const pad = i => String(i).padStart(3, '0');
    const short = n => { const p = String(n).trim().split(/\s+/); return p.length > 1 ? p[0][0] + '. ' + p.slice(1).join(' ') : n; };
    return {
      update(g) {
        const n = g('session.vehicles', 0), all = [], length = Math.max(1, g('session.track_length', 1)), speed = g('player.speed', 0) / 3.6;
        for (let i = 0; i < n; i++) {
          const p = `grid.${pad(i)}`, place = g(p + '.place');
          if (place == null) continue;
          all.push({ place, name: g(p + '.driver', ''), cls: g(p + '.class', ''), dist: g(p + '.laps', 0) * length + g(p + '.lap_dist', 0),
            next: Math.max(0, g(p + '.behind_next', 0)), best: g(p + '.best_lap', 0), pit: g(p + '.in_pits', false), me: g(p + '.is_player', false) });
        }
        all.sort((a, b) => a.place - b.place);
        const mi = all.findIndex(c => c.me), me = all[mi];
        // Colonne de droite : écarts en course (types 10 à 13), meilleurs tours aux essais et en qualifs.
        const type = g('session.type', -1), mode = o.columns || 'auto';
        const gaps = mode === 'gaps' || (mode === 'auto' && type >= 10 && type <= 13);
        const list = all.map((c, j) => ({ ...c, j })).filter(c => !classOnly || !me || c.cls === me.cls);
        const idx = Math.max(0, list.findIndex(c => c.me));
        let start = Math.max(0, idx - o.ahead);
        start = Math.min(start, Math.max(0, list.length - rows));
        // Écart en secondes (positif : derrière moi) : somme des retards « sur la voiture devant » donnés par le jeu,
        // sinon distance / vitesse.
        const gapOf = j => {
          let chain = 0;
          if (j < mi) for (let k = j + 1; k <= mi; k++) chain -= all[k].next; else for (let k = mi + 1; k <= j; k++) chain += all[k].next;
          const dist = me.dist - all[j].dist;
          return Math.abs(chain) > 0.05 || Math.abs(dist) < 5 ? chain : dist / Math.max(speed, 15);
        };
        trs.forEach((r, i) => {
          const c = list[start + i];
          let right = '';
          if (c) {
            if (gaps) {
              if (me && !c.me) {
                const bd = me.dist - c.dist;
                right = Math.abs(bd) >= length ? (bd > 0 ? '+' : '−') + Math.floor(Math.abs(bd) / length) + ' T' : F.signed(gapOf(c.j), 1);
              }
            } else right = c.best > 0 ? F.lap(c.best) : '—';
          }
          const key = c ? [c.place, c.name, c.cls, c.me, c.pit, right].join('|') : '';
          if (r.key === key) return;
          r.key = key;
          r.tr.hidden = !c;
          if (!c) return;
          r.tr.className = (c.me ? 'me ' : '') + (c.pit ? 'pit' : '');
          r.c.style.background = classColor(c.cls);
          r.p.textContent = classOnly ? String(start + i + 1) : c.place;
          r.n.textContent = short(c.name);
          r.g.textContent = right;
        });
      },
    };
  },
};

/* ---------- Session et météo ---------- */
const SESSIONS = { 0: 'Test', 1: 'Essais', 2: 'Essais', 3: 'Essais', 4: 'Essais', 5: 'Qualifs', 6: 'Qualifs', 7: 'Qualifs', 8: 'Qualifs', 9: 'Warm-up', 10: 'Course', 11: 'Course', 12: 'Course', 13: 'Course' };
PW.widgets.session = {
  name: 'Session et météo',
  keys: ['session.', 'weather.', 'lap.number', 'strategy.race_laps_left'],
  hz: 1, size: [280, 150],
  options: [
    { id: 'weather', label: 'Météo', type: 'bool', def: true },
  ],
  mount(root, o) {
    const card = PW.tpl(`<div class="pw-card">
      <div class="pw-title"><span data-k="track">—</span><em data-k="type"></em></div>
      <div class="pw-row"><span class="pw-big" data-k="time">—</span><span class="pw-sub" data-k="laps"></span></div>
      ${o.weather ? `<div class="pw-weather"><div><small>Air</small><b data-k="air">—</b></div><div><small>Piste</small><b data-k="trk">—</b></div><div><small>Pluie</small><b data-k="rain">—</b></div></div>` : ''}</div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card);
    return {
      update(g) {
        b.text('track', g('session.track', '—'));
        b.text('type', SESSIONS[g('session.type', -1)] || '');
        b.text('time', F.hms(g('session.remaining')));
        const max = g('session.max_laps', 0), left = g('strategy.race_laps_left');
        b.text('laps', max > 0 && max < 10000 ? 'Tour ' + g('lap.number', 0) + ' / ' + max : left != null ? '≈ ' + Math.round(left) + ' tours' : '');
        if (b.el.air) {
          b.text('air', F.n(g('weather.ambient'), 0) + '°');
          b.text('trk', F.n(g('weather.track'), 0) + '°');
          const rain = g('weather.rain', 0);
          b.text('rain', rain > 1 ? Math.round(rain) + ' %' : '—');
        }
      },
    };
  },
};

/* ---------- Carte du circuit ---------- */
// Même principe que le widget natif : carte enregistrée pendant un tour propre (/api/track), repère (x, -z) ;
// sans carte, un cercle avec les voitures placées selon leur distance dans le tour.
PW.widgets.trackmap = {
  name: 'Carte du circuit',
  keys: ['grid.', 'session.track', 'session.vehicles', 'demo.map'],
  hz: o => +o.fps || 20, size: [300, 330],
  options: [
    { id: 'size', label: 'Taille de la carte', type: 'number', min: 160, max: 460, step: 10, def: 300 },
    { id: 'rotate', label: 'Rotation', type: 'select', choices: [['0', '0°'], ['90', '90°'], ['180', '180°'], ['270', '270°']], def: '0' },
    { id: 'others', label: 'Afficher les autres voitures', type: 'bool', def: true },
    { id: 'fps', label: 'Fluidité (images/s)', type: 'select', choices: [['10', '10'], ['20', '20'], ['30', '30']], def: '20' },
    { id: 'sectors', label: 'Limites des secteurs', type: 'bool', def: true },
    { id: 'numbers', label: 'Numéro dans les pastilles', type: 'select', choices: [['none', 'Aucun'], ['class', 'Place en classe'], ['overall', 'Place au général']], def: 'none' },
  ],
  mount(root, o) {
    const AC = (getComputedStyle(root).getPropertyValue('--pw-accent') || '').trim() || '#19d3f3', W = +o.size || 300, K = Math.min(1.4, Math.max(0.7, W / 300)), R = 2;
    const ax = 24, ay = 24, aw = W - 48, maxH = ay + aw * 1.15 + 24;
    const card = PW.tpl(`<div class="pw-card" style="width:${W}px;height:${maxH}px;padding:0">
      <canvas width="${W * R}" height="${maxH * R}" style="width:${W}px;height:${maxH}px;position:absolute;left:0;top:0"></canvas>
      </div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card), ctx = card.querySelector('canvas').getContext('2d');
    const angle = (+o.rotate || 0) * Math.PI / 180, sin = Math.sin(angle), cos = Math.cos(angle);
    const rot = p => [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos];
    let key = null, map = null, sectors = [], tried = -1e9;
    const carPos = new Map();

    const demoMap = () => Array.from({ length: 360 }, (_, i) => { const a = i / 360 * Math.PI * 2; return [520 * Math.cos(a) + 130 * Math.cos(3 * a), 300 * Math.sin(a) + 90 * Math.sin(2 * a)]; });
    const colorOf = c => /hyper/i.test(c) ? '#e5393b' : /lmp2/i.test(c) ? '#2d7ff9' : /gt3/i.test(c) ? '#2fbf71' : /lmp3/i.test(c) ? '#a85cf2' : /gte/i.test(c) ? '#f2a93b' : '#7b8494';
    // Lissage de Chaikin d'une boucle fermée.
    const smooth = (p, rounds) => { let c = p; for (let r = 0; r < rounds && c.length > 2; r++) { const n = []; for (let i = 0; i < c.length; i++) { const a = c[i], d = c[(i + 1) % c.length]; n.push([.75 * a[0] + .25 * d[0], .75 * a[1] + .25 * d[1]], [.25 * a[0] + .75 * d[0], .25 * a[1] + .75 * d[1]]); } c = n; } return c; };

    return {
      update(g) {
        const track = g('session.track', ''), demo = !!g('demo.map', false), k = demo ? 'demo' : track;
        if (k !== key) { key = k; map = null; tried = -1e9; if (demo) { map = demoMap(); sectors = [0.31, 0.67]; } else sectors = []; }
        if (!map && !demo && track && performance.now() - tried > 3000) {
          tried = performance.now();
          fetch('/api/track/' + encodeURIComponent(track)).then(r => r.ok ? r.json() : null).then(m => { if (m && key === k) { map = m.points; sectors = m.sectors || []; } }).catch(() => {});
        }
        const n = Math.min(g('session.vehicles', 0), 104), cars = [];
        for (let i = 0; i < n; i++) {
          const p = 'grid.' + String(i).padStart(3, '0');
          const x = g(p + '.x'), z = g(p + '.z'); if (x == null || z == null) continue;
          cars.push({ idx: i, xy: [x, -z], d: g(p + '.lap_dist', 0), cls: g(p + '.class', ''), me: !!g(p + '.is_player'), pit: !!g(p + '.in_pits'), place: g(p + '.place', 0), rank: 0 });
        }
        const seen = {};
        cars.slice().sort((a, b) => a.place - b.place).forEach(c => { seen[c.cls] = (seen[c.cls] || 0) + 1; c.rank = seen[c.cls]; });
        for (let i = cars.length - 1; i >= 0; i--) if (!cars[i].me && !o.others) cars.splice(i, 1);
        // Fluidité : positions publiées ~5 fois par seconde ; on prolonge le mouvement à la vitesse observée (0,25 s au plus).
        const nowS = performance.now() / 1000;
        for (const c of cars) {
          let e = carPos.get(c.idx);
          if (!e) { e = { x: c.xy[0], y: c.xy[1], t: nowS, vx: 0, vy: 0 }; carPos.set(c.idx, e); }
          if (c.xy[0] !== e.x || c.xy[1] !== e.y) {
            const dt = nowS - e.t, dx = c.xy[0] - e.x, dy = c.xy[1] - e.y, ok = dt > 0.005 && dt < 1 && Math.abs(dx) + Math.abs(dy) < 150;
            e.vx = ok ? dx / dt : 0; e.vy = ok ? dy / dt : 0; e.x = c.xy[0]; e.y = c.xy[1]; e.t = nowS;
          }
          const ahead = Math.min(0.25, Math.max(0, nowS - e.t));
          c.xy = [e.x + e.vx * ahead, e.y + e.vy * ahead];
        }
        const pts = map ? map.map(rot) : [];
        let minx = 0, miny = 0, bw = 1, bh = 1;
        if (pts.length) { const xs = pts.map(p => p[0]), ys = pts.map(p => p[1]); minx = Math.min(...xs); miny = Math.min(...ys); bw = Math.max(1, Math.max(...xs) - minx); bh = Math.max(1, Math.max(...ys) - miny); }
        const ah = aw * (pts.length ? Math.min(1.15, Math.max(0.5, bh / bw)) : 1), H = ay + ah + 24;
        card.style.height = H + 'px';
        ctx.setTransform(R, 0, 0, R, 0, 0); ctx.clearRect(0, 0, W, maxH);
        const mode = o.numbers || 'none';
        const dot = (c, x, y) => {
          const num = mode === 'none' ? 0 : mode === 'overall' ? c.place : c.rank;
          ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
          if (c.me) {
            ctx.beginPath(); ctx.arc(x, y, 14 * K, 0, 7); ctx.globalAlpha = .22; ctx.fillStyle = AC; ctx.fill(); ctx.globalAlpha = 1; ctx.fillStyle = 'rgba(0,0,0,0)'; ctx.fill();
            ctx.beginPath(); ctx.arc(x, y, 9 * K, 0, 7); ctx.fillStyle = AC; ctx.fill(); ctx.lineWidth = 2 * K; ctx.strokeStyle = '#e8edf4'; ctx.stroke();
            if (num > 0) { ctx.fillStyle = '#031014'; ctx.font = `700 ${12 * K}px Consolas, monospace`; ctx.fillText(num, x, y + 0.5); }
          } else {
            ctx.beginPath(); ctx.arc(x, y, (num > 0 ? 7 : 4.6) * K, 0, 7); ctx.fillStyle = c.pit ? '#7f8a9a' : colorOf(c.cls); ctx.fill(); ctx.lineWidth = 1.4; ctx.strokeStyle = 'rgba(6,8,12,.9)'; ctx.stroke();
            if (num > 0) { ctx.fillStyle = '#fff'; ctx.font = `700 ${9.5 * K}px Consolas, monospace`; ctx.fillText(num, x, y + 0.5); }
          }
          ctx.textBaseline = 'alphabetic';
        };
        const order = cars.slice().sort((a, b) => (a.me - b.me) || (b.place - a.place));
        const road = pts2 => {
          ctx.beginPath(); pts2.forEach((p, i) => i ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1])); ctx.closePath();
          ctx.lineJoin = 'round'; ctx.lineCap = 'round';
          ctx.lineWidth = 12 * K; ctx.strokeStyle = '#707d92'; ctx.stroke();
          ctx.lineWidth = 8 * K; ctx.strokeStyle = '#1e242e'; ctx.stroke();
          ctx.lineWidth = 1.2; ctx.strokeStyle = 'rgba(255,255,255,.15)'; ctx.stroke();
        };
        const start = (a, dir) => {
          const l = Math.hypot(dir[0], dir[1]) || 1, tx = dir[0] / l, ty = dir[1] / l, nx = -ty * 8 * K, ny = tx * 8 * K;
          ctx.lineCap = 'butt'; ctx.beginPath(); ctx.moveTo(a[0] - nx, a[1] - ny); ctx.lineTo(a[0] + nx, a[1] + ny); ctx.lineWidth = 3.5 * K; ctx.strokeStyle = '#e8edf4'; ctx.stroke();
          const t = [a[0] + tx * 15 * K, a[1] + ty * 15 * K], bk = 5 * K;
          ctx.beginPath(); ctx.moveTo(t[0] - tx * bk - ty * bk, t[1] - ty * bk + tx * bk); ctx.lineTo(t[0], t[1]); ctx.lineTo(t[0] - tx * bk + ty * bk, t[1] - ty * bk - tx * bk);
          ctx.lineWidth = 2; ctx.strokeStyle = AC; ctx.stroke();
        };
        if (pts.length > 2) {
          const s = Math.min(aw / bw, ah / bh), ox = ax + (aw - bw * s) / 2, oy = ay + (ah - bh * s) / 2;
          const to = p => [ox + (p[0] - minx) * s, oy + (p[1] - miny) * s];
          road(smooth(pts, 1).map(to));
          const a = to(pts[0]), c = to(pts[1]); start(a, [c[0] - a[0], c[1] - a[1]]);
          if (o.sectors !== false && sectors.length >= 2) {
            const N = pts.length, at = f => { const i = Math.min(N - 1, Math.floor(f * N)); const p = to(pts[i]), b = to(pts[(i + N - 2) % N]), n = to(pts[(i + 2) % N]); const dx = n[0] - b[0], dy = n[1] - b[1], l = Math.hypot(dx, dy) || 1; return { p, nx: -dy / l, ny: dx / l }; };
            ctx.strokeStyle = '#ffd633'; ctx.lineWidth = 2.5 * K; ctx.lineCap = 'butt';
            sectors.slice(0, 2).forEach(f => { const { p, nx, ny } = at(f); ctx.beginPath(); ctx.moveTo(p[0] - nx * 7.5 * K, p[1] - ny * 7.5 * K); ctx.lineTo(p[0] + nx * 7.5 * K, p[1] + ny * 7.5 * K); ctx.stroke(); });
            const edges = [0, sectors[0], sectors[1], 1], cx = ox + bw * s / 2, cy = oy + bh * s / 2;
            ctx.fillStyle = '#ffd633'; ctx.font = `700 ${10 * K}px Consolas, monospace`; ctx.textAlign = 'center'; ctx.textBaseline = 'middle';
            for (let q = 0; q < 3; q++) {
              const { p, nx, ny } = at((edges[q] + edges[q + 1]) / 2), x1 = p[0] + nx * 17 * K, y1 = p[1] + ny * 17 * K, x2 = p[0] - nx * 17 * K, y2 = p[1] - ny * 17 * K;
              const out = (x1 - cx) ** 2 + (y1 - cy) ** 2 > (x2 - cx) ** 2 + (y2 - cy) ** 2;
              ctx.fillText('S' + (q + 1), out ? x1 : x2, out ? y1 : y2);
            }
            ctx.textBaseline = 'alphabetic';
          }
          order.forEach(c => { const p = to(rot(c.xy)); dot(c, p[0], p[1]); });
        } else {
          const r = Math.max(20, Math.min(aw, ah) / 2 - 8), cx = ax + aw / 2, cy = ay + ah / 2;
          road(Array.from({ length: 72 }, (_, i) => [cx + r * Math.cos(i / 72 * 6.2832), cy + r * Math.sin(i / 72 * 6.2832)]));
          start([cx, cy - r], [1, 0]);
          const len = Math.max(1, g('session.track_length', 1));
          order.forEach(c => { const a = -Math.PI / 2 + c.d / len * 6.2832; dot(c, cx + r * Math.cos(a), cy + r * Math.sin(a)); });
          ctx.fillStyle = '#7f8a9a'; ctx.font = '10.5px sans-serif'; ctx.textAlign = 'center'; ctx.fillText('Un tour propre trace la carte', W / 2, cy);
        }
      },
    };
  },
};

/* ---------- Drapeaux ---------- */
// Un drapeau, ou les deux plus importants s'il y en a plusieurs, affichés en grand ; rien quand il n'y en a aucun.
// Ordre : rouge, neutralisation, noir, jaune, bleu, blanc, vert (8 s après une relance), damier.
PW.widgets.flags = {
  name: 'Drapeaux',
  keys: ['session.phase', 'session.yellow', 'session.sector_yellow.', 'player.blue_flag', 'player.finish_status'],
  hz: 4, size: [290, 44],
  options: [],
  mount(root, o) {
    const box = PW.tpl('<div class="pw-flags"></div>').firstChild;
    root.appendChild(box);
    let before = null, greenUntil = 0, key = '';
    return {
      update(g) {
        const phase = g('session.phase', 5), ystate = g('session.yellow', 0), finish = g('player.finish_status', 0), now = performance.now();
        if (before != null && phase === 5 && before !== 5 && before >= 0) greenUntil = now + 8000;
        before = phase;
        const list = [], add = (long, short, bg, fg, cls) => list.push({ long, short, bg, fg, cls: cls || '' });
        if (phase === 7 || ystate === 7) add('Drapeau rouge', 'Rouge', '#ff4d5e', '#fff');
        if (phase === 6) add('Full course jaune', 'FCY', '#ffb020', '#111');
        if (finish === 3) add('Disqualifié', 'Noir', '#0c0c0c', '#fff', 'black');
        const ys = [1, 2, 3].filter(s => g('session.sector_yellow.' + s, false)).map(s => 'S' + s);
        if (ys.length) add('Jaune · ' + ys.join(' '), 'Jaune ' + ys.join(' '), '#ffd633', '#111');
        if (g('player.blue_flag', false)) add('Drapeau bleu · laisse passer', 'Bleu', '#4f9dff', '#fff');
        if (ystate === 5) add('Dernier tour', 'Dernier tour', '#f0f4fa', '#111');
        if (now < greenUntil) add('Drapeau vert', 'Vert', '#38e07b', '#111');
        if (phase === 8 || finish === 1) add('Arrivée', 'Arrivée', '#fff', '#111');
        const shown = list.slice(0, 2), k = shown.map(f => f.long).join('|');
        if (k === key) return;
        key = k;
        box.className = 'pw-flags ' + (shown.length === 1 ? 'one' : 'two');
        box.replaceChildren(...shown.map(f => {
          const el = document.createElement('div');
          el.className = 'pw-fl ' + f.cls; el.style.background = f.bg; el.style.color = f.fg;
          el.textContent = shown.length === 1 ? f.long : f.short;
          return el;
        }));
      },
    };
  },
};

/* ---------- Données de démonstration (éditeur, ou sans jeu) ---------- */
PW.demo = (() => {
  const d = {
    'game.version': 1, 'demo.map': true, 'session.track_length': 4000, 'session.track': 'Circuit de la Sarthe', 'session.type': 10, 'session.phase': 5, 'session.sector_yellow.1': false, 'session.sector_yellow.2': true, 'player.blue_flag': true, 'session.sector_yellow.3': false,
    'session.remaining': 5 * 3600 + 1234, 'session.max_laps': 0, 'session.vehicles': 12, 'weather.ambient': 21, 'weather.track': 32, 'weather.rain': 0,
    'player.speed': 232, 'player.gear': 5, 'player.rpm': 7100, 'player.rpm_max': 8000, 'player.throttle': 82, 'player.brake': 0, 'player.clutch': 0, 'player.steering': 12,
    'fuel.level': 52.4, 'fuel.capacity': 90, 'fuel.percent': 58, 'energy.virtual': 61,
    'strategy.fuel_per_lap': 3.61, 'strategy.fuel_laps_left': 14.5, 'strategy.energy_per_lap': 3.5, 'strategy.energy_laps_left': 17.4,
    'strategy.limiting': 'fuel', 'strategy.race_laps_left': 48, 'strategy.fuel_to_add': 120.4, 'strategy.energy_to_add': 108,
    'lap.number': 14, 'lap.delta_best': -0.21, 'gap.car_ahead': 2.4, 'gap.car_behind': 3.8,
    'position.place': 4, 'position.last_lap': 207.412, 'position.best_lap': 206.988,
    'tyre.front.compound': 'Medium', 'tyre.rear.compound': 'Medium',
  };
  ['fl', 'fr', 'rl', 'rr'].forEach((n, i) => {
    Object.assign(d, { [`tyre.${n}.temp_left`]: 80 + i, [`tyre.${n}.temp_center`]: 86 + i, [`tyre.${n}.temp_right`]: 92 + i, [`tyre.${n}.carcass`]: 84 + i, [`tyre.${n}.optimal`]: 88,
      [`tyre.${n}.pressure`]: 165 + i, [`tyre.${n}.wear`]: 88 - i * 2, [`brake.${n}.temp`]: 420 + i * 20 });
  });
  const names = ['A. Martin', 'B. Dupont', 'C. Rossi', 'Toi', 'E. Keller', 'F. Sato', 'G. Novak', 'H. Silva', 'I. Moreau', 'J. Weber', 'K. Costa', 'L. Berg'];
  const cls = ['Hypercar', 'Hypercar', 'Hypercar', 'Hypercar', 'LMP2', 'Hypercar', 'LMGT3', 'LMP2', 'LMGT3', 'LMGT3', 'LMP2', 'LMGT3'];
  names.forEach((nm, i) => {
    const p = `grid.${String(i).padStart(3, '0')}`;
    Object.assign(d, { [`${p}.driver`]: nm, [`${p}.class`]: cls[i], [`${p}.place`]: i + 1, [`${p}.laps`]: 14, [`${p}.behind_leader`]: i * 4.3, [`${p}.lap_dist`]: 3200 - i * 160, [`${p}.behind_next`]: i ? 3.1 + Math.sin(i * 0.7) : 0, [`${p}.best_lap`]: 206.9 + i * 0.37,
      [`${p}.in_pits`]: false, [`${p}.is_player`]: nm === 'Toi' });
  });
  return d;
})();
