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

Les curseurs système sont statiques ; les traînées et animations de clic sont dessinées
par une fenêtre transparente qui laisse passer les clics. Les applications qui utilisent
leur propre curseur, les jeux en plein écran exclusif et les écrans Windows protégés
peuvent ignorer ces effets. Les curseurs texte, attente et redimensionnement sont conservés.
Les effets utilisent les coordonnées physiques pour les écrans à plusieurs DPI. Les traînées très rapides peuvent être rognées au-delà de 320 pixels derrière le pointeur.
Aucun service, démarrage automatique, son de clic ou import de paquet CursorFX propriétaire.

Les thèmes vectoriels et l'interface sont originaux et distribués sous licence MIT.
