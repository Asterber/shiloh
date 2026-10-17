//! Variable store and operand evaluation (H46 decoding; confirmed by C22: 70/70 evaluable conditions
//! on fo-005 replay and 70/70 pre-registered on fo-006).
//!
//! Initial values: VCVariable records (class 0x53) of XFiles.gam: value = f38, type = f41 & 0x7f
//! (0 char, 1/2 int, 3 string). Operand kind 4 = a key into the event-context map of the message's runtime trigger
//! (HYPOTHESIS H90, `set_event_ctx`); without a known map it evaluates to `None` ("not evaluable"). Operand kind 1 is a trigger-local variable (HYPOTHESIS H73, `*_in` methods); without a
//! trigger context it is `None` as before.

use super::action::Operands;
use crate::hdb::Hdb;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Var {
    pub value: i32,
    pub typ: u8,
}

/// Result of a type-0 (variable) action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SetResult {
    Ok {
        var: u32,
        value: i32,
    },
    /// lhs is not a VCVariable (kind 1 without a trigger context, kind 4, immediates) — write target not modelled
    LhsNotVariable {
        kind: u8,
    },
    /// H73: write of a trigger-local variable (operand kind 1)
    Local {
        trigger: u32,
        id: u32,
        value: i32,
    },
    UnknownVariable(u32),
    /// rhs not evaluable; the variable becomes unknown (tainted)
    RhsUnknown(u32),
    DivByZero(u32),
    UnknownOp(u8),
}

#[derive(Clone, Default)]
pub struct Vars {
    vars: HashMap<u32, Var>,
    /// variables whose value is unknown after an unevaluable write
    tainted: HashSet<u32>,
    /// H73: trigger-local variables of each runtime trigger (the slots of `LOCAL_GROUPS`, flattened)
    locals: HashMap<u32, [i32; LOCAL_SLOTS]>,
    /// H90: event-context map (key → value) copied into each runtime trigger when its event fires; absent = unknown
    event_ctx: HashMap<u32, Vec<(u32, i32)>>,
    /// H90 switch: evaluate operand kind 4
    pub h90_event_ctx: bool,
    /// H650: VCString texts of the gam (class 0x50 objId → text without the trailing NUL)
    strings: HashMap<u32, Vec<u8>>,
    /// H650: texts the session gave a string variable (SetLanguage); the raw value keeps the old VCString objId (the
    /// save writer gives a changed text a new objId, H475)
    texts: HashMap<u32, Vec<u8>>,
    /// H650 switch: `==` / `!=` with a string variable operand compare texts (the original's string compare,
    /// FUN_005cf870)
    pub h650_string_compare: bool,
}

/// H73: the kind of value a trigger-local slot keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LocalKind {
    Integer,
    /// stored as 0 / 1
    Boolean,
    /// stored as a signed 8-bit value
    Character,
}

impl LocalKind {
    fn store(self, v: i32) -> i32 {
        match self {
            LocalKind::Integer => v,
            LocalKind::Boolean => (v != 0) as i32,
            LocalKind::Character => v as u8 as i8 as i32,
        }
    }
}

/// HYPOTHESIS H73: the trigger-local groups as (first operand id, slot count, kind):
/// Integer A..H, Boolean A..L, Character A..D. The flattened slot array keeps the groups in this order.
const LOCAL_GROUPS: [(u32, usize, LocalKind); 3] =
    [(0, 8, LocalKind::Integer), (10000, 12, LocalKind::Boolean), (20000, 4, LocalKind::Character)];
const LOCAL_SLOTS: usize = 24;

/// The flattened slot and kind of a kind-1 operand id; an id outside every group addresses Integer A.
fn local_slot(id: u32) -> (usize, LocalKind) {
    let mut first_slot = 0;
    for (first_id, n, kind) in LOCAL_GROUPS {
        if let Some(k) = id.checked_sub(first_id).filter(|&k| (k as usize) < n) {
            return (first_slot + k as usize, kind);
        }
        first_slot += n;
    }
    (0, LocalKind::Integer)
}

/// One side of an `==` / `!=` as the string comparison sees it (H650).
enum Side {
    /// a string variable with a known text
    Text(Vec<u8>),
    /// a variable that is not a string (None = unknown)
    Value(Option<i32>),
    /// an immediate (operand kinds 2, 3, 5)
    Immediate(i32),
    /// a trigger-local or event-context operand: outside the string rule
    Other,
}

/// A text that stands for a character: exactly one byte, compared as a signed value.
fn single_char(t: &[u8]) -> Option<i32> {
    match t {
        [c] => Some(*c as i8 as i32),
        _ => None,
    }
}

/// value of a variable as the engine reads it: char sign-extended from 8 bits, else 32-bit
fn read(v: Var) -> i32 {
    if v.typ == 0 {
        v.value as u8 as i8 as i32
    } else {
        v.value
    }
}

impl Vars {
    pub fn from_gam(gam: &Hdb) -> Vars {
        let mut vars = HashMap::new();
        if let Some(c) = gam.class(0x53) {
            for &(id, _) in &c.objects {
                if let Some(r) = gam.read(0x53, id) {
                    vars.insert(id, Var { value: r.u32("f38") as i32, typ: (r.u32("f41") & 0x7f) as u8 });
                }
            }
        }
        let mut strings = HashMap::new();
        if let Some(c) = gam.class(0x50) {
            for &(id, _) in &c.objects {
                let t = gam.read(0x50, id).and_then(|r| r.blob("text")).filter(|b| b.len > 0).and_then(|b| gam.blob_bytes(b));
                let mut t = t.map(<[u8]>::to_vec).unwrap_or_default();
                if let Some(n) = t.iter().position(|&c| c == 0) {
                    t.truncate(n);
                }
                strings.insert(id, t);
            }
        }
        Vars { vars, strings, ..Vars::default() }
    }

    /// H650: the text of a string variable (type 3): the session's text, else its VCString's (missing → ""); `None`
    /// for other variables and unknown (tainted) ones
    pub fn text(&self, id: u32) -> Option<Vec<u8>> {
        let v = self.vars.get(&id).filter(|v| v.typ == 3 && !self.tainted.contains(&id))?;
        Some(self.texts.get(&id).cloned().unwrap_or_else(|| self.strings.get(&(v.value as u32)).cloned().unwrap_or_default()))
    }

    /// H650: the string variables whose text the session changed (variable, text without NUL), objId order
    pub fn changed_texts(&self) -> Vec<(u32, Vec<u8>)> {
        let mut v: Vec<_> = self.texts.iter().map(|(&k, t)| (k, t.clone())).collect();
        v.sort();
        v
    }

    /// H650 / H92: SetLanguage's write (FUN_0048b680; only when the text differs): the string variable
    /// `id` gets `text`. Returns false when `id` is not a known string variable.
    pub fn set_text(&mut self, id: u32, text: &[u8]) -> bool {
        if self.text(id).is_none() {
            return false;
        }
        if self.text(id).as_deref() != Some(text) {
            self.texts.insert(id, text.to_vec());
        }
        true
    }

    pub fn get(&self, id: u32) -> Option<i32> {
        if self.tainted.contains(&id) {
            return None;
        }
        self.vars.get(&id).map(|&v| read(v))
    }

    pub fn operand(&self, v: u32, kind: u8) -> Option<i32> {
        self.operand_in(v, kind, None)
    }

    /// Every VCVariable as stored (raw value, type & 0x7f), objId order; `None` = unknown (tainted) — the save writer
    /// keeps the base file's value for those (`crate::save`).
    pub fn stored(&self) -> Vec<(u32, Option<i32>, u8)> {
        let mut v: Vec<_> =
            self.vars.iter().map(|(&id, x)| (id, (!self.tainted.contains(&id)).then_some(x.value), x.typ)).collect();
        v.sort_by_key(|x| x.0);
        v
    }

    /// the stored type (& 0x7f) of a VCVariable
    pub fn typ(&self, id: u32) -> Option<u8> {
        self.vars.get(&id).map(|v| v.typ)
    }

    /// a VCVariable record with this id exists (its value may be unknown)
    pub fn has(&self, id: u32) -> bool {
        self.vars.contains_key(&id)
    }

    /// Native write of a VCVariable (FUN_005ce4a0): value and type replaced, the type's 0x80 bit kept. `None` value = unknown
    /// (tainted). Returns false for an id that is not a VCVariable.
    pub fn set_native(&mut self, id: u32, value: Option<i32>, typ: u8) -> bool {
        let Some(v) = self.vars.get_mut(&id) else { return false };
        v.typ = typ;
        self.texts.remove(&id);
        match value {
            Some(x) => {
                v.value = x;
                self.tainted.remove(&id);
            }
            None => {
                self.tainted.insert(id);
            }
        }
        true
    }

    /// HYPOTHESIS H73: an event that fires `trigger` zeroes its locals before its ActionList is posted (DoActionList
    /// does not).
    pub fn reset_locals(&mut self, trigger: u32) {
        self.locals.insert(trigger, [0; LOCAL_SLOTS]);
    }

    /// HYPOTHESIS H90: the event dispatch (FUN_004d2710) copies the event's context map into every trigger it fires
    /// before posting the ActionList; `None` = the event's map is not decoded (kind-4 operands not evaluable).
    pub fn set_event_ctx(&mut self, trigger: u32, map: Option<Vec<(u32, i32)>>) {
        match map {
            Some(m) => {
                self.event_ctx.insert(trigger, m);
            }
            None => {
                self.event_ctx.remove(&trigger);
            }
        }
    }

    /// H90 operand kind 4 (the original's operand getter, FUN_005d54f0): no context (trigger 0) → 0; else the trigger's
    /// context map looked up with key = the operand value. A key missing from a known map: not decoded.
    fn event_value(&self, trigger: u32, key: u32) -> Option<i32> {
        if trigger == 0 {
            return Some(0);
        }
        self.event_ctx.get(&trigger)?.iter().find(|e| e.0 == key).map(|e| e.1)
    }

    /// H73: a trigger-local value (no context, trigger 0 → 0; never written → 0)
    pub fn local(&self, trigger: u32, id: u32) -> i32 {
        if trigger == 0 {
            return 0;
        }
        self.locals.get(&trigger).map_or(0, |l| l[local_slot(id).0])
    }

    /// H73: write a trigger-local value as its slot keeps it (no context → no write); returns the stored value
    fn set_local(&mut self, trigger: u32, id: u32, v: i32) -> i32 {
        let (slot, kind) = local_slot(id);
        let v = kind.store(v);
        if trigger != 0 {
            self.locals.entry(trigger).or_insert([0; LOCAL_SLOTS])[slot] = v;
        }
        v
    }

    /// operand value; `trigger` = the runtime trigger of the message (H73), None = no context known
    pub fn operand_in(&self, v: u32, kind: u8, trigger: Option<u32>) -> Option<i32> {
        match kind {
            0 => self.get(v),
            1 => trigger.map(|t| self.local(t, v)),
            2 | 3 | 5 => Some(v as i32),
            4 if self.h90_event_ctx => trigger.and_then(|t| self.event_value(t, v)),
            _ => None, // 4 without H90: not modelled
        }
    }

    /// Condition (C21/C22): ops 0 ==, 1 !=, 2 >, 3 <, 4 >=, 5 <=, 6 and, 7 or. `None` = not evaluable.
    pub fn condition(&self, c: &Operands) -> Option<bool> {
        self.condition_in(c, None)
    }

    pub fn condition_in(&self, c: &Operands, trigger: Option<u32>) -> Option<bool> {
        if self.h650_string_compare && matches!(c.op, 0 | 1) {
            if let Some(eq) = self.string_equal(c) {
                return eq.map(|e| e == (c.op == 0));
            }
        }
        let a = self.operand_in(c.lhs, c.lhs_kind, trigger)?;
        let b = self.operand_in(c.rhs, c.rhs_kind, trigger)?;
        Some(match c.op {
            0 => a == b,
            1 => a != b,
            2 => a > b,
            3 => a < b,
            4 => a >= b,
            5 => a <= b,
            6 => a != 0 && b != 0,
            7 => a != 0 || b != 0,
            _ => return None,
        })
    }

    /// How an operand takes part in a string comparison (H650).
    fn side(&self, v: u32, kind: u8) -> Side {
        match kind {
            0 => self.text(v).map_or_else(|| Side::Value(self.get(v)), Side::Text),
            2 | 3 | 5 => Side::Immediate(v as i32),
            _ => Side::Other,
        }
    }

    /// HYPOTHESIS H650: `==` when an operand is a string variable. `None` = no string
    /// variable takes part (the integer rule applies); `Some(None)` = not evaluable.
    /// - two texts: byte-equal (case-sensitive);
    /// - a text and a variable (either side) or a text left of an immediate: the text must be one character, equal to
    ///   the value;
    /// - an immediate left of a text: the text may be at most one character, its first (0 for "") equal to the value.
    /// A local or event-context operand next to a text keeps the integer rule (no HDB condition has one).
    fn string_equal(&self, c: &Operands) -> Option<Option<bool>> {
        let lhs = self.side(c.lhs, c.lhs_kind);
        let rhs = self.side(c.rhs, c.rhs_kind);
        let char_is = |t: &[u8], v: Option<i32>| match single_char(t) {
            None => Some(false),
            Some(ch) => v.map(|v| v == ch),
        };
        match (lhs, rhs) {
            (Side::Text(a), Side::Text(b)) => Some(Some(a == b)),
            (Side::Text(t), Side::Value(v)) | (Side::Value(v), Side::Text(t)) => Some(char_is(&t, v)),
            (Side::Text(t), Side::Immediate(i)) => Some(char_is(&t, Some(i))),
            (Side::Immediate(i), Side::Text(t)) => {
                let first = t.first().map_or(0, |&b| b as i8 as i32);
                Some(Some(t.len() < 2 && first == i))
            }
            _ => None,
        }
    }

    /// Type-0 action (H46, C22): ops 0 =, 1 ++, 2 --, 3 +=, 4 -=, 5 *=, 6 /=, 7 %=.
    pub fn apply(&mut self, p: &Operands) -> SetResult {
        self.apply_in(p, None)
    }

    pub fn apply_in(&mut self, p: &Operands, trigger: Option<u32>) -> SetResult {
        if let (1, Some(t)) = (p.lhs_kind, trigger) {
            // H73: the same ops (=, ++, +=, …) on a local; the rhs read like any operand
            let b = if matches!(p.op, 1 | 2) { Some(0) } else { self.operand_in(p.rhs, p.rhs_kind, trigger) };
            let a = self.local(t, p.lhs);
            let Some(b) = b else { return SetResult::LhsNotVariable { kind: 1 } };
            let n = match p.op {
                0 => b,
                1 => a.wrapping_add(1),
                2 => a.wrapping_sub(1),
                3 => a.wrapping_add(b),
                4 => a.wrapping_sub(b),
                5 => a.wrapping_mul(b),
                6 | 7 if b == 0 => return SetResult::DivByZero(p.lhs),
                6 => ((a as u32) / (b as u32)) as i32,
                7 => ((a as u32) % (b as u32)) as i32,
                op => return SetResult::UnknownOp(op),
            };
            let value = self.set_local(t, p.lhs, n);
            return SetResult::Local { trigger: t, id: p.lhs, value };
        }
        if p.lhs_kind != 0 {
            return SetResult::LhsNotVariable { kind: p.lhs_kind };
        }
        let Some(&cur) = self.vars.get(&p.lhs) else {
            self.tainted.insert(p.lhs);
            return SetResult::UnknownVariable(p.lhs);
        };
        let b = if matches!(p.op, 1 | 2) { Some(0) } else { self.operand_in(p.rhs, p.rhs_kind, trigger) };
        // the old value is needed by every op except '='; a tainted lhs stays unknown
        let a = if p.op == 0 { Some(0) } else { self.get(p.lhs) };
        let (Some(b), Some(a)) = (b, a) else {
            self.tainted.insert(p.lhs);
            return SetResult::RhsUnknown(p.lhs);
        };
        if matches!(p.op, 6 | 7) && b == 0 {
            self.tainted.insert(p.lhs);
            return SetResult::DivByZero(p.lhs);
        }
        let n = match p.op {
            0 => b,
            1 => a.wrapping_add(1),
            2 => a.wrapping_sub(1),
            3 => a.wrapping_add(b),
            4 => a.wrapping_sub(b),
            5 => a.wrapping_mul(b),
            // unsigned in the exe (H46 note); values seen are small and non-negative
            6 => ((a as u32) / (b as u32)) as i32,
            7 => ((a as u32) % (b as u32)) as i32,
            op => return SetResult::UnknownOp(op),
        };
        self.vars.insert(p.lhs, Var { value: n, typ: cur.typ });
        self.texts.remove(&p.lhs);
        self.tainted.remove(&p.lhs);
        SetResult::Ok { var: p.lhs, value: n }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ops(lhs: u32, rhs: u32, lk: u8, rk: u8, op: u8) -> Operands {
        Operands { lhs, rhs, lhs_kind: lk, rhs_kind: rk, op }
    }

    #[test]
    fn conditions_and_assignments() {
        let mut v = Vars::default();
        v.vars.insert(1, Var { value: 0xff, typ: 0 }); // char -1
        v.vars.insert(2, Var { value: 5, typ: 1 });
        assert_eq!(v.condition(&ops(1, 0xffffffff, 0, 2, 0)), Some(true)); // -1 == -1
        assert_eq!(v.condition(&ops(2, 4, 0, 2, 2)), Some(true)); // 5 > 4
        assert_eq!(v.condition(&ops(2, 4, 0, 1, 2)), None); // system value: not evaluable
        assert_eq!(v.apply(&ops(2, 3, 0, 2, 3)), SetResult::Ok { var: 2, value: 8 });
        assert_eq!(v.apply(&ops(2, 0, 0, 4, 0)), SetResult::RhsUnknown(2));
        assert_eq!(v.get(2), None);
        assert_eq!(v.apply(&ops(2, 7, 0, 2, 0)), SetResult::Ok { var: 2, value: 7 });
        assert_eq!(v.get(2), Some(7));
    }

    #[test]
    fn trigger_locals() {
        let mut v = Vars::default();
        v.vars.insert(3, Var { value: 9, typ: 1 });
        v.reset_locals(0x100);
        assert_eq!(v.apply_in(&ops(10002, 3, 1, 0, 0), Some(0x100)), SetResult::Local { trigger: 0x100, id: 10002, value: 1 });
        assert_eq!(v.condition_in(&ops(10002, 1, 1, 2, 0), Some(0x100)), Some(true));
        assert_eq!(v.apply_in(&ops(2, 0, 1, 2, 1), Some(0x100)), SetResult::Local { trigger: 0x100, id: 2, value: 1 });
        assert_eq!(v.apply_in(&ops(20000, 0x1ff, 1, 2, 0), Some(0x100)), SetResult::Local { trigger: 0x100, id: 20000, value: -1 });
        assert_eq!(v.local(0x200, 2), 0);
        // an id outside the groups addresses Integer A
        v.apply_in(&ops(10012, 7, 1, 2, 0), Some(0x100));
        assert_eq!(v.local(0x100, 0), 7);
        assert_eq!(local_slot(20003), (23, LocalKind::Character));
        assert_eq!(v.local(0, 2), 0);
        v.reset_locals(0x100);
        assert_eq!(v.local(0x100, 2), 0);
        assert_eq!(v.condition(&ops(2, 0, 1, 2, 0)), None);
    }

    #[test]
    fn event_context() {
        let mut v = Vars::default();
        let timer0 = ops(0, 0, 4, 2, 0); // 0x7a41: [ctx key 0 == 0]
        assert_eq!(v.condition_in(&timer0, Some(0x7a3f)), None); // H90 off
        v.h90_event_ctx = true;
        assert_eq!(v.condition_in(&timer0, Some(0x7a3f)), None); // no map known for the trigger
        assert_eq!(v.condition_in(&timer0, Some(0)), Some(true)); // no trigger → 0
        v.set_event_ctx(0x7a3f, Some(vec![(0, 0)]));
        assert_eq!(v.condition_in(&timer0, Some(0x7a3f)), Some(true));
        v.set_event_ctx(0x7a3f, Some(vec![(0, 5)]));
        assert_eq!(v.condition_in(&timer0, Some(0x7a3f)), Some(false));
        assert_eq!(v.condition_in(&ops(0, 5, 4, 2, 0), Some(0x7a3f)), Some(true));
        assert_eq!(v.condition_in(&ops(1, 5, 4, 2, 0), Some(0x7a3f)), None); // key not in the map
        v.set_event_ctx(0x7a3f, None);
        assert_eq!(v.condition_in(&timer0, Some(0x7a3f)), None);
    }

    #[test]
    fn h650_string_compare() {
        let mut v = Vars::default();
        v.vars.insert(0xf9d, Var { value: 0xf9c, typ: 3 }); // ''
        v.vars.insert(0xf9f, Var { value: 0xf9e, typ: 3 }); // 'english'
        v.vars.insert(0x10, Var { value: 0x11, typ: 3 }); // 'Y'
        v.vars.insert(0x20, Var { value: 89, typ: 1 });
        v.strings.insert(0xf9c, vec![]);
        v.strings.insert(0xf9e, b"english".to_vec());
        v.strings.insert(0x11, b"Y".to_vec());
        let ne = ops(0xf9d, 0xf9f, 0, 0, 1);
        assert_eq!(v.condition(&ne), Some(true)); // raw objIds (rule off)
        v.h650_string_compare = true;
        assert_eq!(v.condition(&ne), Some(true)); // '' != 'english'
        assert!(v.set_text(0xf9d, b"english"));
        assert_eq!(v.condition(&ne), Some(false)); // texts equal: no RelocateIFace (H605)
        assert_eq!(v.condition(&ops(0xf9d, 0xf9f, 0, 0, 0)), Some(true));
        assert_eq!(v.changed_texts(), vec![(0xf9d, b"english".to_vec())]);
        assert!(v.set_text(0xf9f, b"English"));
        assert_eq!(v.condition(&ne), Some(true)); // _mbscmp: case-sensitive
        // one string: a 1-character text by its character, longer → not equal
        assert_eq!(v.condition(&ops(0x10, 89, 0, 2, 0)), Some(true));
        assert_eq!(v.condition(&ops(0x10, 0x20, 0, 0, 0)), Some(true));
        assert_eq!(v.condition(&ops(0x20, 0x10, 0, 0, 0)), Some(true));
        assert_eq!(v.condition(&ops(89, 0x10, 2, 0, 0)), Some(true));
        assert_eq!(v.condition(&ops(0xf9d, 101, 0, 2, 0)), Some(false)); // 'english' vs 'e'
        // an immediate left of "" compares with 0; "" left of a number is not one character → not equal
        assert!(v.set_text(0xf9d, b""));
        assert_eq!(v.condition(&ops(0, 0xf9d, 2, 0, 0)), Some(true));
        assert_eq!(v.condition(&ops(0xf9d, 0, 0, 2, 0)), Some(false));
        // ordering ops keep the integer rule
        assert_eq!(v.condition(&ops(0xf9d, 0xf9f, 0, 0, 2)), Some(false));
    }
}
