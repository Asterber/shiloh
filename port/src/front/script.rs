//! Scripted user input for headless frame dumps and UI tests: the step syntax of the research planner's `--steps`
//! (`p:X,Y` hover + click, `h:X,Y,OWNER` click at X,Y whose target OWNER (hex) is given by hand, `m:X,Y` pointer move, `r:X,Y` right click, `o:OWNER` click by owner (hex), `k:VK` one key,
//! `t:TEXT` one key per character, `a:C` Alt+C / `A:C` Shift+Alt+C for a letter or digit C — H2060's Alt hotkeys). Display aid only — every step goes through the same Session / Runner input calls
//! the window uses.
use super::Session;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Input {
    Click(i32, i32),
    Move(i32, i32),
    Right(i32, i32),
    Owner(u32),
    Hand(i32, i32, u32),
    Key(u32),
    Type(String),
    /// H2060: an Alt hotkey (virtual-key code, Shift held)
    Alt(u32, bool),
}

fn pt(s: &str) -> Option<(i32, i32)> {
    let (x, y) = s.split_once(',')?;
    Some((x.trim().parse().ok()?, y.trim().parse().ok()?))
}

fn num(s: &str) -> Option<u32> {
    match s.strip_prefix("0x") {
        Some(h) => u32::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Parse `"p:105,190;m:300,450;o:4700"`.
pub fn parse(steps: &str) -> Result<Vec<Input>, String> {
    steps
        .split(';')
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(|x| {
            let (k, v) = x.split_once(':').ok_or_else(|| format!("step {x}: KIND:VALUE expected"))?;
            let bad = || format!("step {x}: bad value");
            Ok(match k {
                "p" | "x" => pt(v).map(|(a, b)| Input::Click(a, b)).ok_or_else(bad)?,
                "m" => pt(v).map(|(a, b)| Input::Move(a, b)).ok_or_else(bad)?,
                "r" => pt(v).map(|(a, b)| Input::Right(a, b)).ok_or_else(bad)?,
                "h" => {
                    let mut it = v.split(',');
                    let mut n = || it.next().map(str::trim);
                    match (n().and_then(|x| x.parse().ok()), n().and_then(|y| y.parse().ok()), n()) {
                        (Some(x), Some(y), Some(o)) => {
                            Input::Hand(x, y, u32::from_str_radix(o.trim_start_matches("0x"), 16).map_err(|_| bad())?)
                        }
                        _ => return Err(bad()),
                    }
                }
                "o" => u32::from_str_radix(v.trim_start_matches("0x"), 16).map(Input::Owner).map_err(|_| bad())?,
                "k" => num(v).map(Input::Key).ok_or_else(bad)?,
                "t" => Input::Type(v.to_string()),
                "a" | "A" => match super::host::vk(v.trim()) {
                    Some(c) if (0x30..=0x5a).contains(&c) => Input::Alt(c, k == "A"),
                    _ => return Err(bad()),
                },
                _ => return Err(format!("step {x}: unknown kind {k}")),
            })
        })
        .collect()
}

/// Apply one input; the caller settles afterwards (`t:` settles after each key itself).
pub fn apply(s: &mut Session, i: &Input) {
    match *i {
        Input::Click(x, y) => {
            s.hover_at(x, y);
            s.click_at(x, y);
        }
        Input::Move(x, y) => {
            s.hover_at(x, y);
        }
        Input::Right(x, y) => {
            s.mouse = Some((x, y));
            s.r.right_click(x, y);
        }
        Input::Owner(o) => {
            s.r.click(o);
        }
        Input::Hand(x, y, o) => {
            s.mouse = Some((x, y));
            s.r.hover(Some(o));
            s.r.click(o);
        }
        Input::Key(k) => {
            // the window's key path (Esc → the menu, H169 / H404)
            let l = super::host::key(s, k);
            s.log.push(l);
        }
        Input::Alt(k, shift) => {
            let l = super::host::alt(s, k, shift).describe();
            s.log.push(l);
        }
        Input::Type(ref t) => {
            for c in t.chars() {
                // VkKeyScanA as the bot (xfcap.c): '-' / '.' / ',' are VK_OEM_MINUS / PERIOD / COMMA, then their WM_CHAR
                // (H867: two key events)
                for k in super::host::key_events_of_char(c) {
                    s.r.key_press(k);
                }
                super::settle_timers(s, 100, 900_000);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_steps() {
        assert_eq!(
            parse("p:105,190; m:1,2;o:4700;k:13;t:AB;h:117,336,57a3;a:p;A:I").unwrap(),
            [
                Input::Click(105, 190),
                Input::Move(1, 2),
                Input::Owner(0x4700),
                Input::Key(13),
                Input::Type("AB".into()),
                Input::Hand(117, 336, 0x57a3),
                Input::Alt(0x50, false),
                Input::Alt(0x49, true)
            ]
        );
        assert!(parse("q:1").is_err());
        assert!(parse("a:F1").is_err());
    }
}
