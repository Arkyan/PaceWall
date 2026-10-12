# Mémoire partagée de Le Mans Ultimate : référence complète

Tout ce que le jeu publie dans sa zone mémoire `LMU_Data` : les 285 champs, avec leur emplacement à l'octet près,
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
- **Taille** : 324 820 octets.
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
| 332 | 1 300 | `paths` | Cinq dossiers du jeu |
| 1 632 | 126 832 | `scoring` | Session et météo (548 octets), puis 104 voitures de 584 octets, puis 64 Ko de texte |
| 128 464 | 196 356 | `telemetry` | 3 octets d'en-tête, puis 104 voitures de 1 888 octets |

Adresses utiles, depuis le début de la zone :

| Quoi | Adresse |
|---|---|
| Session (`ScoringInfoV01`) | 1 632 |
| Nombre de voitures au classement (`mNumVehicles`) | 1 736 |
| Voiture `i` du classement | 2 192 + `i` × 584 |
| Flux de résultats (texte) | 62 928 |
| Nombre de voitures en télémétrie (`activeVehicles`) | 128 464 |
| Voiture `j` de la télémétrie | 128 468 + `j` × 1 888 |
| Roue `r` de la voiture `j` | 128 468 + `j` × 1 888 + 848 + `r` × 260 |

Exemples : le carburant de la voiture 0 de la télémétrie est à 128 468 + 524 = **128 992** ; la position de la
quatrième ligne du classement (`i` = 3) est à 2 192 + 3 × 584 + 199 = **4 143**.

## 4. Deux tableaux de voitures, deux ordres différents

C'est le piège principal. Chaque voiture apparaît deux fois :

- dans `scoring.vehScoringInfo` : classement, tours, secteurs, écarts, drapeaux, stands ;
- dans `telemetry.telemInfo` : moteur, pédales, pneus, carburant, réglages, position précise.

**L'index d'une voiture n'est pas le même dans les deux tableaux**, et aucun des deux n'est trié par position.
Le lien se fait par `mID`, présent au début des deux structures.

Trouver le joueur :

1. lire `mNumVehicles` ; parcourir les lignes du classement jusqu'à celle dont `mIsPlayer` vaut 1 (octet 196
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
| Classement et session (`scoring`) | environ 5 fois par seconde | `mCurrentET` (adresse 1 700) |
| Télémétrie (`telemetry`) | environ 20 fois par seconde | `mElapsedTime` de la voiture (octet 12 de son entrée) |
| Indicateurs d'événements (`generic.events`) | **aucun changement** en 20 s | à ne pas utiliser comme témoin |

- **Savoir s'il y a du nouveau** : comparer `mCurrentET` et `mElapsedTime` à la lecture précédente. S'ils ne
  bougent plus depuis quelques secondes, le jeu est en pause, dans les menus ou fermé.
- **Les événements ne sont pas des compteurs.** Chaque case de `generic.events` est un indicateur que le jeu
  positionne ; en pratique ils restent figés pendant qu'on roule. TinyPedal ne les teste que « différent de zéro ».
  Ne pas compter dessus pour détecter une mise à jour ni un changement de session.
- **Pas de verrou côté lecteur dans cette zone.** Le jeu écrit pendant qu'on lit : une lecture peut mélanger deux
  instants. Deux parades simples : ne lire que ce dont on a besoin (quelques centaines d'octets plutôt que les
  324 820), et, comme TinyPedal, n'accepter une copie complète que si `mNumVehicles` égale `activeVehicles`.
- **Extrapoler entre deux publications** : à 20 Hz, une voiture à 300 km/h avance de 4 m entre deux positions.
  Pour un affichage fluide (radar, carte), avancer la position de `vitesse × temps écoulé depuis mElapsedTime`.
- **Changement de session** : le détecter par les données, pas par les événements : `mTrackName` ou `mSession`
  qui change, ou `mCurrentET` qui repart en arrière.

## 6. Les structures, champ par champ

Dans les tableaux : le **décalage** est compté depuis le début de la structure ; **LMU** signale un champ ajouté par
Le Mans Ultimate (absent de rFactor 2) ; les unités en gras sont celles qui piègent.

### La zone entière — `SharedMemoryObjectOut` (324 820 octets)

Les quatre blocs, dans l'ordre. Décalages depuis le début de la zone.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `generic` | `SharedMemoryGeneric` | 332 |  | Événements, version du jeu, retour de force, fenêtre. |
| 332 | `paths` | `SharedMemoryPathData` | 1300 |  | Dossiers du jeu. |
| 1632 | `scoring` | `SharedMemoryScoringData` | 126832 |  | Session, météo et classement de toutes les voitures. |
| 128464 | `telemetry` | `SharedMemoryTelemtryData` | 196356 |  | Télémétrie détaillée de toutes les voitures. |

### Bloc `generic` — `SharedMemoryGeneric` (332 octets)

Décalage 0.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `events` | `SharedMemoryEvent` | 64 |  | Un indicateur par type d'événement. Voir la mise en garde sur leur comportement réel. |
| 64 | `gameVersion` | `int` | 4 | entier | Version du jeu (relevé réel : 14200). |
| 68 | `FFBTorque` | `float` | 4 |  | Couple du retour de force envoyé au volant. |
| 72 | `appInfo` | `ApplicationStateV01` | 260 |  | Fenêtre du jeu et écran en cours. |

### Événements — `SharedMemoryEvent` (64 octets)

Seize entiers de 4 octets, dans `generic.events`.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `SME_ENTER` | `unsigned int` | 4 | indicateur | Entrée dans le jeu. |
| 4 | `SME_EXIT` | `unsigned int` | 4 | indicateur | Sortie du jeu. |
| 8 | `SME_STARTUP` | `unsigned int` | 4 | indicateur | Démarrage du moteur de jeu. |
| 12 | `SME_SHUTDOWN` | `unsigned int` | 4 | indicateur | Arrêt du moteur de jeu. |
| 16 | `SME_LOAD` | `unsigned int` | 4 | indicateur | Chargement d'un circuit. |
| 20 | `SME_UNLOAD` | `unsigned int` | 4 | indicateur | Déchargement du circuit. |
| 24 | `SME_START_SESSION` | `unsigned int` | 4 | indicateur | Début d'une session. |
| 28 | `SME_END_SESSION` | `unsigned int` | 4 | indicateur | Fin d'une session. |
| 32 | `SME_ENTER_REALTIME` | `unsigned int` | 4 | indicateur | Le joueur prend le volant. |
| 36 | `SME_EXIT_REALTIME` | `unsigned int` | 4 | indicateur | Le joueur revient au garage. |
| 40 | `SME_UPDATE_SCORING` | `unsigned int` | 4 | indicateur | Le classement vient d'être publié. |
| 44 | `SME_UPDATE_TELEMETRY` | `unsigned int` | 4 | indicateur | La télémétrie vient d'être publiée. |
| 48 | `SME_INIT_APPLICATION` | `unsigned int` | 4 | indicateur | Initialisation de l'application. |
| 52 | `SME_UNINIT_APPLICATION` | `unsigned int` | 4 | indicateur | Fermeture de l'application. |
| 56 | `SME_SET_ENVIRONMENT` | `unsigned int` | 4 | indicateur | Les chemins (`paths`) ont été renseignés. |
| 60 | `SME_FFB` | `unsigned int` | 4 | indicateur | Le couple du retour de force a été mis à jour. |

### Fenêtre du jeu — `ApplicationStateV01` (260 octets)

Dans `generic.appInfo`.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `mAppWindow` | `unsigned long long` | 8 | HWND | Poignée de la fenêtre du jeu. |
| 8 | `mWidth` | `unsigned int` | 4 | pixels | Largeur de l'image. |
| 12 | `mHeight` | `unsigned int` | 4 | pixels | Hauteur de l'image. |
| 16 | `mRefreshRate` | `unsigned int` | 4 | Hz | Fréquence d'affichage. |
| 20 | `mWindowed` | `unsigned int` | 4 | 0 / 1 | Mode fenêtré. |
| 24 | `mOptionsLocation` | `unsigned char` | 1 | code | Où est le joueur : 0 menus, 1 chargement du circuit, 2 moniteur du garage, 3 en piste. |
| 25 | `mOptionsPage` | `char[31]` | 31 | texte | Nom de la page d'options ouverte. |
| 56 | `mExpansion` | `octets[204]` | 204 |  | Réservé. |

### Bloc `paths` — `SharedMemoryPathData` (1 300 octets)

Cinq chemins de 260 caractères.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `userData` | `char[260]` | 260 | texte | Dossier des données utilisateur du jeu (`UserData`). |
| 260 | `customVariables` | `char[260]` | 260 | texte | Fichier des variables personnalisées des plugins. |
| 520 | `stewardResults` | `char[260]` | 260 | texte | Dossier des résultats de session (fichiers XML). |
| 780 | `playerProfile` | `char[260]` | 260 | texte | Dossier du profil du joueur. |
| 1040 | `pluginsFolder` | `char[260]` | 260 | texte | Dossier des plugins. |

### Bloc `scoring` — `SharedMemoryScoringData` (126 832 octets)

Décalages depuis le début du bloc.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `scoringInfo` | `ScoringInfoV01` | 548 |  | La session : circuit, temps, météo, drapeaux. |
| 548 | `scoringStreamSize` | `octets[12]` | 12 | octets | Taille utile de `scoringStream` (un `size_t`, puis du remplissage). |
| 560 | `vehScoringInfo` | `VehicleScoringInfoV01[104]` | 60736 |  | Une ligne par voiture. Seules les `mNumVehicles` premières sont valides. L'ordre n'est **pas** celui du classement : trier par `mPlace`. |
| 61296 | `scoringStream` | `char[65536]` | 65536 | texte | Flux de résultats du jeu : lignes de texte ajoutées depuis la dernière mise à jour, terminées par un zéro. |

### La session — `ScoringInfoV01` (548 octets)

Une seule, au début du bloc `scoring`.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `mTrackName` | `char[64]` | 64 | texte | Nom du circuit. |
| 64 | `mSession` | `int` | 4 | code | Type de session : 0 journée d'essais, 1 à 4 essais libres, 5 à 8 qualifications, 9 échauffement, 10 à 13 course. |
| 68 | `mCurrentET` | `double` | 8 | s | Temps de session actuel. Avance ≈ 5 fois par seconde ; ne bouge plus en pause. |
| 76 | `mEndET` | `double` | 8 | s | Temps de session auquel elle se termine. |
| 84 | `mMaxLaps` | `int` | 4 | tours | Nombre de tours de la session. Très grand nombre quand elle est limitée au temps. |
| 88 | `mLapDist` | `double` | 8 | m | Longueur du tour. |
| 96 | `mResultsStreamPointer` | `octets[8]` | 8 | pointeur | Pointeur interne au jeu, inutilisable de l'extérieur. Le texte est dans `scoringStream`. |
| 104 | `mNumVehicles` | `int` | 4 | entier | Nombre de voitures : les lignes 0 à `mNumVehicles − 1` de `vehScoringInfo` sont valides. |
| 108 | `mGamePhase` | `unsigned char` | 1 | code | 0 avant la session, 1 tours de reconnaissance, 2 grille, 3 tour de formation, 4 feux de départ, 5 drapeau vert, 6 neutralisation (FCY / voiture de sécurité), 7 session arrêtée, 8 session terminée, 9 pause. |
| 109 | `mYellowFlagState` | `signed char` | 1 | code | Neutralisation générale seulement : −1 invalide, 0 aucune, 1 annoncée, 2 stands fermés, 3 stands ouverts au tour du leader, 4 stands ouverts, 5 dernier tour, 6 reprise, 7 course arrêtée. |
| 110 | `mSectorFlag` | `octets[3]` | 3 | code | Drapeau jaune local par secteur. **Jaune seulement si la valeur est 1** : en jeu, la valeur n'est pas 0 au repos. Ordre des trois cases non garanti par l'en-tête. |
| 113 | `mStartLight` | `unsigned char` | 1 | entier | Étape des feux de départ (le nombre d'étapes dépend du circuit). |
| 114 | `mNumRedLights` | `unsigned char` | 1 | entier | Nombre de feux rouges de la procédure. |
| 115 | `mInRealtime` | `bool` | 1 | 0 / 1 | Le joueur est en piste (et pas au moniteur du garage). |
| 116 | `mPlayerName` | `char[32]` | 32 | texte | Nom du joueur. |
| 148 | `mPlrFileName` | `char[64]` | 64 | texte | Nom du fichier de profil (peut être transformé pour rester un nom de fichier valide). |
| 212 | `mDarkCloud` | `double` | 8 | 0 à 1 | Noirceur des nuages. |
| 220 | `mRaining` | `double` | 8 | 0 à 1 | Intensité de la pluie. |
| 228 | `mAmbientTemp` | `double` | 8 | °C | Température de l'air. |
| 236 | `mTrackTemp` | `double` | 8 | °C | Température de la piste. |
| 244 | `mWind` | `TelemVect3` | 24 | m/s | Vitesse du vent, dans le repère du monde. |
| 268 | `mMinPathWetness` | `double` | 8 | 0 à 1 | Humidité minimale sur la trajectoire. |
| 276 | `mMaxPathWetness` | `double` | 8 | 0 à 1 | Humidité maximale sur la trajectoire. |
| 284 | `mGameMode` | `unsigned char` | 1 | code | 1 serveur, 2 client, 3 les deux. |
| 285 | `mIsPasswordProtected` | `bool` | 1 | 0 / 1 | Serveur protégé par mot de passe. |
| 286 | `mServerPort` | `unsigned short` | 2 | entier | Port du serveur. |
| 288 | `mServerPublicIP` | `unsigned int` | 4 | entier | Adresse IP publique du serveur. |
| 292 | `mMaxPlayers` | `int` | 4 | entier | Nombre maximal de voitures dans la session. |
| 296 | `mServerName` | `char[32]` | 32 | texte | Nom du serveur. |
| 328 | `mStartET` | `float` | 4 | s | Heure de début de l'épreuve, en secondes depuis minuit. |
| 332 | `mAvgPathWetness` | `double` | 8 | 0 à 1 | Humidité moyenne sur la trajectoire. |
| 340 | `mSessionTimeRemaining` | `float` | 4 | s | **LMU.** Temps de session restant. |
| 344 | `mTimeOfDay` | `float` | 4 | s | **LMU.** Heure dans le jeu : 0 à 86 400 secondes depuis minuit. |
| 348 | `mIsFixedSetup` | `bool` | 1 | 0 / 1 | **LMU.** Réglages imposés. |
| 349 | `mTrackGripLevel` | `unsigned char` | 1 | code | **LMU.** Gomme déposée sur la piste : 0 verte, 1 faible, 2 moyenne, 3 forte, 4 saturée. La pluie la lave. |
| 350 | `mCloudCoverage` | `unsigned char` | 1 | code | **LMU.** Ciel : 0 dégagé, 1 quelques nuages, 2 partiellement nuageux, 3 très nuageux, 4 couvert, 5 bruine, 6 pluie légère (nuageux), 7 pluie légère (couvert), 8 pluie, 9 forte pluie, 10 orage. |
| 351 | `mTrackLimitsStepsPerPenalty` | `unsigned char` | 1 | pas | **LMU.** Nombre de pas de limites de piste qui déclenchent une pénalité. |
| 352 | `mTrackLimitsStepsPerPoint` | `unsigned char` | 1 | pas | **LMU.** Nombre de pas par point de limites de piste. |
| 353 | `mExpansion` | `octets[187]` | 187 |  | Réservé. |
| 540 | `mVehiclePointer` | `octets[8]` | 8 | pointeur | Pointeur interne au jeu, inutilisable de l'extérieur. Les voitures sont dans `vehScoringInfo`. |

### Une voiture au classement — `VehicleScoringInfoV01` (584 octets)

104 emplacements à la suite. Décalages depuis le début de l'emplacement.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `mID` | `int` | 4 | entier | Identifiant de la voiture, le même que dans la télémétrie. |
| 4 | `mDriverName` | `char[32]` | 32 | texte | Nom du pilote au volant. |
| 36 | `mVehicleName` | `char[64]` | 64 | texte | Nom de la voiture (engagement). |
| 100 | `mTotalLaps` | `short` | 2 | entier | Tours **terminés**. Tour en cours = `mTotalLaps + 1`. |
| 102 | `mSector` | `signed char` | 1 | code | Secteur en cours : **0 = secteur 3**, 1 = secteur 1, 2 = secteur 2. |
| 103 | `mFinishStatus` | `signed char` | 1 | code | 0 en course, 1 arrivé, 2 abandon, 3 disqualifié. |
| 104 | `mLapDist` | `double` | 8 | m | Distance parcourue dans le tour, depuis la ligne. Peut être légèrement négative ou dépasser la longueur du tour. |
| 112 | `mPathLateral` | `double` | 8 | m | Position latérale par rapport à une trajectoire « centrale » très approximative. |
| 120 | `mTrackEdge` | `double` | 8 | m | Bord de piste du côté de la voiture, par rapport à cette même trajectoire. |
| 128 | `mBestSector1` | `double` | 8 | s | Meilleur secteur 1. |
| 136 | `mBestSector2` | `double` | 8 | s | Meilleur temps au bout du secteur 2 (**secteur 1 compris**). |
| 144 | `mBestLapTime` | `double` | 8 | s | Meilleur tour. Zéro ou négatif = aucun. |
| 152 | `mLastSector1` | `double` | 8 | s | Secteur 1 du dernier tour. |
| 160 | `mLastSector2` | `double` | 8 | s | Temps au bout du secteur 2 du dernier tour (secteur 1 compris). |
| 168 | `mLastLapTime` | `double` | 8 | s | Dernier tour. Zéro ou négatif = pas de temps (tour invalide, tour de sortie). |
| 176 | `mCurSector1` | `double` | 8 | s | Secteur 1 du tour en cours, s'il est valide. Remis à zéro au passage de la ligne. |
| 184 | `mCurSector2` | `double` | 8 | s | Temps au bout du secteur 2 du tour en cours (secteur 1 compris), s'il est valide. |
| 192 | `mNumPitstops` | `short` | 2 | entier | Arrêts aux stands effectués. |
| 194 | `mNumPenalties` | `short` | 2 | entier | Pénalités restant à purger. |
| 196 | `mIsPlayer` | `bool` | 1 | 0 / 1 | C'est la voiture du joueur. **Le moyen fiable de trouver le joueur.** |
| 197 | `mControl` | `signed char` | 1 | code | Qui conduit : −1 personne, 0 joueur local, 1 IA locale, 2 joueur distant, 3 replay. |
| 198 | `mInPits` | `bool` | 1 | 0 / 1 | Entre l'entrée et la sortie de la voie des stands. Pas toujours exact pour les voitures distantes. |
| 199 | `mPlace` | `unsigned char` | 1 | entier | Position au classement général, à partir de 1. |
| 200 | `mVehicleClass` | `char[32]` | 32 | texte | Classe en clair (« Hypercar », « LMP2 », « LMGT3 »…). |
| 232 | `mTimeBehindNext` | `double` | 8 | s | Retard sur la voiture classée juste devant. |
| 240 | `mLapsBehindNext` | `int` | 4 | tours | Tours de retard sur la voiture classée juste devant. |
| 244 | `mTimeBehindLeader` | `double` | 8 | s | Retard sur le leader. |
| 252 | `mLapsBehindLeader` | `int` | 4 | tours | Tours de retard sur le leader. |
| 256 | `mLapStartET` | `double` | 8 | s | Temps de session au début du tour en cours. |
| 264 | `mPos` | `TelemVect3` | 24 | m | Position dans le monde. Moins fraîche que celle de la télémétrie (classement ≈ 5 fois par seconde). |
| 288 | `mLocalVel` | `TelemVect3` | 24 | m/s | Vitesse dans le repère de la voiture. |
| 312 | `mLocalAccel` | `TelemVect3` | 24 | m/s² | Accélération dans le repère de la voiture. |
| 336 | `mOri` | `TelemVect3[3]` | 72 | matrice | Matrice d'orientation, comme en télémétrie. |
| 408 | `mLocalRot` | `TelemVect3` | 24 | rad/s | Vitesse de rotation. |
| 432 | `mLocalRotAccel` | `TelemVect3` | 24 | rad/s² | Accélération de rotation. |
| 456 | `mHeadlights` | `bool` | 1 | code | État des phares. |
| 457 | `mPitState` | `unsigned char` | 1 | code | Arrêt au stand : 0 rien, 1 demandé, 2 entrée, 3 arrêté, 4 sortie. |
| 458 | `mServerScored` | `unsigned char` | 1 | 0 / 1 | La voiture est classée par le serveur. |
| 459 | `mIndividualPhase` | `unsigned char` | 1 | code | Phase de jeu propre à cette voiture : les valeurs de `mGamePhase`, plus 9 après la formation, 10 sous jaune, 11 sous bleu (inutilisé). |
| 460 | `mQualification` | `int` | 4 | entier | Position en qualification, à partir de 1 ; −1 si aucune. |
| 464 | `mTimeIntoLap` | `double` | 8 | s | Temps **estimé** depuis le début du tour, d'après la position sur la piste. Base des écarts entre voitures. |
| 472 | `mEstimatedLapTime` | `double` | 8 | s | Temps au tour estimé, utilisé par le jeu pour `mTimeIntoLap` et les retards. Peut changer selon la voiture et les réglages. |
| 480 | `mPitGroup` | `char[24]` | 24 | texte | Groupe de stand (le nom de l'équipe, sauf stand partagé). |
| 504 | `mFlag` | `unsigned char` | 1 | code | Drapeau montré à cette voiture : 0 vert, 6 bleu. Aucune autre valeur. |
| 505 | `mUnderYellow` | `bool` | 1 | 0 / 1 | A franchi la ligne sous neutralisation générale. |
| 506 | `mCountLapFlag` | `unsigned char` | 1 | code | 0 ni le tour ni le temps ne comptent, 1 le tour compte mais pas le temps, 2 les deux comptent. |
| 507 | `mInGarageStall` | `bool` | 1 | 0 / 1 | La voiture est dans son box. |
| 508 | `mUpgradePack` | `octets[16]` | 16 | octets | Options de la voiture, codées. |
| 524 | `mPitLapDist` | `float` | 4 | m | Emplacement du stand de la voiture, en distance dans le tour. |
| 528 | `mBestLapSector1` | `float` | 4 | s | Secteur 1 **du meilleur tour** (pas forcément le meilleur secteur 1). |
| 532 | `mBestLapSector2` | `float` | 4 | s | Temps au bout du secteur 2 du meilleur tour (secteur 1 compris). |
| 536 | `mSteamID` | `unsigned long long` | 8 | entier | **LMU.** Identifiant Steam du pilote, s'il y en a un. |
| 544 | `mVehFilename` | `char[32]` | 32 | texte | **LMU.** Nom du fichier `.veh` qui identifie la voiture. |
| 576 | `mAttackMode` | `short` | 2 | code | **LMU.** Mode attaque (Formule E, hérité). |
| 578 | `mFuelFraction` | `unsigned char` | 1 | 0 à 255 | **LMU.** Carburant ou batterie restant : 0 = vide, 255 = plein. Disponible pour **toutes** les voitures. |
| 579 | `mDRSState` | `bool` | 1 | 0 / 1 | **LMU.** DRS (volet arrière) ouvert. |
| 580 | `mExpansion` | `octets[4]` | 4 |  | Réservé. |

### Bloc `telemetry` — `SharedMemoryTelemtryData` (196 356 octets)

Décalages depuis le début du bloc.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `activeVehicles` | `unsigned char` | 1 | entier | Nombre d'entrées valides dans `telemInfo`. |
| 1 | `playerVehicleIdx` | `unsigned char` | 1 | index | Index de la voiture du joueur dans `telemInfo`. Préférer la recherche par `mIsPlayer` puis `mID`. |
| 2 | `playerHasVehicle` | `bool` | 1 | 0 / 1 | Le joueur a une voiture (faux dans les menus ou en spectateur). |
| 4 | `telemInfo` | `TelemInfoV01[104]` | 196352 |  | Une entrée par voiture, **adversaires compris**. L'index n'est pas celui du classement : associer par `mID`. |

### La télémétrie d'une voiture — `TelemInfoV01` (1 888 octets)

104 emplacements à la suite. Décalages depuis le début de l'emplacement.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `mID` | `int` | 4 | entier | Identifiant de la voiture (« slot »). C'est la clé pour relier télémétrie et classement. Peut être réattribué en multijoueur quand quelqu'un part. |
| 4 | `mDeltaTime` | `double` | 8 | s | Temps écoulé depuis la mise à jour précédente de cette voiture. |
| 12 | `mElapsedTime` | `double` | 8 | s | Temps de session au moment de cette télémétrie. Sert à savoir de quand date la position (extrapolation). |
| 20 | `mLapNumber` | `int` | 4 | entier | Numéro du tour en cours. |
| 24 | `mLapStartET` | `double` | 8 | s | Temps de session au début du tour en cours. Chrono du tour en cours = `mElapsedTime − mLapStartET`. |
| 32 | `mVehicleName` | `char[64]` | 64 | texte | Nom de la voiture (engagement : numéro, équipe…). |
| 96 | `mTrackName` | `char[64]` | 64 | texte | Nom du circuit. |
| 160 | `mPos` | `TelemVect3` | 24 | m | Position dans le monde. Vue de dessus : abscisse `x`, ordonnée `−z` ; `y` est l'altitude. |
| 184 | `mLocalVel` | `TelemVect3` | 24 | m/s | Vitesse dans le repère de la voiture. Vitesse = norme du vecteur ; vitesse vers l'avant = `−z`. |
| 208 | `mLocalAccel` | `TelemVect3` | 24 | m/s² | Accélération dans le repère de la voiture. En g : diviser par 9,81. Latéral = `x`, longitudinal = `z`. |
| 232 | `mOri` | `TelemVect3[3]` | 72 | matrice | Les trois lignes de la matrice d'orientation (repère voiture → monde). Cap = `atan2(mOri[2].x, mOri[2].z)`. |
| 304 | `mLocalRot` | `TelemVect3` | 24 | rad/s | Vitesse de rotation dans le repère de la voiture (`y` = lacet). |
| 328 | `mLocalRotAccel` | `TelemVect3` | 24 | rad/s² | Accélération de rotation dans le repère de la voiture. |
| 352 | `mGear` | `int` | 4 | code | Rapport engagé : −1 marche arrière, 0 point mort, 1 et plus. |
| 356 | `mEngineRPM` | `double` | 8 | tr/min | Régime moteur. |
| 364 | `mEngineWaterTemp` | `double` | 8 | °C | Température d'eau. |
| 372 | `mEngineOilTemp` | `double` | 8 | °C | Température d'huile. |
| 380 | `mClutchRPM` | `double` | 8 | tr/min | Régime côté embrayage (boîte). |
| 388 | `mUnfilteredThrottle` | `double` | 8 | 0 à 1 | Accélérateur, tel que le pilote l'enfonce. |
| 396 | `mUnfilteredBrake` | `double` | 8 | 0 à 1 | Frein, tel que le pilote l'enfonce. |
| 404 | `mUnfilteredSteering` | `double` | 8 | −1 à 1 | Volant, de la gauche (−1) à la droite (+1). |
| 412 | `mUnfilteredClutch` | `double` | 8 | 0 à 1 | Embrayage, tel que le pilote l'enfonce. |
| 420 | `mFilteredThrottle` | `double` | 8 | 0 à 1 | Accélérateur réellement appliqué (après antipatinage, limiteur, etc.). |
| 428 | `mFilteredBrake` | `double` | 8 | 0 à 1 | Frein réellement appliqué (après ABS). |
| 436 | `mFilteredSteering` | `double` | 8 | −1 à 1 | Volant réellement appliqué. |
| 444 | `mFilteredClutch` | `double` | 8 | 0 à 1 | Embrayage réellement appliqué (après anti-calage, départ automatique). |
| 452 | `mSteeringShaftTorque` | `double` | 8 | N·m | Couple dans la colonne de direction (la base du retour de force). |
| 460 | `mFront3rdDeflection` | `double` | 8 | m | Débattement du troisième élément avant. |
| 468 | `mRear3rdDeflection` | `double` | 8 | m | Débattement du troisième élément arrière. |
| 476 | `mFrontWingHeight` | `double` | 8 | m | Hauteur de l'aileron avant. |
| 484 | `mFrontRideHeight` | `double` | 8 | m | Hauteur de caisse avant. |
| 492 | `mRearRideHeight` | `double` | 8 | m | Hauteur de caisse arrière. |
| 500 | `mDrag` | `double` | 8 | N | Traînée aérodynamique. |
| 508 | `mFrontDownforce` | `double` | 8 | N | Appui avant. |
| 516 | `mRearDownforce` | `double` | 8 | N | Appui arrière. |
| 524 | `mFuel` | `double` | 8 | L | Carburant dans le réservoir. |
| 532 | `mEngineMaxRPM` | `double` | 8 | tr/min | Régime du limiteur (rupteur). |
| 540 | `mScheduledStops` | `unsigned char` | 1 | entier | Nombre d'arrêts aux stands programmés. |
| 541 | `mOverheating` | `bool` | 1 | 0 / 1 | Le témoin de surchauffe est affiché. |
| 542 | `mDetached` | `bool` | 1 | 0 / 1 | Une pièce (autre qu'une roue) s'est détachée. Dans LMU, en pratique l'aileron arrière. |
| 543 | `mHeadlights` | `bool` | 1 | 0 / 1 | Phares allumés. |
| 544 | `mDentSeverity` | `octets[8]` | 8 | 0, 1, 2 | Enfoncement de la carrosserie en 8 points autour de la voiture : 0 intact, 1 abîmé, 2 très abîmé. Ordre des zones : voir « Dégâts » plus bas. |
| 552 | `mLastImpactET` | `double` | 8 | s | Temps de session du dernier choc. |
| 560 | `mLastImpactMagnitude` | `double` | 8 |  | Force du dernier choc. |
| 568 | `mLastImpactPos` | `TelemVect3` | 24 | m | Point d'impact du dernier choc, dans le repère de la voiture. |
| 592 | `mEngineTorque` | `double` | 8 | N·m | Couple moteur actuel. |
| 600 | `mCurrentSector` | `int` | 4 | code | Secteur en cours, **base 0** (0, 1, 2), avec la voie des stands dans le bit de signe : `0x80000002` = entrée aux stands depuis le 3e secteur. Secteur = `valeur & 0x7FFFFFFF`. Numérotation différente de `mSector` du classement. |
| 604 | `mSpeedLimiter` | `unsigned char` | 1 | 0 / 1 | Limiteur de vitesse enclenché. |
| 605 | `mMaxGears` | `unsigned char` | 1 | entier | Nombre de rapports avant. |
| 606 | `mFrontTireCompoundIndex` | `unsigned char` | 1 | index | Rang de la gomme avant dans la marque. |
| 607 | `mRearTireCompoundIndex` | `unsigned char` | 1 | index | Rang de la gomme arrière dans la marque. |
| 608 | `mFuelCapacity` | `double` | 8 | L | Capacité du réservoir. |
| 616 | `mFrontFlapActivated` | `unsigned char` | 1 | 0 / 1 | Volet avant actionné. |
| 617 | `mRearFlapActivated` | `unsigned char` | 1 | 0 / 1 | Volet arrière (DRS) actionné. |
| 618 | `mRearFlapLegalStatus` | `unsigned char` | 1 | code | DRS : 0 interdit, 1 conditions réunies mais pas encore autorisé, 2 autorisé. |
| 619 | `mIgnitionStarter` | `unsigned char` | 1 | code | 0 coupé, 1 contact, 2 contact + démarreur. |
| 620 | `mFrontTireCompoundName` | `char[18]` | 18 | texte | Nom de la gomme avant (par exemple « Medium »). |
| 638 | `mRearTireCompoundName` | `char[18]` | 18 | texte | Nom de la gomme arrière. |
| 656 | `mSpeedLimiterAvailable` | `unsigned char` | 1 | 0 / 1 | La voiture a un limiteur de vitesse. |
| 657 | `mAntiStallActivated` | `unsigned char` | 1 | 0 / 1 | L'anti-calage est intervenu. |
| 658 | `mUnused` | `octets[2]` | 2 |  | Inutilisé. |
| 660 | `mVisualSteeringWheelRange` | `float` | 4 | degrés | Débattement **visuel** du volant affiché dans l'habitacle. |
| 664 | `mRearBrakeBias` | `double` | 8 | 0 à 1 | Part du freinage envoyée à l'arrière. Répartition avant = `1 − mRearBrakeBias`. |
| 672 | `mTurboBoostPressure` | `double` | 8 |  | Pression de suralimentation, si la voiture a un turbo. Unité non précisée par l'en-tête. |
| 680 | `mPhysicsToGraphicsOffset` | `float[3]` | 12 | m | Décalage entre le centre de gravité et le centre du modèle 3D. |
| 692 | `mPhysicalSteeringWheelRange` | `float` | 4 | degrés | Débattement **physique** du volant (butée à butée). |
| 696 | `mDeltaBest` | `double` | 8 | s | **LMU.** Avance (négatif) ou retard (positif) sur le meilleur tour, calculé par le jeu. |
| 704 | `mBatteryChargeFraction` | `double` | 8 | 0 à 1 | Charge de la batterie hybride. |
| 712 | `mElectricBoostMotorTorque` | `double` | 8 | N·m | Couple du moteur électrique (négatif en régénération). |
| 720 | `mElectricBoostMotorRPM` | `double` | 8 | tr/min | Régime du moteur électrique. |
| 728 | `mElectricBoostMotorTemperature` | `double` | 8 | °C | Température du moteur électrique. |
| 736 | `mElectricBoostWaterTemperature` | `double` | 8 | °C | Température d'eau du refroidissement du moteur électrique (0 s'il n'y en a pas). |
| 744 | `mElectricBoostMotorState` | `unsigned char` | 1 | code | 0 indisponible, 1 inactif, 2 propulsion, 3 régénération. |
| 745 | `mLapInvalidated` | `bool` | 1 | 0 / 1 | **LMU.** Le tour en cours est invalidé (limites de piste). |
| 746 | `mABSActive` | `bool` | 1 | 0 / 1 | **LMU.** L'ABS est en train d'agir. |
| 747 | `mTCActive` | `bool` | 1 | 0 / 1 | **LMU.** L'antipatinage est en train d'agir. |
| 748 | `mSpeedLimiterActive` | `bool` | 1 | 0 / 1 | **LMU.** Le limiteur de vitesse est en train de brider la voiture. |
| 749 | `mWiperState` | `unsigned char` | 1 | code | **LMU.** Essuie-glace : 0 arrêt, 1 automatique, 2 lent, 3 rapide. |
| 750 | `mTC` | `unsigned char` | 1 | cran | **LMU.** Réglage d'antipatinage. |
| 751 | `mTCMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de crans d'antipatinage. |
| 752 | `mTCSlip` | `unsigned char` | 1 | cran | **LMU.** Réglage du glissement toléré par l'antipatinage. |
| 753 | `mTCSlipMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de crans de glissement. |
| 754 | `mTCCut` | `unsigned char` | 1 | cran | **LMU.** Réglage de la coupure de puissance de l'antipatinage. |
| 755 | `mTCCutMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de crans de coupure. |
| 756 | `mABS` | `unsigned char` | 1 | cran | **LMU.** Réglage d'ABS. |
| 757 | `mABSMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de crans d'ABS. |
| 758 | `mMotorMap` | `unsigned char` | 1 | cran | **LMU.** Cartographie moteur. |
| 759 | `mMotorMapMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de cartographies. |
| 760 | `mMigration` | `unsigned char` | 1 | cran | **LMU.** Migration de freinage. |
| 761 | `mMigrationMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de crans de migration. |
| 762 | `mFrontAntiSway` | `unsigned char` | 1 | cran | **LMU.** Barre antiroulis avant. |
| 763 | `mFrontAntiSwayMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de crans de barre avant. |
| 764 | `mRearAntiSway` | `unsigned char` | 1 | cran | **LMU.** Barre antiroulis arrière. |
| 765 | `mRearAntiSwayMax` | `unsigned char` | 1 | cran | **LMU.** Nombre de crans de barre arrière. |
| 766 | `mLiftAndCoastProgress` | `unsigned char` | 1 | 0 à 255 | **LMU.** Progression du « lift and coast » (économie par relâché). |
| 767 | `mTrackLimitsSteps` | `unsigned char` | 1 | pas | **LMU.** Limites de piste accumulées, en pas : points × `mTrackLimitsStepsPerPoint` de la session. |
| 768 | `mRegen` | `float` | 4 | kW | **LMU.** Puissance récupérée par le moteur électrique. |
| 772 | `mStateOfCharge` | `float` | 4 | % | **LMU.** Charge de la batterie. Plage (0-1 ou 0-100) non contrôlée sur le vrai jeu. |
| 776 | `mVirtualEnergy` | `float` | 4 | fraction | **LMU.** Énergie virtuelle restante. Plage (0-1 ou 0-100) non contrôlée sur le vrai jeu ; le maximum est dans le service web (`fuelInfo.maxVirtualEnergy`). |
| 780 | `mTimeGapCarAhead` | `float` | 4 | s | **LMU.** Écart avec la voiture devant **sur la piste**. |
| 784 | `mTimeGapCarBehind` | `float` | 4 | s | **LMU.** Écart avec la voiture derrière sur la piste. |
| 788 | `mTimeGapPlaceAhead` | `float` | 4 | s | **LMU.** Écart avec la voiture classée juste devant. |
| 792 | `mTimeGapPlaceBehind` | `float` | 4 | s | **LMU.** Écart avec la voiture classée juste derrière. |
| 796 | `mVehicleModel` | `char[30]` | 30 | texte | **LMU.** Marque et modèle (par exemple « Ferrari 499P »). |
| 826 | `mVehicleClass` | `unsigned char` | 1 | code | **LMU.** Classe : 0 Hypercar, 2 LMP2 ELMS, 3 LMP2, 4 LMP3, 5 GTE, 6 GT3, 8 voiture de sécurité, 255 inconnue. Le classement donne la classe en texte. |
| 827 | `mVehicleChampionship` | `unsigned char` | 1 | code | **LMU.** Championnat : 0 à 3 = WEC 2023 à 2026, 16 et 17 = ELMS 2025 et 2026, 255 inconnu. |
| 828 | `mExpansion` | `octets[20]` | 20 |  | Réservé. |
| 848 | `mWheels` | `TelemWheelV01[4]` | 1040 |  | Les quatre roues : 0 avant gauche, 1 avant droite, 2 arrière gauche, 3 arrière droite. Détail dans `TelemWheelV01`. |

### Une roue — `TelemWheelV01` (260 octets)

Quatre à la fin de chaque télémétrie. Décalages depuis le début de la roue.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `mSuspensionDeflection` | `double` | 8 | m | Débattement de la suspension. |
| 8 | `mRideHeight` | `double` | 8 | m | Hauteur de caisse à cette roue. |
| 16 | `mSuspForce` | `double` | 8 | N | Effort dans le poussoir (pushrod). |
| 24 | `mBrakeTemp` | `double` | 8 | **°C** | Température du disque de frein. Attention : en degrés Celsius, alors que les pneus sont en Kelvin. |
| 32 | `mBrakePressure` | `double` | 8 | 0 à 1 | Pression de frein, en fraction : dépend de la pédale et de la répartition. Ce n'est pas une pression réelle. |
| 40 | `mRotation` | `double` | 8 | rad/s | Vitesse de rotation de la roue. Vitesse de roue en m/s = `|mRotation|` × rayon. |
| 48 | `mLateralPatchVel` | `double` | 8 | m/s | Vitesse latérale de la gomme au point de contact. |
| 56 | `mLongitudinalPatchVel` | `double` | 8 | m/s | Vitesse longitudinale de la gomme au point de contact. |
| 64 | `mLateralGroundVel` | `double` | 8 | m/s | Vitesse latérale du sol sous la roue. |
| 72 | `mLongitudinalGroundVel` | `double` | 8 | m/s | Vitesse longitudinale du sol sous la roue. Glissement = écart entre la gomme (`Patch`) et le sol (`Ground`). |
| 80 | `mCamber` | `double` | 8 | rad | Carrossage. Positif vers la gauche pour les roues gauches, vers la droite pour les roues droites. |
| 88 | `mLateralForce` | `double` | 8 | N | Effort latéral du pneu. |
| 96 | `mLongitudinalForce` | `double` | 8 | N | Effort longitudinal du pneu. |
| 104 | `mTireLoad` | `double` | 8 | N | Charge verticale sur le pneu. |
| 112 | `mGripFract` | `double` | 8 | 0 à 1 | Part approximative de la surface de contact qui glisse. |
| 120 | `mPressure` | `double` | 8 | kPa | Pression de l'air dans le pneu. 1 bar = 100 kPa, 1 psi = 6,895 kPa. |
| 128 | `mTemperature` | `double[3]` | 24 | **K** | Température de surface : gauche / centre / droite du pneu (pas intérieur / extérieur). °C = K − 273,15. |
| 152 | `mWear` | `double` | 8 | 0 à 1 | Gomme **restante** : 1 = pneu neuf, descend avec l'usure. L'en-tête dit « wear, fraction of maximum », mais la valeur part de 1 (TinyPedal l'appelle « tread »). Pas proportionnel à la perte d'adhérence. |
| 160 | `mTerrainName` | `char[16]` | 16 | texte | Nom du revêtement sous la roue (préfixe de matériau du fichier TDF du circuit). |
| 176 | `mSurfaceType` | `unsigned char` | 1 | code | Surface sous la roue : 0 sec, 1 mouillé, 2 herbe, 3 terre, 4 gravier, 5 vibreur, 6 spécial. |
| 177 | `mFlat` | `bool` | 1 | 0 / 1 | Pneu crevé. |
| 178 | `mDetached` | `bool` | 1 | 0 / 1 | Roue arrachée. |
| 179 | `mStaticUndeflectedRadius` | `unsigned char` | 1 | cm | Rayon du pneu au repos, en **centimètres** entiers. |
| 180 | `mVerticalTireDeflection` | `double` | 8 | m | Écrasement du pneu par rapport à son rayon (qui dépend de la vitesse). |
| 188 | `mWheelYLocation` | `double` | 8 | m | Hauteur de la roue par rapport à la hauteur de la voiture. |
| 196 | `mToe` | `double` | 8 | rad | Pincement / ouverture actuel, par rapport à la voiture. |
| 204 | `mTireCarcassTemperature` | `double` | 8 | **K** | Température moyenne de la carcasse. La plus proche de ce qu'affiche le jeu. |
| 212 | `mTireInnerLayerTemperature` | `double[3]` | 24 | **K** | Température de la couche de gomme la plus profonde (avant la carcasse) : gauche / centre / droite. |
| 236 | `mOptimalTemp` | `float` | 4 | °C | **LMU.** Température idéale de la gomme montée. |
| 240 | `mCompoundIndex` | `unsigned char` | 1 | index | **LMU.** Rang de la gomme dans la liste des gommes disponibles pour cette voiture sur ce circuit. |
| 241 | `mCompoundType` | `unsigned char` | 1 | code | **LMU.** Famille de gomme : 0 tendre, 1 medium, 2 dure, 3 pluie. |
| 242 | `mExpansion` | `octets[18]` | 18 |  | Réservé. |

### Un vecteur — `TelemVect3` (24 octets)

Trois `double`, soit 24 octets.

| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |
|---:|---|---|---:|---|---|
| 0 | `x` | `double` | 8 |  | Axe X : vers la **gauche** de la voiture (repère local) ou axe du monde. |
| 8 | `y` | `double` | 8 |  | Axe Y : vers le **haut**. |
| 16 | `z` | `double` | 8 |  | Axe Z : vers l'**arrière** de la voiture. Avancer = Z négatif. |

## 7. Conventions

### Repère

- Repère **de la voiture** (`mLocalVel`, `mLocalAccel`, `mLocalRot`) : `x` vers la gauche, `y` vers le haut,
  `z` vers l'arrière. Avancer donne donc un `z` négatif.
- Repère **du monde** (`mPos`) : mètres, `y` = altitude. Pour une carte vue de dessus : abscisse `x`, ordonnée `−z`.
- **Orientation** (`mOri`) : trois lignes d'une matrice. Le produit scalaire d'un vecteur local avec la ligne 0, 1 ou
  2 donne sa composante X, Y ou Z dans le monde. Cap de la voiture : `atan2(mOri[2].x, mOri[2].z)`.
- Passer d'un point du monde au repère d'une voiture (radar) : soustraire la position de la voiture, puis tourner de
  `cap − π` dans le plan (`x`, `−z`).

### Unités qui piègent

| Grandeur | Unité | Conversion |
|---|---|---|
| Températures de pneu (`mTemperature`, carcasse, couche interne) | Kelvin | °C = K − 273,15 |
| Températures de frein, d'eau, d'huile, d'air, de piste, optimale du pneu | °C | |
| Pression de pneu | kPa | bar = kPa ÷ 100 ; psi = kPa ÷ 6,895 |
| Vitesses | m/s | km/h = m/s × 3,6 |
| Rayon de roue (`mStaticUndeflectedRadius`) | centimètres entiers | |
| Angles (carrossage, pincement) | radians | degrés = rad × 57,296 |
| Carburant | litres | |
| `mFuelFraction` | 0 à 255 | % = valeur ÷ 2,55 |

### Temps et tours

- Tous les temps de session sont en secondes depuis le début de la session (`mCurrentET`, `mElapsedTime`,
  `mLapStartET`, `mLastImpactET`). `mStartET` et `mTimeOfDay` sont en secondes depuis minuit.
- Un temps au tour ou de secteur **nul ou négatif** veut dire « pas de temps » (tour invalide, tour de sortie,
  aucun tour bouclé).
- Les « secteurs 2 » sont **cumulés** : `mLastSector2` est le temps au bout du deuxième secteur.
  Secteur 2 seul = `mLastSector2 − mLastSector1` ; secteur 3 = `mLastLapTime − mLastSector2`.
- `mCurSector1` et `mCurSector2` sont remis à zéro dès le passage de la ligne : pour garder les secteurs d'un tour,
  les relever pendant le tour.
- `mTotalLaps` compte les tours terminés. Le numéro du tour en cours, comme l'affiche le jeu, est `mTotalLaps + 1`.

### Numérotation des secteurs

| Champ | Secteur 1 | Secteur 2 | Secteur 3 |
|---|---|---|---|
| `mSector` (classement) | 1 | 2 | **0** |
| `mCurrentSector & 0x7FFFFFFF` (télémétrie) | 0 | 1 | 2 |

### Dégâts : ordre des huit zones de `mDentSeverity`

D'après l'usage qu'en fait TinyPedal (non décrit par l'en-tête), en tournant autour de la voiture :

| Index | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 |
|---|---|---|---|---|---|---|---|---|
| Zone | avant | avant gauche | gauche | arrière gauche | arrière | arrière droit | droite | avant droit |

Une pièce arrachée (`mDetached`) n'apparaît pas dans ce tableau. Les dégâts aérodynamiques chiffrés, ceux des
suspensions et l'usure des freins ne sont **pas** en mémoire partagée (voir la section 10).

## 8. Recettes

| Besoin | Calcul |
|---|---|
| Vitesse | `√(x² + y² + z²)` de `mLocalVel`, × 3,6 pour des km/h |
| Chrono du tour en cours | `mElapsedTime − mLapStartET` |
| Accélérations en g | `mLocalAccel.x ÷ 9,81` (latéral), `mLocalAccel.z ÷ 9,81` (longitudinal) |
| Répartition de freinage avant | `(1 − mRearBrakeBias) × 100` |
| Voiture dans la voie des stands | `mInPits`, ou le bit de signe de `mCurrentSector` |
| Tour en cours invalidé | `mLapInvalidated` (télémétrie du joueur) |
| Dernier tour invalide | `mLastLapTime ≤ 0` alors que `mTotalLaps` a augmenté |
| Position dans la classe | compter les voitures de même `mVehicleClass` qui ont une `mPlace` plus petite, plus un |
| Écart en temps entre deux voitures sur la piste | différence de leurs `mTimeIntoLap`, ramenée dans ± un demi-`mEstimatedLapTime` |
| Fraction du tour parcourue | `mLapDist ÷ ScoringInfo.mLapDist` |
| Temps de session restant | `mSessionTimeRemaining`, ou `mEndET − mCurrentET` |
| Course au temps ou au nombre de tours | `mMaxLaps` très grand = course au temps |
| Drapeau bleu pour le joueur | `mFlag == 6` sur sa ligne du classement |
| Jaune dans un secteur | `mSectorFlag[i] == 1` |
| Neutralisation générale | `mGamePhase == 6` ; le détail est dans `mYellowFlagState` |
| Pluie | `mRaining` (intensité), `mAvgPathWetness` (piste), `mCloudCoverage` (ciel) |
| Carburant des adversaires | `mFuelFraction` du classement (0 à 255) |
| Gomme montée par chaque voiture | `mFrontTireCompoundName` et `mCompoundType` de sa télémétrie |
| Le joueur est-il en piste | `mInRealtime`, ou `mOptionsLocation == 3` |

## 9. Ce qui a été vérifié, et ce qui ne l'a pas été

**Contrôlé automatiquement** (`cargo test -p lmu`) : la taille des douze structures et dix décalages, identiques à
ceux de la bibliothèque de référence. Le script qui génère ce document recalcule tous les décalages et refuse de
produire le fichier s'ils ne retombent pas sur ces valeurs.

**Mesuré sur le vrai jeu** :

- la cadence du classement (≈ 5 Hz) et de la télémétrie (≈ 20 Hz) ;
- les indicateurs d'événements qui restent figés en roulant ;
- `mSectorFlag` qui ne vaut pas 0 au repos (d'où le test « égal à 1 ») ;
- la version du jeu lue dans `gameVersion` (14200) ;
- les données courantes affichées par PaceWall en jeu : carburant, pneus, régime, rapport, pédales, classement,
  chronos, positions sur la carte ;
- le sens gauche / droite du radar (projection d'une voiture dans le repère d'une autre, formule de la section 7) ;
- le drapeau bleu (`mFlag == 6`) et le jaune par secteur (`mSectorFlag`, première case = secteur 1) ;
- les écarts calculés avec `mTimeIntoLap` et `mEstimatedLapTime`, comparés à l'affichage du jeu ;
- les tours invalidés (`mLastLapTime ≤ 0`), les secteurs et les gommes, dans les replays.

**Repris des en-têtes sans vérification en jeu** — à traiter avec prudence :

- la plage de `mVirtualEnergy` et de `mStateOfCharge` (0 à 1 ou 0 à 100) ;
- l'ordre des zones de `mDentSeverity` ;
- les unités que l'en-tête ne précise pas (traînée, appuis, hauteurs, couple de direction, pression de turbo,
  débattement du volant) ;
- le sens exact de chaque indicateur de `generic.events` (déduit de son nom) ;
- tous les champs que PaceWall n'utilise pas : multijoueur (`mServer*`, `mGameMode`), `mAttackMode`, `mUpgradePack`,
  `mPitLapDist`, `mIndividualPhase`, moteur électrique, `scoringStream`, `paths`, `appInfo`.

## 10. Ce qui n'est pas dans la mémoire partagée

Ces données existent dans le jeu mais passent par son service web local (voir [donnees.md](donnees.md)) :

| Donnée | Ressource |
|---|---|
| Prévisions météo de la session | `/rest/sessions/weather` |
| Durée estimée de l'arrêt au stand, part des réparations | `/rest/strategy/pitstop-estimate` |
| Dégâts aérodynamiques, dégâts des suspensions, épaisseur des freins | `/rest/garage/UIScreen/RepairAndRefuel` |
| Menu des stands (carburant, pneus, réparations programmés) | `/rest/garage/UIScreen/RepairAndRefuel` |
| Énergie virtuelle maximale | `/rest/garage/UIScreen/RepairAndRefuel` |
| Échelle de temps de la course, réglage de la voiture | `/rest/sessions`, `/rest/garage/getPlayerGarageData` |

Absents des deux canaux, à notre connaissance : la vitesse limite de la voie des stands et la position de son
entrée et de sa sortie (PaceWall les apprend en roulant), le tracé du circuit (reconstruit à partir des positions),
les limites de secteurs en distance (relevées au passage).

## 11. Exemples de lecture

### Python, sans dépendance

```python
import mmap
import struct

SIZE = 324820
VEH, VEH_SIZE = 2192, 584          # voitures du classement
TEL, TEL_SIZE = 128468, 1888       # voitures de la télémétrie

# Attention : si le jeu ne tourne pas, Python crée une zone vide (que des zéros) au lieu d'échouer.
zone = mmap.mmap(-1, SIZE, "LMU_Data")

def text(offset, length):
    return zone[offset:offset + length].split(b"\0")[0].decode("utf-8", "replace")

count = struct.unpack_from("<i", zone, 1736)[0]          # mNumVehicles
player = next((i for i in range(count) if zone[VEH + i * VEH_SIZE + 196]), None)   # mIsPlayer
if player is not None:
    line = VEH + player * VEH_SIZE
    car_id = struct.unpack_from("<i", zone, line)[0]            # mID
    print(text(line + 4, 32), "P%d" % zone[line + 199])         # mDriverName, mPlace
    active = zone[128464]                                       # activeVehicles
    for j in range(active):
        entry = TEL + j * TEL_SIZE
        if struct.unpack_from("<i", zone, entry)[0] == car_id:
            fuel = struct.unpack_from("<d", zone, entry + 524)[0]        # mFuel
            vx, vy, vz = struct.unpack_from("<3d", zone, entry + 184)    # mLocalVel
            print("%.1f L, %.0f km/h" % (fuel, (vx * vx + vy * vy + vz * vz) ** 0.5 * 3.6))
```

### Rust, avec la bibliothèque de PaceWall

```rust
use lmu::reader::{c_str, Reader};

if let Some(reader) = Reader::open() {
    if let Some((scoring, telemetry)) = reader.player_indexes() {
        let line = reader.vehicle_scoring(scoring).unwrap();
        let car = reader.telemetry(telemetry).unwrap();
        let fuel = car.mFuel; // structure « packed » : copier le champ avant de s'en servir
        println!("{} P{} {fuel:.1} L", c_str(&line.mDriverName), line.mPlace);
    }
}
```

Le lecteur de PaceWall ne copie jamais la zone entière : il lit la session (548 octets), une ligne du classement
(584) ou une télémétrie (1 888) à la demande, et pour les positions de toutes les voitures seulement les champs utiles.

## 12. Licences

Les structures viennent des en-têtes distribués avec le jeu par Studio 397. Le portage Python consulté,
pyLMUSharedMemory, est sous licence MIT (© 2021 Tony Whitley, © 2025 Xiang). TinyPedal (GPLv3) n'a servi que
d'exemple d'utilisation : aucun de ses codes n'est repris.
