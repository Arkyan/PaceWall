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
4. [ ] Overlay minimal (carburant, pneus) dans OBS et en fenêtre Tauri
5. [ ] Moteur de stratégie et alertes
6. [ ] Historique SQLite

## Développer sans le jeu

```bash
cargo run -p fakegame     # faux LMU : crée LMU_Data avec une course simulée
cargo run -p collector    # lit la zone et affiche vitesse, carburant, énergie, pneus (console)
cargo run -p server       # serveur local + explorateur de données : http://127.0.0.1:8470
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
