# RAW to DNG Converter (WebAssembly)

A privacy-first, client-side RAW image to DNG converter designed for iPad and Desktop browsers. This tool allows you to batch convert RAW files (like Sony .ARW) to Adobe DNG format entirely in your browser using Rust and WebAssembly.

## Features

- **Privacy-First**: No data is uploaded to any server. All processing happens locally on your device.
- **Batch Processing**: Convert multiple images sequentially.
- **High-Quality Previews**: Generates 1600px thumbnails using box-filter downsampling.
- **EXIF Preservation**: Extracts and embeds shooting data (Exposure, ISO, Aperture, etc.) using `rawler`.
- **PWA Support**: Can be installed as a standalone app on iOS, Android, and Desktop.
- **ZIP Export**: Option to download all converted DNGs in a single ZIP archive.

## Getting Started

### Using Docker (Recommended)

You can run the converter using Docker. The image is served using the **Angie** webserver.

#### Docker Compose

Create a `docker-compose.yml` file:

```yaml
services:
  raw2dng:
    image: ghcr.io/skipperoo/raw2dng:latest
    ports:
      - "8080:80"
    restart: unless-stopped
```

Then run:

```bash
docker compose up -d
```

Access the application at `http://localhost:8080`.

### Local Development

#### Prerequisites

- [Rust](https://www.rust-lang.org/)
- [wasm-pack](https://rustwasm.github.io/wasm-pack/)
- [ImageMagick](https://imagemagick.org/) (for icon generation)

#### Build

1. Build the WebAssembly module:

   ```bash
   cd raw2dng
   wasm-pack build --target web --release
   ```

2. Generate icons:

   ```bash
   ./create_icons.sh your_master_icon.png
   ```

3. Serve the `www` directory using any static web server (e.g., `python -m http.server`).

## Technical Stack

- **Core Logic**: Rust
- **Wasm Bindings**: `wasm-bindgen`
- **RAW Parsing**: `rawloader` & `rawler`
- **DNG Writing**: `dnglab` (via the `dng` crate)
- **Web Server**: Angie (Nginx fork)
- **Frontend**: Plain JS/HTML5 with Web Workers

## License

© 2026 Leonardo Scoppitto. Built with Rust and WebAssembly.
