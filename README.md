# Polyptyque

Enregistrement simultané de plusieurs fenêtres, chacune dans son propre fichier
MKV, avec **uniquement le son de cette fenêtre**. Conçu pour le montage
multi-angle et l'archivage, sur des sessions pouvant durer 12 heures.

Windows d'abord ; architecture prévue pour macOS et Linux.

> [!NOTE]
> Projet au stade de prototype (étape 0) : moteur d'enregistrement piloté par
> une console. L'interface graphique viendra ensuite.

## Documentation

- [Cahier des charges](docs/cahier-des-charges.md) : besoin, décisions,
  architecture, feuille de route.

## Organisation du code

| Dossier | Rôle |
|---|---|
| `crates/polyptyque-core` | Modèle de données, nommage, synchronisation, commandes FFmpeg (tout système) |
| `crates/polyptyque-windows` | Capture d'image et de son, fenêtres, navigateurs (Windows) |
| `crates/polyptyque-engine` | Sessions d'enregistrement, fils de capture, encodeurs |
| `crates/polyptyque-cli` | Prototype en console |

## Fabrication

Chaque envoi sur GitHub lance les vérifications et fabrique le prototype
Windows (onglet **Actions**, artefact `Polyptyque-prototype-windows`).

En local :

```sh
cargo test --workspace          # tests, dont une session simulée (FFmpeg requis)
cargo build --release -p polyptyque-cli
```

## Licence

GPL v3 ou ultérieure (voir [LICENSE](LICENSE)). FFmpeg, livré à côté, est
distribué sous sa propre licence GPL.
