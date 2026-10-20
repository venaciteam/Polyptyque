# Polyptyque

Enregistrement simultané de plusieurs fenêtres, chacune dans son propre fichier
MKV, avec **uniquement le son de cette fenêtre**. Conçu pour le montage
multi-angle et l'archivage, sur des sessions pouvant durer 12 heures.

Windows d'abord ; architecture prévue pour macOS et Linux.

> [!NOTE]
> Projet en développement : moteur validé (étape 0), interface graphique en
> cours (étape 2).

## Documentation

- [Cahier des charges](docs/cahier-des-charges.md) : besoin, décisions,
  architecture, feuille de route.

## Organisation du code

| Dossier | Rôle |
|---|---|
| `crates/polyptyque-core` | Modèle de données, nommage, synchronisation, commandes FFmpeg (tout système) |
| `crates/polyptyque-windows` | Capture d'image et de son, fenêtres, navigateurs (Windows) |
| `crates/polyptyque-engine` | Sessions d'enregistrement, fils de capture, encodeurs |
| `crates/polyptyque-app` | Interface graphique ([Slint](https://slint.dev)) ; apparence dans `ui/`, thème dans `ui/theme.slint` |
| `crates/polyptyque-cli` | Version en console (prototype de l'étape 0) |

## Branches

| Branche | Rôle |
|---|---|
| `dev` | Travail en cours |
| `main` | Versions validées : `dev` y est fusionnée après validation |

## Fabrication

Chaque envoi sur GitHub lance les vérifications et fabrique le prototype
Windows (onglet **Actions**, artefact `Polyptyque-prototype-windows`).

En local :

```sh
cargo test --workspace          # tests, dont une session simulée (FFmpeg requis)
cargo build --release -p polyptyque-app -p polyptyque-cli
cargo run -p polyptyque-app     # hors Windows : mode démonstration (fausses fenêtres)
```

## Licence

GPL v3 ou ultérieure (voir [LICENSE](LICENSE)). FFmpeg, livré à côté, est
distribué sous sa propre licence GPL.
