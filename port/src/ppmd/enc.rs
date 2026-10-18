//! The matching encoder: a Rust rewrite of the reference's `PPMdEncode` (MIT, see LICENSE-THIRD-PARTY). The port
//! only decodes; the encoder makes test data (the decoder's roundtrip tests, synthetic installers).
use super::*;

/// Encode `input` with `m` into a stream [`Model::decode`] reads back (order below 2 = continue the model, a
/// solid block); `m` must have been used the same way as the decoder's model.
pub fn encode(m: &mut Model, input: &[u8], order: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let (mut low, mut range) = (0u32, 0xFFFF_FFFFu32);
    m.start_model(order as i32).unwrap();
    let mut pc = m.max_context;
    let mut at = 0usize;
    macro_rules! normalize {
        () => {
            loop {
                if (low ^ low.wrapping_add(range)) < TOP {
                } else if range < BOT {
                    range = low.wrapping_neg() & (BOT - 1);
                } else {
                    break;
                }
                out.push((low >> 24) as u8);
                range <<= 8;
                low <<= 8;
            }
        };
    }
    loop {
        // -1: the end of the input, a symbol no context has (escapes down to the end marker)
        let c: i32 = input.get(at).map_or(-1, |&b| b as i32);
        at += 1;
        let ns = m.ns(pc);
        let (low_count, high_count);
        if ns != 0 {
            let mut p = m.stats(pc);
            let scale = m.summ(pc);
            if m.sym(p) as i32 == c {
                high_count = m.freq(p);
                m.prev_success = (2 * high_count > scale) as u8;
                m.run_length += m.prev_success as i32;
                low_count = 0;
                m.found_state = p;
                m.set_freq(p, high_count + 4);
                m.set_summ(pc, scale + 4);
                if high_count + 4 > MAX_FREQ {
                    m.rescale(pc);
                }
            } else {
                m.prev_success = 0;
                let mut cnt = m.freq(p);
                let mut i = ns;
                let mut found = true;
                loop {
                    p += 6;
                    if m.sym(p) as i32 == c {
                        break;
                    }
                    cnt += m.freq(p);
                    i -= 1;
                    if i == 0 {
                        found = false;
                        break;
                    }
                }
                if found {
                    low_count = cnt;
                    high_count = cnt + m.freq(p);
                    m.found_state = p;
                    m.set_freq(p, m.freq(p) + 4);
                    m.set_summ(pc, scale + 4);
                    if m.freq(p) > m.freq(p - 6) {
                        m.swap_states(p, p - 6);
                        p -= 6;
                        m.found_state = p;
                        if m.freq(p) > MAX_FREQ {
                            m.rescale(pc);
                        }
                    }
                } else {
                    low_count = cnt;
                    let ec = m.esc_count;
                    m.char_mask[m.sym(p) as usize] = ec;
                    m.num_masked = ns as u8;
                    let mut i = ns;
                    loop {
                        p -= 6;
                        m.char_mask[m.sym(p) as usize] = ec;
                        i -= 1;
                        if i == 0 {
                            break;
                        }
                    }
                    m.found_state = 0;
                    high_count = scale;
                }
            }
            range /= scale;
            low = low.wrapping_add(low_count * range);
            range *= high_count - low_count;
        } else {
            let p = Model::one_state(pc);
            let row = m.freq2indx[m.freq(p) as usize - 1] as usize;
            let col = m.prev_success as usize
                + m.ns2bsindx[m.ns(m.suffix(pc)) as usize] as usize
                + m.flags(pc) as usize
                + ((m.run_length >> 26) & 0x20) as usize;
            let cnt = m.bin_summ[row][col] as u32;
            let mean = (cnt + (1 << (PERIOD_BITS - 2))) >> PERIOD_BITS;
            if m.sym(p) as i32 == c {
                m.found_state = p;
                m.set_freq(p, m.freq(p) + (m.freq(p) < 196) as u32);
                low_count = 0;
                high_count = cnt;
                m.bin_summ[row][col] = (cnt + INTERVAL - mean) as u16;
                m.prev_success = 1;
                m.run_length += 1;
            } else {
                low_count = cnt;
                let v = (cnt - mean) as u16;
                m.bin_summ[row][col] = v;
                high_count = BIN_SCALE;
                m.init_esc = EXP_ESCAPE[(v >> 10) as usize] as u32;
                m.char_mask[m.sym(p) as usize] = m.esc_count;
                m.num_masked = 0;
                m.prev_success = 0;
                m.found_state = 0;
            }
            range >>= TOT_BITS;
            low = low.wrapping_add(low_count * range);
            range *= high_count - low_count;
        }
        while m.found_state == 0 {
            normalize!();
            loop {
                m.order_fall += 1;
                pc = m.suffix(pc);
                if pc == 0 {
                    for k in [24, 16, 8, 0] {
                        out.push((low >> k) as u8);
                    }
                    return out;
                }
                if m.ns(pc) != m.num_masked as u32 {
                    break;
                }
            }
            let nsc = m.ns(pc);
            let psee;
            let mut scale;
            if nsc != 0xFF {
                let row = m.ns2indx[nsc as usize - 1] as usize;
                let col = (m.summ(pc) > 11 * (nsc + 1)) as usize
                    + 2 * ((2 * nsc) < m.ns(m.suffix(pc)) + m.num_masked as u32) as usize
                    + m.flags(pc) as usize;
                psee = row * 32 + col;
                let s = &mut m.see[psee];
                scale = (s.summ >> s.shift) as u32;
                s.summ = s.summ.wrapping_sub(scale as u16);
                scale += (scale == 0) as u32;
            } else {
                psee = DUMMY_SEE;
                scale = 1;
            }
            let mut cnt = 0u32;
            let mut i = nsc - m.num_masked as u32;
            let mut p = m.stats(pc).wrapping_sub(6);
            let ec = m.esc_count;
            let (mut lc, mut hc) = (0, 0);
            let mut found = false;
            loop {
                loop {
                    p += 6;
                    if m.char_mask[m.sym(p) as usize] != ec {
                        break;
                    }
                }
                cnt += m.freq(p);
                if m.sym(p) as i32 == c {
                    hc = cnt;
                    lc = cnt - m.freq(p);
                    let mut p1 = p;
                    i -= 1;
                    while i > 0 {
                        loop {
                            p1 += 6;
                            if m.char_mask[m.sym(p1) as usize] != ec {
                                break;
                            }
                        }
                        cnt += m.freq(p1);
                        i -= 1;
                    }
                    scale += cnt;
                    m.found_state = p;
                    m.set_freq(p, m.freq(p) + 4);
                    m.set_summ(pc, m.summ(pc) + 4);
                    if m.freq(p) > MAX_FREQ {
                        m.rescale(pc);
                    }
                    m.esc_count = m.esc_count.wrapping_add(1);
                    m.run_length = m.init_rl;
                    let s = &mut m.see[psee];
                    if s.shift < PERIOD_BITS {
                        s.count = s.count.wrapping_sub(1);
                        if s.count == 0 {
                            s.summ = s.summ.wrapping_add(s.summ);
                            s.count = (3u32 << s.shift) as u8;
                            s.shift += 1;
                        }
                    }
                    found = true;
                    break;
                }
                m.char_mask[m.sym(p) as usize] = ec;
                i -= 1;
                if i == 0 {
                    break;
                }
            }
            if !found {
                lc = cnt;
                scale += cnt;
                hc = scale;
                let s = &mut m.see[psee];
                s.summ = s.summ.wrapping_add(scale as u16);
                m.num_masked = nsc as u8;
            }
            range /= scale;
            low = low.wrapping_add(lc * range);
            range *= hc - lc;
        }
        let fsucc = m.succ(m.found_state);
        if m.order_fall == 0 && fsucc > m.text {
            m.max_context = fsucc;
        } else {
            m.update_model(pc);
            if m.esc_count == 0 {
                m.char_mask = [0; 256];
                m.esc_count = 1;
            }
        }
        pc = m.max_context;
        normalize!();
    }
}
