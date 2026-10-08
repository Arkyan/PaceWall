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
    { id: 'unit', label: 'Température', type: 'select', choices: [['c', '°C'], ['f', '°F']], def: 'c' },
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
          const t = (g(`tyre.${n}.temp_left`) + g(`tyre.${n}.temp_center`) + g(`tyre.${n}.temp_right`)) / 3;
          const opt = g(`tyre.${n}.optimal`, 0) || 88;
          const wear = g(`tyre.${n}.wear`, 0);
          b.text(n + 't', isFinite(t) ? Math.round(temp(t)) + '°' : '—');
          b.css(n, '--c', isFinite(t) ? `hsl(${hue(t - opt)} 70% 52%)` : '#666');
          b.scaleX(n + 'w', 1 - wear / 100);
          b.css(n + 'w', 'background', wear > 70 ? 'var(--pw-bad)' : wear > 45 ? 'var(--pw-warn)' : 'var(--pw-ok)');
          const p = g(`tyre.${n}.pressure`);
          b.text(n + 'i', (o.pressure === 'off' ? '' : (p ? press(p) + ' · ' : '')) + F.n(100 - wear, 0) + ' %');
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
  keys: ['grid.', 'session.vehicles'],
  hz: 2, size: [300, 230],
  options: [
    { id: 'ahead', label: 'Voitures devant', type: 'number', min: 0, max: 8, step: 1, def: 3 },
    { id: 'behind', label: 'Voitures derrière', type: 'number', min: 0, max: 8, step: 1, def: 3 },
    { id: 'classOnly', label: 'Ma classe seulement', type: 'bool', def: false },
  ],
  mount(root, o) {
    const rows = o.ahead + o.behind + 1;
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
        const n = g('session.vehicles', 0), cars = [];
        for (let i = 0; i < n; i++) {
          const place = g(`grid.${pad(i)}.place`);
          if (place == null) continue;
          cars.push({ place, name: g(`grid.${pad(i)}.driver`, ''), cls: g(`grid.${pad(i)}.class`, ''), laps: g(`grid.${pad(i)}.laps`, 0),
            lead: g(`grid.${pad(i)}.behind_leader`, 0), pit: g(`grid.${pad(i)}.in_pits`, false), me: g(`grid.${pad(i)}.is_player`, false) });
        }
        const me = cars.find(c => c.me);
        let list = cars.sort((a, b) => a.place - b.place);
        if (o.classOnly && me) list = list.filter(c => c.cls === me.cls);
        const idx = Math.max(0, list.findIndex(c => c.me));
        let start = Math.max(0, idx - o.ahead);
        start = Math.min(start, Math.max(0, list.length - rows));
        trs.forEach((r, i) => {
          const c = list[start + i];
          const key = c ? [c.place, c.name, c.cls, c.me, c.pit, Math.round(c.lead * 10), c.laps].join('|') : '';
          if (r.key === key) return;
          r.key = key;
          r.tr.hidden = !c;
          if (!c) return;
          r.tr.className = (c.me ? 'me ' : '') + (c.pit ? 'pit' : '');
          r.c.style.background = classColor(c.cls);
          r.p.textContent = o.classOnly ? String(start + i + 1) : c.place;
          r.n.textContent = short(c.name);
          let gap = '';
          if (me && !c.me) {
            const dl = me.laps - c.laps, dt = c.lead - me.lead;
            gap = Math.abs(dl) >= 1 && Math.abs(dt) > 30 ? (dl > 0 ? '+' : '−') + Math.abs(dl) + ' T' : F.signed(dt, 1);
          }
          r.g.textContent = c.me ? '' : gap;
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
    const card = PW.tpl(`<div class="pw-card"><div class="pw-flag" data-k="flag" data-base="pw-flag">Jaune</div>
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
        const phase = g('session.phase', 5), yellow = g('session.yellow', 0);
        b.cls('flag', phase === 6 || yellow > 0 ? 'on' : '');
        b.text('flag', phase === 6 ? 'Full course yellow / Safety car' : yellow > 0 ? 'Jaune' : '');
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

/* ---------- Données de démonstration (éditeur, ou sans jeu) ---------- */
PW.demo = (() => {
  const d = {
    'game.version': 1, 'session.track': 'Circuit de la Sarthe', 'session.type': 10, 'session.phase': 5, 'session.yellow': 0,
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
    Object.assign(d, { [`tyre.${n}.temp_left`]: 80 + i, [`tyre.${n}.temp_center`]: 86 + i, [`tyre.${n}.temp_right`]: 92 + i, [`tyre.${n}.optimal`]: 88,
      [`tyre.${n}.pressure`]: 165 + i, [`tyre.${n}.wear`]: 12 + i * 2, [`brake.${n}.temp`]: 420 + i * 20 });
  });
  const names = ['A. Martin', 'B. Dupont', 'C. Rossi', 'Toi', 'E. Keller', 'F. Sato', 'G. Novak', 'H. Silva', 'I. Moreau', 'J. Weber', 'K. Costa', 'L. Berg'];
  const cls = ['Hypercar', 'Hypercar', 'Hypercar', 'Hypercar', 'LMP2', 'Hypercar', 'LMGT3', 'LMP2', 'LMGT3', 'LMGT3', 'LMP2', 'LMGT3'];
  names.forEach((nm, i) => {
    const p = `grid.${String(i).padStart(3, '0')}`;
    Object.assign(d, { [`${p}.driver`]: nm, [`${p}.class`]: cls[i], [`${p}.place`]: i + 1, [`${p}.laps`]: 14, [`${p}.behind_leader`]: i * 4.3,
      [`${p}.in_pits`]: false, [`${p}.is_player`]: nm === 'Toi' });
  });
  return d;
})();
