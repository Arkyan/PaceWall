"""Génère docs/memoire-partagee.md et crates/pacewall/src/ui/memory.tsv (page de l'application) : la référence complète de la mémoire partagée de Le Mans Ultimate.

Les décalages et les tailles sont calculés à partir de crates/lmu/src/data.rs (disposition « packed(4) »), puis
contrôlés contre les valeurs connues ; les descriptions sont dans ce fichier. Un champ ajouté à data.rs sans
description fait échouer la génération : la doc ne peut pas être incomplète.

    python tools/doc/memoire_partagee.py
"""

import io
import os
import re

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
SRC = os.path.join(ROOT, "crates", "lmu", "src", "data.rs")
OUT = os.path.join(ROOT, "docs", "memoire-partagee.md")
APP = os.path.join(ROOT, "crates", "pacewall", "src", "ui", "memory.tsv")

PRIM = {"f64": (8, "double"), "f32": (4, "float"), "i32": (4, "int"), "u32": (4, "unsigned int"), "i16": (2, "short"), "u16": (2, "unsigned short"), "i8": (1, "signed char"), "u8": (1, "unsigned char"), "u64": (8, "unsigned long long")}
CONSTS = {"MAX_MAPPED_VEHICLES": 104, "MAX_PATH_LENGTH": 260}
# Nom de la structure dans les en-têtes C++ du jeu.
CPP = {
    "Vect3": "TelemVect3", "Wheel": "TelemWheelV01", "VehicleTelemetry": "TelemInfoV01", "VehicleScoring": "VehicleScoringInfoV01",
    "ScoringInfo": "ScoringInfoV01", "ApplicationState": "ApplicationStateV01", "ScoringData": "SharedMemoryScoringData",
    "TelemetryData": "SharedMemoryTelemtryData", "PathData": "SharedMemoryPathData", "Event": "SharedMemoryEvent",
    "Generic": "SharedMemoryGeneric", "ObjectOut": "SharedMemoryObjectOut",
}
# Champs « unsigned char » qui sont des booléens dans l'en-tête, et tableaux d'octets qui sont du texte.
BOOLS = set("mFlat mDetached mOverheating mHeadlights mLapInvalidated mABSActive mTCActive mSpeedLimiterActive mIsPlayer mInPits mUnderYellow mInGarageStall mDRSState mInRealtime mIsPasswordProtected mIsFixedSetup playerHasVehicle".split())
TEXTS = set("mTerrainName mVehicleName mTrackName mFrontTireCompoundName mRearTireCompoundName mVehicleModel mDriverName mVehicleClass mPitGroup mVehFilename mPlayerName mPlrFileName mServerName mOptionsPage userData customVariables stewardResults playerProfile pluginsFolder scoringStream".split())


def parse():
    text = io.open(SRC, encoding="utf-8").read()
    structs = {}
    for m in re.finditer(r"pub struct (\w+) \{(.*?)\n\}", text, re.S):
        fields = re.findall(r"pub (\w+): ([^,\n]+),", m.group(2))
        structs[m.group(1)] = fields
    return structs


def layout(structs):
    """Taille, alignement et décalages de chaque structure, avec les règles de « #pragma pack(4) »."""
    done = {}

    def info(ty):
        ty = ty.strip()
        arr = re.match(r"\[(.+); (\w+)\]$", ty)
        if arr:
            size, align, _ = info(arr.group(1))
            n = CONSTS.get(arr.group(2)) or int(arr.group(2))
            return size * n, align, n
        if ty in PRIM:
            return PRIM[ty][0], min(PRIM[ty][0], 4), None
        if ty not in done:
            offset, align, rows = 0, 1, []
            for name, fty in structs[ty]:
                size, a, n = info(fty)
                offset = (offset + a - 1) // a * a
                rows.append((offset, name, fty.strip(), size))
                offset += size
                align = max(align, a)
            done[ty] = ((offset + align - 1) // align * align, align, rows)
        return done[ty][0], done[ty][1], None

    for s in structs:
        info(s)
    return done


def c_type(name, ty):
    arr = re.match(r"\[(.+); (\w+)\]$", ty)
    if arr:
        inner, n = arr.group(1), CONSTS.get(arr.group(2)) or arr.group(2)
        if inner == "u8":
            return f"char[{n}]" if name in TEXTS else f"octets[{n}]"
        return f"{PRIM[inner][1] if inner in PRIM else CPP.get(inner, inner)}[{n}]"
    if ty == "u8" and name in BOOLS:
        return "bool"
    return PRIM[ty][1] if ty in PRIM else CPP.get(ty, ty)


# ---------------------------------------------------------------------------------------------------------------------
# Descriptions : (unité ou valeurs, signification). « LMU » = champ ajouté par Le Mans Ultimate (absent de rFactor 2).
# ---------------------------------------------------------------------------------------------------------------------
D = {}
D["Vect3"] = {
    "x": ("", "Axe X : vers la **gauche** de la voiture (repère local) ou axe du monde."),
    "y": ("", "Axe Y : vers le **haut**."),
    "z": ("", "Axe Z : vers l'**arrière** de la voiture. Avancer = Z négatif."),
}
D["Wheel"] = {
    "mSuspensionDeflection": ("m", "Débattement de la suspension."),
    "mRideHeight": ("m", "Hauteur de caisse à cette roue."),
    "mSuspForce": ("N", "Effort dans le poussoir (pushrod)."),
    "mBrakeTemp": ("**°C**", "Température du disque de frein. Attention : en degrés Celsius, alors que les pneus sont en Kelvin."),
    "mBrakePressure": ("0 à 1", "Pression de frein, en fraction : dépend de la pédale et de la répartition. Ce n'est pas une pression réelle."),
    "mRotation": ("rad/s", "Vitesse de rotation de la roue. Vitesse de roue en m/s = `|mRotation|` × rayon."),
    "mLateralPatchVel": ("m/s", "Vitesse latérale de la gomme au point de contact."),
    "mLongitudinalPatchVel": ("m/s", "Vitesse longitudinale de la gomme au point de contact."),
    "mLateralGroundVel": ("m/s", "Vitesse latérale du sol sous la roue."),
    "mLongitudinalGroundVel": ("m/s", "Vitesse longitudinale du sol sous la roue. Glissement = écart entre la gomme (`Patch`) et le sol (`Ground`)."),
    "mCamber": ("rad", "Carrossage. Positif vers la gauche pour les roues gauches, vers la droite pour les roues droites."),
    "mLateralForce": ("N", "Effort latéral du pneu."),
    "mLongitudinalForce": ("N", "Effort longitudinal du pneu."),
    "mTireLoad": ("N", "Charge verticale sur le pneu."),
    "mGripFract": ("0 à 1", "Part approximative de la surface de contact qui glisse."),
    "mPressure": ("kPa", "Pression de l'air dans le pneu. 1 bar = 100 kPa, 1 psi = 6,895 kPa."),
    "mTemperature": ("**K**", "Température de surface : gauche / centre / droite du pneu (pas intérieur / extérieur). °C = K − 273,15."),
    "mWear": ("0 à 1", "Gomme **restante** : 1 = pneu neuf, descend avec l'usure. L'en-tête dit « wear, fraction of maximum », mais la valeur part de 1 (TinyPedal l'appelle « tread »). Pas proportionnel à la perte d'adhérence."),
    "mTerrainName": ("texte", "Nom du revêtement sous la roue (préfixe de matériau du fichier TDF du circuit)."),
    "mSurfaceType": ("code", "Surface sous la roue : 0 sec, 1 mouillé, 2 herbe, 3 terre, 4 gravier, 5 vibreur, 6 spécial."),
    "mFlat": ("0 / 1", "Pneu crevé."),
    "mDetached": ("0 / 1", "Roue arrachée."),
    "mStaticUndeflectedRadius": ("cm", "Rayon du pneu au repos, en **centimètres** entiers."),
    "mVerticalTireDeflection": ("m", "Écrasement du pneu par rapport à son rayon (qui dépend de la vitesse)."),
    "mWheelYLocation": ("m", "Hauteur de la roue par rapport à la hauteur de la voiture."),
    "mToe": ("rad", "Pincement / ouverture actuel, par rapport à la voiture."),
    "mTireCarcassTemperature": ("**K**", "Température moyenne de la carcasse. La plus proche de ce qu'affiche le jeu."),
    "mTireInnerLayerTemperature": ("**K**", "Température de la couche de gomme la plus profonde (avant la carcasse) : gauche / centre / droite."),
    "mOptimalTemp": ("°C", "**LMU.** Température idéale de la gomme montée."),
    "mCompoundIndex": ("index", "**LMU.** Rang de la gomme dans la liste des gommes disponibles pour cette voiture sur ce circuit."),
    "mCompoundType": ("code", "**LMU.** Famille de gomme : 0 tendre, 1 medium, 2 dure, 3 pluie."),
    "mExpansion": ("", "Réservé."),
}
D["VehicleTelemetry"] = {
    "mID": ("entier", "Identifiant de la voiture (« slot »). C'est la clé pour relier télémétrie et classement. Peut être réattribué en multijoueur quand quelqu'un part."),
    "mDeltaTime": ("s", "Temps écoulé depuis la mise à jour précédente de cette voiture."),
    "mElapsedTime": ("s", "Temps de session au moment de cette télémétrie. Sert à savoir de quand date la position (extrapolation)."),
    "mLapNumber": ("entier", "Numéro du tour en cours."),
    "mLapStartET": ("s", "Temps de session au début du tour en cours. Chrono du tour en cours = `mElapsedTime − mLapStartET`."),
    "mVehicleName": ("texte", "Nom de la voiture (engagement : numéro, équipe…)."),
    "mTrackName": ("texte", "Nom du circuit."),
    "mPos": ("m", "Position dans le monde. Vue de dessus : abscisse `x`, ordonnée `−z` ; `y` est l'altitude."),
    "mLocalVel": ("m/s", "Vitesse dans le repère de la voiture. Vitesse = norme du vecteur ; vitesse vers l'avant = `−z`."),
    "mLocalAccel": ("m/s²", "Accélération dans le repère de la voiture. En g : diviser par 9,81. Latéral = `x`, longitudinal = `z`."),
    "mOri": ("matrice", "Les trois lignes de la matrice d'orientation (repère voiture → monde). Cap = `atan2(mOri[2].x, mOri[2].z)`."),
    "mLocalRot": ("rad/s", "Vitesse de rotation dans le repère de la voiture (`y` = lacet)."),
    "mLocalRotAccel": ("rad/s²", "Accélération de rotation dans le repère de la voiture."),
    "mGear": ("code", "Rapport engagé : −1 marche arrière, 0 point mort, 1 et plus."),
    "mEngineRPM": ("tr/min", "Régime moteur."),
    "mEngineWaterTemp": ("°C", "Température d'eau."),
    "mEngineOilTemp": ("°C", "Température d'huile."),
    "mClutchRPM": ("tr/min", "Régime côté embrayage (boîte)."),
    "mUnfilteredThrottle": ("0 à 1", "Accélérateur, tel que le pilote l'enfonce."),
    "mUnfilteredBrake": ("0 à 1", "Frein, tel que le pilote l'enfonce."),
    "mUnfilteredSteering": ("−1 à 1", "Volant, de la gauche (−1) à la droite (+1)."),
    "mUnfilteredClutch": ("0 à 1", "Embrayage, tel que le pilote l'enfonce."),
    "mFilteredThrottle": ("0 à 1", "Accélérateur réellement appliqué (après antipatinage, limiteur, etc.)."),
    "mFilteredBrake": ("0 à 1", "Frein réellement appliqué (après ABS)."),
    "mFilteredSteering": ("−1 à 1", "Volant réellement appliqué."),
    "mFilteredClutch": ("0 à 1", "Embrayage réellement appliqué (après anti-calage, départ automatique)."),
    "mSteeringShaftTorque": ("N·m", "Couple dans la colonne de direction (la base du retour de force)."),
    "mFront3rdDeflection": ("m", "Débattement du troisième élément avant."),
    "mRear3rdDeflection": ("m", "Débattement du troisième élément arrière."),
    "mFrontWingHeight": ("m", "Hauteur de l'aileron avant."),
    "mFrontRideHeight": ("m", "Hauteur de caisse avant."),
    "mRearRideHeight": ("m", "Hauteur de caisse arrière."),
    "mDrag": ("N", "Traînée aérodynamique."),
    "mFrontDownforce": ("N", "Appui avant."),
    "mRearDownforce": ("N", "Appui arrière."),
    "mFuel": ("L", "Carburant dans le réservoir."),
    "mEngineMaxRPM": ("tr/min", "Régime du limiteur (rupteur)."),
    "mScheduledStops": ("entier", "Nombre d'arrêts aux stands programmés."),
    "mOverheating": ("0 / 1", "Le témoin de surchauffe est affiché."),
    "mDetached": ("0 / 1", "Une pièce (autre qu'une roue) s'est détachée. Dans LMU, en pratique l'aileron arrière."),
    "mHeadlights": ("0 / 1", "Phares allumés."),
    "mDentSeverity": ("0, 1, 2", "Enfoncement de la carrosserie en 8 points autour de la voiture : 0 intact, 1 abîmé, 2 très abîmé. Ordre des zones : voir « Dégâts » plus bas."),
    "mLastImpactET": ("s", "Temps de session du dernier choc."),
    "mLastImpactMagnitude": ("", "Force du dernier choc."),
    "mLastImpactPos": ("m", "Point d'impact du dernier choc, dans le repère de la voiture."),
    "mEngineTorque": ("N·m", "Couple moteur actuel."),
    "mCurrentSector": ("code", "Secteur en cours, **base 0** (0, 1, 2), avec la voie des stands dans le bit de signe : `0x80000002` = entrée aux stands depuis le 3e secteur. Secteur = `valeur & 0x7FFFFFFF`. Numérotation différente de `mSector` du classement."),
    "mSpeedLimiter": ("0 / 1", "Limiteur de vitesse enclenché."),
    "mMaxGears": ("entier", "Nombre de rapports avant."),
    "mFrontTireCompoundIndex": ("index", "Rang de la gomme avant dans la marque."),
    "mRearTireCompoundIndex": ("index", "Rang de la gomme arrière dans la marque."),
    "mFuelCapacity": ("L", "Capacité du réservoir."),
    "mFrontFlapActivated": ("0 / 1", "Volet avant actionné."),
    "mRearFlapActivated": ("0 / 1", "Volet arrière (DRS) actionné."),
    "mRearFlapLegalStatus": ("code", "DRS : 0 interdit, 1 conditions réunies mais pas encore autorisé, 2 autorisé."),
    "mIgnitionStarter": ("code", "0 coupé, 1 contact, 2 contact + démarreur."),
    "mFrontTireCompoundName": ("texte", "Nom de la gomme avant (par exemple « Medium »)."),
    "mRearTireCompoundName": ("texte", "Nom de la gomme arrière."),
    "mSpeedLimiterAvailable": ("0 / 1", "La voiture a un limiteur de vitesse."),
    "mAntiStallActivated": ("0 / 1", "L'anti-calage est intervenu."),
    "mUnused": ("", "Inutilisé."),
    "mVisualSteeringWheelRange": ("degrés", "Débattement **visuel** du volant affiché dans l'habitacle."),
    "mRearBrakeBias": ("0 à 1", "Part du freinage envoyée à l'arrière. Répartition avant = `1 − mRearBrakeBias`."),
    "mTurboBoostPressure": ("", "Pression de suralimentation, si la voiture a un turbo. Unité non précisée par l'en-tête."),
    "mPhysicsToGraphicsOffset": ("m", "Décalage entre le centre de gravité et le centre du modèle 3D."),
    "mPhysicalSteeringWheelRange": ("degrés", "Débattement **physique** du volant (butée à butée)."),
    "mDeltaBest": ("s", "**LMU.** Avance (négatif) ou retard (positif) sur le meilleur tour, calculé par le jeu."),
    "mBatteryChargeFraction": ("0 à 1", "Charge de la batterie hybride."),
    "mElectricBoostMotorTorque": ("N·m", "Couple du moteur électrique (négatif en régénération)."),
    "mElectricBoostMotorRPM": ("tr/min", "Régime du moteur électrique."),
    "mElectricBoostMotorTemperature": ("°C", "Température du moteur électrique."),
    "mElectricBoostWaterTemperature": ("°C", "Température d'eau du refroidissement du moteur électrique (0 s'il n'y en a pas)."),
    "mElectricBoostMotorState": ("code", "0 indisponible, 1 inactif, 2 propulsion, 3 régénération."),
    "mLapInvalidated": ("0 / 1", "**LMU.** Le tour en cours est invalidé (limites de piste)."),
    "mABSActive": ("0 / 1", "**LMU.** L'ABS est en train d'agir."),
    "mTCActive": ("0 / 1", "**LMU.** L'antipatinage est en train d'agir."),
    "mSpeedLimiterActive": ("0 / 1", "**LMU.** Le limiteur de vitesse est en train de brider la voiture."),
    "mWiperState": ("code", "**LMU.** Essuie-glace : 0 arrêt, 1 automatique, 2 lent, 3 rapide."),
    "mTC": ("cran", "**LMU.** Réglage d'antipatinage."),
    "mTCMax": ("cran", "**LMU.** Nombre de crans d'antipatinage."),
    "mTCSlip": ("cran", "**LMU.** Réglage du glissement toléré par l'antipatinage."),
    "mTCSlipMax": ("cran", "**LMU.** Nombre de crans de glissement."),
    "mTCCut": ("cran", "**LMU.** Réglage de la coupure de puissance de l'antipatinage."),
    "mTCCutMax": ("cran", "**LMU.** Nombre de crans de coupure."),
    "mABS": ("cran", "**LMU.** Réglage d'ABS."),
    "mABSMax": ("cran", "**LMU.** Nombre de crans d'ABS."),
    "mMotorMap": ("cran", "**LMU.** Cartographie moteur."),
    "mMotorMapMax": ("cran", "**LMU.** Nombre de cartographies."),
    "mMigration": ("cran", "**LMU.** Migration de freinage."),
    "mMigrationMax": ("cran", "**LMU.** Nombre de crans de migration."),
    "mFrontAntiSway": ("cran", "**LMU.** Barre antiroulis avant."),
    "mFrontAntiSwayMax": ("cran", "**LMU.** Nombre de crans de barre avant."),
    "mRearAntiSway": ("cran", "**LMU.** Barre antiroulis arrière."),
    "mRearAntiSwayMax": ("cran", "**LMU.** Nombre de crans de barre arrière."),
    "mLiftAndCoastProgress": ("0 à 255", "**LMU.** Progression du « lift and coast » (économie par relâché)."),
    "mTrackLimitsSteps": ("pas", "**LMU.** Limites de piste accumulées, en pas : points × `mTrackLimitsStepsPerPoint` de la session."),
    "mRegen": ("kW", "**LMU.** Puissance récupérée par le moteur électrique."),
    "mStateOfCharge": ("%", "**LMU.** Charge de la batterie. Plage (0-1 ou 0-100) non contrôlée sur le vrai jeu."),
    "mVirtualEnergy": ("fraction", "**LMU.** Énergie virtuelle restante. Plage (0-1 ou 0-100) non contrôlée sur le vrai jeu ; le maximum est dans le service web (`fuelInfo.maxVirtualEnergy`)."),
    "mTimeGapCarAhead": ("s", "**LMU.** Écart avec la voiture devant **sur la piste**."),
    "mTimeGapCarBehind": ("s", "**LMU.** Écart avec la voiture derrière sur la piste."),
    "mTimeGapPlaceAhead": ("s", "**LMU.** Écart avec la voiture classée juste devant."),
    "mTimeGapPlaceBehind": ("s", "**LMU.** Écart avec la voiture classée juste derrière."),
    "mVehicleModel": ("texte", "**LMU.** Marque et modèle (par exemple « Ferrari 499P »)."),
    "mVehicleClass": ("code", "**LMU.** Classe : 0 Hypercar, 2 LMP2 ELMS, 3 LMP2, 4 LMP3, 5 GTE, 6 GT3, 8 voiture de sécurité, 255 inconnue. Le classement donne la classe en texte."),
    "mVehicleChampionship": ("code", "**LMU.** Championnat : 0 à 3 = WEC 2023 à 2026, 16 et 17 = ELMS 2025 et 2026, 255 inconnu."),
    "mExpansion": ("", "Réservé."),
    "mWheels": ("", "Les quatre roues : 0 avant gauche, 1 avant droite, 2 arrière gauche, 3 arrière droite. Détail dans `TelemWheelV01`."),
}
D["VehicleScoring"] = {
    "mID": ("entier", "Identifiant de la voiture, le même que dans la télémétrie."),
    "mDriverName": ("texte", "Nom du pilote au volant."),
    "mVehicleName": ("texte", "Nom de la voiture (engagement)."),
    "mTotalLaps": ("entier", "Tours **terminés**. Tour en cours = `mTotalLaps + 1`."),
    "mSector": ("code", "Secteur en cours : **0 = secteur 3**, 1 = secteur 1, 2 = secteur 2."),
    "mFinishStatus": ("code", "0 en course, 1 arrivé, 2 abandon, 3 disqualifié."),
    "mLapDist": ("m", "Distance parcourue dans le tour, depuis la ligne. Peut être légèrement négative ou dépasser la longueur du tour."),
    "mPathLateral": ("m", "Position latérale par rapport à une trajectoire « centrale » très approximative."),
    "mTrackEdge": ("m", "Bord de piste du côté de la voiture, par rapport à cette même trajectoire."),
    "mBestSector1": ("s", "Meilleur secteur 1."),
    "mBestSector2": ("s", "Meilleur temps au bout du secteur 2 (**secteur 1 compris**)."),
    "mBestLapTime": ("s", "Meilleur tour. Zéro ou négatif = aucun."),
    "mLastSector1": ("s", "Secteur 1 du dernier tour."),
    "mLastSector2": ("s", "Temps au bout du secteur 2 du dernier tour (secteur 1 compris)."),
    "mLastLapTime": ("s", "Dernier tour. Zéro ou négatif = pas de temps (tour invalide, tour de sortie)."),
    "mCurSector1": ("s", "Secteur 1 du tour en cours, s'il est valide. Remis à zéro au passage de la ligne."),
    "mCurSector2": ("s", "Temps au bout du secteur 2 du tour en cours (secteur 1 compris), s'il est valide."),
    "mNumPitstops": ("entier", "Arrêts aux stands effectués."),
    "mNumPenalties": ("entier", "Pénalités restant à purger."),
    "mIsPlayer": ("0 / 1", "C'est la voiture du joueur. **Le moyen fiable de trouver le joueur.**"),
    "mControl": ("code", "Qui conduit : −1 personne, 0 joueur local, 1 IA locale, 2 joueur distant, 3 replay."),
    "mInPits": ("0 / 1", "Entre l'entrée et la sortie de la voie des stands. Pas toujours exact pour les voitures distantes."),
    "mPlace": ("entier", "Position au classement général, à partir de 1."),
    "mVehicleClass": ("texte", "Classe en clair (« Hypercar », « LMP2 », « LMGT3 »…)."),
    "mTimeBehindNext": ("s", "Retard sur la voiture classée juste devant."),
    "mLapsBehindNext": ("tours", "Tours de retard sur la voiture classée juste devant."),
    "mTimeBehindLeader": ("s", "Retard sur le leader."),
    "mLapsBehindLeader": ("tours", "Tours de retard sur le leader."),
    "mLapStartET": ("s", "Temps de session au début du tour en cours."),
    "mPos": ("m", "Position dans le monde. Moins fraîche que celle de la télémétrie (classement ≈ 5 fois par seconde)."),
    "mLocalVel": ("m/s", "Vitesse dans le repère de la voiture."),
    "mLocalAccel": ("m/s²", "Accélération dans le repère de la voiture."),
    "mOri": ("matrice", "Matrice d'orientation, comme en télémétrie."),
    "mLocalRot": ("rad/s", "Vitesse de rotation."),
    "mLocalRotAccel": ("rad/s²", "Accélération de rotation."),
    "mHeadlights": ("code", "État des phares."),
    "mPitState": ("code", "Arrêt au stand : 0 rien, 1 demandé, 2 entrée, 3 arrêté, 4 sortie."),
    "mServerScored": ("0 / 1", "La voiture est classée par le serveur."),
    "mIndividualPhase": ("code", "Phase de jeu propre à cette voiture : les valeurs de `mGamePhase`, plus 9 après la formation, 10 sous jaune, 11 sous bleu (inutilisé)."),
    "mQualification": ("entier", "Position en qualification, à partir de 1 ; −1 si aucune."),
    "mTimeIntoLap": ("s", "Temps **estimé** depuis le début du tour, d'après la position sur la piste. Base des écarts entre voitures."),
    "mEstimatedLapTime": ("s", "Temps au tour estimé, utilisé par le jeu pour `mTimeIntoLap` et les retards. Peut changer selon la voiture et les réglages."),
    "mPitGroup": ("texte", "Groupe de stand (le nom de l'équipe, sauf stand partagé)."),
    "mFlag": ("code", "Drapeau montré à cette voiture : 0 vert, 6 bleu. Aucune autre valeur."),
    "mUnderYellow": ("0 / 1", "A franchi la ligne sous neutralisation générale."),
    "mCountLapFlag": ("code", "0 ni le tour ni le temps ne comptent, 1 le tour compte mais pas le temps, 2 les deux comptent."),
    "mInGarageStall": ("0 / 1", "La voiture est dans son box."),
    "mUpgradePack": ("octets", "Options de la voiture, codées."),
    "mPitLapDist": ("m", "Emplacement du stand de la voiture, en distance dans le tour."),
    "mBestLapSector1": ("s", "Secteur 1 **du meilleur tour** (pas forcément le meilleur secteur 1)."),
    "mBestLapSector2": ("s", "Temps au bout du secteur 2 du meilleur tour (secteur 1 compris)."),
    "mSteamID": ("entier", "**LMU.** Identifiant Steam du pilote, s'il y en a un."),
    "mVehFilename": ("texte", "**LMU.** Nom du fichier `.veh` qui identifie la voiture."),
    "mAttackMode": ("code", "**LMU.** Mode attaque (Formule E, hérité)."),
    "mFuelFraction": ("0 à 255", "**LMU.** Carburant ou batterie restant : 0 = vide, 255 = plein. Disponible pour **toutes** les voitures."),
    "mDRSState": ("0 / 1", "**LMU.** DRS (volet arrière) ouvert."),
    "mExpansion": ("", "Réservé."),
}
D["ScoringInfo"] = {
    "mTrackName": ("texte", "Nom du circuit."),
    "mSession": ("code", "Type de session : 0 journée d'essais, 1 à 4 essais libres, 5 à 8 qualifications, 9 échauffement, 10 à 13 course."),
    "mCurrentET": ("s", "Temps de session actuel. Avance ≈ 5 fois par seconde ; ne bouge plus en pause."),
    "mEndET": ("s", "Temps de session auquel elle se termine."),
    "mMaxLaps": ("tours", "Nombre de tours de la session. Très grand nombre quand elle est limitée au temps."),
    "mLapDist": ("m", "Longueur du tour."),
    "mResultsStreamPointer": ("pointeur", "Pointeur interne au jeu, inutilisable de l'extérieur. Le texte est dans `scoringStream`."),
    "mNumVehicles": ("entier", "Nombre de voitures : les lignes 0 à `mNumVehicles − 1` de `vehScoringInfo` sont valides."),
    "mGamePhase": ("code", "0 avant la session, 1 tours de reconnaissance, 2 grille, 3 tour de formation, 4 feux de départ, 5 drapeau vert, 6 neutralisation (FCY / voiture de sécurité), 7 session arrêtée, 8 session terminée, 9 pause."),
    "mYellowFlagState": ("code", "Neutralisation générale seulement : −1 invalide, 0 aucune, 1 annoncée, 2 stands fermés, 3 stands ouverts au tour du leader, 4 stands ouverts, 5 dernier tour, 6 reprise, 7 course arrêtée."),
    "mSectorFlag": ("code", "Drapeau jaune local par secteur. **Jaune seulement si la valeur est 1** : en jeu, la valeur n'est pas 0 au repos. Ordre des trois cases non garanti par l'en-tête."),
    "mStartLight": ("entier", "Étape des feux de départ (le nombre d'étapes dépend du circuit)."),
    "mNumRedLights": ("entier", "Nombre de feux rouges de la procédure."),
    "mInRealtime": ("0 / 1", "Le joueur est en piste (et pas au moniteur du garage)."),
    "mPlayerName": ("texte", "Nom du joueur."),
    "mPlrFileName": ("texte", "Nom du fichier de profil (peut être transformé pour rester un nom de fichier valide)."),
    "mDarkCloud": ("0 à 1", "Noirceur des nuages."),
    "mRaining": ("0 à 1", "Intensité de la pluie."),
    "mAmbientTemp": ("°C", "Température de l'air."),
    "mTrackTemp": ("°C", "Température de la piste."),
    "mWind": ("m/s", "Vitesse du vent, dans le repère du monde."),
    "mMinPathWetness": ("0 à 1", "Humidité minimale sur la trajectoire."),
    "mMaxPathWetness": ("0 à 1", "Humidité maximale sur la trajectoire."),
    "mGameMode": ("code", "1 serveur, 2 client, 3 les deux."),
    "mIsPasswordProtected": ("0 / 1", "Serveur protégé par mot de passe."),
    "mServerPort": ("entier", "Port du serveur."),
    "mServerPublicIP": ("entier", "Adresse IP publique du serveur."),
    "mMaxPlayers": ("entier", "Nombre maximal de voitures dans la session."),
    "mServerName": ("texte", "Nom du serveur."),
    "mStartET": ("s", "Heure de début de l'épreuve, en secondes depuis minuit."),
    "mAvgPathWetness": ("0 à 1", "Humidité moyenne sur la trajectoire."),
    "mSessionTimeRemaining": ("s", "**LMU.** Temps de session restant."),
    "mTimeOfDay": ("s", "**LMU.** Heure dans le jeu : 0 à 86 400 secondes depuis minuit."),
    "mIsFixedSetup": ("0 / 1", "**LMU.** Réglages imposés."),
    "mTrackGripLevel": ("code", "**LMU.** Gomme déposée sur la piste : 0 verte, 1 faible, 2 moyenne, 3 forte, 4 saturée. La pluie la lave."),
    "mCloudCoverage": ("code", "**LMU.** Ciel : 0 dégagé, 1 quelques nuages, 2 partiellement nuageux, 3 très nuageux, 4 couvert, 5 bruine, 6 pluie légère (nuageux), 7 pluie légère (couvert), 8 pluie, 9 forte pluie, 10 orage."),
    "mTrackLimitsStepsPerPenalty": ("pas", "**LMU.** Nombre de pas de limites de piste qui déclenchent une pénalité."),
    "mTrackLimitsStepsPerPoint": ("pas", "**LMU.** Nombre de pas par point de limites de piste."),
    "mExpansion": ("", "Réservé."),
    "mVehiclePointer": ("pointeur", "Pointeur interne au jeu, inutilisable de l'extérieur. Les voitures sont dans `vehScoringInfo`."),
}
D["ApplicationState"] = {
    "mAppWindow": ("HWND", "Poignée de la fenêtre du jeu."),
    "mWidth": ("pixels", "Largeur de l'image."),
    "mHeight": ("pixels", "Hauteur de l'image."),
    "mRefreshRate": ("Hz", "Fréquence d'affichage."),
    "mWindowed": ("0 / 1", "Mode fenêtré."),
    "mOptionsLocation": ("code", "Où est le joueur : 0 menus, 1 chargement du circuit, 2 moniteur du garage, 3 en piste."),
    "mOptionsPage": ("texte", "Nom de la page d'options ouverte."),
    "mExpansion": ("", "Réservé."),
}
D["ScoringData"] = {
    "scoringInfo": ("", "La session : circuit, temps, météo, drapeaux."),
    "scoringStreamSize": ("octets", "Taille utile de `scoringStream` (un `size_t`, puis du remplissage)."),
    "vehScoringInfo": ("", "Une ligne par voiture. Seules les `mNumVehicles` premières sont valides. L'ordre n'est **pas** celui du classement : trier par `mPlace`."),
    "scoringStream": ("texte", "Flux de résultats du jeu : lignes de texte ajoutées depuis la dernière mise à jour, terminées par un zéro."),
}
D["TelemetryData"] = {
    "activeVehicles": ("entier", "Nombre d'entrées valides dans `telemInfo`."),
    "playerVehicleIdx": ("index", "Index de la voiture du joueur dans `telemInfo`. Préférer la recherche par `mIsPlayer` puis `mID`."),
    "playerHasVehicle": ("0 / 1", "Le joueur a une voiture (faux dans les menus ou en spectateur)."),
    "telemInfo": ("", "Une entrée par voiture, **adversaires compris**. L'index n'est pas celui du classement : associer par `mID`."),
}
D["PathData"] = {
    "userData": ("texte", "Dossier des données utilisateur du jeu (`UserData`)."),
    "customVariables": ("texte", "Fichier des variables personnalisées des plugins."),
    "stewardResults": ("texte", "Dossier des résultats de session (fichiers XML)."),
    "playerProfile": ("texte", "Dossier du profil du joueur."),
    "pluginsFolder": ("texte", "Dossier des plugins."),
}
EVENTS = {
    "SME_ENTER": "Entrée dans le jeu.", "SME_EXIT": "Sortie du jeu.", "SME_STARTUP": "Démarrage du moteur de jeu.", "SME_SHUTDOWN": "Arrêt du moteur de jeu.",
    "SME_LOAD": "Chargement d'un circuit.", "SME_UNLOAD": "Déchargement du circuit.", "SME_START_SESSION": "Début d'une session.", "SME_END_SESSION": "Fin d'une session.",
    "SME_ENTER_REALTIME": "Le joueur prend le volant.", "SME_EXIT_REALTIME": "Le joueur revient au garage.", "SME_UPDATE_SCORING": "Le classement vient d'être publié.",
    "SME_UPDATE_TELEMETRY": "La télémétrie vient d'être publiée.", "SME_INIT_APPLICATION": "Initialisation de l'application.", "SME_UNINIT_APPLICATION": "Fermeture de l'application.",
    "SME_SET_ENVIRONMENT": "Les chemins (`paths`) ont été renseignés.", "SME_FFB": "Le couple du retour de force a été mis à jour.",
}
D["Event"] = {k: ("indicateur", v) for k, v in EVENTS.items()}
D["Generic"] = {
    "events": ("", "Un indicateur par type d'événement. Voir la mise en garde sur leur comportement réel."),
    "gameVersion": ("entier", "Version du jeu (relevé réel : 14200)."),
    "FFBTorque": ("", "Couple du retour de force envoyé au volant."),
    "appInfo": ("", "Fenêtre du jeu et écran en cours."),
}
D["ObjectOut"] = {
    "generic": ("", "Événements, version du jeu, retour de force, fenêtre."),
    "paths": ("", "Dossiers du jeu."),
    "scoring": ("", "Session, météo et classement de toutes les voitures."),
    "telemetry": ("", "Télémétrie détaillée de toutes les voitures."),
}

TITLES = [
    ("ObjectOut", "La zone entière", "Les quatre blocs, dans l'ordre. Décalages depuis le début de la zone."),
    ("Generic", "Bloc `generic`", "Décalage 0."),
    ("Event", "Événements", "Seize entiers de 4 octets, dans `generic.events`."),
    ("ApplicationState", "Fenêtre du jeu", "Dans `generic.appInfo`."),
    ("PathData", "Bloc `paths`", "Cinq chemins de 260 caractères."),
    ("ScoringData", "Bloc `scoring`", "Décalages depuis le début du bloc."),
    ("ScoringInfo", "La session", "Une seule, au début du bloc `scoring`."),
    ("VehicleScoring", "Une voiture au classement", "104 emplacements à la suite. Décalages depuis le début de l'emplacement."),
    ("TelemetryData", "Bloc `telemetry`", "Décalages depuis le début du bloc."),
    ("VehicleTelemetry", "La télémétrie d'une voiture", "104 emplacements à la suite. Décalages depuis le début de l'emplacement."),
    ("Wheel", "Une roue", "Quatre à la fin de chaque télémétrie. Décalages depuis le début de la roue."),
    ("Vect3", "Un vecteur", "Trois `double`, soit 24 octets."),
]


def table(name, lay):
    size, _, rows = lay[name]
    out = [f"### {dict((t[0], t[1]) for t in TITLES)[name]} — `{CPP[name]}` ({size:,} octets)".replace(",", " "), "", dict((t[0], t[2]) for t in TITLES)[name], ""]
    out += ["| Décalage | Champ | Type | Octets | Unité / valeurs | Signification |", "|---:|---|---|---:|---|---|"]
    for offset, field, ty, fsize in rows:
        assert field in D[name], f"description manquante : {name}.{field}"
        unit, desc = D[name][field]
        out.append(f"| {offset} | `{field}` | `{c_type(field, ty)}` | {fsize} | {unit} | {desc} |")
    extra = set(D[name]) - {r[1] for r in rows}
    assert not extra, f"descriptions sans champ : {name} {extra}"
    return "\n".join(out) + "\n"


def main():
    structs = parse()
    lay = layout(structs)
    # Garde-fou : mêmes valeurs que les tests de crates/lmu (mesurées avec ctypes sur la bibliothèque de référence).
    for name, size in {"Vect3": 24, "Wheel": 260, "VehicleTelemetry": 1888, "VehicleScoring": 584, "ScoringInfo": 548, "ApplicationState": 260, "ScoringData": 126832, "TelemetryData": 196356, "PathData": 1300, "Event": 64, "Generic": 332, "ObjectOut": 324820}.items():
        assert lay[name][0] == size, (name, lay[name][0], size)
    off = lambda s, f: next(r[0] for r in lay[s][2] if r[1] == f)
    for (s, f), want in {("VehicleTelemetry", "mGear"): 352, ("VehicleTelemetry", "mFuel"): 524, ("VehicleTelemetry", "mDeltaBest"): 696, ("VehicleTelemetry", "mVirtualEnergy"): 776, ("VehicleTelemetry", "mWheels"): 848, ("ObjectOut", "scoring"): 1632, ("ObjectOut", "telemetry"): 128464}.items():
        assert off(s, f) == want, (s, f, off(s, f))

    scoring, telemetry = off("ObjectOut", "scoring"), off("ObjectOut", "telemetry")
    veh, tel = scoring + off("ScoringData", "vehScoringInfo"), telemetry + off("TelemetryData", "telemInfo")
    wheels = off("VehicleTelemetry", "mWheels")
    total = sum(len(lay[t[0]][2]) for t in TITLES)
    num = lambda n: f"{n:,}".replace(",", " ")
    intro = io.open(os.path.join(os.path.dirname(__file__), "memoire_partagee_intro.md"), encoding="utf-8").read()
    outro = io.open(os.path.join(os.path.dirname(__file__), "memoire_partagee_fin.md"), encoding="utf-8").read()
    values = {
        "SIZE": num(lay["ObjectOut"][0]), "FIELDS": str(total), "SCORING": num(scoring), "TELEMETRY": num(telemetry), "VEH": num(veh), "TEL": num(tel),
        "VEH_SIZE": num(lay["VehicleScoring"][0]), "TEL_SIZE": num(lay["VehicleTelemetry"][0]), "WHEELS": num(wheels), "WHEEL_SIZE": num(lay["Wheel"][0]),
        "STREAM": num(scoring + off("ScoringData", "scoringStream")), "PATHS": num(off("ObjectOut", "paths")),
        "EX_FUEL": num(tel + off("VehicleTelemetry", "mFuel")), "FUEL": str(off("VehicleTelemetry", "mFuel")),
        "EX_PLACE": num(veh + 3 * lay["VehicleScoring"][0] + off("VehicleScoring", "mPlace")), "PLACE": str(off("VehicleScoring", "mPlace")),
        "ISPLAYER": str(off("VehicleScoring", "mIsPlayer")), "NUMVEH": num(scoring + off("ScoringInfo", "mNumVehicles")), "ACTIVE": num(telemetry),
        "R_SIZE": str(lay["ObjectOut"][0]), "R_VEH": str(veh), "R_VEH_SIZE": str(lay["VehicleScoring"][0]), "R_TEL": str(tel), "R_TEL_SIZE": str(lay["VehicleTelemetry"][0]),
        "R_NUMVEH": str(scoring + off("ScoringInfo", "mNumVehicles")), "R_ACTIVE": str(telemetry), "R_DRIVER": str(off("VehicleScoring", "mDriverName")), "R_VEL": str(off("VehicleTelemetry", "mLocalVel")),
        "CURRENT_ET": num(scoring + off("ScoringInfo", "mCurrentET")), "ELAPSED": str(off("VehicleTelemetry", "mElapsedTime")),
    }
    for k, v in values.items():
        intro, outro = intro.replace("{" + k + "}", v), outro.replace("{" + k + "}", v)
    assert "{" not in re.sub(r"`[^`]*`|```.*?```", "", intro + outro, flags=re.S), "variable non remplacée"
    body = "\n".join(table(t[0], lay) for t in TITLES)
    io.open(OUT, "w", encoding="utf-8", newline="\n").write(intro.rstrip() + "\n\n" + body + "\n" + outro.rstrip() + "\n")
    # Les mêmes tableaux pour la page « Mémoire du jeu » de l'application : une ligne S par structure, F par champ.
    plain = lambda s: s.replace("**", "").replace("`", "")
    lines = []
    for name, title, note in TITLES:
        lines.append("\t".join(["S", name, CPP[name], str(lay[name][0]), plain(title), plain(note)]))
        for offset, field, ty, fsize in lay[name][2]:
            unit, desc = D[name][field]
            lines.append("\t".join(["F", str(offset), field, c_type(field, ty), str(fsize), plain(unit), plain(desc).replace("LMU. ", "", 1), "1" if desc.startswith("**LMU.**") else "0"]))
    io.open(APP, "w", encoding="utf-8", newline="\n").write("\n".join(lines) + "\n")
    print(f"{total} champs")


main()
