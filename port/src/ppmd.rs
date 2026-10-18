//! PPMd decoder, the variant of Dmitry Shkarin's PPMdMem/C (2001, close to PPMd var.H) that the Gentee installer
//! (CreateInstall) packs its files with ([`crate::gentee`]). Port feature: the game's files are read straight out of
//! the installer on a disc image. Written in Rust after the MIT-licensed reference sources of the Gentee open source
//! project (`src/projects/gea/PPMD`, see LICENSE-THIRD-PARTY).
//!
//! The model must be reproduced exactly, memory allocator included (when the model memory runs out the model
//! restarts, and where it runs out depends on the allocator's block layout). The C original works on 32-bit pointers
//! into one heap; here the heap is a byte vector and the pointers are 32-bit offsets into it, with the same record
//! layouts (a context is 12 bytes, a state 6, the allocator's unit 12), so the allocator behaves byte for byte as
//! the original. Offset 0 is the null pointer; bytes 4..16 hold the free-block list head of
//! [`Model::glue_free_blocks`] (a stack variable in the original); the heap proper starts at [`HEAP0`].
//!
//! The model state survives between [`Model::decode`] calls: an order below 2 continues the previous model (the
//! installer's "solid" blocks), as in the original.

const UNIT_SIZE: u32 = 12;
const N1: usize = 4;
const N2: usize = 4;
const N3: usize = 4;
const N4: usize = (128 + 3 - N1 - 2 * N2 - 3 * N3) / 4;
const N_INDEXES: usize = N1 + N2 + N3 + N4;
const MAX_O: usize = 16;
const UP_FREQ: usize = 5;
const MAX_FREQ: u32 = 124;
const INT_BITS: u32 = 7;
const PERIOD_BITS: u8 = 7;
const TOT_BITS: u32 = INT_BITS + PERIOD_BITS as u32;
const INTERVAL: u32 = 1 << INT_BITS;
const BIN_SCALE: u32 = 1 << TOT_BITS;
const TOP: u32 = 1 << 24;
const BOT: u32 = 1 << 15;
const EXP_ESCAPE: [u8; 16] = [25, 14, 9, 7, 5, 5, 4, 4, 4, 3, 3, 3, 2, 2, 2, 2];
const INIT_BIN_ESC: [u32; 8] = [0x3CDD, 0x1F3F, 0x59BF, 0x48F3, 0x64A1, 0x5ABC, 0x6632, 0x6051];

/// The sentinel block of the free-block gluing (offset in the heap vector).
const S0: u32 = 4;
/// Where the model heap starts in the vector.
pub const HEAP0: u32 = 16;
/// Slack after the heap: the allocator may read a block header just past its end.
const TAIL: usize = 64;

#[derive(Clone, Copy, Default)]
struct See {
    summ: u16,
    shift: u8,
    count: u8,
}

/// Index of the dummy SEE context in [`Model::see`].
const DUMMY_SEE: usize = 24 * 32;

/// The decoder's model (and the sub-allocator's heap).
pub struct Model {
    mem: Vec<u8>,
    size: u32,
    text: u32,
    units_start: u32,
    lo_unit: u32,
    hi_unit: u32,
    free: [u32; N_INDEXES],
    glue_count: u8,
    indx2units: [u8; N_INDEXES],
    units2indx: [u8; 128],
    ns2indx: [u8; 256],
    ns2bsindx: [u8; 256],
    freq2indx: [u8; 197],
    hb2flag: [u8; 256],
    see: Vec<See>,
    bin_summ: [[u16; 64]; 25],
    max_context: u32,
    found_state: u32,
    init_esc: u32,
    order_fall: i32,
    run_length: i32,
    init_rl: i32,
    max_order: i32,
    char_mask: [u8; 256],
    num_masked: u8,
    prev_success: u8,
    esc_count: u8,
    /// whether a model exists (a solid block needs one)
    started: bool,
    /// how often the model memory ran full and the model restarted
    pub restarts: u32,
}

/// A decoding error: the data does not decode (damaged or not PPMd of this variant).
#[derive(Debug)]
pub struct Error(pub &'static str);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "PPMd: {}", self.0)
    }
}

impl Model {
    /// A model with `mb` megabytes of model memory (the original's `PPMdStartSubAllocator`).
    pub fn new(mb: u32) -> Model {
        let size = mb << 20;
        let mut m = Model {
            mem: vec![0; HEAP0 as usize + size as usize + TAIL],
            size,
            text: 0,
            units_start: 0,
            lo_unit: 0,
            hi_unit: 0,
            free: [0; N_INDEXES],
            glue_count: 0,
            indx2units: [0; N_INDEXES],
            units2indx: [0; 128],
            ns2indx: [0; 256],
            ns2bsindx: [0; 256],
            freq2indx: [0; 197],
            hb2flag: [0; 256],
            see: vec![See::default(); 24 * 32 + 1],
            bin_summ: [[0; 64]; 25],
            max_context: 0,
            found_state: 0,
            init_esc: 0,
            order_fall: 0,
            run_length: 0,
            init_rl: 0,
            max_order: 0,
            char_mask: [0; 256],
            num_masked: 0,
            prev_success: 0,
            esc_count: 0,
            started: false,
            restarts: 0,
        };
        m.start_up();
        m
    }

    /// The model memory in megabytes.
    pub fn megabytes(&self) -> u32 {
        self.size >> 20
    }

    // --- heap access -------------------------------------------------------------------------------------------
    fn u8(&self, p: u32) -> u8 {
        self.mem[p as usize]
    }
    fn set_u8(&mut self, p: u32, v: u8) {
        self.mem[p as usize] = v;
    }
    fn u16(&self, p: u32) -> u16 {
        let p = p as usize;
        u16::from_le_bytes([self.mem[p], self.mem[p + 1]])
    }
    fn set_u16(&mut self, p: u32, v: u16) {
        let p = p as usize;
        self.mem[p..p + 2].copy_from_slice(&v.to_le_bytes());
    }
    fn u32(&self, p: u32) -> u32 {
        let p = p as usize;
        u32::from_le_bytes([self.mem[p], self.mem[p + 1], self.mem[p + 2], self.mem[p + 3]])
    }
    fn set_u32(&mut self, p: u32, v: u32) {
        let p = p as usize;
        self.mem[p..p + 4].copy_from_slice(&v.to_le_bytes());
    }
    fn copy(&mut self, dst: u32, src: u32, n: u32) {
        self.mem.copy_within(src as usize..(src + n) as usize, dst as usize);
    }

    // context: NumStats u8, Flags u8, SummFreq u16, Stats u32, Suffix u32
    fn ns(&self, c: u32) -> u32 {
        self.u8(c) as u32
    }
    fn set_ns(&mut self, c: u32, v: u32) {
        self.set_u8(c, v as u8)
    }
    fn flags(&self, c: u32) -> u8 {
        self.u8(c + 1)
    }
    fn set_flags(&mut self, c: u32, v: u8) {
        self.set_u8(c + 1, v)
    }
    fn summ(&self, c: u32) -> u32 {
        self.u16(c + 2) as u32
    }
    fn set_summ(&mut self, c: u32, v: u32) {
        self.set_u16(c + 2, v as u16)
    }
    fn stats(&self, c: u32) -> u32 {
        self.u32(c + 4)
    }
    fn set_stats(&mut self, c: u32, v: u32) {
        self.set_u32(c + 4, v)
    }
    fn suffix(&self, c: u32) -> u32 {
        self.u32(c + 8)
    }
    fn set_suffix(&mut self, c: u32, v: u32) {
        self.set_u32(c + 8, v)
    }
    // state: Symbol u8, Freq u8, Successor u32
    fn sym(&self, s: u32) -> u8 {
        self.u8(s)
    }
    fn freq(&self, s: u32) -> u32 {
        self.u8(s + 1) as u32
    }
    fn set_freq(&mut self, s: u32, v: u32) {
        self.set_u8(s + 1, v as u8)
    }
    fn succ(&self, s: u32) -> u32 {
        self.u32(s + 2)
    }
    fn set_succ(&mut self, s: u32, v: u32) {
        self.set_u32(s + 2, v)
    }
    fn swap_states(&mut self, a: u32, b: u32) {
        let mut t = [0u8; 6];
        t.copy_from_slice(&self.mem[a as usize..a as usize + 6]);
        self.copy(a, b, 6);
        self.mem[b as usize..b as usize + 6].copy_from_slice(&t);
    }
    fn state(&self, s: u32) -> [u8; 6] {
        let mut t = [0u8; 6];
        t.copy_from_slice(&self.mem[s as usize..s as usize + 6]);
        t
    }
    fn set_state(&mut self, s: u32, t: [u8; 6]) {
        self.mem[s as usize..s as usize + 6].copy_from_slice(&t);
    }
    fn one_state(c: u32) -> u32 {
        c + 2
    }

    // --- sub-allocator -----------------------------------------------------------------------------------------
    fn u2b(nu: u32) -> u32 {
        nu * UNIT_SIZE
    }
    fn insert_node(&mut self, p: u32, i: usize) {
        let head = self.free[i];
        self.set_u32(p, head);
        self.free[i] = p;
    }
    fn remove_node(&mut self, i: usize) -> u32 {
        let p = self.free[i];
        self.free[i] = self.u32(p);
        p
    }
    fn split_block(&mut self, pv: u32, old: usize, new: usize) {
        let mut udiff = self.indx2units[old] as u32 - self.indx2units[new] as u32;
        let mut p = pv + Self::u2b(self.indx2units[new] as u32);
        let mut k = self.units2indx[udiff as usize - 1] as usize;
        if self.indx2units[k] as u32 != udiff {
            k -= 1;
            self.insert_node(p, k);
            let ku = self.indx2units[k] as u32;
            p += Self::u2b(ku);
            udiff -= ku;
        }
        let i = self.units2indx[udiff as usize - 1] as usize;
        self.insert_node(p, i);
    }
    fn init_sub_allocator(&mut self) {
        let diff = UNIT_SIZE * (self.size / 8 / UNIT_SIZE * 7);
        self.free = [0; N_INDEXES];
        self.text = HEAP0;
        self.hi_unit = HEAP0 + self.size;
        self.lo_unit = self.hi_unit - diff;
        self.units_start = self.lo_unit;
        self.glue_count = 0;
    }
    // MEM_BLK: Stamp u16, NU u16, next u32, prev u32
    fn glue_free_blocks(&mut self) {
        if self.lo_unit != self.hi_unit {
            self.set_u8(self.lo_unit, 0);
        }
        self.set_u32(S0 + 4, S0);
        self.set_u32(S0 + 8, S0);
        for i in 0..N_INDEXES {
            while self.free[i] != 0 {
                let p = self.remove_node(i);
                self.set_u32(p + 8, S0);
                let n = self.u32(S0 + 4);
                self.set_u32(p + 4, n);
                self.set_u32(n + 8, p);
                self.set_u32(S0 + 4, p);
                self.set_u16(p, 0xFFFF);
                self.set_u16(p + 2, self.indx2units[i] as u16);
            }
        }
        let mut p = self.u32(S0 + 4);
        while p != S0 {
            loop {
                let p1 = p + self.u16(p + 2) as u32 * 12;
                if !(self.u16(p1) == 0xFFFF && (self.u16(p + 2) as u32 + self.u16(p1 + 2) as u32) < 0x10000) {
                    break;
                }
                let (n1, v1) = (self.u32(p1 + 4), self.u32(p1 + 8));
                self.set_u32(v1 + 4, n1);
                self.set_u32(n1 + 8, v1);
                let nu = self.u16(p + 2) + self.u16(p1 + 2);
                self.set_u16(p + 2, nu);
            }
            p = self.u32(p + 4);
        }
        loop {
            let mut p = self.u32(S0 + 4);
            if p == S0 {
                break;
            }
            let (n, v) = (self.u32(p + 4), self.u32(p + 8));
            self.set_u32(v + 4, n);
            self.set_u32(n + 8, v);
            let mut sz = self.u16(p + 2) as u32;
            while sz > 128 {
                self.insert_node(p, N_INDEXES - 1);
                sz -= 128;
                p += 128 * 12;
            }
            let mut i = self.units2indx[sz as usize - 1] as usize;
            if self.indx2units[i] as u32 != sz {
                i -= 1;
                let k = sz - self.indx2units[i] as u32;
                self.insert_node(p + (sz - k) * 12, k as usize - 1);
            }
            self.insert_node(p, i);
        }
    }
    fn alloc_units_rare(&mut self, indx: usize) -> u32 {
        if self.glue_count == 0 {
            self.glue_count = 255;
            self.glue_free_blocks();
            if self.free[indx] != 0 {
                return self.remove_node(indx);
            }
        }
        let mut i = indx;
        loop {
            i += 1;
            if i == N_INDEXES {
                self.glue_count = self.glue_count.wrapping_sub(1);
                let n = Self::u2b(self.indx2units[indx] as u32);
                return if self.units_start - self.text > n {
                    self.units_start -= n;
                    self.units_start
                } else {
                    0
                };
            }
            if self.free[i] != 0 {
                break;
            }
        }
        let p = self.remove_node(i);
        self.split_block(p, i, indx);
        p
    }

    // --- model ---------------------------------------------------------------------------------------------------
    fn start_up(&mut self) {
        let mut k = 1u32;
        let mut i = 0usize;
        while i < N1 {
            self.indx2units[i] = k as u8;
            i += 1;
            k += 1;
        }
        k += 1;
        while i < N1 + N2 {
            self.indx2units[i] = k as u8;
            i += 1;
            k += 2;
        }
        k += 1;
        while i < N1 + N2 + N3 {
            self.indx2units[i] = k as u8;
            i += 1;
            k += 3;
        }
        k += 1;
        while i < N_INDEXES {
            self.indx2units[i] = k as u8;
            i += 1;
            k += 4;
        }
        let mut i = 0usize;
        for k in 0..128usize {
            i += ((self.indx2units[i] as usize) < k + 1) as usize;
            self.units2indx[k] = i as u8;
        }
        self.ns2bsindx[0] = 0;
        self.ns2bsindx[1] = 2;
        self.ns2bsindx[2..11].fill(4);
        self.ns2bsindx[11..].fill(6);
        self.ns2indx[0] = 0;
        self.ns2indx[1] = 1;
        let (mut m, mut k, mut step) = (2u32, 1u32, 1u32);
        for i in 2..256 {
            self.ns2indx[i] = m as u8;
            k -= 1;
            if k == 0 {
                step += 1;
                k = step;
                m += 1;
            }
        }
        for i in 0..UP_FREQ {
            self.freq2indx[i] = i as u8;
        }
        let (mut m, mut k, mut step) = (UP_FREQ as u32, 1u32, 1u32);
        let mut i = UP_FREQ;
        while i < 196 {
            self.freq2indx[i] = m as u8;
            k -= 1;
            if k == 0 {
                step += 1;
                k = step;
                m += 1;
            }
            i += 1;
        }
        self.freq2indx[196] = (m + 1) as u8;
        self.hb2flag[..0x40].fill(0);
        self.hb2flag[0x40..].fill(0x08);
        self.see[DUMMY_SEE] = See { summ: 0, shift: PERIOD_BITS, count: 0 };
    }

    fn restart_model(&mut self) {
        self.char_mask = [0; 256];
        let o = if self.max_order < 12 { self.max_order } else { 12 };
        self.run_length = -o - 1;
        self.init_rl = self.run_length;
        self.init_sub_allocator();
        self.hi_unit -= UNIT_SIZE;
        let mc = self.hi_unit;
        self.max_context = mc;
        self.set_suffix(mc, 0);
        self.order_fall = self.max_order;
        self.set_ns(mc, 255);
        self.set_summ(mc, 257);
        self.set_flags(mc, 0);
        let st = self.lo_unit;
        self.set_stats(mc, st);
        self.found_state = st;
        self.lo_unit += 256 / 2 * UNIT_SIZE;
        self.prev_success = 0;
        for i in 0..256u32 {
            let s = st + i * 6;
            self.set_u8(s, i as u8);
            self.set_freq(s, 1);
            self.set_succ(s, 0);
        }
        let mut i = 0usize;
        for m in 0..25usize {
            while self.freq2indx[i] as usize == m {
                i += 1;
            }
            for k in 0..8 {
                for n in (0..64).step_by(8) {
                    self.bin_summ[m][k + n] = (BIN_SCALE - INIT_BIN_ESC[k] / (i as u32 + 1)) as u16;
                }
            }
        }
        let mut i = 0usize;
        for m in 0..24usize {
            while self.ns2indx[i] as usize == m {
                i += 1;
            }
            for k in 0..32 {
                let shift = PERIOD_BITS - 4;
                self.see[m * 32 + k] = See { summ: ((2 * i as u32 + 5) << shift) as u16, shift, count: 7 };
            }
        }
        self.started = true;
    }

    fn start_model(&mut self, order: i32) -> Result<(), Error> {
        self.esc_count = 1;
        if order < 2 {
            if !self.started {
                return Err(Error("a solid block without a model"));
            }
            self.char_mask = [0; 256];
            let mut pc = self.max_context;
            self.order_fall = self.max_order;
            while self.suffix(pc) != 0 {
                pc = self.suffix(pc);
                self.order_fall -= 1;
            }
            self.found_state = self.stats(pc);
        } else {
            self.max_order = order;
            self.restart_model();
        }
        Ok(())
    }

    fn rescale(&mut self, pc: u32) {
        let old_ns = self.ns(pc) as i32;
        let mut i = old_ns;
        let stats = self.stats(pc);
        let mut p = self.found_state;
        while p != stats {
            self.swap_states(p, p - 6);
            p -= 6;
        }
        self.set_freq(stats, self.freq(stats) + 4);
        self.set_summ(pc, self.summ(pc) + 4);
        let mut esc_freq = self.summ(pc) as i32 - self.freq(p) as i32;
        let adder = (self.order_fall != 0) as u32;
        let f = (self.freq(p) + adder) >> 1;
        self.set_freq(p, f);
        self.set_summ(pc, f);
        loop {
            p += 6;
            esc_freq -= self.freq(p) as i32;
            let f = (self.freq(p) + adder) >> 1;
            self.set_freq(p, f);
            self.set_summ(pc, self.summ(pc) + f);
            if self.freq(p) > self.freq(p - 6) {
                let tmp = self.state(p);
                let mut p1 = p;
                loop {
                    self.copy(p1, p1 - 6, 6);
                    p1 -= 6;
                    if !(p1 != stats && tmp[1] as u32 > self.freq(p1 - 6)) {
                        break;
                    }
                }
                self.set_state(p1, tmp);
            }
            i -= 1;
            if i == 0 {
                break;
            }
        }
        if self.freq(p) == 0 {
            loop {
                i += 1;
                p -= 6;
                if self.freq(p) != 0 {
                    break;
                }
            }
            esc_freq += i;
            let ns = self.ns(pc) as i32 - i;
            self.set_ns(pc, ns as u32);
            if ns == 0 {
                let mut tmp = self.state(stats);
                let mut f = (2 * tmp[1] as i32 + esc_freq - 1) / esc_freq;
                if f > MAX_FREQ as i32 / 3 {
                    f = MAX_FREQ as i32 / 3;
                }
                tmp[1] = f as u8;
                let idx = self.units2indx[(old_ns >> 1) as usize] as usize;
                self.insert_node(stats, idx);
                let os = Self::one_state(pc);
                self.set_state(os, tmp);
                self.found_state = os;
                let fl = (self.flags(pc) & 0x10) + self.hb2flag[tmp[0] as usize];
                self.set_flags(pc, fl);
                return;
            }
            let n0 = (old_ns + 2) >> 1;
            let n1 = (ns + 2) >> 1;
            if n0 != n1 {
                let (i0, i1) = (self.units2indx[n0 as usize - 1] as usize, self.units2indx[n1 as usize - 1] as usize);
                if i0 != i1 {
                    if self.free[i1] != 0 {
                        let np = self.remove_node(i1);
                        self.copy(np, stats, Self::u2b(self.indx2units[i1] as u32));
                        self.insert_node(stats, i0);
                        self.set_stats(pc, np);
                    } else {
                        self.split_block(stats, i0, i1);
                    }
                }
            }
            let mut fl = self.flags(pc) & !0x08;
            let stats = self.stats(pc);
            for k in (0..=self.ns(pc)).rev() {
                fl |= self.hb2flag[self.sym(stats + k * 6) as usize];
            }
            self.set_flags(pc, fl);
        }
        esc_freq -= esc_freq >> 1;
        self.set_summ(pc, (self.summ(pc) as i32 + esc_freq) as u32);
        self.set_flags(pc, self.flags(pc) | 0x04);
        self.found_state = self.stats(pc);
    }

    fn create_successors(&mut self, skip: bool, p1: u32, min_context: u32) -> u32 {
        let mut pc = min_context;
        let up_branch = self.succ(self.found_state);
        let mut ps: [u32; MAX_O] = [0; MAX_O];
        let mut n = 0usize;
        let fsym = self.sym(self.found_state);
        let mut p;
        'search: {
            if !skip {
                ps[n] = self.found_state;
                n += 1;
                if self.suffix(pc) == 0 {
                    break 'search;
                }
            }
            let mut entry = p1 != 0;
            p = p1;
            if entry {
                pc = self.suffix(pc);
            }
            loop {
                if !entry {
                    pc = self.suffix(pc);
                    if self.ns(pc) != 0 {
                        p = self.stats(pc);
                        while self.sym(p) != fsym {
                            p += 6;
                        }
                    } else {
                        p = Self::one_state(pc);
                    }
                }
                entry = false;
                if self.succ(p) != up_branch {
                    pc = self.succ(p);
                    break;
                }
                if n < MAX_O {
                    ps[n] = p;
                }
                n += 1;
                if self.suffix(pc) == 0 {
                    break;
                }
            }
        }
        if n == 0 {
            return pc;
        }
        let up_sym = self.u8(up_branch);
        let up_succ = up_branch + 1;
        let up_freq = if self.ns(pc) != 0 {
            let mut p = self.stats(pc);
            while self.sym(p) != up_sym {
                p += 6;
            }
            let cf = self.freq(p).wrapping_sub(1);
            let s0 = self.summ(pc).wrapping_sub(self.ns(pc)).wrapping_sub(cf);
            1 + if 2 * cf <= s0 { (5 * cf > s0) as u32 } else { (cf + 2 * s0 - 3) / s0 }
        } else {
            self.freq(Self::one_state(pc))
        };
        let flag = self.hb2flag[up_sym as usize] + 2 * self.hb2flag[fsym as usize];
        let n = n.min(MAX_O);
        let mut k = n;
        while k > 0 {
            let pc1 = if self.hi_unit != self.lo_unit {
                self.hi_unit -= UNIT_SIZE;
                self.hi_unit
            } else if self.free[0] != 0 {
                self.remove_node(0)
            } else {
                let a = self.alloc_units_rare(0);
                if a == 0 {
                    return 0;
                }
                a
            };
            self.set_ns(pc1, 0);
            self.set_flags(pc1, flag);
            let os = Self::one_state(pc1);
            self.set_u8(os, up_sym);
            self.set_freq(os, up_freq);
            self.set_succ(os, up_succ);
            self.set_suffix(pc1, pc);
            k -= 1;
            self.set_succ(ps[k], pc1);
            pc = pc1;
        }
        pc
    }

    fn update_model(&mut self, min_context: u32) {
        let fs = self.state(self.found_state);
        let (fs_sym, fs_freq) = (fs[0], fs[1] as u32);
        let mut fs_succ = u32::from_le_bytes([fs[2], fs[3], fs[4], fs[5]]);
        let mut p: u32 = 0;
        if fs_freq < MAX_FREQ / 4 {
            let pc = self.suffix(min_context);
            if pc != 0 {
                if self.ns(pc) != 0 {
                    p = self.stats(pc);
                    if self.sym(p) != fs_sym {
                        loop {
                            p += 6;
                            if self.sym(p) == fs_sym {
                                break;
                            }
                        }
                        if self.freq(p) >= self.freq(p - 6) {
                            self.swap_states(p, p - 6);
                            p -= 6;
                        }
                    }
                    if self.freq(p) < MAX_FREQ - 9 {
                        self.set_freq(p, self.freq(p) + 2);
                        self.set_summ(pc, self.summ(pc) + 2);
                    }
                } else {
                    p = Self::one_state(pc);
                    self.set_freq(p, self.freq(p) + (self.freq(p) < 32) as u32);
                }
            }
        }
        if self.order_fall == 0 {
            let c = self.create_successors(true, p, min_context);
            self.max_context = c;
            self.set_succ(self.found_state, c);
            if c == 0 {
                return self.restart_after_overflow();
            }
            return;
        }
        self.set_u8(self.text, fs_sym);
        self.text += 1;
        let mut successor = self.text;
        if self.text >= self.units_start {
            return self.restart_after_overflow();
        }
        if fs_succ != 0 {
            if fs_succ <= self.text {
                fs_succ = self.create_successors(false, p, min_context);
                if fs_succ == 0 {
                    return self.restart_after_overflow();
                }
            }
            self.order_fall -= 1;
            if self.order_fall == 0 {
                successor = fs_succ;
                if self.max_context != min_context {
                    self.text -= 1;
                }
            }
        } else {
            self.set_succ(self.found_state, successor);
            fs_succ = min_context;
        }
        let flag = self.hb2flag[fs_sym as usize];
        let ns = self.ns(min_context);
        let s0 = self.summ(min_context).wrapping_sub(ns).wrapping_sub(fs_freq);
        let mut pc = self.max_context;
        while pc != min_context {
            let ns1 = self.ns(pc);
            if ns1 != 0 {
                let i = (ns1 + 1) >> 1;
                if ns1 & 1 != 0 {
                    let (i0, i1) = (self.units2indx[i as usize - 1] as usize, self.units2indx[i as usize] as usize);
                    if i0 != i1 {
                        let np = if self.free[i1] != 0 {
                            self.remove_node(i1)
                        } else {
                            let np = self.lo_unit;
                            self.lo_unit += Self::u2b(self.indx2units[i1] as u32);
                            if self.lo_unit > self.hi_unit {
                                self.lo_unit -= Self::u2b(self.indx2units[i1] as u32);
                                let a = self.alloc_units_rare(i1);
                                if a == 0 {
                                    return self.restart_after_overflow();
                                }
                                a
                            } else {
                                np
                            }
                        };
                        let st = self.stats(pc);
                        self.copy(np, st, Self::u2b(i));
                        self.insert_node(st, i0);
                        self.set_stats(pc, np);
                    }
                }
                self.set_summ(pc, self.summ(pc) + (3 * ns1 + 1 < ns) as u32);
            } else {
                let np = if self.free[0] != 0 {
                    self.remove_node(0)
                } else {
                    let np = self.lo_unit;
                    self.lo_unit += UNIT_SIZE;
                    if self.lo_unit > self.hi_unit {
                        self.lo_unit -= UNIT_SIZE;
                        let a = self.alloc_units_rare(0);
                        if a == 0 {
                            return self.restart_after_overflow();
                        }
                        a
                    } else {
                        np
                    }
                };
                let os = self.state(Self::one_state(pc));
                self.set_state(np, os);
                self.set_stats(pc, np);
                let f = self.freq(np);
                if f < MAX_FREQ / 4 - 1 {
                    self.set_freq(np, f + f);
                } else {
                    self.set_freq(np, MAX_FREQ - 4);
                }
                let s = self.freq(np).wrapping_add(self.init_esc).wrapping_add((ns > 2) as u32);
                self.set_summ(pc, s);
            }
            let mut cf = 2 * fs_freq * (self.summ(pc) + 6);
            let sf = s0.wrapping_add(self.summ(pc));
            if cf < 6 * sf {
                cf = 1 + (cf > sf) as u32 + (cf >= 4 * sf) as u32;
                self.set_summ(pc, self.summ(pc) + 4);
            } else {
                cf = 4 + (cf > 9 * sf) as u32 + (cf > 12 * sf) as u32 + (cf > 15 * sf) as u32;
                self.set_summ(pc, self.summ(pc) + cf);
            }
            let n = self.ns(pc) + 1;
            self.set_ns(pc, n);
            let p = self.stats(pc) + n * 6;
            self.set_succ(p, successor);
            self.set_u8(p, fs_sym);
            self.set_freq(p, cf);
            self.set_flags(pc, self.flags(pc) | flag);
            pc = self.suffix(pc);
        }
        self.max_context = fs_succ;
    }

    fn restart_after_overflow(&mut self) {
        self.restarts += 1;
        self.restart_model();
        self.esc_count = 0;
    }

    /// Decode one PPMd stream: `input` (with the 4 bytes that start the range coder) into at most `limit` bytes,
    /// up to the stream's end marker. `order` 2..=16 starts a new model of that order; 0 or 1 continues the
    /// previous model (a solid block). Returns the decoded bytes.
    pub fn decode(&mut self, input: &[u8], order: u32, limit: usize) -> Result<Vec<u8>, Error> {
        if order > MAX_O as u32 {
            return Err(Error("model order above 16"));
        }
        let mut out = Vec::with_capacity(limit.min(1 << 22));
        let mut pos = 0usize;
        // past the end the original reads EOS (-1): all ones
        let getc = |pos: &mut usize| -> u32 {
            let c = input.get(*pos).map_or(0xFFFF_FFFF, |&b| b as u32);
            *pos += 1;
            c
        };
        let (mut low, mut code, mut range) = (0u32, 0u32, 0xFFFF_FFFFu32);
        for _ in 0..4 {
            code = (code << 8) | getc(&mut pos);
        }
        self.start_model(order as i32)?;
        let mut ps = [0u32; 256];
        let mut pc = self.max_context;
        macro_rules! normalize {
            () => {
                loop {
                    if (low ^ low.wrapping_add(range)) < TOP {
                    } else if range < BOT {
                        range = low.wrapping_neg() & (BOT - 1);
                    } else {
                        break;
                    }
                    code = (code << 8) | getc(&mut pos);
                    range <<= 8;
                    low <<= 8;
                }
                if pos > input.len() + 8 {
                    return Err(Error("input ends before the end marker"));
                }
            };
        }
        loop {
            let ns = self.ns(pc);
            let (low_count, high_count);
            if ns != 0 {
                let mut p = self.stats(pc);
                let scale = self.summ(pc);
                range /= scale;
                if range == 0 {
                    return Err(Error("range underflow"));
                }
                let count = code.wrapping_sub(low) / range;
                let mut cnt = self.freq(p);
                if count < cnt {
                    high_count = cnt;
                    self.prev_success = ((high_count << 1) > scale) as u8;
                    self.run_length += self.prev_success as i32;
                    low_count = 0;
                    self.found_state = p;
                    self.set_freq(p, cnt + 4);
                    self.set_summ(pc, scale + 4);
                    if cnt + 4 > MAX_FREQ {
                        self.rescale(pc);
                    }
                } else {
                    self.prev_success = 0;
                    let mut i = ns;
                    let mut found = true;
                    loop {
                        p += 6;
                        cnt += self.freq(p);
                        if cnt > count {
                            break;
                        }
                        i -= 1;
                        if i == 0 {
                            found = false;
                            break;
                        }
                    }
                    if found {
                        high_count = cnt;
                        low_count = cnt - self.freq(p);
                        self.found_state = p;
                        self.set_freq(p, self.freq(p) + 4);
                        self.set_summ(pc, scale + 4);
                        if self.freq(p) > self.freq(p - 6) {
                            self.swap_states(p, p - 6);
                            p -= 6;
                            self.found_state = p;
                            if self.freq(p) > MAX_FREQ {
                                self.rescale(pc);
                            }
                        }
                    } else {
                        low_count = cnt;
                        let ec = self.esc_count;
                        self.char_mask[self.sym(p) as usize] = ec;
                        self.num_masked = ns as u8;
                        let mut i = ns;
                        loop {
                            p -= 6;
                            self.char_mask[self.sym(p) as usize] = ec;
                            i -= 1;
                            if i == 0 {
                                break;
                            }
                        }
                        self.found_state = 0;
                        high_count = scale;
                    }
                }
            } else {
                let p = Self::one_state(pc);
                let row = self.freq2indx[self.freq(p) as usize - 1] as usize;
                let col = self.prev_success as usize
                    + self.ns2bsindx[self.ns(self.suffix(pc)) as usize] as usize
                    + self.flags(pc) as usize
                    + ((self.run_length >> 26) & 0x20) as usize;
                let cnt = self.bin_summ[row][col] as u32;
                range >>= TOT_BITS;
                if range == 0 {
                    return Err(Error("range underflow"));
                }
                let mean = (cnt + (1 << (PERIOD_BITS - 2))) >> PERIOD_BITS;
                if code.wrapping_sub(low) / range < cnt {
                    self.found_state = p;
                    self.set_freq(p, self.freq(p) + (self.freq(p) < 196) as u32);
                    low_count = 0;
                    high_count = cnt;
                    self.bin_summ[row][col] = (cnt + INTERVAL - mean) as u16;
                    self.prev_success = 1;
                    self.run_length += 1;
                } else {
                    low_count = cnt;
                    let v = (cnt - mean) as u16;
                    self.bin_summ[row][col] = v;
                    high_count = BIN_SCALE;
                    self.init_esc = EXP_ESCAPE[(v >> 10) as usize] as u32;
                    self.char_mask[self.sym(p) as usize] = self.esc_count;
                    self.num_masked = 0;
                    self.prev_success = 0;
                    self.found_state = 0;
                }
            }
            low = low.wrapping_add(range.wrapping_mul(low_count));
            range = range.wrapping_mul(high_count - low_count);
            while self.found_state == 0 {
                normalize!();
                loop {
                    self.order_fall += 1;
                    pc = self.suffix(pc);
                    if pc == 0 {
                        return Ok(out);
                    }
                    if self.ns(pc) != self.num_masked as u32 {
                        break;
                    }
                }
                let nsc = self.ns(pc);
                let psee;
                let mut scale;
                if nsc != 0xFF {
                    let row = self.ns2indx[nsc as usize - 1] as usize;
                    let col = (self.summ(pc) > 11 * (nsc + 1)) as usize
                        + 2 * ((2 * nsc) < self.ns(self.suffix(pc)) + self.num_masked as u32) as usize
                        + self.flags(pc) as usize;
                    psee = row * 32 + col;
                    let s = &mut self.see[psee];
                    scale = (s.summ >> s.shift) as u32;
                    s.summ = s.summ.wrapping_sub(scale as u16);
                    scale += (scale == 0) as u32;
                } else {
                    psee = DUMMY_SEE;
                    scale = 1;
                }
                let mut cnt = 0u32;
                let mut i = nsc - self.num_masked as u32;
                let mut p = self.stats(pc).wrapping_sub(6);
                let mut n = 0usize;
                let ec = self.esc_count;
                loop {
                    loop {
                        p += 6;
                        if self.char_mask[self.sym(p) as usize] != ec {
                            break;
                        }
                    }
                    cnt += self.freq(p);
                    ps[n] = p;
                    n += 1;
                    i -= 1;
                    if i == 0 {
                        break;
                    }
                }
                scale += cnt;
                range /= scale;
                if range == 0 {
                    return Err(Error("range underflow"));
                }
                let count = code.wrapping_sub(low) / range;
                let (lc, hc);
                if count < cnt {
                    let mut k = 0usize;
                    let mut c = 0u32;
                    let mut p = ps[0];
                    loop {
                        c += self.freq(p);
                        if c > count {
                            break;
                        }
                        k += 1;
                        p = ps[k];
                    }
                    hc = c;
                    lc = c - self.freq(p);
                    self.found_state = p;
                    self.set_freq(p, self.freq(p) + 4);
                    self.set_summ(pc, self.summ(pc) + 4);
                    if self.freq(p) > MAX_FREQ {
                        self.rescale(pc);
                    }
                    self.esc_count = self.esc_count.wrapping_add(1);
                    self.run_length = self.init_rl;
                    let s = &mut self.see[psee];
                    if s.shift < PERIOD_BITS {
                        s.count = s.count.wrapping_sub(1);
                        if s.count == 0 {
                            s.summ = s.summ.wrapping_add(s.summ);
                            s.count = (3u32 << s.shift) as u8;
                            s.shift += 1;
                        }
                    }
                } else {
                    lc = cnt;
                    hc = scale;
                    for &q in &ps[..n] {
                        self.char_mask[self.sym(q) as usize] = ec;
                    }
                    let s = &mut self.see[psee];
                    s.summ = s.summ.wrapping_add(scale as u16);
                    self.num_masked = nsc as u8;
                }
                low = low.wrapping_add(range.wrapping_mul(lc));
                range = range.wrapping_mul(hc - lc);
            }
            if out.len() < limit {
                out.push(self.sym(self.found_state));
            }
            let fsucc = self.succ(self.found_state);
            if self.order_fall == 0 && fsucc > self.text {
                self.max_context = fsucc;
            } else {
                self.update_model(pc);
                if self.esc_count == 0 {
                    self.char_mask = [0; 256];
                    self.esc_count = 1;
                }
            }
            pc = self.max_context;
            normalize!();
        }
    }
}

mod enc;
pub use enc::encode;

#[cfg(test)]
pub(crate) mod tests;
