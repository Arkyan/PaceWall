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

const lapsClass = v => v == null ? '' : v < PW.prefs.thr_fuel_crit ? 'pw-bad' : v < PW.prefs.thr_fuel_warn ? 'pw-warn' : '';
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
    </div><div class="pw-goal" data-k="${id}goal"></div>`;
}

PW.widgets.fuel = {
  name: 'Carburant et énergie',
  keys: ['fuel.', 'energy.virtual', 'strategy.'],
  hz: 4, size: [300, 150],
  options: [
    { id: 'energy', label: 'Afficher l’énergie virtuelle', type: 'bool', def: true },
    { id: 'margin', label: 'Marge de sécurité (tours)', type: 'number', min: 0, max: 3, step: 0.5, def: 1 },
    { id: 'target', label: 'Objectif de consommation (+1 tour)', type: 'bool', def: true },
  ],
  mount(root, o) {
    const U = PW.unit, fuelName = U.fuelName();
    const vol = v => v == null ? v : U.fuel(v);
    const card = PW.tpl(`<div class="pw-card">${resourceBlock('f', 'Carburant', fuelName)}${o.energy ? '<div class="pw-sep"></div>' + resourceBlock('e', 'Énergie virtuelle', '%') : ''}</div>`).firstChild;
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
      // Objectif : la consommation à tenir pour boucler un tour de plus avec ce qu'il reste.
      let goal = '';
      if (o.target) {
        goal = '+1 tour : —';
        if (level > 0 && per > 0 && laps != null) {
          const need = level / (Math.floor(laps) + 1);
          goal = '+1 tour : ' + need.toFixed(2) + ' ' + (id === 'f' ? fuelName : '%') + '/tour (' + Math.round(Math.max(0, 1 - need / per) * 100) + ' % de moins)';
        }
      }
      b.text(id + 'goal', goal);
    };
    return {
      update(g) {
        const lim = g('strategy.limiting');
        fill('f', vol(g('fuel.level')), g('fuel.percent'), vol(g('strategy.fuel_per_lap')), g('strategy.fuel_laps_left'), vol(g('strategy.fuel_to_add')), lim === 'fuel', 1);
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
    { id: 'pressure', label: 'Pression', type: 'select', choices: [['auto', 'Unité des réglages'], ['kpa', 'kPa'], ['bar', 'bar'], ['psi', 'psi'], ['off', 'Masquer']], def: 'auto' },
    { id: 'brakes', label: 'Températures des freins', type: 'bool', def: false },
    { id: 'tempsrc', label: 'Température affichée', type: 'select', choices: [['carcass', 'Carcasse'], ['inner', 'Couche interne'], ['surface', 'Surface (moyenne)']], def: 'carcass' },
    { id: 'tempoffset', label: 'Décalage de température (°C)', type: 'number', min: -10, max: 10, step: 0.5, def: 0 },
    { id: 'wearmode', label: "Affichage de l'usure", type: 'select', choices: [['state', 'État du pneu (100 → 0 %)'], ['used', 'Usure (0 → 100 %)']], def: 'state' },
    { id: 'unit', label: 'Unité de température', type: 'select', choices: [['auto', 'Unité des réglages'], ['c', '°C'], ['f', '°F']], def: 'auto' },
  ],
  mount(root, o) {
    const names = ['fl', 'fr', 'rl', 'rr'];
    const tyre = n => `<div class="pw-tyre" data-k="${n}" data-base="pw-tyre"><b data-k="${n}t">—</b><div class="wear"><i data-k="${n}w"></i></div><small data-k="${n}i">—</small></div>`;
    const card = PW.tpl(`<div class="pw-card"><div class="pw-title"><span>Pneus</span><em data-k="cmp"></em></div>
      <div class="pw-tyres">${names.map(tyre).join('')}</div>
      ${o.brakes ? '<div class="pw-brakes">' + names.map(n => `<div>${n.toUpperCase()}<b data-k="${n}b">—</b></div>`).join('') + '</div>' : ''}</div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card);
    // « auto » : l'unité des réglages ; une unité imposée sur ce widget garde la priorité.
    const P = PW.prefs;
    const temp = c => o.unit === 'f' ? c * 9 / 5 + 32 : o.unit === 'c' ? c : PW.unit.temp(c);
    const press = kpa => o.pressure === 'bar' ? (kpa / 100).toFixed(2) + ' bar' : o.pressure === 'psi' ? (kpa * 0.145038).toFixed(1) + ' psi' : o.pressure === 'kpa' ? kpa.toFixed(0) + ' kPa' : PW.unit.pressure(kpa);
    // Bleu = froid, vert = dans la fenêtre idéale, rouge = trop chaud (écart à la température optimale).
    const win = Math.max(1, P.thr_temp_window);
    const hue = d => d < 0 ? 130 + Math.min(1, -d / win) * 80 : 130 - Math.min(1, d / win) * 130;
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
          b.css(n + 'w', 'background', wear < P.thr_wear_crit ? 'var(--pw-bad)' : wear < P.thr_wear_warn ? 'var(--pw-warn)' : 'var(--pw-ok)');
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
  keys: ['position.', 'lap.', 'gap.', 'grid.', 'session.vehicles', 'session.track_length', 'player.speed'],
  hz: 5, size: [280, 170],
  options: [
    { id: 'gaps', label: 'Écarts devant / derrière', type: 'bool', def: true },
    { id: 'place', label: 'Place affichée', type: 'select', choices: [['class', 'En classe'], ['overall', 'Au général']], def: 'class' },
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
        // Place en classe par défaut ; au général si demandé ou si le classement manque.
        const mine = o.place === 'overall' ? null : nearCars(g).find(c => c.me);
        b.text('pos', 'P' + (mine ? mine.rank : g('position.place', '—')));
        const d = g('lap.delta_best');
        b.text('delta', F.signed(d, 2));
        b.cls('delta', d == null ? '' : d <= 0 ? 'pw-ok' : 'pw-bad');
        b.text('lap', String(g('lap.number', '—')));
        b.text('last', F.lap(g('position.last_lap')));
        b.text('best', F.lap(g('position.best_lap')));
        if (b.el.ahead) {
          // Voitures qui roulent juste devant et juste derrière sur la piste (voir nearCars).
          let a = g('gap.car_ahead') > 0 ? g('gap.car_ahead') : g('position.behind_next', 0), r = g('gap.car_behind', 0);
          const cars = nearCars(g), me = cars.find(c => c.me);
          if (me) {
            const on = cars.filter(c => !c.me && (!c.pit || me.pit));
            const da = Math.min(Infinity, ...on.filter(c => c.secs > 0).map(c => c.secs));
            const db = Math.min(Infinity, ...on.filter(c => c.secs <= 0).map(c => -c.secs));
            if (da < Infinity || db < Infinity) { a = da < Infinity ? Math.max(0.1, da) : 0; r = db < Infinity ? Math.max(0.1, db) : 0; }
          }
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
    { id: 'mph', label: 'Toujours en mph (sinon : unité des réglages)', type: 'bool', def: false },
  ],
  mount(root, o) {
    const card = PW.tpl(`<div class="pw-card"><div class="pw-rpm"><i data-k="rpm"></i></div>
      <div class="pw-inputs">
        <div class="pw-pedals"><div class="pw-pedal b"><i data-k="brake"></i></div><div class="pw-pedal t"><i data-k="thr"></i></div>${o.clutch ? '<div class="pw-pedal c"><i data-k="clu"></i></div>' : ''}</div>
        <div class="pw-gear" data-k="gear">N</div>
        <div><div class="pw-speed"><span data-k="speed">0</span><small>${o.mph ? 'mph' : PW.unit.speed(0)[1]}</small></div></div>
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
        b.text('speed', String(Math.round(o.mph ? kmh * 0.621371 : PW.unit.speed(kmh)[0])));
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
            next: Math.max(0, g(p + '.behind_next', 0)), best: g(p + '.best_lap', 0), pit: g(p + '.in_pits', false), me: g(p + '.is_player', false),
            into: g(p + '.time_into', 0), est: g(p + '.est_lap', 0) });
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
          // D'abord les temps dans le tour (méthode de TinyPedal).
          const timed = j < mi ? PW.interval(all[j], me, length) : PW.interval(me, all[j], length);
          if (timed != null) return j < mi ? -timed : timed;
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
        b.text('laps', max > 0 && max < 10000 ? 'Tour ' + Math.min(g('lap.number', 0), max) + ' / ' + max : left != null ? '≈ ' + Math.round(left) + ' tours' : '');
        if (b.el.air) {
          b.text('air', F.n(PW.unit.temp(g('weather.ambient')), 0) + '°');
          b.text('trk', F.n(PW.unit.temp(g('weather.track')), 0) + '°');
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
  hz: o => +o.fps || 20, size: [300, 220],
  options: [
    { id: 'size', label: 'Largeur de la zone', type: 'number', min: 100, max: 1000, step: 10, def: 300 },
    { id: 'height', label: 'Hauteur de la zone', type: 'number', min: 100, max: 1000, step: 10, def: 220 },
    { id: 'rotate', label: 'Orientation', type: 'select', choices: [['auto', 'Auto (circuit le plus grand)'], ['0', '0°'], ['90', '90°'], ['180', '180°'], ['270', '270°']], def: 'auto' },
    { id: 'others', label: 'Afficher les autres voitures', type: 'bool', def: true },
    { id: 'fps', label: 'Fluidité (images/s)', type: 'select', choices: [['10', '10'], ['20', '20'], ['30', '30']], def: '20' },
    { id: 'sectors', label: 'Limites des secteurs', type: 'bool', def: true },
    { id: 'numbers', label: 'Numéro dans les pastilles', type: 'select', choices: [['none', 'Aucun'], ['class', 'Place en classe'], ['overall', 'Place au général']], def: 'none' },
  ],
  mount(root, o) {
    const AC = (getComputedStyle(root).getPropertyValue('--pw-accent') || '').trim() || '#19d3f3', W = +o.size || 300, Hz = +o.height || 220, K = Math.min(1.4, Math.max(0.7, Math.min(W, Hz * 1.4) / 300)), R = 2;
    // Zone fixe choisie par le pilote : le circuit s'y adapte et se tourne pour y être le plus grand possible.
    const ax = 24, ay = 24, aw = W - 48, ah = Hz - 48;
    const card = PW.tpl(`<div class="pw-card" style="width:${W}px;height:${Hz}px;padding:0">
      <canvas width="${W * R}" height="${Hz * R}" style="width:${W}px;height:${Hz}px;position:absolute;left:0;top:0"></canvas>
      </div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card), ctx = card.querySelector('canvas').getContext('2d');
    let sin = 0, cos = 1, angKey = '';
    const rot = p => [p[0] * cos - p[1] * sin, p[0] * sin + p[1] * cos];
    // Angle (0 à 179°) qui donne au circuit la plus grande taille dans la zone.
    const bestAngle = pts => {
      let best = 0, bestScale = 0;
      for (let deg = 0; deg < 180; deg++) {
        const s = Math.sin(deg * Math.PI / 180), c = Math.cos(deg * Math.PI / 180);
        let x0 = 1e9, x1 = -1e9, y0 = 1e9, y1 = -1e9;
        for (const p of pts) { const x = p[0] * c - p[1] * s, y = p[0] * s + p[1] * c; if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y; }
        const scale = Math.min(aw / Math.max(1, x1 - x0), ah / Math.max(1, y1 - y0));
        if (scale > bestScale * 1.002) { bestScale = scale; best = deg; }
      }
      return best;
    };
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
        // Orientation : automatique (calculée une fois par circuit) ou imposée.
        if (map) {
          const want = o.rotate === undefined || o.rotate === 'auto' ? 'auto' : String(o.rotate);
          const tag = want + ':' + key + ':' + map.length;
          if (tag !== angKey) {
            angKey = tag;
            const a = (want === 'auto' ? bestAngle(map) : +want || 0) * Math.PI / 180;
            sin = Math.sin(a); cos = Math.cos(a);
          }
        }
        const pts = map ? map.map(rot) : [];
        let minx = 0, miny = 0, bw = 1, bh = 1;
        if (pts.length) { const xs = pts.map(p => p[0]), ys = pts.map(p => p[1]); minx = Math.min(...xs); miny = Math.min(...ys); bw = Math.max(1, Math.max(...xs) - minx); bh = Math.max(1, Math.max(...ys) - miny); }
        const H = Hz;
        ctx.setTransform(R, 0, 0, R, 0, 0); ctx.clearRect(0, 0, W, Hz);
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
// Un drapeau, ou les deux plus importants s'il y en a plusieurs, affichés en grand (couleur seule, le secteur pour le jaune) ;
// rien quand il n'y en a aucun.
// Ordre : rouge, neutralisation, noir, jaune, bleu, blanc, vert (8 s après une relance), damier.
PW.widgets.flags = {
  name: 'Drapeaux',
  keys: ['session.phase', 'session.yellow', 'session.sector_yellow.', 'player.blue_flag', 'player.finish_status', 'grid.', 'session.vehicles', 'session.track_length', 'player.speed'],
  hz: 4, size: [290, 104],
  options: [
    { id: 'anim', label: 'Clignotement (jaune, bleu, rouge)', type: 'bool', def: true },
    { id: 'blueclass', label: 'Bleu « LMP » quand une classe plus rapide arrive', type: 'bool', def: true },
  ],
  mount(root, o) {
    const box = PW.tpl('<div class="pw-flags"></div>').firstChild;
    root.appendChild(box);
    let before = null, greenUntil = 0, key = '';
    return {
      update(g) {
        const phase = g('session.phase', 5), ystate = g('session.yellow', 0), finish = g('player.finish_status', 0), now = performance.now();
        if (before != null && phase === 5 && before !== 5 && before >= 0) greenUntil = now + 8000;
        before = phase;
        const list = [], add = (bg, label, fg, cls) => list.push({ bg, label: label || '', fg, cls: cls || '' });
        if (phase === 7 || ystate === 7) add('#ff4d5e', '', '#fff');
        if (phase === 6) add('#ffb020', '', '#111'); // neutralisation : orange, distincte du jaune d'un secteur
        if (finish === 3) add('#0c0c0c', '', '#fff', 'black');
        const ys = [1, 2, 3].filter(s => g('session.sector_yellow.' + s, false));
        if (ys.length) add('#ffd633', 'S' + ys.join(''), '#111');
        // Bleu : c'est le jeu qui le déclenche ; le sigle indique une voiture d'une classe plus rapide derrière nous.
        if (g('player.blue_flag', false)) add('#4f9dff', o.blueclass !== false ? PW.fasterClassBehind(g) : '', '#fff');
        if (ystate === 5) add('#f0f4fa', '', '#111');
        if (now < greenUntil) add('#38e07b', '', '#111');
        if (phase === 8 || finish === 1) add('#fff', '', '#111', 'checker');
        const shown = list.slice(0, 2), k = shown.map(f => f.bg + f.label).join('|');
        if (k === key) return;
        key = k;
        box.className = 'pw-flags ' + (shown.length === 1 ? 'one' : 'two');
        box.replaceChildren(...shown.map(f => {
          const el = document.createElement('div');
          // Clignotent : le rouge, le bleu et le jaune de secteur, comme au bord de la piste.
          const blink = o.anim && (f.bg === '#ff4d5e' || f.bg === '#4f9dff' || f.label);
          el.className = 'pw-fl ' + f.cls + (blink ? ' blink' : ''); el.style.setProperty('--fl', f.cls === 'black' ? '#eceff6' : f.bg);
          el.innerHTML = '<span></span>' + (f.label ? '<b></b>' : '');
          if (f.label) el.lastChild.textContent = f.label;
          return el;
        }));
      },
    };
  },
};


/* ---------- Relatif, arrêt au stand, radar ---------- */
const gridPad = i => 'grid.' + String(i).padStart(3, '0');
const shortName = n => { const p = String(n).trim().split(/\s+/); return p.length > 1 ? p[0][0] + '. ' + p.slice(1).join(' ') : n; };

// Retard, en secondes, de `behind` sur `ahead` (classé devant lui), calculé comme TinyPedal : la différence de leurs
// temps écoulés dans le tour, estimés par le jeu ; si celui de devant vient de passer la ligne, on ajoute un tour de
// celui de derrière. null si le jeu ne donne pas ces temps.
PW.interval = (ahead, behind, length) => {
  if (!(ahead.est > 1) || !(behind.est > 1)) return null;
  if ((ahead.dist - behind.dist) / Math.max(1, length) < 0) return 0;
  const gap = ahead.into - behind.into;
  return gap >= 0 ? gap : gap + behind.est;
};

// Rapidité d'une classe et son sigle (plus le nombre est grand, plus elle est rapide) ; null si inconnue.
PW.classSpeed = c => { c = String(c).toLowerCase(); return c.includes('hyper') ? [5, 'HYP'] : c.includes('lmp3') ? [3, 'LMP'] : c.includes('lmp') ? [4, 'LMP'] : c.includes('gte') ? [2, 'GTE'] : c.includes('gt3') ? [1, 'GT'] : null; };
// Sigle de la classe de la voiture qui arrive juste derrière nous (à moins de 15 s) si elle est plus rapide que la
// nôtre, '' sinon.
PW.fasterClassBehind = g => {
  const cars = nearCars(g), me = cars.find(c => c.me);
  if (!me) return '';
  const behind = cars.filter(c => !c.me && !c.pit && c.secs < 0 && c.secs > -15).sort((a, b) => b.secs - a.secs)[0];
  const theirs = behind && PW.classSpeed(behind.cls), mine = PW.classSpeed(me.cls);
  return theirs && mine && theirs[0] > mine[0] ? 'LMP' : ''; // toujours « LMP », quelle que soit la classe
};

// Les voitures du classement, situées par rapport au joueur sur la piste (secs > 0 : devant lui, en secondes).
function nearCars(g) {
  const n = g('session.vehicles', 0), L = g('session.track_length', 0);
  let me = -1;
  for (let i = 0; i < n; i++) if (g(gridPad(i) + '.is_player')) { me = i; break; }
  if (me < 0 || L < 100) return [];
  const total = i => g(gridPad(i) + '.laps', 0) * L + g(gridPad(i) + '.lap_dist', 0);
  const myDist = g(gridPad(me) + '.lap_dist', 0), myTotal = total(me), cars = [];
  // Écart en temps : différence des temps dans le tour, ramenée à plus ou moins un demi-tour du joueur ;
  // sans ces temps, distance / vitesse la plus élevée des deux voitures.
  const myTime = g(gridPad(me) + '.time_into', 0), myLap = g(gridPad(me) + '.est_lap', 0), mySpeed = g('player.speed', 0) / 3.6;
  const inGarage = g(gridPad(me) + '.garage', false);
  for (let i = 0; i < n; i++) {
    const p = gridPad(i), dist = g(p + '.lap_dist');
    if (dist == null) continue;
    if (i !== me && g(p + '.garage', false) && !inGarage) continue;
    let ahead = (((dist - myDist) % L) + L) % L;
    if (ahead > L / 2) ahead -= L;
    const t = g(p + '.time_into');
    let secs;
    if (t != null && myLap > 1) { secs = (((t - myTime) % myLap) + myLap) % myLap; if (secs > myLap / 2) secs -= myLap; }
    else secs = ahead / Math.max(15, mySpeed, g(p + '.speed', 0));
    cars.push({ name: g(p + '.driver', ''), cls: g(p + '.class', ''), place: g(p + '.place', 0), rank: 0, secs: i === me ? 0 : secs,
      laps: Math.round((total(i) - myTotal - ahead) / L), pit: g(p + '.in_pits', false), me: i === me });
  }
  const seen = {};
  for (const c of [...cars].sort((a, b) => a.place - b.place)) c.rank = seen[c.cls] = (seen[c.cls] || 0) + 1;
  return cars;
}

PW.widgets.relative = {
  name: 'Relatif',
  keys: ['grid.', 'session.vehicles', 'session.track_length', 'player.speed'],
  hz: 4, size: [300, 200],
  options: [
    { id: 'ahead', label: 'Voitures devant', type: 'number', min: 0, max: 6, step: 1, def: 3 },
    { id: 'behind', label: 'Voitures derrière', type: 'number', min: 0, max: 6, step: 1, def: 3 },
    { id: 'place', label: 'Place affichée', type: 'select', choices: [['class', 'En classe'], ['overall', 'Au général']], def: 'class' },
  ],
  mount(root, o) {
    const rows = o.ahead + o.behind + 1;
    const card = PW.tpl(`<div class="pw-card"><div class="pw-title"><span>Relatif</span><em>sur la piste</em></div><table class="pw-board"><tbody></tbody></table></div>`).firstChild;
    root.appendChild(card);
    const body = card.querySelector('tbody'), trs = [];
    for (let i = 0; i < rows; i++) {
      const tr = document.createElement('tr');
      tr.innerHTML = '<td class="c"></td><td class="p"></td><td class="n"></td><td class="g"></td>';
      body.appendChild(tr); trs.push({ tr, c: tr.children[0], p: tr.children[1], n: tr.children[2], g: tr.children[3], key: '' });
    }
    return {
      update(g) {
        const cars = nearCars(g), me = cars.find(c => c.me);
        const list = new Array(rows).fill(null);
        if (me) {
          // Les voitures aux stands ne comptent pas, sauf si on y est aussi.
          const onTrack = c => !c.me && (!c.pit || me.pit);
          const front = cars.filter(c => onTrack(c) && c.secs > 0).sort((a, b) => a.secs - b.secs).slice(0, o.ahead).reverse();
          const back = cars.filter(c => onTrack(c) && c.secs <= 0).sort((a, b) => b.secs - a.secs).slice(0, o.behind);
          // Le joueur reste à sa ligne, même s'il manque des voitures devant.
          [...front, me, ...back].forEach((c, i) => { list[o.ahead - front.length + i] = c; });
        }
        trs.forEach((r, i) => {
          const c = list[i];
          const gap = c && !c.me ? F.signed(-c.secs, 1) : '';
          const key = c ? [c.place, c.rank, c.name, c.cls, c.me, c.pit, c.laps, gap].join('|') : '';
          if (r.key === key) return;
          r.key = key;
          r.tr.style.visibility = c ? '' : 'hidden';
          if (!c) return;
          // Rouge : il nous a pris un tour. Bleu : on lui a pris un tour. Gris : aux stands.
          r.tr.className = (c.me ? 'me ' : '') + (c.pit ? 'pit' : c.laps > 0 ? 'up' : c.laps < 0 ? 'down' : '');
          r.c.style.background = classColor(c.cls);
          r.p.textContent = o.place === 'overall' ? c.place : c.rank;
          r.n.textContent = shortName(c.name);
          r.g.textContent = gap;
        });
      },
    };
  },
};

PW.widgets.pit = {
  name: 'Arrêt au stand',
  keys: ['grid.', 'session.vehicles', 'session.type', 'session.track_length'],
  hz: 1, size: [300, 120],
  options: [{ id: 'loss', label: 'Temps perdu par un arrêt (s)', type: 'number', min: 20, max: 150, step: 1, def: 60 }],
  mount(root, o) {
    const card = PW.tpl(`<div class="pw-card"><div class="pw-title"><span>Arrêt au stand</span><em>${Math.round(o.loss)} s perdues</em></div>
      <div class="pw-row"><span class="pw-big" data-k="place">—</span><span class="pw-sub" data-k="cls"></span></div>
      <div class="pw-goal" data-k="lost" data-base="pw-goal"></div>
      <div class="pw-kv"><span>Devant</span><i data-k="fn"></i><b data-k="fg" data-base="">—</b></div>
      <div class="pw-kv"><span>Derrière</span><i data-k="bn"></i><b data-k="bg" data-base="">—</b></div></div>`).firstChild;
    root.appendChild(card);
    const b = PW.bind(card);
    return {
      update(g) {
        const n = g('session.vehicles', 0), type = g('session.type', -1), cars = [], length = Math.max(1, g('session.track_length', 1));
        for (let i = 0; i < n; i++) {
          const p = gridPad(i), place = g(p + '.place');
          if (place != null) cars.push({ place, name: g(p + '.driver', ''), cls: g(p + '.class', ''), next: Math.max(0, g(p + '.behind_next', 0)), me: g(p + '.is_player', false),
            dist: g(p + '.laps', 0) * length + g(p + '.lap_dist', 0), into: g(p + '.time_into', 0), est: g(p + '.est_lap', 0) });
        }
        cars.sort((a, c) => a.place - c.place);
        // Retard de chaque voiture sur le leader : somme des écarts entre voitures qui se suivent (temps dans le
        // tour, tours de retard compris ; sinon l'écart donné par le jeu).
        let sum = 0;
        cars.forEach((c, i) => {
          if (i) {
            const t = PW.interval(cars[i - 1], c, length);
            sum += t == null ? c.next : t + Math.max(0, Math.floor((cars[i - 1].dist - c.dist) / length)) * c.est;
          }
          c.behind = sum;
        });
        const me = cars.find(c => c.me);
        if (!me || !(type >= 10 && type <= 13)) {
          b.text('place', '—'); b.text('cls', ''); b.text('lost', 'Estimation en course uniquement'); b.cls('lost', '');
          for (const k of ['fn', 'bn']) b.text(k, '');
          for (const k of ['fg', 'bg']) b.text(k, '—');
          return;
        }
        // Après l'arrêt : notre retard augmente du temps perdu ; on se replace parmi les autres.
        const mine = me.behind + o.loss, others = cars.filter(c => !c.me);
        const place = 1 + others.filter(c => c.behind < mine).length;
        const front = others.filter(c => c.behind < mine).sort((a, c) => c.behind - a.behind)[0];
        const back = others.filter(c => c.behind >= mine).sort((a, c) => a.behind - c.behind)[0];
        const lost = place - me.place;
        b.text('place', 'P' + place);
        b.text('cls', 'P' + (1 + others.filter(c => c.behind < mine && c.cls === me.cls).length) + ' en classe');
        b.text('lost', lost > 0 ? lost + ' place(s) perdue(s)' : 'aucune place perdue');
        b.cls('lost', lost > 0 ? 'pw-warn' : 'pw-ok');
        const line = (nk, gk, c) => {
          b.text(nk, c ? shortName(c.name) : '');
          const gap = c ? Math.abs(c.behind - mine) : null;
          b.text(gk, c ? gap.toFixed(1) + ' s' : 'personne');
          // Moins de 3 s d'écart à la sortie : on ressort dans le trafic.
          b.cls(gk, c && gap < 3 ? 'pw-warn' : '');
        };
        line('fn', 'fg', front); line('bn', 'bg', back);
      },
    };
  },
};

PW.widgets.radar = {
  name: 'Radar de proximité',
  keys: ['grid.', 'session.vehicles'],
  hz: o => +o.fps || 60, size: [200, 200],
  options: [
    { id: 'range', label: 'Rayon affiché (m)', type: 'number', min: 8, max: 40, step: 1, def: 20 },
    { id: 'autohide', label: 'Masquer quand personne n’est proche', type: 'bool', def: true },
    { id: 'show', label: 'Apparaît quand une voiture est à moins de (m)', type: 'number', min: 10, max: 80, step: 5, def: 35 },
    { id: 'fps', label: 'Fluidité (images/s)', type: 'select', choices: [['15', '15'], ['30', '30'], ['60', '60 (le plus fluide)']], def: '60' },
  ],
  mount(root, o) {
    const S = 200, R = S / 2 - 6, range = Math.max(8, Math.min(40, +o.range || 20)), show = Math.max(range, +o.show || 35);
    const scale = R / range, carW = 2 * scale, carL = 4.7 * scale;
    // Un disque sombre, des cercles tous les 10 m et un axe, pour juger les distances d'un coup d'œil.
    let rings = '';
    for (let m = 10; m < range - 0.5; m += 10) rings += `<u style="width:${2 * m * scale}px;height:${2 * m * scale}px"></u>`;
    const card = PW.tpl(`<div class="pw-radar" style="width:${S}px;height:${S}px"><div class="disc" style="width:${2 * R}px;height:${2 * R}px">${rings}<i class="h"></i><i class="v"></i></div><b class="l"></b><b class="r"></b><div class="cars"></div><s class="self"></s></div>`).firstChild;
    root.appendChild(card);
    const left = card.querySelector('b.l'), right = card.querySelector('b.r'), box = card.querySelector('.cars'), self = card.querySelector('.self');
    const place = (el, rgt, fwd, turn) => { el.style.transform = `translate(${S / 2 + rgt * scale - carW / 2}px, ${S / 2 - fwd * scale - carL / 2}px) rotate(${(-turn * 180 / Math.PI).toFixed(1)}deg)`; };
    for (const el of [self]) { el.style.width = carW + 'px'; el.style.height = carL + 'px'; el.style.borderRadius = carW / 2 + 'px'; }
    place(self, 0, 0, 0);
    for (const [el, sign] of [[left, -1], [right, 1]]) {
      el.style.cssText = `left:${S / 2 + sign * (carW / 2 + 0.9 * scale) - 2}px;top:${S / 2 - carL * 0.75}px;height:${carL * 1.5}px`;
    }
    let last = null, dir = null;
    const seen = new Map(); // dernière mesure de chaque voiture, pour prolonger son mouvement
    return {
      update(g) {
        // Comme TinyPedal : position et cap lus dans la télémétrie de chaque voiture (repère x, −z) ; chaque voiture
        // est avancée jusqu'à l'instant de celle du joueur, à sa vitesse entre ses deux dernières mesures.
        const n = g('session.vehicles', 0), others = [], clock = performance.now() / 1000;
        let me = null, yaw = null, myTime = null;
        for (let i = 0; i < n; i++) {
          const p = gridPad(i), x = g(p + '.x'), z = g(p + '.z');
          if (x == null || z == null) continue;
          const t = g(p + '.t');
          if (g(p + '.is_player')) { me = [x, -z]; yaw = g(p + '.yaw', null); myTime = t; }
          else if (!g(p + '.in_pits', false)) others.push([i, x, -z, t == null ? clock : t, g(p + '.yaw', null)]);
        }
        const now = myTime == null ? clock : myTime;
        const moved = others.map(([i, x, y, t, theirYaw]) => {
          let s = seen.get(i);
          if (!s) seen.set(i, s = { x, y, t, vx: 0, vy: 0 });
          if (x !== s.x || y !== s.y) {
            const dt = t - s.t, dx = x - s.x, dy = y - s.y;
            const ok = dt > 0.005 && dt < 1 && (Math.abs(dx) + Math.abs(dy)) / dt < 120;
            s.vx = ok ? dx / dt : 0; s.vy = ok ? dy / dt : 0; s.x = x; s.y = y; s.t = t;
          }
          const lead = Math.max(0, Math.min(0.3, now - s.t));
          return [s.x + s.vx * lead, s.y + s.vy * lead, theirYaw];
        });
        // Sans cap donné par le jeu (démonstration) : la direction du dernier déplacement, gardée à l'arrêt.
        if (me && yaw == null) {
          if (last) {
            const dx = me[0] - last[0], dy = me[1] - last[1], len = Math.hypot(dx, dy);
            if (len > 0.3 && len < 60) {
              const nx = dx / len, ny = dy / len;
              if (dir) { const bx = dir[0] * 0.6 + nx * 0.4, by = dir[1] * 0.6 + ny * 0.4, l = Math.hypot(bx, by) || 1; dir = [bx / l, by / l]; } else dir = [nx, ny];
              last = me;
            } else if (len >= 60) last = me;
          } else last = me;
        }
        // Voitures voisines dans le repère du joueur : (à droite, devant) en mètres, et leur angle par rapport à nous.
        const near = [];
        if (me && (yaw != null || dir)) for (const [x, y, theirYaw] of moved) {
          const rx = x - me[0], ry = y - me[1];
          let rgt, fwd;
          if (yaw != null) {
            // Rotation de TinyPedal : angle = cap du joueur − π ; l'axe vertical pointe vers l'arrière.
            const a = yaw - Math.PI, s = Math.sin(a), c = Math.cos(a);
            rgt = c * rx - s * ry; fwd = -(c * ry + s * rx);
          } else { fwd = rx * dir[0] + ry * dir[1]; rgt = rx * -dir[1] + ry * dir[0]; }
          if (Math.hypot(rgt, fwd) <= show) near.push([rgt, fwd, yaw != null && theirYaw != null ? theirYaw - yaw : 0]);
        }
        card.style.visibility = !near.length && o.autohide ? 'hidden' : '';
        // Danger : à notre hauteur (rouge), toute proche (orange), sinon neutre.
        const beside = c => Math.abs(c[0]) > 0.8 && Math.abs(c[0]) < 4.6 && Math.abs(c[1]) < 4.9;
        const close = c => Math.abs(c[0]) < 4.6 && Math.abs(c[1]) < 10;
        for (const [el, sign] of [[left, -1], [right, 1]]) {
          const side = near.filter(c => c[0] * sign > 0.8 && Math.abs(c[0]) < 4.6);
          el.style.background = side.some(beside) ? 'var(--pw-bad)' : side.some(close) ? 'var(--pw-warn)' : 'transparent';
        }
        // Les voitures hors du cercle ont seulement fait apparaître le radar.
        const drawn = near.filter(c => Math.hypot(c[0], c[1]) <= range - 1);
        while (box.children.length < drawn.length) {
          const el = document.createElement('s');
          el.style.width = carW + 'px'; el.style.height = carL + 'px'; el.style.borderRadius = carW / 2 + 'px';
          box.appendChild(el);
        }
        [...box.children].forEach((el, i) => {
          const c = drawn[i];
          el.hidden = !c;
          if (!c) return;
          place(el, c[0], c[1], c[2]);
          el.style.background = beside(c) ? 'var(--pw-bad)' : close(c) ? 'var(--pw-warn)' : '#c4ccd8';
        });
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
