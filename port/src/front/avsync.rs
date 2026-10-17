//! Picture / sound timing of the interactive fronts (port side, no engine code: the original plays its movies through
//! QuickTime, C9). HYPOTHESES H1790–H1795 (RE_NOTES; measured with a probe, not traced against the
//! original — the bottle had no audio driver, E7).
//!
//! The session clock ([`super::Session::now`], the intro's clock) picks both the video frame and the sound position
//! ([`super::Session::audio_cues`]). What reaches the eyes and ears differs by the outputs' latencies: a sample mixed
//! now is heard `L` later (the audio queue of the platform), a frame drawn now is seen about one display refresh later.
//! The fronts therefore delay the picture by the difference ([`super::Session::av_delay`]) — the picture follows the
//! audio clock (what is heard), not the mixer's write head.
//!
//! - [`WallClock`] (H1790): wall-clock elapsed time → whole-ms ticks without losing the remainder. The window loop took
//!   `as_millis()` of each iteration and dropped the fraction — ~0.67 ms of a 16.67 ms vsync frame, the game clock ran
//!   ~4 % slow, the sound (played at the device's rate) drifted ahead of the picture and was pulled back 200 ms
//!   ([`super::host::Mix::sync`]) every ~5 s.
//! - [`Pump`] (H1791–H1793): the web sink's queue kept at a fixed depth from the frames the sink reports as consumed
//!   (no wall-clock lead that grows with the audio device's clock drift or the sink's stalls), and the latency estimate
//!   that becomes the picture delay.

use std::time::Duration;

/// HYPOTHESIS H1790: the window loop's clock — whole milliseconds per iteration, the sub-millisecond rest carried to the
/// next one (the game clock = wall time).
#[derive(Clone, Copy, Debug, Default)]
pub struct WallClock {
    carry_us: u64,
}

impl WallClock {
    /// Elapsed wall time since the previous step → ms to tick (`max_ms` clamps a stall, as the old `dt.min(250)`; the
    /// clamped part is dropped, not carried).
    pub fn step(&mut self, elapsed: Duration, max_ms: u64) -> u64 {
        let us = self.carry_us + elapsed.as_micros() as u64;
        let ms = us / 1000;
        if ms > max_ms {
            self.carry_us = 0;
            return max_ms;
        }
        self.carry_us = us % 1000;
        ms
    }
}

/// Display latency assumed for both fronts: a frame drawn now is on screen at the next refresh (60 Hz). Estimate, not
/// measured (no photodiode); the window and the browser both present on vsync.
pub const DISPLAY_MS: f64 = 1000.0 / 60.0;

/// HYPOTHESIS H1794: the window's (SDL) sound latency — a sample mixed in the callback is heard after the buffer that
/// plays meanwhile: on average half a buffer of waiting for the callback (a cue set by the loop is mixed at the next
/// one) plus one buffer (SDL's CoreAudio / WASAPI backends keep two). `samples` = the obtained buffer size in frames.
pub fn sdl_latency_ms(samples: u32, rate: u32) -> f64 {
    1.5 * samples as f64 * 1000.0 / rate.max(1) as f64
}

/// The picture delay for an audio latency `latency_ms` (minus the display's own latency, never negative).
pub fn picture_delay_ms(latency_ms: f64) -> u64 {
    (latency_ms - DISPLAY_MS).max(0.0).round() as u64
}

/// HYPOTHESES H1791–H1793: the web front's audio pump (port/web: worker.js → the sink in audio.js, an AudioWorklet or
/// the ScriptProcessor fallback). Each tick the pump tops the sink's queue up to `target` frames and estimates the
/// latency of the frames it mixes, from the sink's last [`Report`].
#[derive(Clone, Debug)]
pub struct Pump {
    pub rate: u32,
    /// queue depth to keep, frames
    pub target: u32,
    /// frames sent to the sink so far
    pub produced: u64,
    /// smoothed latency (mix → heard), seconds; None before the first estimate
    pub latency_s: Option<f64>,
    /// the sink ran dry: the next mixing step's latency replaces the estimate (the voices it starts keep it)
    reseed: bool,
}

/// What the sink last said, and how old that is.
#[derive(Clone, Copy, Debug, Default)]
pub struct Report {
    /// frames taken from the queue so far (dropped ones included)
    pub consumed: u64,
    /// seconds from the report until the first frame not yet taken reaches the graph's output (the AudioWorklet: the
    /// quantum it just took; the ScriptProcessor: its buffer's playback time + the buffer)
    pub lead_s: f64,
    /// the sink takes this much at once (the ScriptProcessor's buffer; 0 = continuously, the AudioWorklet's 128-frame
    /// quanta): no frames are taken until it has played that long
    pub block_s: f64,
    /// the AudioContext's own latency, graph output → speaker (baseLatency + outputLatency)
    pub out_s: f64,
    /// seconds since the report
    pub age_s: f64,
}

/// What one pump step sends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Step {
    /// frames of silence first (H1792: the sink ran dry or this is the start — the queue is refilled with silence, so
    /// what is mixed next is heard as late as the estimate says)
    pub silence: u32,
    /// then this many mixed frames
    pub mix: u32,
    /// the latency of the first mixed frame, seconds (mix → heard)
    pub latency_s: f64,
}

impl Pump {
    pub fn new(rate: u32, target: u32) -> Pump {
        Pump { rate: rate.max(1), target, produced: 0, latency_s: None, reseed: true }
    }

    /// One tick: what to send now (at most a quarter second of mixed frames at once, or the target if it is longer).
    pub fn step(&mut self, r: Report) -> Step {
        let rate = self.rate as f64;
        // frames the sink took since its report (it plays on while the report travels; a report older than 0.1 s
        // means the sink stands still — a suspended context — and is not extrapolated further)
        let age = r.age_s.clamp(0.0, 0.1);
        let taken = (r.consumed + ((age - r.block_s).max(0.0) * rate) as u64).min(self.produced);
        let queued = self.produced - taken;
        // H1792: the sink took all that was sent (it ran dry: a worker stall, a hidden tab; or nothing was sent yet)
        let dry = r.consumed >= self.produced;
        let (silence, mix) = if dry {
            (self.target, 0)
        } else {
            (0, (self.target as u64).saturating_sub(queued).min((self.rate as u64 / 4).max(self.target as u64)) as u32)
        };
        // H1793: the first frame mixed now is heard after the queue, the sink's own lead and the context's latency
        let l = (queued + silence as u64) as f64 / rate + (r.lead_s - age).max(0.0) + r.out_s;
        if dry {
            self.reseed = true;
            self.latency_s.get_or_insert(l);
        } else {
            self.latency_s = Some(match self.latency_s {
                Some(p) if !self.reseed && (l - p).abs() < 0.25 => p + 0.1 * (l - p),
                _ => l,
            });
            self.reseed = false;
        }
        self.produced += (silence + mix) as u64;
        Step { silence, mix, latency_s: l }
    }

    /// The picture delay (ms) for the current latency estimate.
    pub fn picture_delay_ms(&self) -> u64 {
        picture_delay_ms(self.latency_s.unwrap_or(0.0) * 1000.0)
    }
}

/// HYPOTHESIS H1795: one audio tick of the web front (port/web `xf_pump`, after the frame's `xf_tick`): the pump's
/// step, the voices put where their frames will be heard in step with the picture (a voice off by more than 40 ms is
/// moved — after a stall of the worker; less, its rate is trimmed by up to 0.3 % — the device clock's drift), the
/// picture delay updated. `out` = interleaved stereo i16 to send.
pub fn web_audio(s: &mut super::Session, mix: &mut super::host::Mix, pump: &mut Pump, r: Report, out: &mut Vec<i16>) -> Step {
    let st = pump.step(r);
    s.av_delay = pump.picture_delay_ms();
    // the first mixed frame is heard `st.latency_s` from now, its picture is seen av_delay + a refresh from now
    let offset = st.latency_s - (s.av_delay as f64 + DISPLAY_MS) / 1000.0;
    // a step of silence only mixes nothing: the voices follow at the next step (else they would start one tick late)
    if st.mix > 0 {
        let cues = s.audio_cues();
        mix.sync_to(&cues, offset, 0.04, 0.003);
    }
    out.clear();
    out.resize(st.silence as usize * 2, 0);
    let n = out.len();
    out.resize(n + st.mix as usize * 2, 0);
    mix.fill(&mut out[n..]);
    st
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H1790: 72.4 s of 16.67 ms vsync iterations tick 72.4 s of game time (the old `as_millis()` per iteration: 69.5 s)
    #[test]
    fn wall_clock_keeps_the_fraction() {
        let frame = Duration::from_micros(16_667);
        let mut c = WallClock::default();
        let n = 4343; // 72.39 s
        let (mut new, mut old) = (0u64, 0u64);
        for _ in 0..n {
            new += c.step(frame, 250);
            old += frame.as_millis() as u64;
        }
        let wall = n as u64 * 16_667 / 1000;
        assert!(wall.abs_diff(new) <= 1, "{new} vs {wall}");
        assert!(wall - old > 2800, "the old clock lost {} ms", wall - old);
        // a stall is clamped, not carried
        assert_eq!(c.step(Duration::from_millis(900), 250), 250);
        assert_eq!(c.step(Duration::from_micros(10_400), 250), 10);
    }

    /// A sink model as audio.js: takes `quantum` frames every `quantum / rate` (device clock, `ppm` off the wall clock)
    /// and reports every `every` takes; the worker ticks every 16.67 ms. Returns (max queue, latency estimate at the
    /// end, true latency at the end) in ms. `block` = the ScriptProcessor (whole buffers), else the AudioWorklet.
    fn run(quantum: u32, every: u32, block: bool, ppm: f64, secs: f64, target: u32, out_s: f64) -> (f64, f64, f64) {
        let rate = 48_000u32;
        let mut p = Pump::new(rate, target);
        let (mut pushed, mut pulled) = (0u64, 0u64);
        let dev_rate = rate as f64 * (1.0 + ppm * 1e-6);
        let q = quantum as f64 / dev_rate;
        let (mut t, mut next_pull, mut takes) = (0.0f64, 0.0f64, 0u32);
        let (mut max_q, mut rep, mut rep_t) = (0u64, Report { out_s, ..Default::default() }, 0.0f64);
        let mut last_true = 0.0;
        let mut pull_t = 0.0;
        while t < secs {
            while next_pull <= t {
                let avail = pushed - pulled;
                pulled += avail.min(quantum as u64);
                pull_t = next_pull;
                takes += 1;
                if takes % every == 0 {
                    // the buffer taken plays from now on; the next frame after it
                    rep = Report { consumed: pulled, lead_s: q, block_s: if block { q } else { 0.0 }, out_s, age_s: 0.0 };
                    rep_t = next_pull;
                }
                next_pull += q;
            }
            let st = p.step(Report { age_s: t - rep_t, ..rep });
            pushed += st.silence as u64;
            // true latency of the first frame mixed now: what is queued, the rest of the buffer playing, the context's
            last_true = (pushed - pulled) as f64 / dev_rate + (pull_t + q - t).max(0.0) + out_s;
            pushed += st.mix as u64;
            max_q = max_q.max(pushed - pulled);
            t += 1.0 / 60.0;
        }
        (max_q as f64 * 1000.0 / rate as f64, p.latency_s.unwrap() * 1000.0, last_true * 1000.0)
    }

    /// H1791 / H1793: the queue stays at its target over an hour whatever the device clock's drift (the old wall-clock
    /// lead grew to its 0.5 s cap), and the estimate follows the true latency.
    #[test]
    fn pump_depth_is_bounded_and_estimated() {
        // the AudioWorklet (128-frame quanta, a report every 8) and the ScriptProcessor fallback (1024-frame buffers)
        for (quantum, every, block, target) in [(128u32, 8u32, false, 3840u32), (1024, 1, true, 4800)] {
            for ppm in [-300.0, 0.0, 300.0] {
                let (max_q, est, truth) = run(quantum, every, block, ppm, 3600.0, target, 0.02);
                let tq = target as f64 * 1000.0 / 48_000.0;
                eprintln!("quantum {quantum} ppm {ppm}: max queue {max_q:.1} ms (target {tq:.1}), latency {est:.1} ms (true {truth:.1})");
                // a report is up to 21 ms stale: a few ms over the target, never growing
                assert!(max_q <= tq + 5.0, "quantum {quantum} ppm {ppm}: queue {max_q} ms > {tq}");
                assert!((est - truth).abs() < 15.0, "quantum {quantum} ppm {ppm}: estimate {est} vs {truth}");
            }
        }
    }

    #[test]
    fn delays() {
        // SDL 1024 frames at 44.1 kHz: 34.8 ms heard, 16.7 ms seen → the picture 18 ms late
        assert_eq!(picture_delay_ms(sdl_latency_ms(1024, 44_100)), 18);
        assert_eq!(picture_delay_ms(5.0), 0);
    }
}
