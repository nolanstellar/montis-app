#!/bin/bash
# PUBLIER LA VERSION MAC (universelle, signée Developer ID, notarisée, agrafée) sur la publication GitHub de la version courante.
# À lancer sur le Mac de l'éditeur, une fois le tag vX.Y.Z poussé (la chaîne GitHub construit Windows/Linux et crée la publication).
#   bash scripts/publier-mac.sh
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
V=$(node -p "require('./package.json').version")
B=src-tauri/target/universal-apple-darwin/release/bundle
set -a; source ~/Montis/licences/apple-notarisation.env; set +a
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/montis.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
echo "== construction universelle v$V (signature + notarisation de l'app par Tauri)"
npx tauri build --target universal-apple-darwin --config src-tauri/tauri.macos-local.conf.json
echo "== dmg : signature, notarisation, agrafe"
codesign --force --sign "Developer ID Application: Nolan Viel (NK8ZTP3KWS)" --timestamp "$B/dmg/Montis_${V}_universal.dmg"
xcrun notarytool submit "$B/dmg/Montis_${V}_universal.dmg" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" --wait --timeout 30m | grep -E "status:" | tail -1
xcrun stapler staple "$B/dmg/Montis_${V}_universal.dmg"
cp "$B/dmg/Montis_${V}_universal.dmg" "$B/dmg/Montis-Mac.dmg"
echo "== attente de la publication GitHub v$V (chaîne Windows/Linux)"
for i in $(seq 1 60); do gh release view "v$V" -R nolanstellar/montis-app >/dev/null 2>&1 && break; sleep 20; done
gh release view "v$V" -R nolanstellar/montis-app >/dev/null 2>&1 || gh release create "v$V" -R nolanstellar/montis-app -t "Montis v$V" -n "Montis pour Mac (universel, signé et notarisé) : Montis-Mac.dmg. Windows : Montis-Windows-setup.exe."
echo "== dépôt des fichiers Mac"
gh release upload "v$V" -R nolanstellar/montis-app "$B/dmg/Montis_${V}_universal.dmg" "$B/dmg/Montis-Mac.dmg" "$B/macos/Montis.app.tar.gz" "$B/macos/Montis.app.tar.gz.sig" --clobber
echo "== latest.json : entrées Mac (attend celui de la chaîne s'il n'est pas encore là)"
T=$(mktemp -d)
for i in $(seq 1 60); do gh release download "v$V" -R nolanstellar/montis-app -p latest.json -D "$T" >/dev/null 2>&1 && break; sleep 20; done
[ -f "$T/latest.json" ] || echo "{\"version\":\"$V\",\"notes\":\"\",\"pub_date\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"platforms\":{}}" > "$T/latest.json"
python3 - "$T/latest.json" "$B/macos/Montis.app.tar.gz.sig" "$V" <<'PY'
import json,sys
p,sig,v=sys.argv[1:]; d=json.load(open(p)); s=open(sig).read().strip()
url=f"https://github.com/nolanstellar/montis-app/releases/download/v{v}/Montis.app.tar.gz"
for k in ("darwin-universal","darwin-aarch64","darwin-x86_64"): d.setdefault("platforms",{})[k]={"signature":s,"url":url}
d["version"]=v; json.dump(d,open(p,"w"),indent=2); print("plateformes :", ", ".join(d["platforms"]))
PY
gh release upload "v$V" -R nolanstellar/montis-app "$T/latest.json" --clobber
echo "== vérification Gatekeeper sur le fichier téléchargé"
curl -sL -o "$T/t.dmg" "https://github.com/nolanstellar/montis-app/releases/latest/download/Montis-Mac.dmg"
xattr -w com.apple.quarantine "0083;$(printf %x $(date +%s));Safari;$(uuidgen)" "$T/t.dmg"
spctl -a -t open --context context:primary-signature -vv "$T/t.dmg" 2>&1 | head -2
rm -rf "$T"
echo "PUBLIÉ : v$V"
