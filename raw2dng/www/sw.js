const CACHE_NAME = "raw2dng-v1";
const ASSETS = [
  "./",
  "./index.html",
  "./manifest.json",
  "./worker.js",
  "../pkg/raw_dng_converter.js",
  "../pkg/raw_dng_converter_bg.wasm"
];

self.addEventListener("install", (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => {
      return cache.addAll(ASSETS);
    })
  );
});

self.addEventListener("fetch", (event) => {
  event.respondWith(
    caches.match(event.request).then((response) => {
      return response || fetch(event.request);
    })
  );
});
