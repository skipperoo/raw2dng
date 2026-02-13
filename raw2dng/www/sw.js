import { downloadZip } from "https://cdn.jsdelivr.net/npm/client-zip@2.1.0/+esm";

const CACHE_NAME = "raw2dng-v2";
const ASSETS = [
  "./",
  "./index.html",
  "./manifest.json",
  "./worker.js",
  "./pkg/raw_dng_converter.js",
  "./pkg/raw_dng_converter_bg.wasm",
  "https://cdn.jsdelivr.net/npm/client-zip@2.1.0/+esm"
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      return cache.addAll(ASSETS);
    })
  );
  self.skipWaiting();
});

self.addEventListener("activate", (event) => {
  event.waitUntil(clients.claim());
});

self.addEventListener("fetch", (event) => {
  const url = new URL(event.request.url);
  
  if (url.pathname.endsWith("/download-zip")) {
    event.respondWith(handleZipDownload());
  } else {
    event.respondWith(
      caches.match(event.request).then((response) => {
        return response || fetch(event.request);
      })
    );
  }
});

async function handleZipDownload() {
  try {
    const opfsRoot = await navigator.storage.getDirectory();
    const files = [];
    
    // We iterate over all files in the root OPFS directory
    // and include all .dng files in the ZIP.
    for await (const [name, handle] of opfsRoot.entries()) {
      if (name.endsWith(".dng")) {
        files.push(await handle.getFile());
      }
    }

    if (files.length === 0) {
      return new Response("No files found to ZIP", { status: 404 });
    }

    const zipResponse = downloadZip(files);
    
    // Add Content-Disposition header to trigger download
    const headers = new Headers(zipResponse.headers);
    headers.set("Content-Disposition", `attachment; filename=converted_images_${Date.now()}.zip`);
    
    return new Response(zipResponse.body, {
      headers: headers
    });
  } catch (err) {
    return new Response(`Error generating ZIP: ${err.message}`, { status: 500 });
  }
}
