#!/bin/sh
# Retakes every screenshot in docs/screenshots inside the isolated KWin.
# Dry run with temporary settings: nothing touches the card or your config.
# Usage: dev/screenshots.sh [out-dir]   (default: docs/screenshots)
cd "$(dirname "$0")/.."
OUT=$(realpath -m "${1:-docs/screenshots}"); mkdir -p "$OUT"
cargo build --release -q
cat > /tmp/sbshots-run.sh <<EOS
#!/bin/sh
B="$PWD/target/release/sbcommand"; A="$PWD/assets/demo"; O="$OUT"
shot() {
  d=\$(mktemp -d /tmp/sbshot-cfg.XXXX); echo "\$5" > "\$d/settings.json"
  env SBCOMMAND_DRYRUN=1 SBCOMMAND_CONFIG_DIR="\$d" SBCOMMAND_SHOT_SIZE=1280x820 SBCOMMAND_SHOT="\$O/\$1.png" \\
      SBCOMMAND_DEMO_MEDIA="\$3" SBCOMMAND_DEMO_ART="\$A/\$4.png" \${6:+SBCOMMAND_SHOT_PATH=\$O/signal-path-\$6.png} "\$B" \$2
  rm -rf "\$d"
}
ROCK='"eqs":{"music":"Rock"},"eq":{"speakers":"Rock","headphones":"Rock"}'
shot music "" music music "{\"activity\":\"music\",\$ROCK,\"listen\":\"51\"}" music
shot movie "" movie film '{"activity":"movie","styles":{"movie":"Cinema"},"listen":"51"}' film
shot game "" game game '{"activity":"game","styles":{"game":"FPS"},"game_footsteps":true,"listen":"51"}'
shot pure "" music music "{\"activity\":\"music\",\"sbx_enabled\":false,\"listen\":\"51\",\$ROCK}"
shot rears "" music music "{\"activity\":\"music\",\"listen\":\"51\",\"stereo_on\":\"rear\",\$ROCK}" rears
shot mixer mixer movie film '{"activity":"movie","boost_db":3,"listen":"51"}'
shot crystalvoice crystalvoice "" music '{"activity":"music","rec":{"voice_clarity":true,"noise_reduction":true,"smart_volume":true,"svm_level":60,"focus":true,"focus_angle":90}}'
shot setup setup "" music '{"activity":"music","speaker_type":"dtt2200","crossover":140}'
shot settings settings "" music '{"activity":"music"}'
EOS
chmod +x /tmp/sbshots-run.sh
timeout 500 dev/isolated-kwin.sh /tmp/sbshots-run.sh >/dev/null 2>&1 || true
rm -f /tmp/sbshots-run.sh
command -v magick >/dev/null && [ -f "$OUT/music.png" ] && [ -f "$OUT/signal-path-music.png" ] && \
  magick "$OUT/music.png" "$OUT/signal-path-music.png" -geometry +243+404 -composite "$OUT/hero.png"
ls "$OUT"
