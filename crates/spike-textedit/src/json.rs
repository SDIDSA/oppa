//! Minimal hand-rolled JSON writer (the spike rig is throwaway tooling; the
//! core stays zero-dependency and this crate adds nothing but a `windows`
//! feature set, so JSON is emitted with ~90 lines rather than a serde tree).
//! Only the shapes the rig needs are supported; all strings are UTF-8.

#[derive(Clone, Debug)]
pub enum J {
    S(String),
    N(f64),
    B(bool),
    A(Vec<J>),
    O(Vec<(String, J)>),
}

impl J {
    pub fn s(v: impl Into<String>) -> J {
        J::S(v.into())
    }
    pub fn n(v: impl Into<f64>) -> J {
        J::N(v.into())
    }
    pub fn b(v: bool) -> J {
        J::B(v)
    }
    pub fn a(v: Vec<J>) -> J {
        J::A(v)
    }
    pub fn o(v: Vec<(String, J)>) -> J {
        J::O(v)
    }

    pub fn write(&self, out: &mut String) {
        match self {
            J::S(v) => write_json_string(v, out),
            J::N(v) => {
                if v.is_finite() {
                    out.push_str(&format!("{}", v));
                } else {
                    out.push_str("null");
                }
            }
            J::B(v) => out.push_str(if *v { "true" } else { "false" }),
            J::A(v) => {
                out.push('[');
                for (i, item) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    item.write(out);
                }
                out.push(']');
            }
            J::O(v) => {
                out.push('{');
                for (i, (k, val)) in v.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    write_json_string(k, out);
                    out.push(':');
                    val.write(out);
                }
                out.push('}');
            }
        }
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write(&mut out);
        out
    }
}

fn write_json_string(v: &str, out: &mut String) {
    out.push('"');
    for c in v.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
}
