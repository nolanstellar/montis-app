# Montis — l'application

Montis est l'assistant de Stellar Agency. Cette application l'installe sur votre ordinateur : il vit dans la barre des
menus (Mac) ou la zone de notification (Windows), s'appelle par la voix ou un raccourci clavier, répond, agit sur ce
poste, puis s'efface. Le cerveau tourne sur le serveur de votre entreprise ; l'application en est les mains.

## Télécharger

Dernière version : **[Releases](https://github.com/nolanstellar/montis-app/releases/latest)**.

| Système | Fichier |
|---|---|
| Mac Apple Silicon (M1 et suivants) | `Montis_x.y.z_aarch64.dmg` |
| Mac Intel | `Montis_x.y.z_x64.dmg` |
| Windows 10/11 | `Montis_x.y.z_x64-setup.exe` (ou `.msi`) |
| Linux | `.AppImage` ou `.deb` |

Version non signée : sur Mac, clic droit sur l'application › **Ouvrir** la première fois (ou Réglages Système ›
Confidentialité et sécurité › « Ouvrir quand même ») ; sur Windows, « Informations complémentaires » › « Exécuter quand même ».

## Premier lancement

1. L'icône Montis apparaît dans la barre. Cliquez-la, ou appuyez sur **Option + Espace** (Mac) / **Ctrl + Espace** (Windows).
2. Entrez le mot de passe d'entreprise une fois.
3. Présentez-vous (prénom, façon de vous adresser à vous, tutoiement, nom de l'assistant : il devient le mot qui le réveille).
4. Cochez ce qu'il peut faire sur cet ordinateur.

## Autorisations à accorder vous-même (Mac)

Réglages Système › Confidentialité et sécurité :
- **Microphone** › Montis — pour vous entendre.
- **Enregistrement d'écran** › Montis — pour les captures d'écran.
- **Accessibilité** › Montis — pour déplacer et redimensionner les fenêtres des autres applications.
- **Automatisation** › Montis › System Events, Finder — pour le volume, la veille, les applications.
Chacune est demandée une fois par macOS, à la première action qui en a besoin.

Sur Windows, aucune autorisation système n'est nécessaire au-delà du micro (demandé à la première écoute).

## Réglages de l'application

Icône Montis › Réglages de l'application : adresse du cœur, raccourci clavier, fenêtre compacte ou complète.
Les mises à jour se cherchent depuis le même menu et s'installent seules.

## Ce que fait la coque

Fichiers (chercher, ouvrir, lire, créer, renommer, déplacer), applications (lancer, fermer, basculer), capture d'écran,
presse-papiers, volume, luminosité (Windows), veille, verrouillage, impression, informations système, fenêtres
(gauche, droite, plein écran, réduire, déplacer), notifications. Tout le reste — mémoire, agenda, mails, décisions —
appartient au cœur.

## Télécharger (adresses stables, dernière version)

- Mac (puce Apple et Intel, un seul fichier, signé et notarisé) : https://github.com/nolanstellar/montis-app/releases/latest/download/Montis-Mac.dmg
- Windows : https://github.com/nolanstellar/montis-app/releases/latest/download/Montis-Windows-setup.exe

## Publier une version

1. `npm version <x.y.z>` (met à jour package.json ; reporter la version dans `src-tauri/tauri.conf.json` et `src-tauri/Cargo.toml`), commit, `git tag vx.y.z`, `git push --tags` : la chaîne GitHub construit Windows et Linux, crée la publication, dépose `latest.json` et `Montis-Windows-setup.exe`.
2. **Mac, sur le Mac de l'éditeur** (certificat Developer ID et clé d'API dans le trousseau / `~/Montis/licences`) :
   ```bash
   set -a; source ~/Montis/licences/apple-notarisation.env; set +a
   export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/montis.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
   npx tauri build --target universal-apple-darwin --config src-tauri/tauri.macos-local.conf.json
   ```
   L'application est signée, notarisée et agrafée par la construction. Puis signer, notariser et agrafer le .dmg, et déposer :
   ```bash
   B=src-tauri/target/universal-apple-darwin/release/bundle
   codesign --force --sign "Developer ID Application: Nolan Viel (NK8ZTP3KWS)" --timestamp $B/dmg/Montis_<v>_universal.dmg
   xcrun notarytool submit $B/dmg/Montis_<v>_universal.dmg --key ~/Montis/licences/AuthKey_3PHYJH735H.p8 --key-id 3PHYJH735H --issuer aba0c4de-2473-46df-bbe5-5bd4aaca41ca --wait
   xcrun stapler staple $B/dmg/Montis_<v>_universal.dmg
   cp $B/dmg/Montis_<v>_universal.dmg $B/dmg/Montis-Mac.dmg
   gh release upload v<v> $B/dmg/Montis_<v>_universal.dmg $B/dmg/Montis-Mac.dmg $B/macos/Montis.app.tar.gz $B/macos/Montis.app.tar.gz.sig --clobber
   ```
   Enfin compléter `latest.json` de la publication avec les entrées `darwin-universal`, `darwin-aarch64`, `darwin-x86_64` (url du `Montis.app.tar.gz`, signature du `.sig`) et le redéposer.
3. Vérification : télécharger `Montis-Mac.dmg` depuis l'adresse stable, poser la quarantaine (`xattr -w com.apple.quarantine "0083;…;Safari;…"`), `spctl -a -t open --context context:primary-signature -vv` sur le .dmg et `spctl -a -vv -t exec` sur l'app montée : « accepted, source=Notarized Developer ID ».
