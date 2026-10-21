---
title: Polyptyque - Tests du 2026-10-22
aliases: ["Tests portable et installateur Polyptyque", "Protocole de test du 22 octobre"]
tags: [polyptyque, projet, tests, windows]
created: 2026-10-22
status: todo
type: guide
---

# Polyptyque - Tests du 2026-10-22

> [!abstract] Résumé
> Tests à mener sur la machine de référence après les travaux de la nuit du 21 au 22 octobre : version portable, installateur, nouveautés (reprise après incident, boutons Rétablir et Rouvrir, instance unique…) et non-régression de ce qui marchait déjà.
> Chaque test indique quoi faire et ce qui est attendu. Cocher au fur et à mesure, noter tout écart.

## Table des matières

1. [[#Récupérer la fabrication]]
2. [[#A - Version portable]]
3. [[#B - Installateur]]
4. [[#C - Nouveautés de la nuit]]
5. [[#D - Non-régression]]
6. [[#E - À me renvoyer]]

> [!warning] Sans enjeu
> Tous ces tests se font sur des enregistrements de quelques minutes, jamais sur un enregistrement important. Plusieurs tests arrêtent Polyptyque de force exprès.

---

## Récupérer la fabrication

1. GitHub > dépôt Polyptyque > onglet **Actions** > la dernière exécution verte de la branche `dev`.
2. En bas de la page, artefact **`Polyptyque-windows`** : il contient `Polyptyque-Setup.exe` et `Polyptyque-portable.zip`.

> [!tip] Ordre conseillé
> A (portable) puis C et D avec la version portable, puis B (installateur) à la fin : l'installateur se teste vite, et la désinstallation termine la séance.

---

## A - Version portable

- [ ] **A1 - Lancement**
	- Faire : décompresser `Polyptyque-portable.zip` dans un dossier neuf (par exemple `Bureau\Polyptyque-portable`), puis lancer `Polyptyque.exe` à la racine de ce dossier (pas `Update.exe`, ni le programme du sous-dossier `current`).
	- Attendu : Polyptyque s'ouvre. Dans **Réglages**, en bas à gauche : « Polyptyque 0.1.0 · portable ». Un dossier `données` apparaît à la racine, avec `journaux`.
- [ ] **A2 - Tout reste dans le dossier**
	- Faire : créer une scène, une source navigateur (s'y connecter à un site), puis changer deux réglages, l'un après l'autre.
	- Attendu : `données` contient `config.toml`, `config.toml.precedent` (version précédente), `sauvegardes` (une copie par jour), `profils` et `journaux`. Rien de nouveau dans `%APPDATA%\Polyptyque` ni dans `%LOCALAPPDATA%\Polyptyque` (regarder les dates de modification si ces dossiers existent déjà à cause de la version installée).
- [ ] **A3 - Enregistrement**
	- Faire : enregistrer une minute, puis « Tout arrêter ».
	- Attendu : fichiers dans le dossier choisi dans les réglages (par défaut `Vidéos\Polyptyque`).
- [ ] **A4 - Déplacement du dossier**
	- Faire : fermer Polyptyque, déplacer tout le dossier ailleurs (autre disque ou clé USB si possible), relancer.
	- Attendu : scènes et réglages présents ; la source navigateur s'ouvre **toujours connectée** (son profil a suivi).
- [ ] **A5 - Dossier d'enregistrement absent**
	- Faire : dans les réglages, choisir un dossier sur une clé USB, débrancher la clé, démarrer un enregistrement.
	- Attendu : l'enregistrement démarre avec le message « Dossier … inaccessible : enregistrement dans …\Vidéos\Polyptyque ».

---

## B - Installateur

- [ ] **B1 - Installation**
	- Faire : lancer `Polyptyque-Setup.exe`. Si Windows affiche « Windows a protégé votre ordinateur » : « Informations complémentaires », puis « Exécuter quand même » (le programme n'est pas encore signé).
	- Attendu : animation des trois panneaux, aucune question, raccourcis sur le Bureau et dans le menu Démarrer, Polyptyque s'ouvre tout seul.
- [ ] **B2 - Version installée**
	- Attendu : les réglages n'affichent **pas** « portable ». Les données sont dans `%APPDATA%\Polyptyque` et `%LOCALAPPDATA%\Polyptyque`.
- [ ] **B3 - Instance unique**
	- Faire : Polyptyque ouvert, relancer depuis le raccourci du Bureau. Puis lancer aussi la version portable.
	- Attendu : à chaque fois, aucune deuxième fenêtre : la fenêtre déjà ouverte revient au premier plan.
- [ ] **B4 - Désinstallation** (en tout dernier)
	- Faire : Paramètres de Windows > Applications > Polyptyque > Désinstaller.
	- Attendu : Polyptyque disparaît du menu Démarrer et du Bureau ; `%APPDATA%\Polyptyque` et `%LOCALAPPDATA%\Polyptyque` (scènes, connexions) sont **conservés**.

---

## C - Nouveautés de la nuit

- [ ] **C1 - Rétablir une fenêtre réduite**
	- Faire : pendant un enregistrement, réduire une fenêtre capturée dans la barre des tâches.
	- Attendu : la carte affiche « Réduite (image figée) » et un bouton **« Rétablir la fenêtre »**. Un clic fait revenir la fenêtre **sans lui donner le focus**, et la carte repasse à « Enregistre ». Une fenêtre qui était en plein écran revient en plein écran (et prend alors le focus).
	- Variante : l'écran débranché d'hier. La fenêtre doit revenir **sur l'écran principal**.
- [ ] **C2 - Rouvrir une fenêtre perdue**
	- Faire : pendant un enregistrement, fermer un Firefox capturé, puis cliquer tout de suite sur **« Rouvrir la fenêtre »** sur sa carte.
	- Attendu : Firefox revient sur ses onglets, toujours connecté ; la carte repasse à « Enregistre ». Si Firefox n'a pas fini de se fermer, un message demande de réessayer quelques secondes plus tard.
- [ ] **C3 - Réouverture automatique**
	- Faire : fermer un Firefox capturé et ne rien toucher pendant 30 secondes.
	- Attendu : après environ 20 secondes, message « La fenêtre de … a disparu : réouverture automatique », et Firefox revient tout seul.
- [ ] **C4 - Plantage de Polyptyque**
	- Faire : pendant un enregistrement d'une minute, ouvrir un terminal (Windows + R, `cmd`, Entrée) et coller :
	  ```
	  taskkill /F /IM polyptyque.exe
	  ```
	  Puis relancer Polyptyque.
	- Attendu : une fenêtre **« Enregistrement interrompu »** liste les fichiers, en principe tous « Complet » (FFmpeg a fini de les écrire seul). Boutons « Fermer » et « Reprendre l'enregistrement ».
- [ ] **C5 - Coupure de courant simulée**
	- Faire : pendant un enregistrement d'une minute, coller dans le terminal (Polyptyque et ses encodeurs arrêtés d'un coup, comme lors d'une coupure) :
	  ```
	  taskkill /F /IM ffmpeg.exe /IM polyptyque.exe
	  ```
	  Puis relancer Polyptyque.
	- Attendu : « Enregistrement interrompu » avec des fichiers « Interrompu, lisible, à réparer » (un fichier qu'un encodeur aurait eu le temps de terminer s'affiche « Complet » : c'est normal). Cliquer **« Réparer les fichiers »** : chaque ligne passe à « Réparé : prêt pour le montage », puis message de fin. Ouvrir un fichier réparé dans VLC : la durée s'affiche et le déplacement dans la vidéo est rapide.
	- Puis : **« Reprendre l'enregistrement »** démarre un nouvel enregistrement de la même scène.
- [ ] **C6 - Journal**
	- Faire : après C1, ouvrir le dernier `polyptyque_….log` des journaux.
	- Attendu : lignes « fenêtre réduite : image figée… », « fenêtre rétablie » et, à l'arrêt, une ligne « bilan » par source (durée, fichiers, taille, images écrites, perdues et reçues).
- [ ] **C7 - Mise en veille**
	- Faire : pendant un enregistrement, Démarrer > Marche/Arrêt > Mettre en veille. Réveiller l'ordinateur après une minute.
	- Attendu : au réveil, l'enregistrement est arrêté (message « Mise en veille de l'ordinateur : enregistrement arrêté ») et une fenêtre propose « Reprendre l'enregistrement ? ». Les fichiers d'avant la veille sont complets et ne contiennent pas la minute de veille.
- [ ] **C8 - Disque débranché pendant l'enregistrement** (si une clé ou un disque USB est disponible)
	- Faire : enregistrer sur la clé, la débrancher au bout d'une minute, attendre une minute, puis « Tout arrêter ».
	- Attendu : les cartes affichent « Dossier … inaccessible : la suite est enregistrée dans … » ; la suite de chaque source est dans `Vidéos\Polyptyque\…_partie2.mkv`.

---

## D - Non-régression

> [!info] Pourquoi
> Beaucoup de code a bougé cette nuit. Ces tests vérifient que ce qui marchait hier marche toujours.

- [ ] **D1 - Aperçus** : aperçus en direct nets, définition adaptée quand on agrandit la fenêtre de Polyptyque ; <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>F11</kbd> les coupe et les rétablit.
- [ ] **D2 - Enregistrement complet** : trois sources, deux minutes, « Tout arrêter ». Trois fichiers de même durée, son de chaque fenêtre isolé, synchronisation correcte.
- [ ] **D3 - Source par source** : pendant un enregistrement, arrêter une source avec le bouton rond de sa carte, puis la relancer. Les autres continuent ; la source relancée a un nouveau fichier.
- [ ] **D4 - Encodeur relancé** : pendant un enregistrement, Gestionnaire des tâches > onglet Détails > un `ffmpeg.exe` > Fin de tâche. La carte signale « Encodeur relancé… » et un fichier `…_partie2.mkv` apparaît ; l'enregistrement continue. Après « Tout arrêter », une fenêtre **« Fichiers à réparer »** propose de réparer la première partie, coupée net : accepter, puis vérifier dans VLC que sa durée s'affiche.
- [ ] **D5 - Micro** (si activé dans les réglages) : fichier `…_Micro.mka`, calé sur les vidéos.
- [ ] **D6 - Conversion MP4** : « Convertir en MP4… » sur un fichier ; le MP4 se lit et aucun fichier `….partiel` ne reste dans le dossier.
- [ ] **D7 - Raccourcis** : <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>F9</kbd> démarre et arrête ; <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>F10</kbd> réduit et rappelle Polyptyque.
- [ ] **D8 - Quitter pendant un enregistrement** : fermer la fenêtre demande une confirmation, puis finalise les fichiers.
- [ ] **D9 - Réglages** : changer un réglage, fermer, relancer : le réglage est conservé.

---

## E - À me renvoyer

- Les tests cochés, et pour chaque écart : ce qui s'est passé, et une capture si possible.
- Le dossier des journaux (`données\journaux` pour la version portable, `%LOCALAPPDATA%\Polyptyque\journaux` pour la version installée), au moins le dernier `polyptyque_….log` et le dossier `session_…` des tests C4 et C5.

Voir aussi : [[cahier-des-charges]], [[analyse-des-risques]].
