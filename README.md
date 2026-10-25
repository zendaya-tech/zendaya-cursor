# Zendaya Cursor

Application Windows native en Rust pour sélectionner et appliquer des thèmes **ZCursor**.
La collection contient uniquement les archives `.zcursor` disponibles sur votre ordinateur.
Aucun thème de démonstration ni atelier PNG dans l’interface.

## Utiliser un thème

1. Dans **Mes curseurs**, choisissez une carte avec aperçu ou cliquez sur **Importer un .zcursor**.
2. Le panneau d’aperçu affiche les vrais curseurs, animations et points de clic du thème.
3. **Effets** active les traînées et animations au clic contenues dans le thème sélectionné.
   Un thème sans effets affiche cette information ; aucun effet générique ne lui est substitué.
4. **Préférences** règle la résolution et la vitesse des animations pour ce thème.
   Les réglages sont enregistrés séparément pour chaque archive.
5. Cliquez sur **Appliquer au bureau**. **Restaurer Windows** ou **Ctrl + Alt + R**
   recharge le schéma de curseurs Windows et arrête les effets.

Les archives importées sont copiées dans `%APPDATA%/ZendayaCursor/themes/`.
Le logiciel consulte aussi `dist/zcursor/` et le dossier `themes/` à côté de l’exécutable.
Les préférences sont conservées dans `%APPDATA%/ZendayaCursor/settings.json`.
Au démarrage, le dernier thème est sélectionné mais il n’est jamais appliqué automatiquement.
Changer de sélection ou de réglages ne change pas le thème déjà appliqué avant le prochain clic sur Appliquer.

## Lancer avec Cargo

```powershell
cargo run -- --theme "dist/zcursor/Fluent.zcursor"
```

Les versions distribuées sont des applications graphiques sans console.
Les builds locaux désactivent le PDB final pour réduire l’espace disque nécessaire.

## Format ZCursor et conversion

Une archive `.zcursor` contient `theme.json`, des bandes PNG, un aperçu et éventuellement
la source originale. Voir [le format](docs/zcursor-format.md).

```powershell
python tools/convert_cursorfx.py "C:\Users\Public\Documents\Stardock\CursorFX\Themes" dist/zcursor
cargo run -- --check-zcursor dist/zcursor
```

Le convertisseur nécessite Python et Pillow. Il préserve les fichiers originaux,
les frames, les hotspots, les auteurs et les paramètres non documentés.
Les lignes des bitmaps CursorFX sont inversées à la conversion pour produire des PNG
orientés du haut vers le bas. Les hotspots sont déjà dans ce repère et restent inchangés.
Les premières archives converties avant la version 0.3.0 sont corrigées à la lecture,
sans modifier le fichier : les nouveaux PNG ne sont pas inversés une seconde fois.

`--check-zcursor` valide les archives et les curseurs CUR/ANI via Win32, sans les appliquer.
Les tests vérifient l’orientation, les hotspots, les animations, les variantes de résolution,
les préférences par thème, les fichiers hostiles et les handles Windows.

## Limites

Les curseurs de texte, attente, lien et redimensionnement utilisent les rôles Windows correspondants.
Les images de traînée et de clic sont dessinées dans une fenêtre transparente qui laisse passer les clics.
Certaines applications utilisent leurs propres curseurs et peuvent ignorer le thème système.
Les réglages propriétaires CursorFX ne sont pas tous transposés ; les données originales restent dans l’archive.
Les effets très éloignés du pointeur peuvent être rognés au-delà de 320 pixels.
La fermeture normale du logiciel restaure les curseurs Windows.

## Versions Windows

Le workflow **Windows executable** compile, teste et vérifie le projet à chaque push sur `main`.
Les tags `vX.Y.Z` correspondant à `Cargo.toml` publient l’exécutable Windows x64,
un ZIP portable et `SHA256SUMS.txt` dans **Releases**. Les versions publiées ne sont pas remplacées.

Le code du projet est sous licence MIT. Les thèmes tiers conservent leurs auteurs et licences.
