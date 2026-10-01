# Vision

Status: current. Source: `12-archive/DESIGN.md` §1 (charter, R1).

We are building a **cross-platform GUI framework** targeting **Windows,
Linux, Android, and Web** — one UI model, written once, presented natively
per platform.

Why this project exists:

1. **Pipeline ownership where it pays.** On Windows, Linux, and Android
   the framework owns the rendering pipeline (scene schema through
   pixels). On Web it owns the scene schema while the browser owns the
   pixels (DOM is a first-class renderer backend, not a concession).
2. **Accessibility from day one.** The accessibility/semantic tree is
   computed from the UI model and emitted as part of the renderer
   contract. Retrofitting it later is the single most expensive mistake
   UI frameworks make.
3. **Performance without a multi-year rasterizer project.** v1 embeds an
   existing rasterizer behind our own display list; a custom GPU pipeline
   is a v2 decision.

What we are explicitly not building is in
[non-goals.md](non-goals.md). Priorities and their order are in
[goals.md](goals.md). Working principles are in
[principles.md](principles.md). Canonical terms are in
[terminology.md](terminology.md). Sequencing is in
[roadmap.md](roadmap.md).
