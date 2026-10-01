// Offline skeleton for the hello-web scaffold (G20, decision 374).
//
// Served next to index.html + bootstrap.js over http(s) (service
// workers refuse file://). Registered from bootstrap.js; the
// wasm-bindgen bundle names below follow the package stem
// (`hello_web` — `cargo oppa new` renames them with the package).
// Cache-first: updates need a CACHE bump (stated tradeoff — no
// background-update story here). Verify by hand: build, serve,
// load once online, reload offline — the app boots.

const CACHE = 'hello-web-v1';
const ASSETS = [
  './index.html',
  './manifest.json',
  './bootstrap.js',
  './pkg/hello_web.js',
  './pkg/hello_web_bg.wasm',
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
