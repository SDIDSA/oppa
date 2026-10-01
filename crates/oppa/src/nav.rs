//! Stack-first navigation (G6 — decisions 218–219).
//!
//! No router/back-stack/deep-links existed anywhere in core
//! (HANDOFF-V2 §4 G6). This module is the v1 model: a
//! framework-owned stack of route keys with named outcomes on every
//! op, plus deep-link parse/encode as stack syntax.
//!
//! Design (see decisions 218–219 in `docs/04-planning/state.md`):
//!
//! - **Stack-first (218).** Identity is the stack position, not the
//!   URI: [`NavStack`] holds [`Route`]s (name + ordered params);
//!   deep-links parse *into* stack ops and encode back *out of* them.
//!   The web history bridge (popstate → pop, push → pushState) is
//!   OQ-G6-1 — URIs are syntax, never the source of truth.
//! - **Host-independent state.** `NavStack` is a plain struct (no
//!   `Runtime`, no scheduling): apps hold it in a `Signal` (or
//!   `keyed_state`) for reactivity and persist route prefs through
//!   [`KvStore`](crate::store::KvStore). No router/input changes this
//!   round — shell back-button/deep-link intake is OQ-G6-2.
//! - **Named outcomes, never silent.** Empty-stack pops, replaces on
//!   an empty stack, and malformed links spell their results
//!   ([`PopOutcome`], [`ReplaceOutcome`], [`NavError`]) — the same
//!   precedent as [`PasteOutcome`](crate::editing::PasteOutcome).
//!   Consecutive-duplicate pushes are allowed (dedupe is app policy,
//!   stated — e.g. login flows `replace` instead).
//! - **No route tables in v1.** Segment validation, typed params,
//!   and guards are app-owned (OQ-G6-3); the core parses syntax and
//!   round-trips it byte-faithfully (minimal percent codec, tested).
//!
//! Out of scope: animated transitions between routes (the TIME
//! evaluator interpolates paint values, not stack membership),
//! per-route state restoration (compose `keyed_state` + `KvStore`
//! — usage pattern, no new code).
//!
//! ## BackPress: the dismiss-first chain (round 3.3, OQ-G11-2)
//!
//! One system back press (Android `BACK`, desktop `ESC` — shells
//! classify both to [`keys::ESCAPE`](crate::input::keys::ESCAPE))
//! dismisses exactly one layer, in this order:
//!
//! 1. **Author popups.** Author-owned overlay state (e.g. a
//!    `Modal`'s `open` signal) closes first — the author (or the
//!    runner acting for the author) flips it and consumes the
//!    press. The host cannot close what it does not own, so this
//!    step lives outside [`ComponentHost`](crate::component::ComponentHost).
//! 2. **Composition.** [`ComponentHost::handle_back`](crate::component::ComponentHost::handle_back)
//!    cancels an active IME composition (reverts — where focus
//!    loss would commit it, locked #27).
//! 3. **Focus.** With nothing composing, the focused node blurs.
//! 4. **Navigation / exit (runner-owned).** [`BackOutcome::Unhandled`](crate::component::BackOutcome)
//!    means no host-owned layer remained: the runner pops the
//!    [`NavStack`] when deeper than root, else finishes/exits the
//!    app. Runners never exit silently past a consumed step —
//!    each press consumes exactly one layer, so exit only ever
//!    follows a press that dismissed nothing.
//!
//! The router's `ESC` arm runs step 2–3 through `handle_back`
//! (desktop parity — one chain everywhere); Android runners call
//! `handle_back` directly on `BACK` (then settle + repaint, like
//! every other direct host mutation).

use std::fmt;

/// Navigation failure (loud by construction).
#[derive(Clone, Debug, PartialEq)]
pub enum NavError {
    /// Empty name (`""`, `"/"`, or `"?"`-only input).
    EmptyRoute(String),
    /// A raw (unencoded) character outside the accepted set in a
    /// name, key, or value (control/space/`#` — encode first).
    InvalidChar { what: String, char: char },
    /// A `%` escape that is truncated or non-hex.
    BadEscape(String),
}

impl fmt::Display for NavError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NavError::EmptyRoute(input) => write!(f, "empty route in {input:?}"),
            NavError::InvalidChar { what, char } => {
                write!(
                    f,
                    "invalid character {char:?} in {what} — percent-encode first"
                )
            }
            NavError::BadEscape(input) => write!(f, "bad percent-escape in {input:?}"),
        }
    }
}

impl std::error::Error for NavError {}

/// One stack entry: a route name plus ordered params (order kept —
/// `Vec`, never a map, so encode/decode round-trips deterministically).
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub name: String,
    pub params: Vec<(String, String)>,
}

impl Route {
    /// Programmatic constructor (unencoded input — names/params with
    /// structural characters are refused with "encode first", never
    /// silently mangled).
    pub fn new(name: &str) -> Result<Self, NavError> {
        check_name(name)?;
        Ok(Self {
            name: name.to_string(),
            params: Vec::new(),
        })
    }

    pub fn param(mut self, key: &str, value: &str) -> Result<Self, NavError> {
        check_chars(key, "param key")?;
        check_chars(value, "param value")?;
        self.params.push((key.to_string(), value.to_string()));
        Ok(self)
    }

    /// Parses `"name?k=v&k2=v2"` (a leading `/` or scheme prefix like
    /// `"app://"` is stripped — deep-link wrappers are transport, not
    /// identity). Raw structural characters (controls, spaces, `#`)
    /// are refused ("encode first"); `%` escapes decode into data
    /// (decoded text is never re-validated — carrying spaces is what
    /// encoding is for). Refuses loudly on empty names and bad escapes.
    pub fn parse(link: &str) -> Result<Self, NavError> {
        let mut s = link.trim();
        if let Some(after) = s.split_once("://") {
            s = after.1;
        }
        s = s.trim_start_matches('/');
        let (name_part, query) = match s.split_once('?') {
            Some((n, q)) => (n, q),
            None => (s, ""),
        };
        check_raw(name_part, "route name")?;
        let name = percent_decode(name_part)?;
        if name.is_empty() {
            return Err(NavError::EmptyRoute(link.to_string()));
        }
        let mut params = Vec::new();
        if !query.is_empty() {
            for pair in query.split('&') {
                let (k, v) = match pair.split_once('=') {
                    Some((k, v)) => (k, v),
                    None => (pair, ""),
                };
                check_raw(k, "param key")?;
                check_raw(v, "param value")?;
                params.push((percent_decode(k)?, percent_decode(v)?));
            }
        }
        Ok(Self { name, params })
    }

    /// Encodes back to `"name?k=v&..."` (no leading slash — wrappers
    /// are the shell's job). Round-trips [`Route::parse`] exactly.
    pub fn to_path(&self) -> String {
        let mut out = percent_encode(&self.name);
        if !self.params.is_empty() {
            out.push('?');
            let pairs: Vec<String> = self
                .params
                .iter()
                .map(|(k, v)| format!("{}={}", percent_encode(k), percent_encode(v)))
                .collect();
            out.push_str(&pairs.join("&"));
        }
        out
    }
}

fn check_name(name: &str) -> Result<(), NavError> {
    if name.is_empty() {
        return Err(NavError::EmptyRoute(name.to_string()));
    }
    check_chars(name, "route name")
}

/// Raw-input check (parse path): structural characters refused, `%`
/// allowed (escape introducer — decoded text is data, never
/// re-validated). `&`/`=` ride through raw inside values only when
/// encoded; raw they act as separators (standard URI behavior,
/// stated — not a silent rule).
fn check_raw(s: &str, what: &str) -> Result<(), NavError> {
    for c in s.chars() {
        if c.is_control() || c == ' ' || c == '#' {
            return Err(NavError::InvalidChar {
                what: what.to_string(),
                char: c,
            });
        }
    }
    Ok(())
}

fn check_chars(s: &str, what: &str) -> Result<(), NavError> {
    for c in s.chars() {
        if c.is_control() || c == ' ' || c == '#' || c == '%' || c == '&' || c == '=' || c == '?' {
            return Err(NavError::InvalidChar {
                what: what.to_string(),
                char: c,
            });
        }
    }
    Ok(())
}

fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || "-_.~/".contains(c) {
            out.push(c);
        } else {
            for b in c.encode_utf8(&mut [0; 4]).bytes() {
                out.push_str(&format!("%{b:02X}"));
            }
        }
    }
    out
}

fn percent_decode(s: &str) -> Result<String, NavError> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if i + 3 > bytes.len() {
                return Err(NavError::BadEscape(s.to_string()));
            }
            let hex = &s[i + 1..i + 3];
            let v = u8::from_str_radix(hex, 16).map_err(|_| NavError::BadEscape(s.to_string()))?;
            out.push(v);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| NavError::BadEscape(s.to_string()))
}

/// Result of [`NavStack::pop`] / [`NavStack::go_back`] — back at the
/// root is `AtRoot` (shells exit the app/activity on it — Android
/// back at root finishes, it never pops past), never a panic and
/// never a silent no-op.
#[derive(Clone, Debug, PartialEq)]
pub enum PopOutcome {
    Popped(Route),
    AtRoot,
}

/// Result of [`NavStack::replace`] — replacing an empty stack pushes
/// (the screen must show something; the outcome names it).
#[derive(Clone, Debug, PartialEq)]
pub enum ReplaceOutcome {
    Replaced { old: Route, new: Route },
    PushedEmpty(Route),
}

/// Framework-owned back-stack (decision 218): push/pop/replace/reset
/// over [`Route`]s. Plain state — hold it in a `Signal` for
/// reactivity (`stack.clone()` in/out; the stack itself never
/// schedules).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NavStack {
    entries: Vec<Route>,
}

impl NavStack {
    pub fn new(root: Route) -> Self {
        Self {
            entries: vec![root],
        }
    }

    /// Empty stack (no root — `current` is `None` until the first
    /// push/replace; most apps want [`NavStack::new`]).
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn depth(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn current(&self) -> Option<&Route> {
        self.entries.last()
    }

    /// Pushes `route` (consecutive duplicates allowed — dedupe is app
    /// policy). Returns the new depth.
    pub fn push(&mut self, route: Route) -> usize {
        self.entries.push(route);
        self.entries.len()
    }

    /// Pops the top (`AtRoot` when 0/1 entries... precisely: pops
    /// while more than one entry remains; a single root never pops —
    /// the root is the app, not a page).
    pub fn pop(&mut self) -> PopOutcome {
        if self.entries.len() <= 1 {
            return PopOutcome::AtRoot;
        }
        PopOutcome::Popped(self.entries.pop().expect("len > 1"))
    }

    /// Shell back-button entry point (OQ-G6-2 wires it): same as
    /// [`NavStack::pop`].
    pub fn go_back(&mut self) -> PopOutcome {
        self.pop()
    }

    /// Swaps the top (`PushedEmpty` on an empty stack).
    pub fn replace(&mut self, route: Route) -> ReplaceOutcome {
        match self.entries.pop() {
            Some(old) => {
                self.entries.push(route.clone());
                ReplaceOutcome::Replaced { old, new: route }
            }
            None => {
                self.entries.push(route.clone());
                ReplaceOutcome::PushedEmpty(route)
            }
        }
    }

    /// Resets to exactly `root` (logout / root-switch flows).
    pub fn reset(&mut self, root: Route) {
        self.entries.clear();
        self.entries.push(root);
    }

    /// Parses a deep-link and pushes every segment... precisely: one
    /// link parses to one [`Route`] (multi-segment links like
    /// `"a/b?x=1"` keep the full path as the name — segment splitting
    /// is app policy, OQ-G6-3). Returns the new depth.
    pub fn push_link(&mut self, link: &str) -> Result<usize, NavError> {
        let route = Route::parse(link)?;
        Ok(self.push(route))
    }

    pub fn entries(&self) -> &[Route] {
        &self.entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(name: &str) -> Route {
        Route::new(name).expect("valid test route")
    }

    #[test]
    fn push_pop_root_never_pops() {
        let mut nav = NavStack::new(route("home"));
        assert_eq!(nav.depth(), 1);
        assert_eq!(nav.current(), Some(&route("home")));
        assert_eq!(nav.push(route("settings")), 2);
        assert_eq!(
            nav.push(route("settings")),
            3,
            "duplicates allowed (app policy)"
        );
        assert_eq!(nav.pop(), PopOutcome::Popped(route("settings")));
        assert_eq!(nav.pop(), PopOutcome::Popped(route("settings")));
        assert_eq!(nav.pop(), PopOutcome::AtRoot, "root never pops");
        assert_eq!(nav.pop(), PopOutcome::AtRoot, "stable at root");
        assert_eq!(nav.current(), Some(&route("home")));
    }

    #[test]
    fn replace_names_both_outcomes() {
        let mut nav = NavStack::new(route("login"));
        assert_eq!(
            nav.replace(route("home")),
            ReplaceOutcome::Replaced {
                old: route("login"),
                new: route("home"),
            }
        );
        let mut empty = NavStack::empty();
        assert_eq!(
            empty.replace(route("home")),
            ReplaceOutcome::PushedEmpty(route("home"))
        );
        assert_eq!(empty.depth(), 1);
    }

    #[test]
    fn reset_returns_to_root() {
        let mut nav = NavStack::new(route("home"));
        nav.push(route("a"));
        nav.push(route("b"));
        nav.reset(route("login"));
        assert_eq!(nav.entries(), &[route("login")]);
    }

    #[test]
    fn deep_link_round_trips_with_params() {
        let r = Route::parse("app://user/42?tab=posts&filter=a%20b").expect("parses");
        assert_eq!(r.name, "user/42");
        assert_eq!(
            r.params,
            vec![
                ("tab".to_string(), "posts".to_string()),
                ("filter".to_string(), "a b".to_string()),
            ]
        );
        assert_eq!(r.to_path(), "user/42?tab=posts&filter=a%20b");
        let again = Route::parse(&r.to_path()).expect("reparses");
        assert_eq!(again, r, "encode/decode round-trips exactly");
    }

    #[test]
    fn deep_link_refuses_loudly() {
        assert_eq!(Route::parse(""), Err(NavError::EmptyRoute(String::new())));
        assert_eq!(
            Route::parse("app://"),
            Err(NavError::EmptyRoute("app://".to_string()))
        );
        assert!(matches!(Route::parse("a%2"), Err(NavError::BadEscape(_))));
        assert!(matches!(Route::parse("a%ZZ"), Err(NavError::BadEscape(_))));
        assert_eq!(Route::new(""), Err(NavError::EmptyRoute(String::new())));
        let mut nav = NavStack::new(route("home"));
        assert!(nav.push_link("").is_err(), "bad links never push");
        assert_eq!(nav.depth(), 1, "failed link leaves the stack untouched");
        assert_eq!(nav.push_link("settings?tab=main").expect("pushes"), 2);
    }

    #[test]
    fn stack_holds_in_a_signal_for_reactivity() {
        // The documented reactive pattern (no scheduler coupling in
        // the stack itself): hold in a Signal, clone in/out.
        let rt = crate::reactive::Runtime::new();
        let nav = rt.signal(NavStack::new(route("home")));
        nav.update(|mut n| {
            n.push(route("settings"));
            n
        });
        let current = nav.get();
        assert_eq!(current.depth(), 2);
        assert_eq!(current.current(), Some(&route("settings")));
    }
}
