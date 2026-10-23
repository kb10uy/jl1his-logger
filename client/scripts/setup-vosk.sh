#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p static/models .vosk
for model in vosk-model-small-en-us-0.15 vosk-model-small-ja-0.22; do
    archive="static/models/$model.tar.gz"
    words=".vosk/$model.words"
    if [[ -s "$archive" && -s "$words" ]]; then
        echo "$model is ready"
        continue
    fi
    curl -fsSL --retry 3 -o "$work/$model.zip" "https://alphacephei.com/vosk/models/$model.zip"
    unzip -q "$work/$model.zip" -d "$work/$model"
    if [[ ! -s "$archive" ]]; then
        tar -C "$work/$model" -czf "$archive" "$model"
        echo "$archive"
    fi
    if [[ -s "$work/$model/$model/graph/words.txt" ]]; then
        cp "$work/$model/$model/graph/words.txt" "$words"
    else
        cp "$work/$model/$model/graph/Gr.fst" "$words"
    fi
    echo "$words"
done
