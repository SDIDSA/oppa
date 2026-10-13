use super::*;

/// Framework image-cache service (§9.6: core-side, content-addressed; decode
/// workers are §9.1 workers and in-flight decodes are generation-independent
/// — only the await is generation-scoped). M2 holds the key→id map; async
/// decode + mailbox arrive with the worker executor (M2b/M4 scope).
#[derive(Clone, Default)]
pub struct ImageCache {
    inner: Rc<RefCell<ImageCacheInner>>,
}

#[derive(Default)]
struct ImageCacheInner {
    map: HashMap<String, crate::vnode::ImageId>,
    reverse: HashMap<crate::vnode::ImageId, String>,
    /// Pre-decoded static pixels (Phase 36 PR4, decision 359):
    /// straight-alpha RGBA8 `(width, height, bytes)` per id, deposited
    /// once via [`ImageCache::insert_pixels`] — every backend serves
    /// from this one deposit (video stays out — animated frames never
    /// land here).
    pixels: HashMap<crate::vnode::ImageId, (u32, u32, Vec<u8>)>,
    next: u64,
}

impl ImageCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Content-addressed lookup-or-insert. Synchronous in M2 (pre-decoded
    /// static images per BUILD-ORDER §5.7); never fails loudly on a miss —
    /// a miss inserts.
    pub fn load(&self, key: &str) -> crate::vnode::ImageId {
        let mut inner = self.inner.borrow_mut();
        if let Some(id) = inner.map.get(key) {
            return *id;
        }
        inner.next += 1;
        let id = crate::vnode::ImageId(inner.next);
        inner.map.insert(key.to_string(), id);
        inner.reverse.insert(id, key.to_string());
        id
    }

    /// Reverse lookup: the key an id was loaded under (round 4.4 —
    /// the DOM backend renders it as the `<img>` URL). `None` for
    /// ids this cache never issued (foreign/dummy ids refuse
    /// loudly downstream, never render as empty sources).
    pub fn key_of(&self, id: crate::vnode::ImageId) -> Option<String> {
        self.inner.borrow().reverse.get(&id).cloned()
    }

    /// Deposits pre-decoded static pixels under `key` (Phase 36 PR4,
    /// decision 359 — the `oppa-image` RGBA8 shape): returns the
    /// content-addressed id every backend serves from (CPU/GPU
    /// `insert_image` twins pull via [`ImageCache::pixels_of`], the
    /// DOM backend renders a data URI). Length must equal
    /// `width × height × 4` (refuses loudly — a short buffer is a
    /// decode bug, never a cropped image); zero sizes refuse loudly.
    /// Re-depositing a key replaces its pixels (last wins, stated).
    pub fn insert_pixels(
        &self,
        key: &str,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    ) -> crate::vnode::ImageId {
        if width == 0 || height == 0 {
            panic!("image cache: insert_pixels({key}) with zero size — refused, never silent");
        }
        if rgba.len() != width as usize * height as usize * 4 {
            panic!(
                "image cache: insert_pixels({key}): {} bytes != {width}x{height}x4 — refused, never silent",
                rgba.len()
            );
        }
        let mut inner = self.inner.borrow_mut();
        if let Some(id) = inner.map.get(key).copied() {
            inner.pixels.insert(id, (width, height, rgba));
            return id;
        }
        inner.next += 1;
        let id = crate::vnode::ImageId(inner.next);
        inner.map.insert(key.to_string(), id);
        inner.reverse.insert(id, key.to_string());
        inner.pixels.insert(id, (width, height, rgba));
        id
    }

    /// Pre-decoded pixels for an id (`(width, height, RGBA8)`), if
    /// deposited (backends pull from here — see
    /// [`ImageCache::insert_pixels`]). Cloned (pixels are mechanism
    /// state — backends own their copy after deposit).
    pub fn pixels_of(&self, id: crate::vnode::ImageId) -> Option<(u32, u32, Vec<u8>)> {
        self.inner.borrow().pixels.get(&id).cloned()
    }
}
