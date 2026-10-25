# Zendaya Cursor

Application de personnalisation du curseur Windows, écrite en Rust avec egui/eframe.
Projet indépendant inspiré du principe de CursorFX, sans code ni thèmes Stardock.

## Lancer

```powershell
cargo run
```

Windows 10/11 est nécessaire pour appliquer un curseur au système et afficher les effets
sur le bureau. Les thèmes et l'atelier restent utilisables sur les autres plateformes.

## Utilisation

- Choisir parmi six thèmes originaux, régler la couleur, la taille et l'ombre.
- Déplacer la souris et cliquer dans l'aperçu pour tester les effets.
- Cliquer sur **Appliquer au bureau** pour remplacer les pointeurs normal et liens,
  et activer la traînée et les effets de clic configurés.
- **Restaurer Windows** ou **Ctrl + Alt + R** recharge le schéma Windows configuré et
  coupe les effets. La fermeture normale restaure aussi les curseurs.
- L'atelier importe des PNG (20 Mo maximum, 4096 × 4096 maximum), règle le point actif
  et exporte des fichiers `.cur` standard à bitmap 32 bits avec transparence.

Les préférences sont enregistrées dans `%APPDATA%\ZendayaCursor\settings.json`.
Le thème système n'est jamais activé automatiquement au démarrage. Les images importées
restent à leur emplacement d'origine : conserver le fichier pour réutiliser son thème.
En cas d'arrêt forcé, relancer l'application et restaurer Windows, ou utiliser :

```powershell
cargo run -- --restore
```

## Vérification

```powershell
cargo test
cargo run -- --self-test
cargo clippy --all-targets -- -D warnings
```

`--self-test` crée et libère les six curseurs natifs sans remplacer ceux du système.
`--snapshot C:\chemin\apercu.png` ouvre l'application, capture son interface puis la ferme.

## Limites de cette première version

Les formes intégrées et les PNG simples restent statiques. Les thèmes `.zcursor`
chargent leurs animations comme curseurs Windows `.ani`. Les traînées et animations de clic sont dessinées
par une fenêtre transparente qui laisse passer les clics. Les applications qui utilisent
leur propre curseur, les jeux en plein écran exclusif et les écrans Windows protégés
peuvent ignorer ces effets. Les thèmes ZCursor peuvent remplacer les rôles normal, texte, attente, lien, aide, déplacement et redimensionnement.
Les effets utilisent les coordonnées physiques pour les écrans à plusieurs DPI. Les traînées très rapides peuvent être rognées au-delà de 320 pixels derrière le pointeur.
Aucun service, démarrage automatique, son de clic ou import de paquet CursorFX propriétaire.

Les thèmes vectoriels et l'interface sont originaux et distribués sous licence MIT.

## Exécutables Windows sur GitHub

Le workflow **Windows executable** compile et teste l'application à chaque push sur `main`.
Dans **Actions**, ouvrir le run réussi et télécharger l'artefact Windows x64 : il contient
un `.exe` direct, un ZIP portable et `SHA256SUMS.txt`. Le ZIP inclut aussi les instructions
et la licence. L'exécutable de release est une application graphique sans console.

Un tag `vX.Y.Z` correspondant à `Cargo.toml` publie automatiquement ces fichiers dans
**Releases** après les contrôles. On peut aussi lancer le workflow manuellement et cocher
**publish**. Une version déjà publiée ne peut pas être remplacée.

## Conversion des thèmes CursorFX en ZCursor

Le convertisseur autonome `tools/convert_cursorfx.py` crée des archives `.zcursor`
avec PNG, animations, hotspots, aperçu et source originale. Voir [le format et la commande](docs/zcursor-format.md).
Les fichiers convertis sont conservés localement dans `dist/zcursor/`.
Dans **Mes curseurs**, cliquez sur **Importer un .zcursor**. Le thème est copié dans
`%APPDATA%/ZendayaCursor/themes/`, puis apparaît dans la collection. Les archives locales
de `dist/zcursor/` et du dossier `themes/` à côté de l’exécutable sont aussi proposées.
Choisissez un thème, vérifiez ses rôles et animations dans l’aperçu, puis cliquez sur
**Appliquer au bureau**. Importer ou démarrer le logiciel ne modifie pas les curseurs système.

Les dimensions, hotspots et variantes de résolution sont conservés. Le menu d’aperçu
sélectionne également l’échelle utilisée lors de l’application. L’atelier peut exporter
l’image sélectionnée en `.cur`. Les effets de type traînée suivent le mouvement et les
autres effets sont déclenchés au clic ; les indicateurs propriétaires CursorFX non
transposés restent conservés dans l’archive.

```powershell
cargo run -- --theme "dist/zcursor/Fluent.zcursor"
cargo run -- --check-zcursor "dist/zcursor"
```

`--check-zcursor` vérifie les archives et charge les fichiers CUR/ANI avec Win32,
sans les appliquer au système. Les archives sont lues sans extraction ; les chemins
traversants, fichiers dupliqués, dimensions incohérentes et ressources excessives
sont rejetés.
