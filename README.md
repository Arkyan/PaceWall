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
2. [ ] Structures `#[repr(C, packed(4))]` et lecture des valeurs brutes
3. [ ] Serveur WebSocket, envoi des différences
4. [ ] Overlay minimal (carburant, pneus) dans OBS et en fenêtre Tauri
5. [ ] Moteur de stratégie et alertes
6. [ ] Historique SQLite
