---
title: Polyptyque - Tests de la programmation
aliases: ["Tests programmation Polyptyque", "Protocole de test des enregistrements programmés"]
tags: [polyptyque, projet, tests, windows, programmation]
created: 2026-10-23
status: todo
type: guide
---

# Polyptyque - Tests de la programmation

> [!abstract] Résumé
> Tests à mener sur la machine de référence pour la programmation des enregistrements : créneaux, démarrage et arrêt à l'heure, démarrage manqué, enregistrements lancés à la main pendant un créneau, anti-veille, lancement au démarrage de Windows, session verrouillée, changement d'heure.
> Chaque test indique quoi faire et ce qui est attendu. Cocher au fur et à mesure, noter tout écart.
> Comportement de référence : [[cahier-des-charges#Programmation des enregistrements]].

## Table des matières

1. [[#Récupérer la fabrication]]
2. [[#A - Programmations]]
3. [[#B - À l'heure prévue]]
4. [[#C - Situations prévues]]
5. [[#D - Veille et démarrage de Windows]]
6. [[#E - Changement d'heure]]
7. [[#F - Non-régression]]
8. [[#G - À me renvoyer]]

> [!warning] Sans enjeu
> Créneaux courts (deux ou trois minutes), sur une scène de test. Le plus simple : programmer le début deux minutes plus tard, la fin cinq minutes plus tard.

> [!tip] Heures
> Les heures se saisissent comme « 21:00 », « 21h », « 21h30 » ; la date comme « 25/10/2026 » ou « 25/10 ».

---

## Récupérer la fabrication

1. GitHub > dépôt Polyptyque > onglet **Actions** > la dernière exécution verte de la branche `dev`. Fabrication vérifiée : [exécution 55](https://github.com/venaciteam/polyptyque/actions/runs/39981299667), [artefact `Polyptyque-windows`](https://github.com/venaciteam/polyptyque/actions/runs/39981299667/artifacts/12514191661) (commit `2727ed4`).
2. En bas de la page, artefact **`Polyptyque-windows`** : il contient `Polyptyque-Setup.exe` et `Polyptyque-portable.zip`.

---

## A - Programmations

- [ ] **A1 - Pastille**
	- Attendu : en haut à droite, une pastille **« Programmer »** avec une horloge. Un clic ouvre la fenêtre « Programmations » (vide), avec le bouton « Nouvelle programmation » et l'option « Lancer Polyptyque au démarrage de Windows » (désactivée).
- [ ] **A2 - Créer**
	- Faire : « Nouvelle programmation » : scène de test, début dans deux minutes, fin dans cinq, « Une seule fois », date du jour. Enregistrer.
	- Attendu : message « Programmation enregistrée : prochain créneau aujourd'hui à … ». La liste affiche « Scène · HH:MM → HH:MM » et « Une fois, le … · Prochain : aujourd'hui à … ». La pastille affiche « Scène · aujourd'hui à … ».
- [ ] **A3 - Créneau qui passe minuit**
	- Faire : nouvelle programmation 23:00 → 01:00, « Certains jours de la semaine », cocher deux jours.
	- Attendu : sous les heures, « Se termine le lendemain. » ; dans la liste, « 23:00 → 01:00 (lendemain) » et les jours en abrégé.
- [ ] **A4 - Refus**
	- Faire : essayer successivement : début égal à la fin ; « Certains jours » sans aucun jour ; une date passée ; un créneau qui chevauche celui de A2.
	- Attendu : à chaque fois un message rouge dans le formulaire (le chevauchement nomme l'autre programmation) ; rien n'est enregistré.
- [ ] **A5 - Désactiver, modifier, supprimer**
	- Faire : désactiver la programmation de A3 avec son interrupteur, la modifier (crayon), puis la supprimer (corbeille).
	- Attendu : « Désactivée » dans la liste ; les changements sont gardés après une fermeture et une réouverture de Polyptyque ; la suppression la retire.

---

## B - À l'heure prévue

- [ ] **B1 - Démarrage**
	- Faire : laisser Polyptyque ouvert jusqu'au début du créneau de A2, sur une autre scène que celle programmée.
	- Attendu : à l'heure pile, la scène programmée devient la scène en cours et s'enregistre. Message « Enregistrement démarré. Enregistrement programmé : arrêt prévu à … ». La pastille affiche « Arrêt programmé à … » (bordure rouge).
- [ ] **B2 - Verrouillée pendant l'enregistrement**
	- Faire : ouvrir les programmations pendant l'enregistrement.
	- Attendu : la programmation en cours a une bordure rouge, « Enregistre jusqu'à … », crayon, corbeille et interrupteur inactifs.
- [ ] **B3 - Arrêt**
	- Attendu : à l'heure de fin, arrêt seul, message « Fin du créneau programmé. N fichier(s) enregistré(s)… ». La liste affiche « Terminée ».
- [ ] **B4 - Bout à bout**
	- Faire : deux programmations collées (par exemple 21:00 → 21:03 et 21:03 → 21:06).
	- Attendu : à 21:03, le premier enregistrement s'arrête, puis le second démarre sans question dès les fichiers finalisés (quelques secondes).

---

## C - Situations prévues

- [ ] **C1 - Démarrage manqué, avec réponse**
	- Faire : programmer un créneau qui commence dans une minute et dure cinq minutes. Fermer Polyptyque (confirmer « Quitter quand même »). Le rouvrir deux minutes après le début du créneau.
	- Attendu : fenêtre « Enregistrement programmé » : le créneau a commencé à …, lancer maintenant ? Compte à rebours « Sans réponse, l'enregistrement démarre dans 1 min 5x s ». « Ne pas enregistrer » : message « Enregistrement programmé annulé pour ce créneau », plus rien jusqu'à la fin du créneau.
- [ ] **C2 - Démarrage manqué, sans réponse**
	- Faire : comme C1, puis ne rien toucher.
	- Attendu : au bout de deux minutes, l'enregistrement démarre seul, puis s'arrête à la fin du créneau. (La fenêtre réduite qui revient à l'écran pour la question se vérifie en D4.)
- [ ] **C3 - Enregistrement manuel déjà en cours**
	- Faire : démarrer un enregistrement à la main une minute avant le début d'un créneau.
	- Attendu : au début du créneau, petite fenêtre d'information (bouton « Compris ») ; l'enregistrement continue. L'arrêter à la main pendant le créneau : **pas** de confirmation, et rien ne redémarre. À la fin du créneau, rien ne se passe.
- [ ] **C4 - Lancement à la main pendant un créneau**
	- Faire : pendant un créneau sans enregistrement (par exemple après C1, réponse « Ne pas enregistrer »), démarrer à la main. Essayer les trois réponses, sur trois créneaux :
		- « Arrêter à … » : l'enregistrement s'arrête à la fin du créneau ;
		- « Ne pas l'arrêter » : il continue après la fin du créneau ;
		- aucune réponse : au bout de deux minutes, message « L'enregistrement continuera après … », et il continue après la fin du créneau.
	- Variante : démarrer une seule source avec le bouton rond de sa carte : même question.
- [ ] **C5 - Arrêt à la main d'un enregistrement programmé**
	- Faire : pendant un enregistrement programmé, cliquer « Tout arrêter ». Annuler, puis recommencer avec <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>F9</kbd> et confirmer.
	- Attendu : confirmation « Créneau d'enregistrement en cours » à chaque fois ; après confirmation, arrêt. Fermer puis rouvrir Polyptyque pendant le même créneau : **aucune** question, rien ne redémarre. Le créneau suivant (le lendemain, ou une autre programmation) reste prévu.
- [ ] **C6 - Quitter**
	- Faire : une programmation à venir, rien n'enregistre : fermer Polyptyque.
	- Attendu : confirmation « Programmation active : … est programmé aujourd'hui à … ». Pendant un enregistrement programmé : la confirmation habituelle précise « jusqu'à … » et que les programmations ne se lancent pas Polyptyque fermé.
- [ ] **C7 - Plantage pendant un enregistrement programmé**
	- Faire : pendant un enregistrement programmé, coller dans un terminal (<kbd>Windows</kbd> + <kbd>R</kbd>, `cmd`, Entrée) :
	  ```
	  taskkill /F /IM polyptyque.exe
	  ```
	  Relancer Polyptyque dans le créneau.
	- Attendu : le bilan « Enregistrement interrompu » et, au-dessus, la question du démarrage manqué avec son compte à rebours ; sans réponse, l'enregistrement reprend dans de nouveaux fichiers.

---

## D - Veille et démarrage de Windows

- [ ] **D1 - Anti-veille**
	- Faire : une programmation à venir, rien n'enregistre. Ouvrir une invite de commandes **en administrateur** et lancer :
	  ```
	  powercfg /requests
	  ```
	- Attendu : `polyptyque.exe` apparaît sous **DISPLAY** et **SYSTEM**. Supprimer ou désactiver toutes les programmations : la commande ne le liste plus (hors enregistrement). L'écran ne s'éteint pas tant qu'une programmation est active.
- [ ] **D2 - Mise en veille forcée pendant un enregistrement programmé**
	- Faire : pendant un enregistrement programmé, Démarrer > Marche/Arrêt > Mettre en veille ; réveiller l'ordinateur au bout d'une minute, avant la fin du créneau.
	- Attendu : au réveil, **pas** la fenêtre « Reprendre l'enregistrement ? », mais la question du démarrage manqué (compte à rebours, reprise sans réponse).
- [ ] **D3 - Lancement au démarrage**
	- Faire : activer « Lancer Polyptyque au démarrage de Windows » (dans les programmations). Redémarrer Windows et ouvrir la session.
	- Attendu : Polyptyque s'ouvre seul, **réduit** dans la barre des tâches, sans ouvrir les fenêtres de la scène. Gestionnaire des tâches > Applications de démarrage : une entrée pour Polyptyque, activée.
- [ ] **D4 - Redémarrage pendant un créneau**
	- Faire : option D3 activée, programmation en cours d'enregistrement : redémarrer Windows (accepter l'arrêt malgré l'enregistrement), ouvrir la session pendant le créneau.
	- Attendu : Polyptyque lancé réduit, puis la question du démarrage manqué (la fenêtre revient à l'écran) ; sans réponse, l'enregistrement reprend au bout de deux minutes.
- [ ] **D5 - Désactivé dans Windows**
	- Faire : Paramètres > Applications > Démarrage : désactiver Polyptyque. Rouvrir les programmations.
	- Attendu : note orange « Windows empêche ce lancement… ». Couper puis remettre l'option dans Polyptyque : la note disparaît, et Polyptyque est de nouveau activé dans les Paramètres.
- [ ] **D6 - Session verrouillée et écran éteint** (le test demandé)
	- Faire : programmer un créneau de trois minutes qui commence dans deux minutes. Verrouiller la session (<kbd>Windows</kbd> + <kbd>L</kbd>), puis éteindre l'écran avec son bouton. Rallumer et déverrouiller après la fin du créneau.
	- Attendu : enregistrement démarré et arrêté à l'heure (journal), fichiers avec l'image des fenêtres pendant le verrouillage. Noter si l'image est noire ou figée.
	- Variante, si une mise à jour de Windows est en attente : « Mettre à jour et redémarrer » avec D3 activé et l'option de Windows « Utiliser mes informations de connexion pour terminer automatiquement la configuration » : Windows ouvre la session et la verrouille seul ; vérifier que Polyptyque se lance et que l'enregistrement programmé démarre.
- [ ] **D7 - Version portable déplacée**
	- Faire : avec la version portable, activer D3, fermer, déplacer le dossier, relancer une fois depuis son nouvel emplacement, puis redémarrer Windows.
	- Attendu : Polyptyque se lance depuis le nouvel emplacement. Sous l'option : note sur la version portable.

---

## E - Changement d'heure

> [!info] Dimanche 25 octobre 2026
> À 03:00 (heure d'été), les horloges reviennent à 02:00 : la nuit compte une heure de plus. Test facultatif, l'ordinateur doit rester allumé.

- [ ] **E1 - Nuit plus longue**
	- Faire : programmation « Une seule fois », le 25/10/2026, de 02:50 à 03:10.
	- Attendu : démarrage à 02:50 (première occurrence de cette heure, encore en heure d'été), arrêt à 03:10 en heure d'hiver : **1 h 20** d'enregistrement réel (durée des fichiers).

---

## F - Non-régression

- [ ] **F1 - Sans programmation** : enregistrement manuel normal ; mise en veille pendant l'enregistrement : « Reprendre l'enregistrement ? » au réveil, comme avant ; quitter sans programmation : pas de nouvelle question.
- [ ] **F2 - Suppression d'une scène** : la confirmation indique « Sa programmation aussi » (ou « Ses N programmations ») ; elles disparaissent de la liste.
- [ ] **F3 - Réglages** : l'interrupteur « Lancer Polyptyque au démarrage de Windows » des réglages suit celui des programmations, et inversement.
- [ ] **F4 - Désinstallation** (en tout dernier) : après la désinstallation, Polyptyque n'est plus dans les applications de démarrage.

---

## G - À me renvoyer

- Les tests cochés, et pour chaque écart : ce qui s'est passé, et une capture si possible.
- Le dernier `polyptyque_….log` des journaux (`%LOCALAPPDATA%\Polyptyque\journaux`, ou `données\journaux` pour la version portable) : chaque décision de la programmation y est notée (« créneau … », « question … », « anti-veille »…).
- Pour D6 : un fichier enregistré pendant le verrouillage.

Voir aussi : [[cahier-des-charges]], [[analyse-des-risques]], [[protocole-test-2026-10-22]].
