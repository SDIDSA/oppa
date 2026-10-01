// Offline skeleton for an Oppa wasm app (Round 25.5, decision 340).
//
// Copy next to your index.html + bootstrap.js, served over http(s)
// (service workers refuse file://). Register from bootstrap.js:
//   if ('serviceWorker' in navigator) navigator.serviceWorker.register('./sw.js');
//
// Replace ASSETS with your real bundle names (wasm-bindgen emits
// <crate>.js + <crate>_bg.wasm into pkg/). Cache-first: updates
// need a CACHE bump (stated tradeoff — no background-update story
// here). Manual verification: build, serve, load once online,
// reload offline — the app boots (no automation claims it).

const CACHE = 'hello-v1';
const ASSETS = [
  './index.html',
  './bootstrap.js',
  './pkg/hello.js',
  './pkg/hello_bg.wasm',
];

self.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE).then((cache) => cache.addAll(ASSETS)).then(() => self.skipWaiting()),
  );
});

self.addEventListener('activate', (event) => {
  event.waitUntil(
    caches
      .keys()
      .then((keys) =>
        Promise.all(keys.filter((key) => key !== CACHE).map((key) => caches.delete(key))),
      )
      .then(() => self.clients.claim()),
  );
});

self.addEventListener('fetch', (event) => {
  if (event.request.method !== 'GET') {
    return;
  }
  event.respondWith(
    caches.match(event.request).then((hit) => hit || fetch(event.request)),
  );
});
