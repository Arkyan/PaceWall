# Mémoire partagée de Le Mans Ultimate : référence complète

Tout ce que le jeu publie dans sa zone mémoire `LMU_Data` : les {FIELDS} champs, avec leur emplacement à l'octet près,
leur type, leur unité et ce qu'ils veulent dire. Ce document se suffit à lui-même : on peut écrire un lecteur dans
n'importe quel langage avec.

> Fichier **généré** par `python tools/doc/memoire_partagee.py` à partir de
> [`crates/lmu/src/data.rs`](../crates/lmu/src/data.rs). Ne pas le modifier à la main : corriger le script
> (descriptions) ou `data.rs` (disposition), puis régénérer. Un champ sans description fait échouer la génération.

Voir aussi [donnees.md](donnees.md) : le service web du jeu (ce qui n'est pas en mémoire) et l'usage qu'en fait PaceWall.

## D'où viennent ces informations

| Source | Ce qu'elle apporte | Fiabilité |
|---|---|---|
| En-têtes C++ fournis avec le jeu (`Support\SharedMemoryInterface` : `SharedMemoryInterface.hpp`, `InternalsPlugin.hpp`), lus à travers le portage Python [pyLMUSharedMemory](https://github.com/TinyPedal/pyLMUSharedMemory) (MIT) | Disposition, types, commentaires de chaque champ | Référence |
| Tests de `crates/lmu` (`cargo test -p lmu`) | 12 tailles et 10 décalages, comparés à la bibliothèque de référence | Contrôlé automatiquement |
| Relevés sur le vrai jeu avec `pacewall.exe --probe` (version 14200) | Cadences réelles, comportement des événements, drapeaux de secteur | Mesuré |
| TinyPedal (GPLv3), comme exemple d'utilisation | Sens pratique de certains champs (usure, dégâts, cap) | Indirect |

Les unités marquées dans les tableaux viennent des commentaires de l'en-tête. Quand l'en-tête ne dit rien, l'unité
donnée est celle de rFactor 2 (dont LMU reprend le moteur) : c'est le cas des efforts aérodynamiques, des hauteurs
et du couple de direction. La liste de ce qui a été réellement vérifié en jeu est à la fin.

## 1. Ouvrir la zone

- **Nom** : `LMU_Data` (objet « file mapping » nommé de Windows, dans l'espace de noms de la session).
- **Taille** : {SIZE} octets.
- **Accès** : lecture seule. `OpenFileMappingW(FILE_MAP_READ, FALSE, L"LMU_Data")` puis `MapViewOfFile`.
- **Quand existe-t-elle** : quand le jeu tourne avec l'option **Enable Plugins** activée (Réglages > Gameplay).
  Tant que l'ouverture échoue, le jeu n'est pas lancé : réessayer toutes les quelques secondes.
- **Aucune écriture** : on ne peut rien envoyer au jeu par cette zone.

## 2. Règles de lecture

- **Alignement sur 4 octets** (`#pragma pack(4)`). Un `double` peut donc commencer à un décalage qui n'est pas
  multiple de 8. En C : `#pragma pack(push, 4)`. En Rust : `#[repr(C, packed(4))]`, et lire les champs par copie.
  En Python : `_pack_ = 4`. En C# : `[StructLayout(LayoutKind.Sequential, Pack = 4)]`.
- **Petit-boutiste** (x86-64), entiers et flottants au format habituel.
- **Textes** : tableaux d'octets terminés par un zéro, de taille fixe. Couper au premier zéro. Les noms de pilotes
  avec accents ne sont pas toujours de l'UTF-8 valide : décoder sans faire échouer la lecture.
- **Booléens** : un octet, 0 ou 1. Les lire comme un entier, jamais comme un `bool` natif.
- **Pointeurs** : deux champs de 8 octets (`mResultsStreamPointer`, `mVehiclePointer`) sont des adresses internes au
  jeu. Les ignorer.
- **Champs « réservés »** (`mExpansion`, `mUnused`) : place gardée pour de futures versions. Ne rien y lire.

## 3. Plan de la zone

| Décalage | Octets | Bloc | Contenu |
|---:|---:|---|---|
| 0 | 332 | `generic` | Événements, version du jeu, retour de force, fenêtre du jeu |
| {PATHS} | 1 300 | `paths` | Cinq dossiers du jeu |
| {SCORING} | 126 832 | `scoring` | Session et météo (548 octets), puis 104 voitures de {VEH_SIZE} octets, puis 64 Ko de texte |
| {TELEMETRY} | 196 356 | `telemetry` | 3 octets d'en-tête, puis 104 voitures de {TEL_SIZE} octets |

Adresses utiles, depuis le début de la zone :

| Quoi | Adresse |
|---|---|
| Session (`ScoringInfoV01`) | {SCORING} |
| Nombre de voitures au classement (`mNumVehicles`) | {NUMVEH} |
| Voiture `i` du classement | {VEH} + `i` × {VEH_SIZE} |
| Flux de résultats (texte) | {STREAM} |
| Nombre de voitures en télémétrie (`activeVehicles`) | {ACTIVE} |
| Voiture `j` de la télémétrie | {TEL} + `j` × {TEL_SIZE} |
| Roue `r` de la voiture `j` | {TEL} + `j` × {TEL_SIZE} + {WHEELS} + `r` × {WHEEL_SIZE} |

Exemples : le carburant de la voiture 0 de la télémétrie est à {TEL} + {FUEL} = **{EX_FUEL}** ; la position de la
quatrième ligne du classement (`i` = 3) est à {VEH} + 3 × {VEH_SIZE} + {PLACE} = **{EX_PLACE}**.

## 4. Deux tableaux de voitures, deux ordres différents

C'est le piège principal. Chaque voiture apparaît deux fois :

- dans `scoring.vehScoringInfo` : classement, tours, secteurs, écarts, drapeaux, stands ;
- dans `telemetry.telemInfo` : moteur, pédales, pneus, carburant, réglages, position précise.

**L'index d'une voiture n'est pas le même dans les deux tableaux**, et aucun des deux n'est trié par position.
Le lien se fait par `mID`, présent au début des deux structures.

Trouver le joueur :

1. lire `mNumVehicles` ; parcourir les lignes du classement jusqu'à celle dont `mIsPlayer` vaut 1 (octet {ISPLAYER}
   de la ligne) ;
2. lire son `mID` ;
3. lire `activeVehicles` ; parcourir la télémétrie jusqu'à l'entrée qui a ce `mID`.

`playerVehicleIdx` donne en principe l'index de télémétrie directement ; la recherche par `mID` est celle
qu'utilise TinyPedal et celle que PaceWall a retenue. Un `mID` peut être réattribué en multijoueur après un départ :
refaire la recherche quand le classement change.

## 5. Cadence et cohérence

Relevé sur le vrai jeu (20 s, lecture toutes les 50 ms) :

| Donnée | Cadence constatée | Témoin à surveiller |
|---|---|---|
| Classement et session (`scoring`) | environ 5 fois par seconde | `mCurrentET` (adresse {CURRENT_ET}) |
| Télémétrie (`telemetry`) | environ 20 fois par seconde | `mElapsedTime` de la voiture (octet {ELAPSED} de son entrée) |
| Indicateurs d'événements (`generic.events`) | **aucun changement** en 20 s | à ne pas utiliser comme témoin |

- **Savoir s'il y a du nouveau** : comparer `mCurrentET` et `mElapsedTime` à la lecture précédente. S'ils ne
  bougent plus depuis quelques secondes, le jeu est en pause, dans les menus ou fermé.
- **Les événements ne sont pas des compteurs.** Chaque case de `generic.events` est un indicateur que le jeu
  positionne ; en pratique ils restent figés pendant qu'on roule. TinyPedal ne les teste que « différent de zéro ».
  Ne pas compter dessus pour détecter une mise à jour ni un changement de session.
- **Pas de verrou côté lecteur dans cette zone.** Le jeu écrit pendant qu'on lit : une lecture peut mélanger deux
  instants. Deux parades simples : ne lire que ce dont on a besoin (quelques centaines d'octets plutôt que les
  {SIZE}), et, comme TinyPedal, n'accepter une copie complète que si `mNumVehicles` égale `activeVehicles`.
- **Extrapoler entre deux publications** : à 20 Hz, une voiture à 300 km/h avance de 4 m entre deux positions.
  Pour un affichage fluide (radar, carte), avancer la position de `vitesse × temps écoulé depuis mElapsedTime`.
- **Changement de session** : le détecter par les données, pas par les événements : `mTrackName` ou `mSession`
  qui change, ou `mCurrentET` qui repart en arrière.

## 6. Les structures, champ par champ

Dans les tableaux : le **décalage** est compté depuis le début de la structure ; **LMU** signale un champ ajouté par
Le Mans Ultimate (absent de rFactor 2) ; les unités en gras sont celles qui piègent.
