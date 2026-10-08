# Données exploitables dans LMU

Référence : TinyPedal (GPLv3, lecture seule pour savoir ce qui existe, aucun code copié) et
`pyLMUSharedMemory` (MIT) qui reprend les en-têtes C++ officiels (`SharedMemoryInterface.hpp`,
`InternalsPlugin.hpp`).

## Mémoire partagée native

- Nom : `LMU_Data`. À activer en jeu : Réglages > Gameplay > `Enable Plugins`.
- 104 véhicules maximum. Structures en `pack=4`.
- `telemetry.telemInfo[i]` : carburant, régime, rapport, pédales, températures, énergie électrique, delta best, compound.
- `scoring.scoringInfo` / `vehScoringInfo[i]` : session, météo, drapeaux, classement, écarts.
- `generic.events` : drapeaux de mise à jour (`SME_UPDATE_SCORING`, `SME_UPDATE_TELEMETRY`).

## API REST locale du jeu (plus lente)

- Prévisions météo par session, usure freins, dégâts, énergie virtuelle max, plein absolu,
  estimation de l'arrêt au stand, réglages de la voiture.

## À confirmer

- Température et usure des pneus (champs `LMUWheel`).
- Énergie virtuelle courante côté mémoire partagée.
