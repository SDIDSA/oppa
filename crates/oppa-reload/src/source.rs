//! Component providers for the swap protocol: in-process manifests for
//! headless testing/fuzzing, real dylibs for the product loop.

use oppa::ComponentDesc;

/// A component provider: a manifest table plus, for dylibs, ownership of
/// the loaded library (transferred to the harness on swap so adopted
/// pointers stay valid).
pub trait ComponentSource {
    fn name(&self) -> &str;
    fn entries(&self) -> &[ComponentDesc];
    /// Transfers library ownership to the harness (`None` for static
    /// sources — nothing to unload).
    fn into_library(self: Box<Self>) -> Option<libloading::Library>;
}

/// In-process manifest (headless harness): runs the full drain/adopt/
/// evict protocol with zero dylib machinery. The fuzzer and most reload
/// tests drive this; the real-dylib test proves the same protocol
/// survives `dlopen`/`FreeLibrary`.
pub struct StaticSource {
    name: &'static str,
    descs: &'static [ComponentDesc],
}

impl StaticSource {
    pub fn new(name: &'static str, descs: &'static [ComponentDesc]) -> Self {
        Self { name, descs }
    }
}

impl ComponentSource for StaticSource {
    fn name(&self) -> &str {
        self.name
    }

    fn entries(&self) -> &[ComponentDesc] {
        self.descs
    }

    fn into_library(self: Box<Self>) -> Option<libloading::Library> {
        None
    }
}

/// A real hot dylib: loads `oppa_component_manifest` and holds the
/// `Library` alive while any adopted pointer or function entry is used.
pub struct DylibSource {
    path: std::path::PathBuf,
    lib: Option<libloading::Library>,
    descs: Vec<ComponentDesc>,
}

impl DylibSource {
    /// Loads the dylib at `path` and scans its manifest.
    ///
    /// # Safety
    ///
    /// Loading executes foreign code (the new components' static
    /// initializers). The manifest view is copied; entries stay valid
    /// while `self` (and later the harness, via
    /// [`ComponentSource::into_library`]) holds the library.
    pub unsafe fn load(path: impl Into<std::path::PathBuf>) -> Result<Self, libloading::Error> {
        let path = path.into();
        let lib = libloading::Library::new(&path)?;
        let manifest: libloading::Symbol<unsafe extern "C" fn() -> oppa::ManifestView> =
            lib.get(b"oppa_component_manifest")?;
        let view = manifest();
        let descs = view.as_slice().to_vec();
        Ok(Self {
            path,
            lib: Some(lib),
            descs,
        })
    }

    pub fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl ComponentSource for DylibSource {
    fn name(&self) -> &str {
        self.path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<dylib>")
    }

    fn entries(&self) -> &[ComponentDesc] {
        &self.descs
    }

    fn into_library(self: Box<Self>) -> Option<libloading::Library> {
        let DylibSource { lib, .. } = *self;
        lib
    }
}
