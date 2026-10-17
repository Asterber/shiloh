//! hdbdump — print every record of every class with a schema, one line each (a canonical text format, used to check
//! the Rust reader against an independent reader of the research notes).
//!
//!   hdbdump <XFiles.hdb|XFiles.gam>
use std::fmt::Write as _;
use std::io::Write as _;
use xf::hdb::{schema, Hdb, Value};

fn join<T: std::fmt::Display>(v: &[T]) -> String {
    v.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(",")
}

fn value(h: &Hdb, v: &Value, out: &mut String) {
    match v {
        Value::Int(x) => write!(out, "{x:x}").unwrap(),
        Value::I16(x) => write!(out, "{x}").unwrap(),
        Value::Blob(b) => write!(out, "{:x}:{:x}", b.mark, b.len).unwrap(),
        Value::Rect(r) => out.push_str(&join(r)),
        Value::Point(p) => out.push_str(&join(p)),
        Value::Region { kind, rect } => write!(out, "{kind:x}/{}", join(rect)).unwrap(),
        Value::List(l) => {
            write!(out, "{:x}:{:x}:{:x}:{:x}[", l.count, l.mark, l.cls, l.id).unwrap();
            let ids: Vec<String> = h.idlist(l).iter().map(|x| format!("{x:x}")).collect();
            out.push_str(&ids.join(","));
            out.push(']');
        }
        Value::Arr(a) => out.push_str(&a.iter().map(|x| format!("{x:x}")).collect::<Vec<_>>().join(",")),
    }
}

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: hdbdump <file.hdb>");
        std::process::exit(2);
    };
    let h = match Hdb::open(&path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("hdbdump: {e}");
            std::process::exit(2);
        }
    };
    let mut out = String::new();
    writeln!(
        out,
        "version {:x} root {:x} classes {}/{} app {:x}",
        h.version,
        h.root,
        h.classes.len(),
        h.nclasses,
        h.app_version
    )
    .unwrap();
    for c in &h.classes {
        writeln!(out, "class {:x} count {} indexed {}", c.id, c.count, c.objects.len()).unwrap();
        if schema(c.id).is_none() {
            continue;
        }
        for &(id, _) in &c.objects {
            let Some(r) = h.read(c.id, id) else { continue };
            write!(out, "{:x} {:x} {:x} {:x} {:x} {:x}", r.cls, r.id, r.mark, r.end, r.flags, r.vers).unwrap();
            for f in &r.fields {
                write!(out, " {}=", f.name).unwrap();
                value(&h, &f.value, &mut out);
            }
            out.push('\n');
        }
    }
    std::io::stdout().lock().write_all(out.as_bytes()).unwrap();
}
