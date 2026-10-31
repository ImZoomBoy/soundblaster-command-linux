#!/bin/sh
# Retakes every screenshot in docs/screenshots inside the isolated KWin.
# Dry run with temporary settings: nothing touches the card or your config.
# Usage: dev/screenshots.sh [out-dir]   (default: docs/screenshots)
# shot <name> <page> <media> <art> <settings> [signal-path name] [WxH] [delay ms]
cd "$(dirname "$0")/.."
OUT=$(realpath -m "${1:-docs/screenshots}"); mkdir -p "$OUT"
cargo build --release -q
cat > /tmp/sbshots-run.sh <<EOS
#!/bin/sh
B="$PWD/target/release/sbcommand"; A="$PWD/assets/demo"; O="$OUT"
shot() {
  d=\$(mktemp -d /tmp/sbshot-cfg.XXXX); echo "\$5" > "\$d/settings.json"
  env SBCOMMAND_DRYRUN=1 SBCOMMAND_CONFIG_DIR="\$d" SBCOMMAND_SHOT_SIZE=\${7:-1400x860} SBCOMMAND_SHOT="\$O/\$1.png" \\
      SBCOMMAND_SHOT_DELAY=\${8:-2200} SBCOMMAND_DEMO_MEDIA="\$3" SBCOMMAND_DEMO_ART="\$A/\$4.png" \\
      \${6:+SBCOMMAND_SHOT_PATH=\$O/signal-path-\$6.png} "\$B" \$2
  rm -rf "\$d"
}
ROCK='"eqs":{"music":"Rock"},"eq":{"speakers":"Rock","headphones":"Rock"}'
shot music "" music music "{\"activity\":\"music\",\$ROCK,\"listen\":\"51\"}" music
shot movie "" movie film '{"activity":"movie","styles":{"movie":"Cinema"},"listen":"51"}' film 1400x860 4500
shot game "" game game '{"activity":"game","styles":{"game":"FPS"},"game_footsteps":true,"listen":"51"}' "" 1400x860 4500
shot pure "" music music "{\"activity\":\"music\",\"sbx_enabled\":false,\"listen\":\"51\",\$ROCK}"
shot rears "" music music "{\"activity\":\"music\",\"listen\":\"51\",\"stereo_on\":\"rear\",\"layout\":\"rows\",\$ROCK}" rears
shot look look music music "{\"activity\":\"music\",\$ROCK}"
shot sound sound music music "{\"activity\":\"music\",\$ROCK}"
shot compact "" music music "{\"activity\":\"music\",\$ROCK}" "" 400x720
shot compact-album "" music music "{\"activity\":\"music\",\"compact_top\":\"album\",\$ROCK}" "" 400x720
shot compact-small "" game game '{"activity":"game"}' "" 480x820 4500
shot compact-mini "" music music "{\"activity\":\"music\",\"compact_view\":\"mini\",\$ROCK}" "" 440x720
shot compact-mini-film "" movie film '{"activity":"movie","compact_view":"mini"}' "" 440x720 4500
shot strip "" music music "{\"activity\":\"music\",\$ROCK}" "" 1240x76
shot blueprint "" music music "{\"activity\":\"music\",\"theme\":\"blueprint\",\$ROCK}"
shot screenprint "" game game '{"activity":"game","theme":"screenprint"}' "" 1400x860 4500
shot callouts "" movie film '{"activity":"movie","theme":"callouts"}' "" 1400x860 4500
shot mixer mixer movie film '{"activity":"movie","boost_db":3,"listen":"51"}'
shot crystalvoice crystalvoice "" music '{"activity":"music","rec":{"voice_clarity":true,"noise_reduction":true,"smart_volume":true,"svm_level":60,"focus":true,"focus_angle":90}}'
shot setup setup "" music '{"activity":"music","speaker_type":"dtt2200","crossover":140}'
shot settings settings "" music '{"activity":"music"}'
# Every window size: half a screen in each theme (Meta+Left on a 1920 x 1080 screen), the Strip
# dragged taller, the Remote wide and short; and the full-screen speaker test.
shot half-oys "" music music "{\"activity\":\"music\",\$ROCK}" "" 960x1032
shot half-blueprint "" movie film '{"activity":"movie","theme":"blueprint"}' "" 960x1032 4500
shot half-screenprint "" game game '{"activity":"game","theme":"screenprint"}' "" 960x1032 4500
shot half-callouts "" music music "{\"activity\":\"music\",\"theme\":\"callouts\",\$ROCK}" "" 960x1032
shot strip-tall "" music music "{\"activity\":\"music\",\$ROCK}" "" 1240x180
shot remote-wide "" movie film '{"activity":"movie"}' "" 1400x420 4500
shot speaker-test speaker-test music music "{\"activity\":\"music\",\$ROCK}"
EOS
chmod +x /tmp/sbshots-run.sh
SBTEST_W=1920 SBTEST_H=1080 timeout 900 dev/isolated-kwin.sh /tmp/sbshots-run.sh >/dev/null 2>&1 || true
rm -f /tmp/sbshots-run.sh
[ -f "$OUT/music.png" ] && cp "$OUT/music.png" "$OUT/hero.png"
ls "$OUT"
