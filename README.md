<p align="center">
  <img src="assets/logo/polyptyque-512.png" alt="Logo de Polyptyque" width="112">
</p>

<h1 align="center">Polyptyque</h1>

<p align="center">
  <strong>Enregistrez plusieurs fenêtres en même temps, chacune dans son propre fichier, avec son propre son.</strong><br>
  Pour le montage multi-angle et l'archivage, sur des sessions qui peuvent durer douze heures.
</p>

<p align="center">
  <img src="docs/captures/interface-enregistrement.png" alt="Polyptyque pendant un enregistrement : trois sources, chacune avec son aperçu, son niveau sonore et la taille de son fichier" width="860">
</p>

---

## Sommaire

- [Pourquoi Polyptyque](#pourquoi-polyptyque)
- [Ses atouts](#ses-atouts)
- [Léger par conception](#léger-par-conception)
- [Installation](#installation)
- [Prise en main](#prise-en-main)
- [Pour le montage](#pour-le-montage)
- [Limites connues](#limites-connues)
- [Pour les développeurs](#pour-les-développeurs)
- [Licence](#licence)

## Pourquoi Polyptyque

Les logiciels d'enregistrement classiques composent une seule image à partir de plusieurs sources, et l'enregistrent dans **un seul fichier** avec **un seul mélange sonore**. Pour monter ensuite chaque source séparément, il faut des extensions fragiles, et le son reste mélangé : deux fenêtres de navigateur jouent dans le même programme, donc dans la même piste.

Polyptyque fait l'inverse : **chaque fenêtre est enregistrée à part**, dans son propre fichier, avec **uniquement son propre son**. Trois fenêtres donnent trois fichiers, calés sur la même horloge, prêts à être superposés dans un logiciel de montage.

## Ses atouts

**Le son de chaque fenêtre, et rien d'autre**
- Chaque source navigateur a son propre profil : le son d'un onglet de Firefox ne se mélange pas avec celui d'un autre Firefox, ni avec Chrome ou Discord.
- Le son d'une application (Discord, un jeu, Tor Browser…) est capté à la source, sans le reste du système.
- Micro en option, dans un fichier séparé.

**Des fichiers prêts pour le montage**
- Un fichier par source, toutes sur une horloge commune : l'image et le son restent alignés d'un fichier à l'autre.
- Cadence d'images constante, attendue par les logiciels de montage, même quand une fenêtre ne bouge pas.
- Conversion en MP4 sans perte de qualité, pour Premiere Pro et les autres logiciels qui gèrent mal le MKV.

**Pensé pour les longues sessions**
- Testé sur 14 h 15 d'enregistrement continu avec trois sources : aucune coupure, fichiers calés à quelques millisecondes près, mémoire stable.
- Fichiers écrits au fil de l'eau : un plantage ne fait perdre qu'environ une seconde, et un fichier coupé net (panne de courant) reste lisible.
- Espace disque surveillé, avec arrêt propre s'il vient à manquer.
- Mise en veille automatique empêchée. Une mise en veille demandée (capot fermé, bouton) ne peut pas l'être : l'enregistrement est alors arrêté proprement, et sa reprise proposée au réveil.
- Si Windows s'éteint ou se déconnecte pendant un enregistrement, il le signale et laisse annuler ; sinon, les fichiers sont finalisés avant la fermeture.

**Résistant aux incidents**
- Après un plantage ou une coupure de courant, Polyptyque fait le bilan des fichiers au lancement suivant, propose de réparer ceux qui ont été coupés net (sans perte de qualité) et de reprendre l'enregistrement.
- Si l'encodeur s'arrête ou se fige, Polyptyque le relance seul, dans un nouveau fichier, sans interrompre les autres sources.
- Une fenêtre fermée ou plantée donne une image noire, puis est rattachée automatiquement dès qu'elle revient. Un navigateur fermé par erreur est rouvert tout seul ; une fenêtre réduite revient d'un clic, même si son écran a été débranché.
- Disque externe débranché en cours de route : la suite est enregistrée dans `Vidéos\Polyptyque`.
- Réglages protégés : version précédente et sauvegarde quotidienne, restaurées automatiquement si le fichier est abîmé.

**Il retrouve vos fenêtres**
- Les navigateurs sont reconnus par leur profil, pas par leur titre qui change à chaque onglet.
- Au lancement, Polyptyque ouvre les fenêtres de la scène en cours (désactivable dans les réglages). Le bouton « Ouvrir les fenêtres » les ouvre à la demande, celui de chaque carte une seule.
- Au démarrage d'un enregistrement, Polyptyque rouvre les fenêtres absentes : chaque navigateur revient sur ses onglets, connexions conservées.

**Simple à utiliser**
- Scènes (groupes de sources), aperçus en direct de chaque source, raccourcis clavier globaux.
- Un bouton pour tout enregistrer, ou un bouton par source pour enregistrer une source seule.
- Encodage par la carte graphique (NVIDIA, AMD, Intel) quand elle est disponible, ou par le processeur. Si la carte refuse un encodage de plus (limite des cartes GeForce, autres logiciels compris), la source passe seule sur le processeur.

## Léger par conception

Polyptyque doit tourner pendant des heures à côté d'un jeu, d'un appel vidéo ou de plusieurs navigateurs, sans les ralentir.

- **Programme natif**, écrit en [Rust](https://www.rust-lang.org) : pas de navigateur embarqué ni de moteur web pour l'interface. L'interface ([Slint](https://slint.dev)) est dessinée directement.
- **La carte graphique fait le gros du travail** :
  - la capture des fenêtres ;
  - la mise à l'échelle et la conversion des couleurs ;
  - l'encodage vidéo (NVENC, AMF, Quick Sync).
- **Rien n'est recalculé inutilement** : une image n'est traitée que si la fenêtre a changé, et une image répétée ne coûte aucune copie en mémoire.
- **Aperçus mesurés** : environ dix images par seconde, une définition adaptée à leur taille à l'écran, et un raccourci pour les couper.
- **Un encodeur par source** : chacun travaille de son côté, et une panne reste limitée à sa source.

## Installation

**Configuration requise**
- Windows 10 (version 2004 ou plus récente) ou Windows 11, 64 bits.
- Carte NVIDIA : pilote 570 ou plus récent. Les cartes AMD et Intel, ou l'encodage par le processeur, fonctionnent aussi.

**Installer**
1. Télécharger `Polyptyque-Setup.exe` depuis la page des [versions](https://github.com/venaciteam/Polyptyque/releases).
2. Le lancer : aucune question, aucun droit administrateur. Polyptyque s'ouvre à la fin, avec un raccourci sur le Bureau et dans le menu Démarrer.

> [!TIP]
> Pour les longues sessions, régler les « heures d'activité » de Windows Update (dans ses options avancées) sur la plage où l'on enregistre : Windows ne redémarre pas de lui-même pendant ces heures, sauf mise à jour imposée par une entreprise.

> [!NOTE]
> Polyptyque n'est pas encore signé numériquement. Windows peut afficher « Windows a protégé votre ordinateur » : cliquer sur « Informations complémentaires », puis « Exécuter quand même ».

**Mises à jour**

Polyptyque vérifie au démarrage, puis toutes les six heures, si une nouvelle version existe et la propose d'un clic, jamais pendant un enregistrement. Cette vérification peut être désactivée dans les réglages.

**Désinstaller**

Paramètres de Windows > Applications. Les scènes, les réglages et les connexions des navigateurs sont conservés.

**Version portable**

`Polyptyque-portable.zip` s'utilise sans installation : décompresser où l'on veut (disque, clé USB), puis lancer `Polyptyque.exe` à la racine du dossier.
- Scènes, réglages, connexions des navigateurs et journaux sont rangés dans le sous-dossier `données`. Rien n'est écrit ailleurs sur le PC : déplacer le dossier, ou le copier sur un autre PC, emporte tout.
- Les enregistrements vont dans le dossier choisi dans les réglages, ou dans `Vidéos\Polyptyque` si ce dossier n'existe pas sur ce PC.

## Prise en main

1. **Ajouter une source navigateur** : choisir le navigateur, un nom, éventuellement une adresse. Polyptyque l'ouvre avec un profil qui lui est propre : s'y connecter, préparer ses onglets. Tout est conservé pour les fois suivantes.
2. **Ajouter une source application** : choisir une fenêtre ouverte (Discord, un jeu, Tor Browser…).
3. **Ouvrir les fenêtres** : au lancement, Polyptyque ouvre seul les fenêtres de la scène en cours (navigateurs, et applications avec « Relancer »). Le bouton « Ouvrir les fenêtres » du panneau des sources le fait à la demande, et celui de chaque carte ouvre une seule source. L'ouverture au lancement se désactive dans les réglages.
4. **Démarrer l'enregistrement** : Polyptyque retrouve ou rouvre les fenêtres manquantes, puis affiche l'état de chaque source.
5. **Tout arrêter** : les fichiers sont finalisés proprement. Le bouton rond de chaque aperçu démarre ou arrête une seule source.
6. **Fichiers** : par défaut dans `Vidéos\Polyptyque` (dossier modifiable dans les réglages), un fichier MKV par source, nommés par défaut `date_heure_scène_source.mkv`. Le bouton « Fichiers » ouvre ce dossier.

**Raccourcis clavier**, actifs même quand Polyptyque est en arrière-plan et modifiables dans les réglages :

| Raccourci | Action |
|---|---|
| <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>F9</kbd> | Démarrer ou arrêter l'enregistrement |
| <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>F10</kbd> | Afficher ou réduire Polyptyque |
| <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>F11</kbd> | Activer ou couper les aperçus |

## Pour le montage

- **MKV ou MP4** : pendant l'enregistrement, Polyptyque écrit en MKV, un format qui reste lisible même après une coupure. « Convertir en MP4… » crée une copie MP4 à côté, sans perte de qualité, et la conversion peut être automatique en fin d'enregistrement. Le MKV est conservé ; prévoir le double d'espace disque.
- **Micro** : son fichier `…_Micro.mka` devient `…_Micro.m4a` après conversion.
- **Parties** : si l'encodeur a été relancé, la suite d'une source se trouve dans `…_partie2.mkv`, à placer juste après la première partie.
- **Fichier coupé net** (coupure de courant) : il se lit, mais sans durée ni index. La réparation proposée au lancement suivant les reconstruit sans réencodage ; la conversion en MP4 le fait aussi.
- **Source ajoutée en cours de route** : son fichier commence à l'heure indiquée dans son nom.

## Limites connues

- Le son est isolé **par programme**, pas par onglet : tous les onglets d'un même profil de navigateur partagent la même piste. Une source navigateur par contenu à isoler suffit à le contourner.
- Une fenêtre réduite dans la barre des tâches n'est plus dessinée par Windows : son image se fige, et sa carte affiche « Réduite ». Le bouton « Rétablir la fenêtre » de la carte la ramène, même pendant l'enregistrement. Une fenêtre cachée derrière d'autres s'enregistre normalement.
- Les contenus protégés (Netflix, Disney+…) apparaissent souvent en noir.
- Certains jeux en plein écran exclusif donnent une image noire : préférer le mode « fenêtré sans bordure ».

La liste complète, les choix techniques et la feuille de route sont dans le [cahier des charges](docs/cahier-des-charges.md).

## Pour les développeurs

**Organisation**

| Dossier | Rôle |
|---|---|
| `crates/polyptyque-core` | Scènes, sources, réglages et leurs sauvegardes, nommage, calculs de synchronisation, commandes FFmpeg, version portable, journal de la session en cours ; le contrat `Platform` que chaque système doit remplir |
| `crates/polyptyque-windows` | Implémentation Windows : capture d'image (Windows.Graphics.Capture) et de son (WASAPI), fenêtres, navigateurs, tubes, copie unique, veille et fermeture de session |
| `crates/polyptyque-engine` | Sessions d'enregistrement : horloge commune, capture, encodeurs FFmpeg surveillés et relancés, dossier de secours, conversion MP4 et réparation des fichiers coupés net |
| `crates/polyptyque-app` | Interface graphique : apparence dans `ui/` (thème dans `ui/theme.slint`), logique dans `src/` (dont le bilan après un incident), mises à jour |
| `crates/polyptyque-cli` | Version en console, livrée pour le diagnostic |
| `assets/logo` | Logo et icônes, générés par `generer.py` |
| `docs` | Cahier des charges, analyse des risques, fiches de test et captures d'écran |

Seule `polyptyque-windows` dépend du système : un portage vers macOS ou Linux consistera à écrire son équivalent.

**Compiler et tester**

```sh
cargo test --workspace                                    # tests, dont des sessions simulées (FFmpeg requis)
cargo build --release -p polyptyque-app -p polyptyque-cli
cargo run -p polyptyque-app                               # hors Windows : mode démonstration
```

Les sessions simulées rejouent des enregistrements complets avec de fausses fenêtres et un vrai FFmpeg, et vérifient :
- les durées et la synchronisation des fichiers ;
- la conversion MP4 ;
- l'enregistrement source par source ;
- la reprise après un plantage ou un blocage réel de FFmpeg, et la réparation du fichier coupé net ;
- la suite de l'enregistrement dans le dossier de secours quand le dossier choisi disparaît ;
- le passage sur le processeur quand la carte graphique refuse un encodage.

Le mode démonstration permet de développer l'interface sans Windows. Des variables d'environnement y simulent des situations à tester :

| Variable | Situation simulée |
|---|---|
| `POLYPTYQUE_DEMO_REDUITE` | La fenêtre de Discord est réduite |
| `POLYPTYQUE_DEMO_PERDUE` | La fenêtre de Chrome est fermée |
| `POLYPTYQUE_DEMO_MISE_A_JOUR` | Une nouvelle version est disponible |

**Branches et fabrication**
- `dev` reçoit le travail en cours ; `main` reçoit les versions validées.
- Chaque envoi lance les vérifications sur Linux et Windows, et fabrique l'installateur. On le trouve dans l'onglet **Actions**, artefact `Polyptyque-windows`.

**Publier une version**
1. Mettre à jour `version` dans le `Cargo.toml` de la racine, par exemple `0.2.0`.
2. Sur `main`, créer et envoyer l'étiquette correspondante :
   ```sh
   git tag v0.2.0
   git push origin v0.2.0
   ```
3. La fabrication publie la version sur GitHub avec son installateur. Les copies installées la proposent alors d'elles-mêmes.

## Licence

Polyptyque est un logiciel libre et gratuit, sous licence [GPL v3 ou ultérieure](LICENSE). Il s'appuie sur :
- [FFmpeg](https://ffmpeg.org), livré à côté sous sa propre licence GPL ;
- [Slint](https://slint.dev) ;
- [Velopack](https://velopack.io) ;
- la crate [`windows`](https://github.com/microsoft/windows-rs) de Microsoft.
