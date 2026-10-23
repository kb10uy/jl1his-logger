#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../static"

background='#3b6dc3'

render() {
    magick -background none -density 288 logo.svg -resize "$1x$1" -depth 8 "$2"
}

render 192 icon-192.png
render 512 icon-512.png
magick -background none -density 288 logo.svg -define icon:auto-resize=48,32,16 favicon.ico

full_bleed() {
    magick -background "$background" -density 288 logo.svg -flatten -resize "$2x$2" \
        -gravity center -extent "$1x$1" -depth 8 "$3"
}

full_bleed 180 160 apple-touch-icon.png
full_bleed 192 162 icon-maskable-192.png
full_bleed 512 434 icon-maskable-512.png
