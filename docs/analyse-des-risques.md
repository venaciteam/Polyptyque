---
title: Polyptyque - Analyse des risques et reprise après incident
aliases: ["Analyse des risques Polyptyque", "Reprise après incident"]
tags: [polyptyque, projet, fiabilite, analyse-des-risques]
created: 2026-10-21
status: draft
type: projet
---

# Polyptyque - Analyse des risques et reprise après incident

> [!abstract] Résumé
> Inventaire de ce qui peut mal tourner pendant un enregistrement : défaillance de Polyptyque, de Windows ou du matériel, des programmes capturés, de l'encodage ou du disque, et erreurs de manipulation.
> Pour chaque cas : ce que fait Polyptyque **aujourd'hui** (vérifié dans le code de la branche `dev` au 2026-10-21, ou signalé « à vérifier »), ce qui manque, et la proposition.
> Les propositions sont regroupées en trois lots par priorité, à valider avant toute réalisation.
> Complète le [[cahier-des-charges|cahier des charges]].

## Table des matières

1. [[#Principes]]
2. [[#Méthode]]
3. [[#A - Polyptyque lui-même]]
4. [[#B - Windows et le matériel]]
5. [[#C - Programmes capturés]]
6. [[#D - Encodage et disque]]
7. [[#E - Erreurs de manipulation]]
8. [[#Déjà couvert]]
9. [[#Plan d'action proposé]]
	1. [[#Lot 1 - Reprise après incident]]
	2. [[#Lot 2 - Situations prévisibles]]
	3. [[#Lot 3 - Confort et prévention]]
10. [[#Tests à mener sur la machine de référence]]
11. [[#Questions ouvertes]]
12. [[#Sources]]

---

## Principes

> [!important] Cinq règles
> 1. **Ne jamais perdre ce qui a été enregistré.** Un incident peut couper un enregistrement, jamais abîmer ce qui est déjà sur le disque.
> 2. **Ne jamais échouer en silence.** Tout problème se voit dans l'interface, avec une explication et, si possible, une action pour le résoudre.
> 3. **Se rétablir seul quand c'est sûr**, sinon rendre la reprise possible en un clic.
> 4. **Ne rien faire de destructif ou d'inattendu sans accord** (supprimer, déplacer une fenêtre, écraser un fichier).
> 5. **Tout noter dans le journal**, pour comprendre après coup.

## Méthode

Chaque scénario porte un identifiant (A1, B3…) repris dans le plan d'action.

| Colonne | Valeurs |
|---|---|
| Probabilité | faible, moyenne, forte |
| Gravité | faible (gêne), moyenne (perte de temps), forte (perte d'une partie d'un enregistrement), critique (perte de données ou de réglages) |
| Priorité | P1 (lot 1), P2 (lot 2), P3 (lot 3), — (déjà couvert) |

---

## A - Polyptyque lui-même

| N° | Scénario | Prob. | Grav. | Aujourd'hui | Proposition | Prio. |
|---|---|---|---|---|---|---|
| A1 | Erreur interne de l'interface pendant un enregistrement | faible | forte | Polyptyque se ferme d'un coup. Les FFmpeg reçoivent la fin de leurs flux et ferment proprement leurs fichiers (probable, **à vérifier**). Au redémarrage, rien ne signale l'interruption et rien n'est repris | Journal de session (voir [[#Lot 1 - Reprise après incident]]) : au redémarrage, bilan des fichiers et bouton « Reprendre l'enregistrement » | P1 |
| A2 | Erreur interne dans le fil de capture ou d'écriture d'une source | faible | forte | Le fil s'arrête **sans prévenir** : la source peut continuer avec le son seul, ou plus rien, alors que la carte affiche encore « Enregistre » | Chaque fil protégé : l'erreur est affichée sur la carte et la capture de la source est relancée, comme l'encodeur | P1 |
| A3 | Interface figée | très faible | moyenne | L'enregistrement continue (le moteur est indépendant), mais ne peut être arrêté qu'en fermant Polyptyque de force ; les fichiers sont alors fermés par FFmpeg (probable, **à vérifier**) | Arrêt d'urgence par le raccourci global, traité hors de l'interface | P3 |
| A4 | Deux Polyptyque ouverts en même temps | moyenne | moyenne | Rien ne l'empêche : réglages écrasés par l'un ou l'autre, raccourcis refusés au second, fichiers de même nom (FFmpeg refuse d'écraser : une source échoue) | Instance unique : un second lancement ramène la fenêtre déjà ouverte | P1 |
| A5 | Fichier de réglages abîmé (scènes, sources) | faible | critique | Copie dans `config.toml.invalide`, puis réglages vierges ; la sauvegarde suivante remplace l'original. Écriture atomique, mais sans forçage sur le disque : une coupure de courant juste après une sauvegarde peut laisser un fichier vide | Écriture forcée sur le disque ; cinq sauvegardes tournantes ; restauration automatique de la dernière bonne, avec message | P1 |
| A6 | Conversion MP4 interrompue (plantage, coupure) | faible | moyenne | Le MP4 partiel garde son nom définitif et semble valide alors qu'il est tronqué | Écriture sous un nom temporaire, renommé seulement après vérification ; restes supprimés au démarrage | P2 |
| A7 | Journaux qui s'accumulent | forte | faible | Un journal par lancement et un dossier par session, jamais supprimés | Suppression automatique après 30 jours | P3 |
| A8 | Mise à jour interrompue | faible | moyenne | Velopack remplace le dossier du programme d'un bloc ; en cas d'échec, l'ancienne version est relancée (documentation Velopack) | Rien à ajouter | — |
| A9 | Mémoire qui grimpe sur 12 h | inconnue | forte | Non mesuré | Mesure pendant le test d'endurance (Gestionnaire des tâches, en début et en fin de session) | test |

## B - Windows et le matériel

| N° | Scénario | Prob. | Grav. | Aujourd'hui | Proposition | Prio. |
|---|---|---|---|---|---|---|
| B1 | Coupure de courant, écran bleu | faible | critique | Fichiers MKV lisibles (écrits par blocs d'une seconde) ; la fin est perdue, y compris ce que Windows n'avait pas encore écrit sur le disque (non mesuré). Fichiers sans index : le déplacement dans la vidéo est lent en montage. Rien n'est signalé au redémarrage | Au redémarrage : session interrompue détectée (A1), fichiers **réparés sans réencodage** (index reconstruit), reprise proposée | P1 |
| B2 | Redémarrage forcé par Windows Update | moyenne | forte | Blocage avec explication dans l'écran d'arrêt ; si l'arrêt est forcé, fichiers finalisés. Pas de reprise après le redémarrage | %%RECHERCHE-B2%% | P2 |
| B3 | Mise en veille demandée (capot fermé, bouton, menu) | moyenne | forte | La mise en veille **automatique** est empêchée pendant l'enregistrement, pas celle-ci. Comportement de l'horloge au réveil : %%RECHERCHE-B3%% | %%PROPOSITION-B3%% | P2 |
| B4 | Écran débranché avec des fenêtres capturées (vu en test le 2026-10-21) | moyenne | moyenne | Les fenêtres passent « Réduites » : image figée (noire ici), son conservé, reprise dès leur retour | Bouton « Rétablir la fenêtre » sur la carte (déjà prévu) | P1 |
| B5 | Verrouillage de la session (Win+L) | moyenne | forte | L'écran reste allumé pendant l'enregistrement, donc pas de verrouillage automatique. Capture pendant un verrouillage manuel : **à vérifier** | Selon le test : avertissement, ou reprise automatique au déverrouillage | test |
| B6 | Redémarrage du pilote graphique | faible | forte | Probable (non testé) : capture perdue puis rattachée, encodeur relancé dans une nouvelle partie | Rien à ajouter si le test le confirme | test |
| B7 | Changement de sortie audio (casque branché, enceintes) | forte | moyenne | Le son est capté par programme et non par sortie ; effet d'un changement de sortie : **à vérifier** | Selon le test | test |
| B8 | Batterie critique | faible | forte | Arrêt non bloquable (voulu) ; finalisation tentée | Rien à ajouter | — |
| B9 | Changement d'heure (été/hiver, fuseau) | forte | faible | Horloge interne indépendante de l'heure affichée ; seuls les noms de fichiers utilisent l'heure de départ | Rien à ajouter | — |

## C - Programmes capturés

| N° | Scénario | Prob. | Grav. | Aujourd'hui | Proposition | Prio. |
|---|---|---|---|---|---|---|
| C1 | Navigateur ou application fermé, planté, ou redémarré par une mise à jour | forte | forte | Image noire et silence, puis rattachement automatique au retour. **Mais** pendant l'enregistrement, aucun bouton pour rouvrir la fenêtre, et la relance automatique d'une application ne joue qu'au démarrage | Bouton « Rouvrir » sur les cartes « Fenêtre perdue » ; relance automatique en cours d'enregistrement pour les sources qui l'autorisent | P1 |
| C2 | Fenêtre réduite par erreur | forte | moyenne | Image figée, avertissement « Réduite (image figée) » | Bouton « Rétablir la fenêtre » (B4) | P1 |
| C3 | Profil de navigateur supprimé à la main | faible | moyenne | Recréé vide **sans avertissement** : connexions à refaire | Message « Profil introuvable : il faudra te reconnecter » | P3 |
| C4 | Navigateur désinstallé ou déplacé | faible | moyenne | Message d'échec de lancement au démarrage de l'enregistrement | Proposer de choisir le nouvel emplacement | P3 |
| C5 | Jeu en plein écran exclusif, contenu protégé | moyenne | moyenne | Image noire (limite connue, documentée) | Rien à ajouter | — |

## D - Encodage et disque

| N° | Scénario | Prob. | Grav. | Aujourd'hui | Proposition | Prio. |
|---|---|---|---|---|---|---|
| D1 | Encodeur planté ou figé | faible | forte | Relance automatique dans `…_partie2.mkv`, environ une seconde perdue (testé en simulation) | Rien à ajouter | — |
| D2 | Trop d'encodages simultanés pour une carte GeForce | faible | forte | NVIDIA limite le nombre d'encodages simultanés **pour tout l'ordinateur**, tous logiciels confondus (OBS, Discord, enregistrement NVIDIA en arrière-plan…) : 12 aujourd'hui selon NVIDIA, au moins 8 avec le pilote 570 exigé par Polyptyque. Au-delà, FFmpeg refuse avec un message trompeur (`OpenEncodeSessionEx failed: out of memory (10)`). L'encodeur refusé échoue trois fois, puis la source passe en erreur avec ce message. La limite réelle sera sans doute plutôt la puissance de la puce (à mesurer, voir le cahier des charges) | Reconnaître ce refus : bascule automatique de la source sur l'encodage par le processeur, avec un message clair (« limite de la carte NVIDIA atteinte, autres logiciels compris ») | P2 |
| D3 | Disque plein | moyenne | forte | Estimation avant le départ, alerte, arrêt propre sous 1 Go libre | Rien à ajouter | — |
| D4 | Disque externe débranché, dossier devenu inaccessible | faible | forte | L'encodeur échoue en boucle, puis chaque source passe en erreur avec un message technique | Bascule automatique vers un dossier de secours (`Vidéos\Polyptyque`) et message clair | P1 |
| D5 | Dossier sur une clé ou un disque en FAT32 | moyenne | moyenne | Fichiers limités à 4 Go par ce format : l'encodeur échoue à 4 Go et repart dans une nouvelle partie (environ une seconde perdue à chaque fois) | Détection à l'enregistrement des réglages et au démarrage : avertissement, et découpage propre avant 4 Go | P2 |
| D6 | Dossier sur un disque lent ou en réseau | moyenne | moyenne | Retard de l'encodeur et images perdues affichés | Avertissement au choix du dossier si le lecteur est amovible ou réseau | P3 |
| D7 | FFmpeg supprimé ou bloqué par l'antivirus | faible | forte | Au démarrage : message clair. En cours d'enregistrement : la relance échoue et la source passe en erreur | Message dédié (« FFmpeg a disparu : vérifier l'antivirus ») | P3 |

## E - Erreurs de manipulation

| N° | Scénario | Prob. | Grav. | Aujourd'hui | Proposition | Prio. |
|---|---|---|---|---|---|---|
| E1 | Fermer une fenêtre capturée par erreur | forte | forte | Voir C1 | Voir C1 | P1 |
| E2 | Supprimer une source ou une scène par erreur | moyenne | moyenne | Confirmation demandée ; pas d'annulation | Bouton « Annuler » dans le message pendant quelques secondes | P3 |
| E3 | Quitter Polyptyque pendant un enregistrement | moyenne | forte | Confirmation, puis finalisation | Rien à ajouter | — |
| E4 | Lancer Polyptyque deux fois | moyenne | moyenne | Voir A4 | Voir A4 | P1 |
| E5 | Choisir un dossier inadapté (clé, réseau, disque presque plein) | moyenne | moyenne | Estimation de l'autonomie affichée | Voir D5 et D6 | P2 |
| E6 | Débrancher le micro | moyenne | faible | Silence, puis reprise automatique au rebranchement | Rien à ajouter | — |
| E7 | Réglages trop lourds pour la machine (définition, débit, nombre de sources) | moyenne | moyenne | Retard de l'encodeur et images perdues affichés par source | Test de capacité au démarrage (déjà prévu au cahier des charges) | P3 |
| E8 | Oublier un enregistrement en cours | moyenne | moyenne | Arrêt automatique quand le disque est presque plein | Durée maximale facultative dans les réglages | P3 |
| E9 | Déplacer ou renommer un fichier en cours d'enregistrement | faible | forte | **À vérifier** | Selon le test | test |
| E10 | Modifier une scène pendant un enregistrement | moyenne | faible | Impossible (scènes verrouillées) | Rien à ajouter | — |

---

## Déjà couvert

> [!success] Points solides, vérifiés
> - Encodeur planté ou figé : relance dans une nouvelle partie, image et son recalés (tests de simulation avec un vrai plantage et un vrai blocage de FFmpeg).
> - Fichiers MKV écrits au fil de l'eau, par blocs d'une seconde.
> - Fenêtre perdue : image noire, silence, rattachement automatique (confirmé en test réel).
> - Disque surveillé, arrêt propre ; mise en veille automatique empêchée.
> - Arrêt ou déconnexion de Windows : blocage avec explication, sinon finalisation.
> - Micro débranché, accès au micro refusé : repris ou expliqué.
> - Fermeture de Polyptyque pendant un enregistrement : confirmation et finalisation.
> - Réglages écrits de façon atomique (jamais à moitié).

---

## Plan d'action proposé

### Lot 1 - Reprise après incident

> [!todo] Priorité 1
> - [ ] **Journal de session** (A1, B1) : au démarrage d'un enregistrement, Polyptyque note la scène, les sources et les fichiers en cours dans un petit fichier, effacé à l'arrêt normal. S'il est encore là au lancement suivant, c'est qu'il y a eu un incident :
> 	- bilan affiché : quels fichiers, quelle durée, lisibles ou non ;
> 	- **réparation sans réencodage** des fichiers interrompus (index reconstruit, durée correcte), l'original étant gardé jusqu'à la réussite ;
> 	- bouton **« Reprendre l'enregistrement »** (nouveaux fichiers, suffixe `_reprise`).
> - [ ] **Fils protégés** (A2) : une erreur interne dans une source est affichée et sa capture relancée.
> - [ ] **Instance unique** (A4) : un second lancement ramène la fenêtre existante.
> - [ ] **Réglages sauvegardés** (A5) : écriture forcée sur le disque, cinq sauvegardes tournantes, restauration automatique.
> - [ ] **Rouvrir et rétablir** (C1, C2, B4) : boutons « Rouvrir » (fenêtre perdue) et « Rétablir la fenêtre » (fenêtre réduite) pendant l'enregistrement ; relance automatique des sources qui l'autorisent.
> - [ ] **Dossier de secours** (D4) : si le dossier d'enregistrement devient inaccessible, la suite est écrite dans `Vidéos\Polyptyque`, avec message.

### Lot 2 - Situations prévisibles

> [!todo] Priorité 2
> - [ ] **Mise en veille** (B3) : %%PROPOSITION-B3-PLAN%%
> - [ ] **Après un redémarrage de Windows Update** (B2) : %%PROPOSITION-B2-PLAN%%
> - [ ] **Limite d'encodages de la carte NVIDIA** (D2) : bascule automatique sur le processeur pour la source refusée.
> - [ ] **FAT32** (D5) : avertissement et découpage propre avant 4 Go.
> - [ ] **Conversion MP4** (A6) : nom temporaire jusqu'à la vérification.

### Lot 3 - Confort et prévention

> [!todo] Priorité 3
> - [ ] Journaux supprimés après 30 jours (A7).
> - [ ] Arrêt d'urgence par raccourci, même si l'interface est figée (A3).
> - [ ] Profil de navigateur disparu signalé (C3) ; navigateur déplacé retrouvé (C4).
> - [ ] Annulation d'une suppression (E2).
> - [ ] Avertissement pour un dossier sur disque amovible ou réseau (D6) ; message dédié si FFmpeg disparaît (D7).
> - [ ] Test de capacité au démarrage (E7) ; durée maximale facultative (E8).

---

## Tests à mener sur la machine de référence

> [!warning] Sans enjeu
> À faire sur un enregistrement de test de quelques minutes, jamais sur un enregistrement important.

| Test | Comment | Ce qu'on vérifie |
|---|---|---|
| Fermeture brutale | Gestionnaire des tâches > Polyptyque > Fin de tâche, pendant un enregistrement | Les MKV sont lisibles jusqu'à la fermeture (A1, A3) |
| Pilote graphique | <kbd>Win</kbd> + <kbd>Ctrl</kbd> + <kbd>Maj</kbd> + <kbd>B</kbd> (redémarre le pilote graphique, écran noir une seconde) | Capture rattachée, nouvelle partie si l'encodeur a planté (B6) |
| Sortie audio | Brancher ou changer de casque pendant l'enregistrement | Le son des sources continue (B7) |
| Verrouillage | <kbd>Win</kbd> + <kbd>L</kbd> pendant deux minutes, puis déverrouiller | Image pendant le verrouillage, reprise après (B5) |
| Mise en veille | Démarrer > Marche/Arrêt > Mettre en veille pendant un enregistrement, réveiller après deux minutes | Fichiers lisibles, durée du rattrapage d'images figées, image et son repris après le réveil (B3) |
| Renommage | Renommer un MKV en cours d'enregistrement dans l'Explorateur | Refus de Windows ou effet sur l'enregistrement (E9) |
| Mémoire | Relever la mémoire de Polyptyque et des FFmpeg au début et à la fin du test de 12 h | Pas de croissance continue (A9) |

---

## Questions ouvertes

- [ ] **Reprise après incident** : proposée d'un clic, ou automatique (Polyptyque relancé et l'enregistrement repris sans intervention, utile pour une session de nuit sans surveillance) ?
- [ ] **Mise en veille demandée pendant un enregistrement** : arrêter proprement, ou mettre en pause et reprendre au réveil ?
- [ ] **Réparation des fichiers interrompus** : remplacer le fichier abîmé par sa version réparée, ou garder les deux ?
- [ ] **Durée de conservation des journaux** : 30 jours ?

---

## Sources

%%SOURCES%%
