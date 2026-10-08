# Données de Le Mans Ultimate : ce qui existe et comment le jeu les envoie

Référence rapide pour développer PaceWall. Les structures Rust sont dans
[`crates/lmu/src/data.rs`](../crates/lmu/src/data.rs) et le lecteur dans
[`crates/lmu/src/reader.rs`](../crates/lmu/src/reader.rs).

**Sources** : en-têtes C++ du jeu (`SharedMemoryInterface.hpp`, `InternalsPlugin.hpp`) via
`pyLMUSharedMemory` (MIT), et la façon dont TinyPedal (GPLv3) les consomme. TinyPedal sert de
catalogue, aucun de ses codes n'est copié.

**Légende** : ✅ vérifié dans les sources de référence · ⚠️ à confirmer avec le vrai jeu.

---

## 1. Comment LMU envoie les données

Deux canaux, qui ne servent pas aux mêmes choses.

| | Mémoire partagée | API REST locale |
|---|---|---|
| Nature | Zone mémoire Windows nommée `LMU_Data` | Serveur HTTP du jeu, sur la machine |
| Usage | Temps réel : voiture, pilotage, classement, session, météo actuelle | Données lentes : prévisions, usure freins/dégâts, estimation de stand, réglages |
| Cadence conseillée | Selon le widget, de 1 à 60 Hz | 0,1 à 1 s par ressource (TinyPedal : 0,1 s à 1 s) |
| Coût de lecture | Très faible (lecture directe) | Requête HTTP + JSON, à espacer |
| Activation | `Enable Plugins` dans Réglages > Gameplay | Le jeu doit tourner (port à confirmer ⚠️) |

### 1.1 Mémoire partagée

- **Nom** : `LMU_Data`. Ouverte en lecture seule (`OpenFileMappingW` + `MapViewOfFile`).
- **Taille** : 324 820 octets. Disposition `packed(4)` (les `f64` ne sont pas alignés sur 8).
- **Contenu** (`SharedMemoryObjectOut`), dans l'ordre :

| Bloc | Décalage | Taille | Rôle |
|---|---|---|---|
| `generic` | 0 | 332 | Compteurs d'événements, version du jeu, couple FFB, fenêtre du jeu |
| `paths` | 332 | 1 300 | Chemins des dossiers du jeu (données utilisateur, résultats, plugins…) |
| `scoring` | 1 632 | 126 832 | Session, météo, classement des 104 voitures, flux de résultats (64 Ko) |
| `telemetry` | 128 464 | 196 356 | Télémétrie détaillée des 104 voitures |

- **Quand ça change** : ⚠️ relevé réel (LMU 14200, `pacewall.exe --probe`) : les compteurs de `generic.events` **restent figés** (valeur 16, 0 changement en 20 s). Les vrais témoins sont le temps de session `mCurrentET` (≈ 5 Hz) et le temps de la voiture `mElapsedTime` (≈ 20 Hz) : voir `pwcore::pulse`. Ancienne hypothèse, fausse : le jeu incrémente les compteurs de `generic.events`.
  `SME_UPDATE_TELEMETRY` et `SME_UPDATE_SCORING` augmentent à chaque publication. Si le compteur
  ne bouge plus, le jeu est en pause, fermé ou dans les menus. C'est le signal pour ralentir la lecture.
- **Autres événements** : `SME_START_SESSION`, `SME_END_SESSION`, `SME_ENTER_REALTIME`,
  `SME_EXIT_REALTIME`, `SME_LOAD`, `SME_UNLOAD`, `SME_STARTUP`, `SME_SHUTDOWN`, `SME_ENTER`,
  `SME_EXIT`, `SME_INIT_APPLICATION`, `SME_UNINIT_APPLICATION`, `SME_SET_ENVIRONMENT`, `SME_FFB`.
  Utiles pour détecter un changement de session (remise à zéro de l'historique).
- **Pas de verrou** : le jeu écrit pendant qu'on lit, donc une lecture peut mélanger deux instants.
  TinyPedal recopie la zone seulement quand le nombre de voitures du scoring égale celui de la
  télémétrie. ⚠️ À reprendre en version légère (relire si le compteur a changé pendant la lecture).

### 1.2 Pièges connus

1. **Trouver le joueur** : l'index dans `scoring.vehScoringInfo` n'est pas forcément celui de
   `telemetry.telemInfo`. TinyPedal cherche la ligne du scoring avec `mIsPlayer = 1`, prend son `mID`,
   puis retrouve l'entrée de télémétrie qui a le même `mID`. Notre `Reader::player_index()` utilise
   `playerVehicleIdx`, à remplacer par cette méthode ⚠️.
2. **Les identifiants se réutilisent** : en multijoueur un `mID` peut être repris après un départ.
3. **Pause** : TinyPedal détecte le gel quand `scoringInfo.mCurrentET` ne change plus.
4. **Unités mélangées** : températures de pneus en Kelvin, de freins et moteur en °C, vitesses en m/s.
5. **Champs `packed`** : toujours copier avant de lire ou d'afficher (jamais de référence).
6. **Pneus d'un coup** : `mWheels` est un tableau de 4 (0 = avant gauche, 1 = avant droite,
   2 = arrière gauche, 3 = arrière droite).
7. **Secteur** : `mSector` du scoring vaut 0 pour le secteur 3, 1 pour le secteur 1, 2 pour le secteur 2.
   Dans la télémétrie, `mCurrentSector` est en base 0 et le bit de signe indique la voie des stands.

---

## 2. Télémétrie d'une voiture : `VehicleTelemetry` (1 888 octets × 104)

Chaque voiture de la grille a une entrée (adversaires compris). Pour un overlay joueur on n'en lit qu'une.

### Identité et temps
| Champ | Unité / valeurs | Note |
|---|---|---|
| `mID` | entier | Identifiant de slot |
| `mElapsedTime` | s | Temps de session |
| `mDeltaTime` | s | Temps depuis la dernière mise à jour |
| `mLapNumber` | entier | Tour en cours |
| `mLapStartET` | s | Début du tour en cours |
| `mVehicleName`, `mTrackName` | texte (64) | |
| `mVehicleModel` | texte (30) | Marque et modèle |
| `mVehicleClass` | 0 Hypercar, 2 LMP2 ELMS, 3 LMP2, 4 LMP3, 5 GTE, 6 GT3, 8 Pace car, 255 inconnu | |
| `mVehicleChampionship` | 0-3 WEC 2023-26, 16-17 ELMS 2025-26, 255 inconnu | |

### Moteur et transmission
| Champ | Unité / valeurs |
|---|---|
| `mGear` | -1 marche arrière, 0 neutre, 1+ |
| `mMaxGears` | entier |
| `mEngineRPM`, `mEngineMaxRPM` | tr/min |
| `mEngineWaterTemp`, `mEngineOilTemp` | °C |
| `mEngineTorque` | N·m |
| `mClutchRPM` | tr/min |
| `mTurboBoostPressure` | pression (si turbo) |
| `mOverheating` | bool |

### Pilotage
| Champ | Plage |
|---|---|
| `mUnfilteredThrottle`, `mUnfilteredBrake`, `mUnfilteredClutch` | 0 à 1 (valeur brute du volant/pédales) |
| `mFilteredThrottle`, `mFilteredBrake`, `mFilteredClutch` | 0 à 1 (après filtrage du jeu) |
| `mUnfilteredSteering`, `mFilteredSteering` | -1 à 1 (gauche vers droite) |
| `mSteeringShaftTorque` | couple de colonne |
| `mVisualSteeringWheelRange`, `mPhysicalSteeringWheelRange` | débattement visuel / physique |

### Carburant et énergie (stratégie)
| Champ | Unité / valeurs | Note |
|---|---|---|
| `mFuel` | litres | |
| `mFuelCapacity` | litres | |
| `mVirtualEnergy` | fraction restante | Énergie virtuelle (Hypercar notamment) ⚠️ plage exacte (0-100) |
| `mBatteryChargeFraction` | 0 à 1 | Batterie hybride |
| `mStateOfCharge` | pourcentage | |
| `mRegen` | kW | Récupération |
| `mElectricBoostMotorState` | 0 indisponible, 1 inactif, 2 propulsion, 3 régénération | |
| `mElectricBoostMotorTorque`, `…RPM`, `…Temperature`, `mElectricBoostWaterTemperature` | | Moteur électrique |
| `mLiftAndCoastProgress` | 0 à 255 | Progression de l'économie par relâché |
| `mScheduledStops` | entier | Arrêts prévus |

### Aides et réglages embarqués
`mTC`/`mTCMax`, `mTCSlip`/`mTCSlipMax`, `mTCCut`/`mTCCutMax`, `mABS`/`mABSMax`,
`mMotorMap`/`mMotorMapMax`, `mMigration`/`mMigrationMax` (migration de frein),
`mFrontAntiSway`/`…Max`, `mRearAntiSway`/`…Max` : niveau actuel et maximum pour chaque réglage.
`mRearBrakeBias` : fraction de freinage à l'arrière.
États : `mABSActive`, `mTCActive`, `mSpeedLimiter`, `mSpeedLimiterAvailable`, `mSpeedLimiterActive`,
`mAntiStallActivated`, `mHeadlights`, `mWiperState` (0 arrêt, 1 auto, 2 lent, 3 rapide),
`mIgnitionStarter` (0, 1 allumage, 2 allumage + démarreur), `mFrontFlapActivated`, `mRearFlapActivated`,
`mRearFlapLegalStatus` (0 interdit, 1 détecté mais pas encore autorisé, 2 autorisé).

### Écarts et chrono
| Champ | Unité |
|---|---|
| `mDeltaBest` | s, avance/retard sur le meilleur tour |
| `mTimeGapCarAhead`, `mTimeGapCarBehind` | s, voiture devant / derrière sur la piste |
| `mTimeGapPlaceAhead`, `mTimeGapPlaceBehind` | s, place devant / derrière au classement |
| `mLapInvalidated` | bool |
| `mTrackLimitsSteps` | pas de limites de piste accumulés |

### Dynamique et position
`mPos`, `mLocalVel` (m/s), `mLocalAccel`, `mOri` (matrice d'orientation), `mLocalRot`, `mLocalRotAccel`
(tous en `Vect3`). La vitesse est la norme de `mLocalVel` ; sur l'axe de la voiture c'est `z` (négatif en avant).
Aéro : `mDrag`, `mFrontDownforce`, `mRearDownforce`, `mFrontWingHeight`, `mFrontRideHeight`, `mRearRideHeight`,
`mFront3rdDeflection`, `mRear3rdDeflection`.

### Dégâts
`mDetached` (pièce arrachée), `mDentSeverity` (8 zones : 0 aucun, 1 léger, 2 fort), `mLastImpactET`,
`mLastImpactMagnitude`, `mLastImpactPos`.

### Pneus : `mWheels[4]` (`Wheel`, 260 octets chacun)
| Champ | Unité / valeurs | Note |
|---|---|---|
| `mTemperature[3]` | **Kelvin** (−273,15 pour °C) | Surface gauche / centre / droite (pas intérieur / extérieur) |
| `mTireCarcassTemperature` | Kelvin | Carcasse |
| `mTireInnerLayerTemperature[3]` | Kelvin | Couche interne |
| `mOptimalTemp` | °C | Température idéale du composé |
| `mPressure` | kPa | |
| `mWear` | 0 à 1 | Fraction de l'usure maximale, pas proportionnelle à la perte de grip |
| `mGripFract` | 0 à 1 | Part de la surface de contact qui glisse |
| `mTireLoad` | N | Charge |
| `mCamber`, `mToe` | rad | |
| `mFlat`, `mDetached` | bool | Crevé / roue arrachée |
| `mCompoundIndex`, `mCompoundType` | type : 0 tendre, 1 medium, 2 dur, 3 pluie | |
| `mSurfaceType` | 0 sec, 1 mouillé, 2 herbe, 3 terre, 4 gravier, 5 vibreur, 6 spécial | Surface sous la roue |
| `mTerrainName` | texte (16) | |
| `mBrakeTemp` | **°C** | Frein de cette roue |
| `mBrakePressure` | 0 à 1 | Fraction, pas une pression réelle |
| `mRotation` | rad/s | |
| `mSuspensionDeflection`, `mRideHeight`, `mSuspForce` | m, m, N | Suspension |
| `mLateralForce`, `mLongitudinalForce` | N | |
| `mLateralPatchVel`, `mLongitudinalPatchVel` (+ `Ground`) | m/s | Glissements |

Les noms de composés sont aussi dans `mFrontTireCompoundName` / `mRearTireCompoundName` (texte, 18).

---

## 3. Classement d'une voiture : `VehicleScoring` (584 octets × 104)

| Champ | Valeurs |
|---|---|
| `mID`, `mDriverName` (32), `mVehicleName` (64), `mVehicleClass` (texte, 32), `mSteamID` | Identité |
| `mIsPlayer` | 1 pour le joueur local (sert à le retrouver) |
| `mControl` | -1 personne, 0 joueur, 1 IA locale, 2 distant, 3 replay |
| `mPlace` | Position, base 1 |
| `mTotalLaps` | Tours terminés |
| `mLapDist` | Distance dans le tour (m) |
| `mPathLateral`, `mTrackEdge` | Position latérale approximative sur la piste |
| `mBestLapTime`, `mLastLapTime` | s |
| `mBestSector1/2`, `mLastSector1/2`, `mCurSector1/2` | s (le secteur 2 inclut le 1) |
| `mBestLapSector1/2` | Secteurs du meilleur tour |
| `mTimeBehindNext`, `mLapsBehindNext` | Retard sur la voiture devant |
| `mTimeBehindLeader`, `mLapsBehindLeader` | Retard sur le leader |
| `mTimeIntoLap`, `mEstimatedLapTime` | Temps estimé dans le tour / durée estimée |
| `mNumPitstops`, `mNumPenalties` | Arrêts effectués, pénalités en cours |
| `mInPits`, `mPitState` | 0 aucun, 1 demandé, 2 entrée, 3 arrêté, 4 sortie |
| `mPitGroup` (24), `mPitLapDist` | Équipe de stand, position du stand |
| `mInGarageStall` | Dans le box |
| `mFinishStatus` | 0 aucun, 1 arrivé, 2 abandon, 3 disqualifié |
| `mFlag` | 0 vert, 6 bleu |
| `mUnderYellow`, `mCountLapFlag` | Jaune pris, comptage du tour (0 non, 1 tour sans temps, 2 tour et temps) |
| `mFuelFraction` | 0 à 255 (pourcentage de carburant ou de batterie, voitures adverses incluses) |
| `mAttackMode`, `mDRSState` | Mode attaque, état du DRS |
| `mQualification` | Position en qualif (-1 si invalide) |
| `mHeadlights`, `mServerScored`, `mIndividualPhase` | |
| `mPos`, `mLocalVel`, `mLocalAccel`, `mOri`, `mLocalRot`, `mLocalRotAccel` | Dynamique, comme en télémétrie |

---

## 4. Session, météo, drapeaux : `ScoringInfo` (548 octets)

| Champ | Valeurs |
|---|---|
| `mTrackName` | Nom du circuit |
| `mSession` | 0 test, 1-4 essais, 5-8 qualifs, 9 warmup, 10-13 course |
| `mGamePhase` | 0 garage, 1 reco, 2 grille, 3 formation, 4 feux, 5 vert, 6 FCY / safety car, 7 arrêtée, 8 terminée, 9 pause |
| `mYellowFlagState` | -1 invalide, 0 aucun, 1 en attente, 2 stands fermés, 3 stands (tour du leader), 4 stands ouverts, 5 dernier tour, 6 reprise, 7 course arrêtée |
| `mSectorFlag[3]` | Jaune local par secteur |
| `mStartLight`, `mNumRedLights` | Feux de départ |
| `mCurrentET`, `mEndET` | Temps de session, fin de session (s) |
| `mSessionTimeRemaining` | s |
| `mMaxLaps` | Tours max (Calculer en fonction de la durée de la session) |
| `mLapDist` | Longueur du tour (m) |
| `mNumVehicles` | Voitures dans la session |
| `mInRealtime` | En piste (pas dans le moniteur) |
| `mTimeOfDay` | 0 à 86 400 s |
| **Météo** | |
| `mRaining` | 0 à 1 (intensité de la pluie) |
| `mDarkCloud` | 0 à 1 |
| `mCloudCoverage` | 0 dégagé, 1 nuages légers, 2 partiellement nuageux, 3 très nuageux, 4 couvert, 5 bruine, 6 pluie légère (nuageux), 7 pluie légère (couvert), 8 pluie, 9 forte pluie, 10 orage |
| `mAmbientTemp`, `mTrackTemp` | °C |
| `mWind` | `Vect3`, vitesse du vent |
| `mMinPathWetness`, `mMaxPathWetness`, `mAvgPathWetness` | 0 à 1, humidité de la trajectoire |
| `mTrackGripLevel` | 0 vert, 1 faible, 2 moyen, 3 élevé, 4 saturé (gomme posée, lavée par la pluie) |
| **Multijoueur** | |
| `mGameMode` | 1 serveur, 2 client, 3 les deux |
| `mServerName`, `mServerPort`, `mServerPublicIP`, `mMaxPlayers`, `mIsPasswordProtected` | |
| **Divers** | |
| `mPlayerName`, `mPlrFileName` | Pilote et profil |
| `mIsFixedSetup` | Réglages fixes |
| `mTrackLimitsStepsPerPenalty`, `mTrackLimitsStepsPerPoint` | Barème des limites de piste |

---

## 5. Hors mémoire partagée : API REST locale

Ressources utilisées par TinyPedal (chemins relatifs au serveur du jeu) :

| Chemin | Donne | Cadence TinyPedal |
|---|---|---|
| `/rest/sessions/weather` | Prévisions météo par session (essais, qualif, course) | 0,1 s mini |
| `/rest/sessions` | Échelle de temps de la course (`race_timescale`), qualif privée | 0,1 s mini |
| `/rest/garage/getPlayerGarageData` | Débattement du volant, dernier réglage de la voiture | 0,1 s mini |
| `/rest/garage/UIScreen/RepairAndRefuel` | Dégâts aéro, usure freins par roue, dégâts de suspension, énergie virtuelle **maximale**, plein absolu | 0,2 s |
| `/rest/strategy/pitstop-estimate` | Durée estimée de l'arrêt, dont la réparation | 1 s |

⚠️ À confirmer avec le vrai jeu : port d'écoute, format exact des réponses JSON, nécessité d'activer
une option. Les données de ce canal sont celles qui manquent à la mémoire partagée pour une stratégie complète.

---

## 6. Ce qu'on peut faire avec ces données

| Widget / fonction | Données utilisées |
|---|---|
| Carburant et énergie : tours restants, par tour, à prévoir | `mFuel`, `mFuelCapacity`, `mVirtualEnergy`, `mLapNumber`, `mLapStartET`, `maxVirtualEnergy` (REST) |
| Pneus : températures par zone, usure, pression, écart à l'optimum | `mWheels[].mTemperature`, `mWear`, `mPressure`, `mOptimalTemp` |
| Freins : températures, usure | `mWheels[].mBrakeTemp`, usure freins (REST) |
| Écarts : devant / derrière, au leader, delta best | `mTimeGap*`, `mTimeBehind*`, `mDeltaBest` |
| Classement multi-classes | `VehicleScoring.mPlace`, `mVehicleClass` |
| Météo : pluie actuelle, prévisions, grip, température de piste | `ScoringInfo`, `/rest/sessions/weather` |
| Stands : arrêt prévu, durée estimée, état | `mScheduledStops`, `mPitState`, `/rest/strategy/pitstop-estimate` |
| Pédales / volant / rapport / tr/min | `mFiltered*`, `mGear`, `mEngineRPM` |
| Aides (TC, ABS, répartition, carto moteur) | `mTC`, `mABS`, `mRearBrakeBias`, `mMotorMap`, `mMigration` |
| Drapeaux et phase de course | `mGamePhase`, `mYellowFlagState`, `mSectorFlag`, `mFlag` |
| Historique par tour | Tout ce qui précède, échantillonné à chaque passage de ligne (`mLapNumber` change) |
| Clips automatiques | `mLastImpactET`, `mPlace`, `mTotalLaps`, `mPitState` |

## 6 bis. Carte du circuit

| Donnée | Source | Usage |
|---|---|---|
| Position dans le monde | `VehicleScoring.mPos` (x, y, z en mètres) pour toutes les voitures | Voitures sur la carte |
| Distance dans le tour | `VehicleScoring.mLapDist` | Découpage du tour, carte de secours en cercle |
| Longueur du tour | `ScoringInfo.mLapDist` | Position sur le cercle |
| Validité du tour | `VehicleScoring.mLastLapTime > 0` | Valider un tour enregistré |

Repère retenu comme TinyPedal : abscisse `mPos.x`, ordonnée `-mPos.z`. ⚠️ À confirmer avec le vrai jeu : sens et
orientation de la carte (l'option de rotation corrige un décalage).

## 6 ter. Pneus et secteurs

- **État du pneu** : `mWear` est la bande de gomme **restante** (1 = neuve, « tread » chez TinyPedal). PaceWall expose
  `tyre.*.wear` = `mWear × 100` : 100 % neuf, tombe vers 0 % quand le pneu est usé. Le widget peut aussi l'afficher en
  usure (0 → 100 %). L'historique garde l'état et calcule l'usure par tour comme la baisse d'un tour à l'autre.
  ⚠️ Sens de `mWear` déduit de TinyPedal, à confirmer sur le jeu (la valeur doit partir de ~100 et descendre).
- **Températures** : `tyre.*.carcass` (`mTireCarcassTemperature`, la plus proche de l'affichage du jeu d'après TinyPedal),
  `tyre.*.inner` (moyenne de `mTireInnerLayerTemperature`), `temp_left/center/right` (surface). Le widget propose les trois
  sources et un décalage en °C.
- **Secteurs** : `VehicleScoring.mSector` (0 = secteur 3, 1 = secteur 1, 2 = secteur 2). PaceWall relève la distance
  de tour aux changements 1→2 et 2→0 sur un tour sans stands et les range dans `tracks/<circuit>.json` (`sectors`,
  fractions du tour). ⚠️ À confirmer avec le vrai jeu.

## 7. Mesure et vérification

- `cargo test -p lmu` : vérifie que la disposition des structures correspond à celle du jeu
  (12 tailles et 10 décalages mesurés avec `ctypes` sur le portage de référence).
- `cargo run -p fakegame` puis `cargo run -p collector` : test de bout en bout sans le jeu.
  Les valeurs du faux jeu sont inventées.
- Avec le vrai jeu, les points marqués ⚠️ ci-dessus sont à confirmer (plage de `mVirtualEnergy`,
  API REST, lecture cohérente, détection du joueur).
