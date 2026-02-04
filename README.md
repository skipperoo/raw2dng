# RAW to DNG Converter

A privacy-first, client-side RAW image to DNG converter designed for Mobile, Tablets and Desktop browsers. This tool allows you to batch convert RAW files (like Sony .ARW) to Adobe DNG format entirely in your browser.
It is written in Rust and compile to WASM, while the image processing is based on [Dnglab](https://github.com/dnglab/dnglab), so it should support the same RAW formats, but YMMV.

## Features

- **Privacy-First**: The photos are converted locally on your device.
- **Batch Processing**: Convert multiple images sequentially.
- **ZIP Export**: Option to download all converted DNGs in a single ZIP archive.
- **EXIF Preservation**: Extracts and embeds shooting data (Exposure, ISO, Aperture, etc.) using `rawler` (tested on .ARW and .CR2 formats).
- **PWA Support**: Can be installed as a standalone app on iOS, Android, and Desktop.

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

Use the provided `docker-compose-dev.yml` file:

```bash
docker compose -f docker-compose-dev.yml --build
```

## License

MIT
