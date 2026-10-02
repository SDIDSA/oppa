use std::io::{Read, Write};
use std::time::Duration;

use super::msg::{
    field, pad8, F_INTERFACE, F_MEMBER, F_PATH, F_REPLY_SERIAL, F_SIGNATURE, MSG_METHOD_CALL,
};
use super::sig::{parse_sig, parse_type};
use super::transport::parse_address;
use super::*;

#[test]
fn sig_model_aligns_per_spec() {
    assert_eq!(parse_sig("ssa{sv}").expect("sig").len(), 3);
    let (t, n) = parse_type(b"a(sa(us))").expect("nested");
    assert_eq!(n, 9);
    assert!(matches!(t, Type::Array(_)));
    assert_eq!(Type::Struct(vec![]).align(), 8);
    assert_eq!(Type::U32.align(), 4);
    assert_eq!(Type::Byte.align(), 1);
}

#[test]
fn values_round_trip_through_signatures() {
    let cases: Vec<(&str, DVal)> = vec![
        ("b", DVal::Bool(true)),
        ("u", DVal::U32(42)),
        ("s", DVal::Str("title".to_string())),
        ("o", DVal::Path("/a/b".to_string())),
        ("ay", DVal::Bytes(vec![1, 2, 3])),
        (
            "a{sv}",
            DVal::Array(vec![
                DVal::Struct(vec![
                    DVal::Str("multiple".to_string()),
                    DVal::Variant(Box::new(DVal::Bool(true))),
                ]),
                DVal::Struct(vec![
                    DVal::Str("filters".to_string()),
                    DVal::Variant(Box::new(DVal::Array(vec![DVal::Struct(vec![
                        DVal::Str("Images".to_string()),
                        DVal::Array(vec![DVal::Struct(vec![
                            DVal::U32(0),
                            DVal::Str("*.png".to_string()),
                        ])]),
                    ])]))),
                ]),
            ]),
        ),
        (
            "(ua{sv})",
            DVal::Struct(vec![
                DVal::U32(0),
                DVal::Array(vec![DVal::Struct(vec![
                    DVal::Str("uris".to_string()),
                    DVal::Variant(Box::new(DVal::Array(vec![DVal::Str(
                        "file:///tmp/a.txt".to_string(),
                    )]))),
                ])]),
            ]),
        ),
    ];
    for (sig, val) in &cases {
        let types = parse_sig(sig).expect("static sig");
        assert_eq!(types.len(), 1);
        let mut buf = Vec::new();
        marshal_value(&mut buf, 0, &types[0], val).unwrap_or_else(|e| panic!("marshal {sig}: {e}"));
        let back = parse_body(&buf, sig).unwrap_or_else(|e| panic!("parse {sig}: {e}"));
        assert_eq!(back, vec![val.clone()], "round-trip {sig}");
    }
}

#[test]
fn method_call_frames_and_parses() {
    let bytes = build_method_call(
        7,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.FileChooser",
        "OpenFile",
        "ssa{sv}",
        &[
            DVal::Str(String::new()),
            DVal::Str("Pick".to_string()),
            DVal::Array(vec![]),
        ],
    )
    .expect("frames");
    let (msg, consumed) = parse_message(&bytes).expect("parses");
    assert_eq!(consumed, bytes.len(), "exact framing");
    assert_eq!(msg.mtype, MSG_METHOD_CALL);
    assert_eq!(msg.serial, 7);
    assert_eq!(msg.member.as_deref(), Some("OpenFile"));
    assert_eq!(msg.sig, "ssa{sv}");
    let body = parse_body(&msg.body, &msg.sig).expect("body parses");
    assert_eq!(body.len(), 3);
}

#[test]
fn truncation_refuses_loudly() {
    assert!(parse_message(b"l\x01").is_err(), "short header");
    let bytes =
        build_method_call(1, "d", "/p", "i", "M", "s", &[DVal::Str("x".into())]).expect("frames");
    assert!(
        parse_message(&bytes[..bytes.len() - 3]).is_err(),
        "cut body"
    );
    assert!(parse_body(b"\x01\x00", "s").is_err(), "cut string");
    assert!(parse_sig("a(").is_err(), "open array");
    assert!(parse_sig("z").is_err(), "bad type byte");
}

#[test]
fn address_forms_parse_or_refuse() {
    assert_eq!(
        parse_address("unix:path=/run/user/1000/bus;autolaunch:").expect("unix path"),
        BusAddr::UnixPath("/run/user/1000/bus".to_string())
    );
    assert_eq!(
        parse_address("unix:path=/run/user/1000/bus,guid=abc").expect("guid suffix"),
        BusAddr::UnixPath("/run/user/1000/bus".to_string())
    );
    assert_eq!(
        parse_address("tcp:host=127.0.0.1,port=99").expect("tcp"),
        BusAddr::Tcp {
            host: "127.0.0.1".to_string(),
            port: 99
        }
    );
    assert!(parse_address("tcp:host=x").is_err(), "tcp needs port");
    assert!(parse_address("nonce:foo=1").is_err(), "unknown transport");
    #[cfg(unix)]
    assert!(
        matches!(
            parse_address("unix:abstract=/tmp/x"),
            Ok(BusAddr::UnixAbstract(_))
        ),
        "abstract parses on unix"
    );
    #[cfg(not(unix))]
    assert!(
        parse_address("unix:abstract=/tmp/x").is_err(),
        "abstract refuses loudly off unix"
    );
    assert!(parse_address("").is_err(), "empty refuses");
}

/// Loopback D-Bus peer: AUTH-OKs, answers Hello/AddMatch/calls
/// with canned replies, then emits one Response signal — the
/// whole client flow against scripted bytes, no real bus.
fn fake_peer(
    listener: std::net::TcpListener,
    handle: String,
    uris: Vec<String>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().expect("accept");
        s.set_read_timeout(Some(Duration::from_secs(10)))
            .expect("timeout");
        // AUTH line.
        let mut got = Vec::new();
        loop {
            let mut one = [0u8; 1];
            s.read_exact(&mut one).expect("auth byte");
            got.push(one[0]);
            if got.len() >= 2 && got[got.len() - 2..] == *b"\r\n" {
                break;
            }
        }
        assert!(got.starts_with(b"AUTH EXTERNAL"), "auth shape");
        s.write_all(b"OK deadbeef\r\n").expect("ok");
        // BEGIN line.
        let mut got = Vec::new();
        loop {
            let mut one = [0u8; 1];
            s.read_exact(&mut one).expect("begin byte");
            got.push(one[0]);
            if got.len() >= 2 && got[got.len() - 2..] == *b"\r\n" {
                break;
            }
        }
        // Framed calls: Hello, AddMatch, OpenFile — reply in kind.
        for expect_member in ["Hello", "AddMatch", "OpenFile"] {
            let msg = read_one(&mut s);
            assert_eq!(msg.member.as_deref(), Some(expect_member));
            let reply_sig;
            let reply_vals: Vec<DVal>;
            if expect_member == "Hello" {
                reply_sig = "s";
                reply_vals = vec![DVal::Str(":9.99".to_string())];
            } else if expect_member == "OpenFile" {
                reply_sig = "o";
                reply_vals = vec![DVal::Path(handle.clone())];
            } else {
                reply_sig = "";
                reply_vals = vec![];
            }
            s.write_all(&reply_msg(msg.serial, reply_sig, &reply_vals))
                .expect("reply");
        }
        // The Response signal for the awaited handle.
        let mut uri_vals = Vec::new();
        for u in &uris {
            uri_vals.push(DVal::Str(u.clone()));
        }
        s.write_all(&signal_msg(
            &handle,
            "(ua{sv})",
            &[DVal::Struct(vec![
                DVal::U32(0),
                DVal::Array(vec![DVal::Struct(vec![
                    DVal::Str("uris".to_string()),
                    DVal::Variant(Box::new(DVal::Array(uri_vals))),
                ])]),
            ])],
        ))
        .expect("signal");
        // Hold the socket briefly so the client read lands.
        std::thread::sleep(Duration::from_millis(200));
    })
}

fn read_exact_stream(s: &mut std::net::TcpStream, n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    s.read_exact(&mut buf).expect("peer read");
    buf
}

fn read_one(s: &mut std::net::TcpStream) -> InMsg {
    let head = read_exact_stream(s, 16);
    let fl = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
    let rest = read_exact_stream(s, fl);
    let mut prefix = head;
    prefix.extend_from_slice(&rest);
    // Body length sits at bytes 4..8; the body itself starts
    // 8-aligned past the fields — the pad bytes between are
    // message bytes too, so they must be READ (not just
    // zero-filled locally), or every later message misaligns.
    let body_len = u32::from_le_bytes([prefix[4], prefix[5], prefix[6], prefix[7]]) as usize;
    let body_start = prefix.len() + (8 - prefix.len() % 8) % 8;
    let mut full = prefix;
    full.extend_from_slice(&read_exact_stream(s, body_start - full.len() + body_len));
    let (msg, _) = parse_message(&full).expect("peer parses client");
    msg
}

fn reply_msg(serial: u32, sig: &str, vals: &[DVal]) -> Vec<u8> {
    // Minimal METHOD_RETURN with REPLY_SERIAL (+ SIGNATURE).
    let types = parse_sig(sig).expect("static");
    let mut body = Vec::new();
    for (t, v) in types.iter().zip(vals.iter()) {
        marshal_value(&mut body, 0, t, v).expect("static");
    }
    let mut msg = vec![b'l', MSG_METHOD_RETURN, 0, 1];
    msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
    msg.extend_from_slice(&99u32.to_le_bytes());
    while msg.len() % 8 != 0 {
        msg.push(0);
    }
    // Fields: REPLY_SERIAL + SIGNATURE when non-empty, built
    // with the production encoder (the helper must not drift
    // from it — that drift is exactly what this test pins).
    let mut fields = Vec::new();
    field(&mut fields, 16, F_REPLY_SERIAL, &DVal::U32(serial)).expect("static");
    if !sig.is_empty() {
        field(&mut fields, 16, F_SIGNATURE, &DVal::Sig(sig.to_string())).expect("static");
    }
    let mut framed = Vec::new();
    framed.extend_from_slice(&msg[..12]);
    framed.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    framed.extend_from_slice(&fields);
    pad8(&mut framed, 0);
    framed.extend_from_slice(&body);
    framed
}

fn signal_msg(handle: &str, sig: &str, vals: &[DVal]) -> Vec<u8> {
    let types = parse_sig(sig).expect("static");
    let mut body = Vec::new();
    for (t, v) in types.iter().zip(vals.iter()) {
        marshal_value(&mut body, 0, t, v).expect("static");
    }
    let mut msg = vec![b'l', MSG_SIGNAL, 0, 1];
    msg.extend_from_slice(&(body.len() as u32).to_le_bytes());
    msg.extend_from_slice(&77u32.to_le_bytes());
    pad8(&mut msg, 0);
    let mut fields = Vec::new();
    // Built with the production encoder (absolute base 16 —
    // detached buffers misalign variants, which is exactly the
    // bug this helper once had).
    field(&mut fields, 16, F_PATH, &DVal::Path(handle.to_string())).expect("static");
    field(
        &mut fields,
        16,
        F_INTERFACE,
        &DVal::Str("org.freedesktop.portal.Request".to_string()),
    )
    .expect("static");
    field(
        &mut fields,
        16,
        F_MEMBER,
        &DVal::Str("Response".to_string()),
    )
    .expect("static");
    field(&mut fields, 16, F_SIGNATURE, &DVal::Sig(sig.to_string())).expect("static");
    let mut framed = Vec::new();
    framed.extend_from_slice(&msg[..12]);
    framed.extend_from_slice(&(fields.len() as u32).to_le_bytes());
    framed.extend_from_slice(&fields);
    pad8(&mut framed, 0);
    framed.extend_from_slice(&body);
    framed
}

/// Peer-constructed replies and signals parse through the same
/// production parser (pins the fake peer honest — a malformed
/// canned reply fails here, not in the loopback test).
#[test]
fn peer_reply_and_signal_frame_exactly() {
    let reply = reply_msg(41, "s", &[DVal::Str(":9.99".to_string())]);
    let (msg, consumed) = parse_message(&reply).expect("reply parses");
    assert_eq!(consumed, reply.len());
    assert_eq!(msg.mtype, MSG_METHOD_RETURN);
    assert_eq!(msg.reply_to, Some(41));
    assert_eq!(msg.sig, "s");
    let signal = signal_msg(
        "/org/freedesktop/portal/desktop/request/9_99/oppa0",
        "(ua{sv})",
        &[DVal::Struct(vec![DVal::U32(0), DVal::Array(vec![])])],
    );
    let (sig, consumed) = parse_message(&signal).expect("signal parses");
    assert_eq!(consumed, signal.len());
    assert_eq!(sig.mtype, MSG_SIGNAL);
    assert_eq!(sig.member.as_deref(), Some("Response"));
    assert_eq!(sig.sig, "(ua{sv})");
    assert_eq!(
        sig.path.as_deref(),
        Some("/org/freedesktop/portal/desktop/request/9_99/oppa0")
    );
}

#[test]
fn hello_call_and_response_flow_over_loopback() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("loopback listener");
    let port = listener.local_addr().expect("port").port();
    let handle = "/org/freedesktop/portal/desktop/request/9_99/oppa0".to_string();
    let peer = fake_peer(
        listener,
        handle.clone(),
        vec!["file:///tmp/a%20b.txt".to_string()],
    );
    let stream = std::net::TcpStream::connect(("127.0.0.1", port)).expect("connect");
    let mut conn = BusConn::handshake(stream, Duration::from_secs(5)).expect("handshake");
    assert_eq!(conn.sender(), ":9.99");
    let mut stashed = Vec::new();
    // Empty-body call first (the AddMatch shape the dialog
    // layer subscribes with — serials advance, nothing stashes).
    conn.call(
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "AddMatch",
        "s",
        &[DVal::Str("type='signal'".to_string())],
        &mut stashed,
        Duration::from_secs(5),
    )
    .expect("addmatch replies");
    let reply = conn
        .call(
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.FileChooser",
            "OpenFile",
            "ssa{sv}",
            &[
                DVal::Str(String::new()),
                DVal::Str("Pick".to_string()),
                DVal::Array(vec![]),
            ],
            &mut stashed,
            Duration::from_secs(5),
        )
        .expect("open call replies");
    let vals = parse_body(&reply.body, &reply.sig).expect("handle parses");
    assert_eq!(vals, vec![DVal::Path(handle.clone())]);
    // The signal the peer emitted after the reply lands in the
    // stash on the next call… instead read it directly: one more
    // framed message is waiting.
    let sig = conn
        .read_message("signal", Duration::from_secs(5))
        .expect("signal arrives");
    assert_eq!(sig.mtype, MSG_SIGNAL);
    assert_eq!(sig.member.as_deref(), Some("Response"));
    assert_eq!(sig.path.as_deref(), Some(handle.as_str()));
    let body = parse_body(&sig.body, &sig.sig).expect("response parses");
    assert_eq!(
        body,
        vec![DVal::Struct(vec![
            DVal::U32(0),
            DVal::Array(vec![DVal::Struct(vec![
                DVal::Str("uris".to_string()),
                DVal::Variant(Box::new(DVal::Array(vec![DVal::Str(
                    "file:///tmp/a%20b.txt".to_string()
                )]))),
            ])]),
        ])]
    );
    peer.join().expect("peer exits");
    assert!(stashed.is_empty(), "no stray signals swallowed");
}
