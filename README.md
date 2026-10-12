# PaceWall

Overlays de course pour **Le Mans Ultimate** : carburant et énergie, pneus, chrono, pilotage, classement, météo, stratégie
et historique des sessions. Une application Windows **native et légère** ; le web ne sert que pour OBS.

## Objectifs

- Des overlays visibles par le pilote, par-dessus le jeu.
- Les mêmes overlays dans OBS, sans capturer le jeu (source navigateur).
- Consommer le moins possible en course, seul ou avec un stream.

## Architecture : trois programmes en un seul exécutable

| Mode | Lancement | Rôle | Mesuré (release) |
|---|---|---|---|
| **Processus de fond** | `pacewall.exe` (ou au démarrage de Windows) | Icône de la zone de notification, raccourci Ctrl+Alt+O, surveillance du jeu, enregistrement de l'historique, serveur OBS à la demande. Aucune interface chargée. | 0,0 % CPU · 12 Mo |
| **Overlay joueur** | lancée par le processus de fond quand le jeu démarre, fermée quand il s'arrête | Un widget = une petite fenêtre Windows transparente, insensible aux clics, dessinée par le processeur (tiny-skia). Pas de GPU, pas de navigateur. Lit le jeu directement. | ≈ 2 % CPU · 17 Mo |
| **Fenêtre principale** | `pacewall.exe --ui` (clic sur l'icône) | Accueil, éditeur d'overlays, données en direct, historique, réglages (egui). Fermée, elle libère toute sa mémoire. | 0,1 % CPU · 90 Mo |
| **Serveur OBS** | activé dans Réglages | Page pour une source navigateur OBS : les widgets y arrivent déjà dessinés, en images. Désactivé, il ne consomme rien. | — |

À titre de comparaison, la première version (overlay dans un navigateur embarqué WebView2) pesait environ 380 Mo et 14 % de CPU.

```
LMU ──► pwcore::live (mémoire partagée) ──┬─► overlay native (--overlay)
                                          ├─► fenêtre principale (--ui)
                                          └─► serveur OBS (WebSocket : images des widgets)
```

## Installation

`pacewall.exe` s'installe lui-même, sans installateur externe et sans droits administrateur :
- au premier lancement, un **configurateur** en sept étapes prépare le jeu, fait choisir les widgets (une installation neuve
  n'en affiche aucun), les unités et la voix, présente OBS, et propose d'installer ;
- ou Réglages > Application > **Installer PaceWall** ; en ligne de commande : `pacewall.exe --install [--desktop] [--autostart]` ;
- l'installation copie le programme dans `%LOCALAPPDATA%\Programs\PaceWall`, crée un raccourci du menu Démarrer (et du bureau
  si demandé), peut lancer PaceWall avec Windows, et l'ajoute à « Applications installées » ;
- la désinstallation (Réglages, « Applications installées » ou `pacewall.exe --uninstall [--purge]`) retire tout ; `--purge`
  supprime aussi l'historique, les mises en page et les réglages.

## Versions et mises à jour

- **Fichier d'installation** : `PaceWall-Setup-x.y.z.exe` est le programme lui-même sous un autre nom. Lancé ainsi, il ouvre
  une fenêtre d'installation : première installation, mise à jour d'une version en place (réglages et données conservés),
  ou « Essayer sans installer ».
- **Numéro de version** : un seul pour tout le projet, dans `Cargo.toml` (`[workspace.package]`). Il est inscrit dans le
  programme (propriétés du fichier, « Applications installées », Réglages) avec la révision du code.
- **Mises à jour** : PaceWall interroge le dépôt GitHub des versions (constante `REPO` de `crates/pacewall/src/update.rs`)
  au démarrage puis deux fois par jour, annonce une nouvelle version sur l'accueil et dans le menu de l'icône, et l'installe
  en un clic. Le fichier téléchargé n'est lancé que si son empreinte SHA-256 est celle annoncée. Réglage `update_check`.
- **Publier une version** : décrire la version dans `CHANGELOG.md` (section `## x.y.z`), puis
  `powershell -ExecutionPolicy Bypass -File tools\release.ps1 -Version x.y.z`. Le script teste, compile, fabrique
  `dist\PaceWall-Setup-x.y.z.exe` et `dist\latest.json`, commite, pose l'étiquette `vx.y.z`, envoie le tout et crée la version sur GitHub avec ses deux fichiers.
  Il utilise l'identifiant GitHub que git a déjà sur l'ordinateur ; `-NoPublish` s'arrête avant l'envoi, `-NoGit` ne fait
  qu'un essai local.
- Le programme n'est pas signé : au premier lancement, Windows SmartScreen affiche « Windows a protégé votre ordinateur »
  (Informations complémentaires > Exécuter quand même). Seul un certificat de signature payant supprime cet écran.

## Utilisation

1. Lancer `pacewall.exe` : l'icône apparaît dans la zone de notification, clic gauche pour ouvrir la fenêtre.
2. Dans Le Mans Ultimate : Réglages > Gameplay > **Enable Plugins**, et jeu en **fenêtré ou sans bordure**.
3. L'overlay s'ouvre toute seule avec le jeu. **Ctrl+Alt+O** l'affiche ou la masque.
4. **Overlays** : place les widgets (glisser-déposer), règle taille, opacité et options. Deux mises en page : *Joueur* et *OBS*.
5. **OBS** : activer le serveur dans Réglages, puis source navigateur 1920×1080 sur `http://127.0.0.1:8470/overlay/obs`
   (`?only=fuel,tyres` pour un widget par source).

## Widgets

Carburant et énergie (tours restants, conso, quantité à ajouter), pneus (température par rapport à l'optimum, usure, pression),
chrono et écarts, pilotage (rapport, vitesse, pédales, **graphe des inputs**), classement (devant/derrière, couleurs de classe),
session et météo, **carte du circuit**. Ils ne sont écrits qu'une fois, en Rust (`crates/pacewall/src/widgets.rs`) : l'overlay du joueur, l'aperçu de l'éditeur et la page d'OBS utilisent ce même dessin. Pour OBS, le serveur dessine chaque widget (`crates/pacewall/src/obs_painter.rs`) et envoie l'image à la page, qui la pose à sa place : le rendu est identique par construction. Une image n'est renvoyée que si elle a changé ; la cadence est plafonnée par `obs_fps` (30 par défaut). Options de la page : `?only=fuel,tyres`, `?demo=1`.

Aussi : **relatif** (voitures autour de soi sur la piste, écart en secondes, rouge = il nous a pris un tour, bleu = on lui en a pris un), **arrêt au stand** (place et trafic à la sortie, d'après un temps perdu réglable), **radar de proximité** (voitures à côté, alerte de côté rouge ou orange) et **objectif de consommation** dans le widget carburant (consommation à tenir pour boucler un tour de plus).

## Prévisions météo

Le widget **Prévisions météo** affiche les cinq étapes de la session (début, 25 %, 50 %, 75 %, fin) : ciel, risque de pluie, température et délai avant chaque étape. Ces prévisions ne sont pas dans la mémoire partagée : elles viennent du service web local du jeu (`http://127.0.0.1:6397/rest/sessions/weather`, le même que lit TinyPedal ; port réglable par `lmu_api_port`). PaceWall ne l'interroge que si le widget ou l'alerte « pluie prévue » sert, une fois toutes les 30 secondes. Pour essayer sans le jeu : `PACEWALL_FORECAST_FILE=<fichier json>`.

## État de la voiture et durée d'arrêt

Le même service web du jeu donne la **durée de l'arrêt au stand** (`/rest/strategy/pitstop-estimate` : carburant, pneus, réparations) et l'**état de la voiture** (`/rest/garage/UIScreen/RepairAndRefuel` : dégâts aérodynamiques, suspensions, épaisseur des freins). Le widget **Arrêt au stand** s'en sert pour calculer la place de sortie (arrêt estimé par le jeu + traversée de la voie des stands, réglable) ; le widget **État de la voiture** montre la carrosserie par zones (mémoire partagée), les roues et suspensions, l'aérodynamique restante, le temps de réparation et les freins. Interrogation toutes les 2 secondes, seulement si un de ces widgets est affiché.

## Réglages communs

Onglet Réglages : **unités** (km/h ou mph, °C ou °F, kPa / bar / psi, litres ou gallons), **seuils de couleur** (carburant, état et température des pneus), **couleur d'accent** de la fenêtre, et **raccourcis** : une touche (« Ctrl+Alt+O ») ou un bouton de volant (« Joy1:B7 ») pour afficher l'overlay, couper la voix, ou masquer et réafficher un type de widget en roulant. Les touches sont des raccourcis globaux de Windows ; les boutons sont lus vingt fois par seconde, seulement s'ils servent.

## Carte du circuit

Inspirée de TinyPedal. Au premier tour propre sur un circuit, PaceWall enregistre la trajectoire et la garde dans
`%APPDATA%\PaceWall\tracks\<circuit>.json` ; elle est réutilisée ensuite, y compris par OBS (`/api/track/<circuit>`).

- Un tour est enregistré s'il commence à la ligne, ne passe pas par les stands, ne contient aucune téléportation
  (saut de plus de 100 m) et se termine par un tour **valide** (temps du dernier tour positif).
- Zone fixe : tu choisis la largeur et la hauteur de la carte dans l'éditeur ; le circuit s'y adapte et se tourne tout seul
  (orientation « Auto ») pour y être le plus grand possible. Une orientation fixe (0°, 90°, 180°, 270°) reste possible.
- Repère : (`mPos.x`, `-mPos.z`).
- Tant qu'il n'y a pas de carte, le widget affiche un cercle avec les voitures placées selon leur distance dans le tour.
- Voitures : carré à la couleur de leur classe, ma voiture en cyan, voitures aux stands grisées.

## Alertes vocales

La voix intégrée à Windows (hors ligne, voix française si elle est installée) prévient pendant que tu roules, uniquement
en piste : carburant et énergie (5, 3, 2 puis 1 tour restant), pneus (froids, trop chauds, usés, crevaison), drapeau jaune,
full course yellow et reprise, pluie. Chaque alerte a un délai de relance. Réglages > Alertes vocales : cases, volume, essai
de la voix (`pacewall.exe --say "texte"` en ligne de commande). Problèmes de voix : `%TEMP%\pw-speech.log`.

### Voix de l'ingénieur et crédits

Les alertes sont dites par des clips pré-enregistrés (voix d'homme par défaut, voix de femme au choix, ou voix de synthèse
Windows), fabriqués avec Piper (`tools/voice/generate.py`) puis passés à un effet radio. Crédits des voix :
- homme : fr_FR-upmc-medium, locuteur « pierre », données UPMC (Université Pierre et Marie Curie), licence
  [CC BY-SA 4.0](https://creativecommons.org/licenses/by-sa/4.0/) : les clips en dérivent et se repartagent sous la même licence ;
- femme : fr_FR-siwis-medium, données SIWIS (Université d'Édimbourg), licence [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/).

## Replays de course

Pendant une session, l'enregistreur note deux fois par seconde la position et le classement de **toutes** les voitures (fichier `replays\<session>.pwr` à côté de la base, compressé par blocs d'une minute : quelques Mo pour deux heures). Aucun coût en dehors de cet échantillon et d'une écriture par minute.

Historique > une session > **Revoir la course** : la carte, le classement, la session et le chrono sont rejoués (lecture, pause, ×1 à ×32, barre de temps) ; les voitures glissent entre deux images. Réglages `replay_record` (true) et `replay_keep` (15 replays gardés, les plus anciens sont supprimés).

## Simulation sans le jeu

Onglet Alertes > Simulation (ou `pacewall.exe --simulate`) : un faux Le Mans Ultimate pour tout essayer.
- **Scénario d'alertes** (90 s) : déclenche chaque alerte vocale.
- **Course réaliste**, **qualifications**, **essais libres** (`--session race|quali|practice [--speed N] [--laps N]`) : un
  circuit fictif de 5,9 km, 32 voitures sur trois classes (Hypercar, LMP2, LMGT3) avec leur rythme et leur régularité,
  tours chronométrés, consommation, usure et température des pneus, arrêts aux stands, jaunes locaux, full course yellow,
  drapeaux bleus, pluie. Le temps de jeu est accéléré (jusqu'à 8×) pour qu'une course tienne en quelques minutes ;
  `cargo test -p fakegame report -- --ignored --nocapture` imprime le bilan d'une course complète.
- ⚠️ Les données sont écrites d'après mes hypothèses sur le jeu : la simulation ne valide pas la lecture du vrai LMU.

## Développement sans le jeu

```bash
cargo run -p fakegame                 # faux LMU : crée LMU_Data avec une course simulée (FAKE_LAP_SECONDS=8 pour des tours courts)
cargo run -p pacewall                 # processus de fond : l'overlay s'ouvre quand fakegame tourne
cargo run -p pacewall -- --ui         # fenêtre principale (--ui overlays | data | history | settings)
cargo run -p pacewall -- --overlay    # overlay seule
cargo run -p server                   # serveur OBS seul (http://127.0.0.1:8470)
cargo test -p lmu                     # la disposition des structures correspond à celle du jeu
cargo build -p pacewall --release     # target/release/pacewall.exe (≈ 7 Mo)
```

Variables utiles : `PACEWALL_DB` (dossier des fichiers de données, ex. `.dev/history.db`), `PACEWALL_PORT`,
`PACEWALL_ONLY=inputs,fuel` (overlay : limiter les widgets), `PACEWALL_DEBUG=1` (overlay : compteurs et durées dans `%TEMP%\pw-overlay.log`).

## Organisation du dépôt

| Crate | Contenu |
|---|---|
| `lmu` | Structures de la mémoire partagée de LMU (`packed(4)`, tailles et décalages vérifiés par test) et lecteur partiel |
| `pwcore` | Lecture du jeu (`live`), clés de données (`flatten`), stratégie, historique SQLite, réglages, mises en page, données de démo |
| `pacewall` | L'application : processus de fond, overlay, fenêtre principale, widgets natifs, rendu logiciel |
| `server` | Serveur web pour OBS (axum) : envoie à la page les widgets dessinés par l'application |
| `fakegame` | Faux jeu pour développer sans LMU |
| `collector` | Petit outil console qui affiche les valeurs lues |

## Données

- Mémoire partagée du jeu, champ par champ : [docs/memoire-partagee.md](docs/memoire-partagee.md).
- Sources, champs et pièges : [docs/donnees.md](docs/donnees.md).
- Fichiers dans `%APPDATA%\PaceWall\` : `history.db` (tours), `layouts.json` (mises en page), `settings.json` (réglages).
- **Stratégie** (`strategy.*`) : consommation moyenne des 3 derniers tours propres, tours restants, quantité à ajouter.
  Un tour avec arrêt aux stands ou incomplet n'est pas compté.
- **Historique** : un tour est enregistré quand le nombre de tours terminés augmente (consommations hors tours avec arrêt).

## Optimisations notables

- Processus séparés : rien n'est chargé quand rien n'est affiché.
- Overlay sans GPU : transparence et clics traversants via des fenêtres « en couches » ; le coût est proportionnel à la surface des widgets.
- Chaque widget a sa cadence ; il n'est redessiné qu'avec de nouvelles données (sauf le graphe, qui avance avec le temps).
- Cache d'images de glyphes, aplats écrits directement en mémoire, polices système (rien d'embarqué).
- Le jeu n'est lu que si quelqu'un en a besoin ; la zone mémoire n'est jamais copiée en entier.

## À faire

- Alertes vocales (carburant, pneus, météo).
- Comparaison de deux tours, import des fichiers de résultats.
- Valider avec le vrai jeu les points marqués ⚠️ dans `docs/donnees.md` (plage de l'énergie virtuelle, API REST, lecture cohérente).
