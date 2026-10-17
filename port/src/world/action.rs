//! VCAction decoding: type byte, optional condition prefix, operand pairs, type-6 procedure calls.
//!
//! Sources (RE_NOTES): C17 (type byte, dispatch = type & 0x7f), C21 (bit 0x80 = 12-byte condition prefix;
//! the action runs iff the condition is true), H46/C22 (operand pair layout, condition and variable ops),
//! C25 (type-6 blob = named procedure call).

use crate::hdb::Hdb;

/// One VCAction record (class 0x41): `data` blob + `type` byte.
#[derive(Clone, Debug)]
pub struct Action {
    pub id: u32,
    /// raw type byte (bit 0x80 = condition prefix)
    pub typ: u8,
    pub data: Vec<u8>,
}

impl Action {
    pub fn load(h: &Hdb, id: u32) -> Option<Action> {
        let r = h.read(0x41, id)?;
        let data = h.blob_bytes(r.blob("data")?)?.to_vec();
        Some(Action { id, typ: r.u32("type") as u8, data })
    }
    /// dispatch value of the interpreter switch (C17)
    pub fn kind(&self) -> u8 {
        self.typ & 0x7f
    }
    /// 12-byte condition prefix (C21), if bit 0x80 is set
    pub fn condition(&self) -> Option<Operands> {
        if self.typ & 0x80 != 0 {
            Operands::parse(&self.data)
        } else {
            None
        }
    }
    /// blob after the optional condition prefix
    pub fn body(&self) -> &[u8] {
        if self.typ & 0x80 != 0 {
            self.data.get(12..).unwrap_or(&[])
        } else {
            &self.data
        }
    }
}

pub fn be32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}

/// Operand pair (H46; the original's operand reader, FUN_0043e090):
/// `[u32 lhs][u32 rhs][u8 lhsKind][u8 rhsKind][u8 op][u8]`.
/// Kinds: 0 VCVariable objId, 2/3/5 immediate, 1 system value, 4 event-context object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Operands {
    pub lhs: u32,
    pub rhs: u32,
    pub lhs_kind: u8,
    pub rhs_kind: u8,
    pub op: u8,
}

impl Operands {
    /// Needs 11 bytes: type-0 bodies end after `op` (e.g. 0x11fdc `000027120000025e010000`).
    pub fn parse(b: &[u8]) -> Option<Operands> {
        if b.len() < 11 {
            return None;
        }
        Some(Operands { lhs: be32(b, 0)?, rhs: be32(b, 4)?, lhs_kind: b[8], rhs_kind: b[9], op: b[10] })
    }
}

/// Type-6 action body (C25): a named procedure call.
/// Layout (fo-005/fo-006 blobs): name in a 14-byte NUL-terminated field, u16 BE argument count,
/// count x u32 BE arguments, count x u8 argument kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcCall {
    pub name: String,
    pub args: Vec<u32>,
    pub kinds: Vec<u8>,
}

impl ProcCall {
    pub fn parse(body: &[u8]) -> Option<ProcCall> {
        let field = body.get(..14)?;
        let end = field.iter().position(|&c| c == 0).unwrap_or(14);
        let name = String::from_utf8_lossy(&field[..end]).into_owned();
        let n = u16::from_be_bytes([*body.get(14)?, *body.get(15)?]) as usize;
        let mut args = Vec::with_capacity(n);
        for i in 0..n {
            args.push(be32(body, 16 + 4 * i)?);
        }
        let kinds = body.get(16 + 4 * n..16 + 5 * n).map(|k| k.to_vec()).unwrap_or_default();
        Some(ProcCall { name, args, kinds })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_call_layout() {
        // 0x13e55 body (fo-006): DoActionList(0x12144, 0x12146)
        let b = hex("446f416374696f6e4c6973740000000200012144000121460202");
        let p = ProcCall::parse(&b).unwrap();
        assert_eq!(p.name, "DoActionList");
        assert_eq!(p.args, vec![0x12144, 0x12146]);
        assert_eq!(p.kinds, vec![2, 2]);
        // 0x12701: name field carries garbage after the NUL
        let b = hex("504441457363617065006974007500010000025e00");
        let p = ProcCall::parse(&b).unwrap();
        assert_eq!(p.name, "PDAEscape");
        assert_eq!(p.args, vec![0x25e]);
    }

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }
}
