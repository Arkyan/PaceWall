# PaceWall

Overlays de course pour **Le Mans Ultimate** : télémétrie en direct, stratégie (carburant, énergie virtuelle, pneus, météo) et historique des sessions.

## Objectifs

- Des overlays visibles par le pilote, par-dessus le jeu.
- Les mêmes overlays dans OBS, via une source navigateur, sans capturer le jeu.
- Une consommation minimale en course, seul ou en stream.

## Architecture

```
LMU ─► collector (Rust) ─► serveur local (HTTP + WebSocket)
        mémoire partagée    ├─► fenêtre overlay Tauri (transparente, clics traversants)
        + API REST du jeu   └─► http://localhost:PORT/overlay/... (source navigateur OBS)
```

Les widgets sont écrits une seule fois en web (Svelte) et chargés par les deux sorties.

## Sources de données

Voir [docs/donnees.md](docs/donnees.md).

## Étapes

1. [x] Squelette et détection de la mémoire partagée `LMU_Data`
2. [x] Structures `#[repr(C, packed(4))]` (tailles et décalages vérifiés contre la référence) et lecture des valeurs brutes
3. [x] Serveur WebSocket (différences seulement, abonnement par préfixe) et explorateur de données
4. [x] Overlays web (6 widgets), éditeur, mises en page joueur / OBS
   - [ ] Fenêtre Tauri transparente aux clics pour l'overlay joueur
5. [~] Moteur de stratégie (conso, tours restants, à ajouter : fait) ; alertes vocales à faire
6. [x] Historique SQLite (enregistrement automatique des tours) et page de consultation `/history`

## Développer sans le jeu

```bash
cargo run -p fakegame     # faux LMU : crée LMU_Data avec une course simulée
cargo run -p collector    # lit la zone et affiche vitesse, carburant, énergie, pneus (console)
cargo run -p server       # serveur local : explorateur http://127.0.0.1:8470, historique /history
FAKE_LAP_SECONDS=6 cargo run -p fakegame   # tours courts pour tester l'historique
cargo test -p lmu         # vérifie que les structures ont la même disposition que le jeu
```

Les valeurs du faux jeu sont inventées : elles servent à tester le pipeline, pas la stratégie.

## Protocole WebSocket (`ws://127.0.0.1:8470/ws`)

Le client envoie son abonnement ; le serveur ne lit le jeu que si au moins un client écoute.

```json
{ "sub": ["fuel.", "tyre.fl."], "hz": 10 }
```

Réponses (seules les valeurs modifiées sont envoyées, `m` donne unité et libellé des nouvelles clés,
`x` liste les clés disparues) :

```json
{ "t": "u", "game": true, "d": { "fuel.level": 89.58 }, "m": { "fuel.level": ["L", "Carburant"] } }
```

Clés : `session.*`, `weather.*`, `player.*`, `fuel.*`, `energy.*`, `lap.*`, `gap.*`, `aids.*`, `car.*`,
`pit.*`, `position.*`, `tyre.{fl,fr,rl,rr}.*`, `brake.{fl,fr,rl,rr}.*`, `grid.NNN.*` (classement complet,
lu uniquement si demandé). Unités déjà converties : km/h, °C, %, secondes.

## Historique

- Enregistré en continu par un thread séparé (2 lectures/s, une écriture SQLite par tour), même sans overlay.
- Base : `%APPDATA%\PaceWall\history.db` (variable `PACEWALL_DB` pour la changer).
- Par tour : temps, validité, passage aux stands, carburant / énergie / usure consommés (hors tours avec arrêt),
  températures moyennes pneus et freins, météo, position, gomme.
- Le tour en cours au moment où PaceWall se branche est ignoré (il serait incomplet).
- API : `GET /api/sessions`, `GET /api/sessions/{id}/laps`, `DELETE /api/sessions/{id}`.

## Overlays

| Page | Rôle |
|---|---|
| `/editor` | Éditeur : ajouter, glisser, régler les widgets ; mises en page « Joueur » et « OBS » ; démo ou jeu en direct |
| `/overlay/player` | Overlay joueur (fond transparent) |
| `/overlay/obs` | Overlay pour une source navigateur OBS (1920×1080, fond transparent) |
| `/overlay/obs?only=fuel,tyres` | Seulement certains widgets (une source OBS par widget) |
| `/overlay/player?demo=1` | Données factices, pour voir le rendu sans jeu |

- **Widgets** : carburant et énergie, pneus, chrono et écarts, pilotage, classement, session et météo.
- Les overlays disparaissent quand le jeu n'est pas là, et se rechargent seules quand la mise en page est enregistrée.
- Positions en pourcentage de l'écran, tailles relatives à une hauteur de 1080 px : la mise en page suit la résolution.
- Mises en page enregistrées dans `%APPDATA%\PaceWall\layouts.json`.
- **Stratégie** (calculée côté serveur, `strategy.*`) : consommation moyenne des 3 derniers tours propres, tours restants,
  quantité à ajouter pour finir. Un tour avec arrêt ou incomplet n'est pas compté.
- Ajouter un widget : une entrée dans `crates/server/overlay/widgets.js` (nom, clés suivies, options, `mount`).
