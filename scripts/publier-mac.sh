#!/bin/bash
# PUBLIER LA VERSION MAC (universelle, signée Developer ID, notarisée, agrafée) sur la publication GitHub de la version courante.
# Le canal se déduit de la version : « x.y.z-beta.n » → application « Montis Bêta » (pré-version, publication mobile « beta ») ;
# « x.y.z » → application stable (seulement sur « publie en stable » de Nolan ; l'application stable suit releases/latest).
#   bash scripts/publier-mac.sh
set -euo pipefail
cd "$(dirname "$0")/.."
export PATH="$HOME/.cargo/bin:$PATH"
V=$(node -p "require('./package.json').version")
# TROIS FICHIERS PORTENT LE NUMÉRO (package.json, tauri.conf.json, Cargo.toml) et ils peuvent diverger : le 07/09,
# une bêta 8 a été publiée sous l'étiquette 7 parce que seul tauri.conf.json avait été relevé. On refuse de construire
# tant qu'ils ne disent pas la même chose — une étiquette fausse est pire qu'une publication manquée.
VC=$(node -p "require('./src-tauri/tauri.conf.json').version")
VR=$(grep -m1 '^version = ' src-tauri/Cargo.toml | sed 's/.*"\(.*\)".*/\1/')
if [ "$V" != "$VC" ] || [ "$V" != "$VR" ]; then
  echo "VERSIONS DIVERGENTES — package.json $V, tauri.conf.json $VC, Cargo.toml $VR. Alignez-les puis relancez." >&2
  exit 1
fi
B=src-tauri/target/universal-apple-darwin/release/bundle
BETA=0; case "$V" in *-beta*) BETA=1;; esac
NOM=$([ $BETA = 1 ] && echo "Montis-Beta" || echo "Montis")
set -a; source ~/Montis/licences/apple-notarisation.env; set +a
export TAURI_SIGNING_PRIVATE_KEY="$(cat ~/.tauri/montis.key)" TAURI_SIGNING_PRIVATE_KEY_PASSWORD=""
# Les surcouches de configuration : signature locale, et le canal bêta (nom, identifiant, point de mise à jour) le cas échéant.
CONF=$(mktemp -t montis-conf).json
if [ $BETA = 1 ]; then python3 - src-tauri/tauri.macos-local.conf.json src-tauri/tauri.beta.conf.json "$CONF" <<'PY'
import json,sys
a=json.load(open(sys.argv[1])); b=json.load(open(sys.argv[2]))
def fusion(x,y):
    for k,v in y.items():
        if isinstance(v,dict) and isinstance(x.get(k),dict): fusion(x[k],v)
        else: x[k]=v
    return x
json.dump(fusion(a,b),open(sys.argv[3],"w"))
PY
else cp src-tauri/tauri.macos-local.conf.json "$CONF"; fi
echo "== construction universelle v$V ($([ $BETA = 1 ] && echo 'canal BÊTA, Montis Bêta' || echo 'canal STABLE')) — signature + notarisation de l'app par Tauri"
rm -rf "$B/dmg" "$B/macos"
npx tauri build --target universal-apple-darwin --config "$CONF"
DMG=$(ls "$B"/dmg/*.dmg | head -1); TAR=$(ls "$B"/macos/*.app.tar.gz | head -1); SIG="$TAR.sig"
echo "== dmg : signature, notarisation, agrafe ($DMG)"
codesign --force --sign "Developer ID Application: Nolan Viel (NK8ZTP3KWS)" --timestamp "$DMG"
xcrun notarytool submit "$DMG" --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" --wait --timeout 30m | grep -E "status:" | tail -1
xcrun stapler staple "$DMG"
# Noms sans espace ni accent pour GitHub ; l'archive de mise à jour porte le nom du canal.
# En canal STABLE, les fichiers portent DÉJÀ leur nom de destination : `cp` sur lui-même rend 1, et `set -e` arrêtait la
# publication juste après la notarisation — application construite, signée, notariée, agrafée… et jamais mise en ligne
# (07/09). En bêta le nom changeait, le défaut ne s'était donc jamais montré : il n'apparaît qu'en stable, c'est-à-dire
# au seul moment où il compte. Les quatre copies passent maintenant par la même garde.
copier() { [ "$1" -ef "$2" ] || cp "$1" "$2"; }
copier "$DMG" "$B/dmg/${NOM}_${V}_universal.dmg"; copier "$DMG" "$B/dmg/${NOM}-Mac.dmg"; copier "$TAR" "$B/macos/${NOM}.app.tar.gz"; copier "$SIG" "$B/macos/${NOM}.app.tar.gz.sig"
# LE TAG DÉCLENCHE LA CHAÎNE WINDOWS/LINUX — et rien ne le posait (07/09). Pour les bêtas il était poussé à la main, si
# bien que le défaut ne s'est vu qu'en stable : le script attendait vingt minutes une publication que personne n'avait
# demandée. Il le pose lui-même, à partir du commit courant, et ne fait rien s'il existe déjà.
if ! git rev-parse "v$V" >/dev/null 2>&1; then
  git tag -a "v$V" -m "Montis v$V" && echo "== tag v$V posé sur $(git rev-parse --short HEAD)"
fi
git push -q origin "v$V" 2>/dev/null || true
echo "== attente de la publication GitHub v$V (chaîne Windows/Linux)"
for i in $(seq 1 60); do gh release view "v$V" -R nolanstellar/montis-app >/dev/null 2>&1 && break; sleep 20; done
gh release view "v$V" -R nolanstellar/montis-app >/dev/null 2>&1 || gh release create "v$V" -R nolanstellar/montis-app $([ $BETA = 1 ] && echo --prerelease) -t "Montis v$V" -n "Montis pour Mac (universel, signé et notarisé) : ${NOM}-Mac.dmg."
echo "== dépôt des fichiers Mac"
gh release upload "v$V" -R nolanstellar/montis-app "$B/dmg/${NOM}_${V}_universal.dmg" "$B/dmg/${NOM}-Mac.dmg" "$B/macos/${NOM}.app.tar.gz" "$B/macos/${NOM}.app.tar.gz.sig" --clobber
echo "== latest.json : entrées Mac (attend celui de la chaîne s'il n'est pas encore là)"
T=$(mktemp -d)
for i in $(seq 1 60); do gh release download "v$V" -R nolanstellar/montis-app -p latest.json -D "$T" >/dev/null 2>&1 && break; sleep 20; done
[ -f "$T/latest.json" ] || echo "{\"version\":\"$V\",\"notes\":\"\",\"pub_date\":\"$(date -u +%Y-%m-%dT%H:%M:%SZ)\",\"platforms\":{}}" > "$T/latest.json"
python3 - "$T/latest.json" "$B/macos/${NOM}.app.tar.gz.sig" "$V" "$NOM" <<'PY'
import json,sys
p,sig,v,nom=sys.argv[1:]; d=json.load(open(p)); s=open(sig).read().strip()
url=f"https://github.com/nolanstellar/montis-app/releases/download/v{v}/{nom}.app.tar.gz"
for k in ("darwin-universal","darwin-aarch64","darwin-x86_64"): d.setdefault("platforms",{})[k]={"signature":s,"url":url}
d["version"]=v; json.dump(d,open(p,"w"),indent=2); print("plateformes :", ", ".join(d["platforms"]))
PY
gh release upload "v$V" -R nolanstellar/montis-app "$T/latest.json" --clobber
if [ $BETA = 1 ]; then
  echo "== canal bêta : publication mobile « beta »"
  gh release view beta -R nolanstellar/montis-app >/dev/null 2>&1 || gh release create beta -R nolanstellar/montis-app --prerelease -t "Canal bêta (Montis Bêta)" -n "Publication mobile : latest.json de la dernière pré-version. Ne pas supprimer."
  gh release upload beta -R nolanstellar/montis-app "$T/latest.json" --clobber
fi
echo "== vérification Gatekeeper sur le fichier téléchargé"
URL=$([ $BETA = 1 ] && echo "https://github.com/nolanstellar/montis-app/releases/download/v$V/${NOM}-Mac.dmg" || echo "https://github.com/nolanstellar/montis-app/releases/latest/download/Montis-Mac.dmg")
curl -sL -o "$T/t.dmg" "$URL"
xattr -w com.apple.quarantine "0083;$(printf %x $(date +%s));Safari;$(uuidgen)" "$T/t.dmg"
spctl -a -t open --context context:primary-signature -vv "$T/t.dmg" 2>&1 | head -2
rm -rf "$T"
# La copie de construction n'est pas une installation : on la retire, et Spotlight n'indexe plus ce dossier.
rm -rf "$B"/macos/*.app "$B"/dmg/*.app; touch src-tauri/target/.metadata_never_index
echo "PUBLIÉ : v$V ($NOM)"
# APRÈS UNE PUBLICATION EN STABLE (point 23) : on pose le repère et on écrit la note — ce qui change, ce qu'il faut
# vérifier en premier, et ce qui ne se prouve que sur un vrai poste. Écrite à partir des commits, jamais de mémoire.
if [ $BETA = 0 ] && [ -d ../agent-stellar ]; then
  NOTE=../agent-stellar/donnees/note-publication-$V.md
  node --experimental-strip-types --no-warnings ../agent-stellar/scripts/note-publication.ts "" "$V" > "$NOTE" 2>/dev/null \
    && echo "NOTE DE PUBLICATION : $NOTE" && cat "$NOTE"
  git -C ../agent-stellar rev-parse HEAD > ../agent-stellar/donnees/PUBLIE-STABLE
  echo "Repère posé : la prochaine note partira d'ici."
fi
