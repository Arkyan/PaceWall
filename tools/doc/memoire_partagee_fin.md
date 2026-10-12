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
  chronos, positions sur la carte.

**Repris des en-têtes sans vérification en jeu** — à traiter avec prudence :

- la plage de `mVirtualEnergy` et de `mStateOfCharge` (0 à 1 ou 0 à 100) ;
- l'ordre des trois cases de `mSectorFlag` (secteur 1 en premier, ou secteur 3 comme `mSector`) ;
- le signe gauche / droite quand on projette une voiture dans le repère d'une autre ;
- `mTimeIntoLap` et `mEstimatedLapTime` dans les cas limites (stands, tour de sortie, voiture arrêtée) ;
- le comportement de `mFlag` pour le drapeau bleu ;
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

SIZE = {R_SIZE}
VEH, VEH_SIZE = {R_VEH}, {R_VEH_SIZE}          # voitures du classement
TEL, TEL_SIZE = {R_TEL}, {R_TEL_SIZE}       # voitures de la télémétrie

# Attention : si le jeu ne tourne pas, Python crée une zone vide (que des zéros) au lieu d'échouer.
zone = mmap.mmap(-1, SIZE, "LMU_Data")

def text(offset, length):
    return zone[offset:offset + length].split(b"\0")[0].decode("utf-8", "replace")

count = struct.unpack_from("<i", zone, {R_NUMVEH})[0]          # mNumVehicles
player = next((i for i in range(count) if zone[VEH + i * VEH_SIZE + {ISPLAYER}]), None)   # mIsPlayer
if player is not None:
    line = VEH + player * VEH_SIZE
    car_id = struct.unpack_from("<i", zone, line)[0]            # mID
    print(text(line + {R_DRIVER}, 32), "P%d" % zone[line + {PLACE}])         # mDriverName, mPlace
    active = zone[{R_ACTIVE}]                                       # activeVehicles
    for j in range(active):
        entry = TEL + j * TEL_SIZE
        if struct.unpack_from("<i", zone, entry)[0] == car_id:
            fuel = struct.unpack_from("<d", zone, entry + {FUEL})[0]        # mFuel
            vx, vy, vz = struct.unpack_from("<3d", zone, entry + {R_VEL})    # mLocalVel
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
