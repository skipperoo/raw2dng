#!/bin/bash

# Check if an input file was provided
if [ -z "$1" ]; then
    echo "Usage: $0 <input_image.png>"
    exit 1
fi

INPUT_IMAGE=$1

# Check if input file exists
if [ ! -f "$INPUT_IMAGE" ]; then
    echo "Error: File $INPUT_IMAGE not found."
    exit 1
fi

# Ensure target directories exist
mkdir -p www/icons

echo "Generating icons from $INPUT_IMAGE..."

# Generate PNG icons for PWA
magick "$INPUT_IMAGE" -resize 192x192 www/icons/icon-192.png
magick "$INPUT_IMAGE" -resize 512x512 www/icons/icon-512.png

# Generate Apple Touch Icon (180x180 is standard for iPhone/iPad)
magick "$INPUT_IMAGE" -resize 180x180 www/apple-touch-icon.png

# Generate Favicon (multi-resolution)
magick "$INPUT_IMAGE" -define icon:auto-resize=64,48,32,16 www/favicon.ico

echo "Done! Icons created in www/icons/, www/favicon.ico and www/apple-touch-icon.png"
