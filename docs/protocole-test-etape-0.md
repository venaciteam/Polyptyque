---
title: Polyptyque - Protocole de test de l'étape 0
aliases: ["Tests étape 0 Polyptyque"]
tags: [polyptyque, projet, tests, windows]
created: 2026-10-20
status: in-progress
type: guide
---

# Polyptyque - Protocole de test de l'étape 0

> [!warning] Document historique
> Ce protocole concerne le prototype en console de l'étape 0 (2026-10-20). Depuis, `polyptyque.exe` est l'interface graphique, la console s'appelle `polyptyque-console.exe`, et la fabrication livre l'artefact `Polyptyque-windows` (installateur et zip portable). Pour les tests actuels, voir [[protocole-test-2026-10-22]].

> [!abstract] Résumé
> Tests à mener sur la machine de référence (Windows 11, RTX 3080) pour valider le prototype du moteur : son isolé par fenêtre, reconnaissance des fenêtres, charge, puis endurance de 12 h. Chaque test indique quoi faire, quoi observer et quoi transmettre en retour.

## Table des matières

1. [[#Récupérer le prototype]]
2. [[#Test 1 - Son isolé avec Firefox]]
3. [[#Test 2 - Chrome et mélange de navigateurs]]
4. [[#Test 3 - Application Discord]]
5. [[#Test 4 - Fenêtre perdue puis retrouvée]]
6. [[#Test 5 - Charge]]
7. [[#Test 6 - Endurance]]
8. [[#Quoi transmettre]]

---

## Récupérer le prototype

1. Sur GitHub, dépôt **Polyptyque**, onglet **Actions**.
2. Ouvrir la dernière exécution **« Vérifications et fabrication »** marquée d'une coche verte.
3. En bas de la page, section **Artifacts** : télécharger **Polyptyque-prototype-windows**.
4. Extraire le zip dans un dossier simple, par exemple `Documents\Polyptyque-test`.
5. Lancer `polyptyque.exe`.

> [!warning] Alerte Windows au premier lancement
> « Windows a protégé votre ordinateur » : cliquer sur **Informations complémentaires**, puis **Exécuter quand même**. Si rien ne se lance du tout, Smart App Control est probablement actif : le noter et me le dire.

Tout se pilote avec les flèches et la touche <kbd>Entrée</kbd>.

## Test 1 - Son isolé avec Firefox

**Objectif** : le point le plus risqué du projet. Deux fenêtres Firefox, deux sons différents, deux fichiers propres.

1. **Ajouter une source navigateur** → Firefox → nom « Firefox A » → adresse d'une vidéo YouTube (une musique, par exemple) → ouvrir maintenant.
2. Même chose : « Firefox B », avec une vidéo **au son très différent** (une voix parlée, par exemple).
3. Lancer la lecture dans les deux fenêtres, puis **Démarrer l'enregistrement**.
4. Laisser tourner **1 minute**, puis <kbd>Q</kbd>.
5. Ouvrir les deux fichiers MKV (dossier `Vidéos\Polyptyque`) avec VLC.

- [ ] Chaque fichier ne contient **que** le son de sa fenêtre
- [ ] L'image et le son sont calés (mouvements des lèvres)
- [ ] Pas de bordure jaune autour des fenêtres pendant l'enregistrement
- [ ] Fermer Polyptyque, le relancer : les deux sources sont toujours là et les connexions sont conservées

## Test 2 - Chrome et mélange de navigateurs

Refaire le test 1 avec **Chrome** (ou Edge), puis avec **un Firefox et un Chrome** dans la même scène.

- [ ] Sons isolés avec Chrome
- [ ] Sons isolés avec le mélange Firefox et Chrome

## Test 3 - Application Discord

1. **Ajouter une source application** → choisir la fenêtre Discord.
2. Enregistrer 1 minute avec un son dans Discord (salon vocal, vidéo).

- [ ] Le fichier contient le son de Discord uniquement
- [ ] La fenêtre est retrouvée après avoir fermé puis rouvert Discord (hors enregistrement)

## Test 4 - Fenêtre perdue puis retrouvée

Pendant un enregistrement avec Discord :

1. Fermer complètement Discord : la source passe à **Perdue**.
2. Rouvrir Discord : la source repasse à **Connectée** en quelques secondes.
3. Réduire une fenêtre dans la barre des tâches : la source passe à **Réduite**.
4. Fermer une fenêtre de navigateur gérée, puis appuyer sur <kbd>R</kbd> : Polyptyque la rouvre sur son profil et la rattache.

- [ ] Le fichier contient une image noire et du silence pendant l'absence, puis reprend
- [ ] La durée du fichier est identique à celle des autres sources

## Test 5 - Charge

Scène de **4 sources** en 1080p à 60 i/s, toutes avec une vidéo en lecture, pendant 5 minutes. Ouvrir le Gestionnaire des tâches → **Performances** → **GPU** et regarder la courbe **Video Encode**.

- [ ] La colonne **Perdues** reste à 0 (ou très bas)
- [ ] Noter l'utilisation de Video Encode et du processeur : ……

## Test 6 - Endurance

1. **1 heure** avec 3 sources. Vérifier les fichiers.
2. Si tout va bien : **12 heures**.

Pendant le test, ne pas mettre l'ordinateur en veille (Polyptyque l'empêche, mais pas une fermeture du capot d'un portable).

- [ ] Les fichiers s'ouvrent et vont jusqu'au bout
- [ ] Toutes les sources ont **la même durée**
- [ ] L'image et le son sont **toujours calés à la fin** du fichier
- [ ] Mémoire de `polyptyque.exe` stable dans le Gestionnaire des tâches (noter au début et à la fin : …… / ……)

## Quoi transmettre

Pour chaque test, en cas de problème comme de réussite :

1. Une **capture d'écran** de l'écran de suivi pendant l'enregistrement.
2. Le dossier des **journaux**, compressé en zip : coller `%LOCALAPPDATA%\Polyptyque\journaux` dans la barre d'adresse de l'Explorateur.
3. Ce qui a été observé, avec les cases cochées ci-dessus.

> [!tip] Rien de personnel dans les journaux
> Les journaux contiennent les titres des fenêtres capturées et les chemins des fichiers. Les relire avant de les partager si un titre est sensible.
