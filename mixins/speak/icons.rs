use std::cell::RefCell;

/// How long a talking indicator stays after the last voice packet
/// (`TalkCache.TIMEOUT`, 250 ms = 5 ticks).
pub const FADE_TICKS: u64 = 5;

/// The indicator colours, 0xAARRGGBB.
pub const COLOR_TALKING: u32 = 0xFF55FF55;
pub const COLOR_WHISPER: u32 = 0xFFFFCC55;
pub const COLOR_MUTED: u32 = 0xFFAA2222;
pub const COLOR_DEAFENED: u32 = 0xFFAA66CC;
pub const COLOR_OFF: u32 = 0xFF999999;
/// The disconnected icon's colour (`icons/disconnected`).
pub const COLOR_DISCONNECTED: u32 = 0xFFCC4444;

const MARGIN: f32 = 4.0;
const LINE_H: f32 = 9.0;
const PAD: f32 = 1.0;
const STRIDE: f32 = LINE_H + PAD;
const ICON_X: f32 = 12.0;
const MAX_NAME: usize = 16;
const MAX_TALKERS: usize = 8;

/// One draw item, in GUI pixels relative to the HUD box's top-left corner.
#[derive(Clone, Debug, PartialEq)]
pub enum Draw {
    Rect { x: f32, y: f32, w: f32, h: f32, argb: u32 },
    Text { x: f32, y: f32, line: String, argb: u32 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    /// `icons/speaker`: two waves, transmitting normally.
    Talking,
    /// `icons/speaker_whisper`: one wave, transmitting whispered.
    Whispering,
    /// `icons/microphone_off`: a slash, the microphone muted.
    Muted,
    /// `icons/speaker_off` struck through: hearing nothing (deafened).
    Deafened,
    /// `icons/speaker_forced_off`: bare and gray, the voice chat off.
    Off,
    /// `icons/disconnected`: the session is down.
    Disconnected,
}

impl Icon {
    pub fn color(self) -> u32 {
        match self {
            Icon::Talking => COLOR_TALKING,
            Icon::Whispering => COLOR_WHISPER,
            Icon::Muted => COLOR_MUTED,
            Icon::Deafened => COLOR_DEAFENED,
            Icon::Off => COLOR_OFF,
            Icon::Disconnected => COLOR_DISCONNECTED,
        }
    }

    /// The icon's pixels, as 1-GUI-px rects in a 7×7 cell at (x, y): a
    /// speaker core (a back and a cone) and what its state adds.
    pub fn draw(self, x: f32, y: f32) -> Vec<Draw> {
        let color = self.color();
        let rect = |x: f32, y: f32, w: f32, h: f32| Draw::Rect { x, y, w, h, argb: color };
        let mut out = vec![rect(x, y + 2.0, 1.0, 3.0), rect(x + 1.0, y + 1.0, 2.0, 5.0)];
        let dot = |out: &mut Vec<Draw>, dx: f32, dy: f32| out.push(rect(x + dx, y + dy, 1.0, 1.0));
        match self {
            // A short arc and a tall one, like sound carrying.
            Icon::Talking => {
                out.push(rect(x + 4.0, y + 2.0, 1.0, 3.0));
                out.push(rect(x + 6.0, y + 1.0, 1.0, 5.0));
            }
            Icon::Whispering => out.push(rect(x + 5.0, y + 2.0, 1.0, 3.0)),
            Icon::Muted => {
                dot(&mut out, 4.0, 5.0);
                dot(&mut out, 5.0, 4.0);
                dot(&mut out, 6.0, 3.0);
            }
            Icon::Deafened => {
                for d in 0..5 {
                    let d = d as f32;
                    dot(&mut out, 1.0 + d, 1.0 + d);
                }
            }
            Icon::Off | Icon::Disconnected => {}
        }
        out
    }
}

/// One talking player (`TalkCache`'s fresh entries, as the HUD reads them).
#[derive(Clone, Debug)]
struct Talker {
    sender: u128,
    name: String,
    whisper: bool,
    last_tick: u64,
}

#[derive(Default)]
struct Local {
    talking: bool,
    whispering: bool,
    muted: bool,
    deafened: bool,
    disabled: bool,
    hide_icons: bool,
    disconnected: bool,
}

#[derive(Default)]
struct State {
    local: Local,
    talkers: Vec<Talker>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

pub fn set_local(
    talking: bool,
    whispering: bool,
    muted: bool,
    deafened: bool,
    disabled: bool,
) {
    STATE.with_borrow_mut(|s| {
        s.local.talking = talking;
        s.local.whispering = whispering;
        s.local.muted = muted;
        s.local.deafened = deafened;
        s.local.disabled = disabled;
    });
}

pub fn set_disconnected(disconnected: bool) {
    STATE.with_borrow_mut(|s| s.local.disconnected = disconnected);
}

/// When true the HUD draws nothing (the `hide_icons` config).
pub fn set_hide_icons(hide: bool) {
    STATE.with_borrow_mut(|s| s.local.hide_icons = hide);
}

/// A voice packet from `sender` arrived at `tick` (`TalkCache.updateLevel`
/// through the sound packets): they are talking, a whisper or not, at
/// `position`, audible within `range`, in `dimension`. The mix already
/// filtered by those; the HUD only shows who is talking.
pub fn heard(
    sender: u128,
    name: &str,
    dimension: &str,
    position: [f64; 3],
    whisper: bool,
    range: f32,
    tick: u64,
) {
    let _ = (dimension, position, range);
    STATE.with_borrow_mut(|s| {
        let name = trunc(name);
        if let Some(t) = s.talkers.iter_mut().find(|t| t.sender == sender) {
            t.name = name;
            t.whisper = whisper;
            t.last_tick = tick;
        } else if s.talkers.len() < MAX_TALKERS {
            s.talkers.push(Talker { sender, name, whisper, last_tick: tick });
        } else if let Some(stalest) = s.talkers.iter_mut().min_by_key(|t| t.last_tick) {
            *stalest = Talker { sender, name, whisper, last_tick: tick };
        }
    });
}

/// Advance to `tick`: forget talkers whose last packet is older than the
/// fade. Saturating, so a backwards tick forgets no one.
pub fn tick(tick: u64) {
    STATE.with_borrow_mut(|s| {
        s.talkers
            .retain(|t| tick.saturating_sub(t.last_tick) <= FADE_TICKS);
    });
}

/// The most recently heard talker's uuid and name, if any: the settings
/// page's volume button reads it.
pub fn last_talker() -> Option<(u128, String)> {
    STATE.with_borrow(|s| {
        s.talkers
            .iter()
            .max_by_key(|t| t.last_tick)
            .map(|t| (t.sender, t.name.clone()))
    })
}

/// The local indicator, in `RenderEvents`' priority order: disconnected →
/// speaker off (disabled) → deafened → mic muted → whisper → talking.
/// Nothing when all clear.
fn local_icon(local: &Local) -> Option<Icon> {
    if local.disconnected {
        Some(Icon::Disconnected)
    } else if local.disabled {
        Some(Icon::Off)
    } else if local.deafened {
        Some(Icon::Deafened)
    } else if local.muted {
        Some(Icon::Muted)
    } else if local.whispering {
        Some(Icon::Whispering)
    } else if local.talking {
        Some(Icon::Talking)
    } else {
        None
    }
}

/// The draw list for this frame, for a screen of `gui_w` x `gui_h`: the
/// local mic state's icon first, then the talking players, as a top-left
/// column. A line that would run off the screen is dropped.
pub fn draw(gui_w: f32, gui_h: f32) -> Vec<Draw> {
    let _ = gui_w;
    STATE.with_borrow(|s| {
        let mut out = Vec::new();
        if s.local.hide_icons {
            return out;
        }
        let mut y = MARGIN;
        if let Some(icon) = local_icon(&s.local)
            && y + LINE_H <= gui_h
        {
            out.extend(icon.draw(MARGIN, y));
            y += STRIDE;
        }
        for t in &s.talkers {
            if y + LINE_H > gui_h {
                break;
            }
            let icon = if t.whisper { Icon::Whispering } else { Icon::Talking };
            out.extend(icon.draw(MARGIN, y + PAD));
            out.push(Draw::Text {
                x: ICON_X,
                y,
                line: t.name.clone(),
                argb: icon.color(),
            });
            y += STRIDE;
        }
        out
    })
}

/// A name at most `MAX_NAME` characters, with `…` when it was cut.
fn trunc(name: &str) -> String {
    if name.chars().count() < MAX_NAME {
        name.to_owned()
    } else {
        let cut: String = name.chars().take(MAX_NAME - 1).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn talker_line(draws: &[Draw], name: &str) -> bool {
        draws
            .iter()
            .any(|d| matches!(d, Draw::Text { line, .. } if line == name))
    }

    fn first_color(draws: &[Draw]) -> Option<u32> {
        draws.iter().find_map(|d| match d {
            Draw::Rect { argb, .. } => Some(*argb),
            _ => None,
        })
    }

    #[test]
    fn a_talker_appears_and_fades() {
        heard(1, "Alex", "minecraft:overworld", [0.0; 3], false, 100.0, 5);
        assert!(talker_line(&draw(1024.0, 1024.0), "Alex"));
        tick(5 + FADE_TICKS);
        assert!(talker_line(&draw(1024.0, 1024.0), "Alex"), "still fading");
        tick(5 + FADE_TICKS + 1);
        assert!(!talker_line(&draw(1024.0, 1024.0), "Alex"), "gone");
    }

    #[test]
    fn only_eight_talkers_with_the_stalest_evicted() {
        for i in 0..10u128 {
            heard(i, &format!("P{i}"), "d", [0.0; 3], false, 1.0, i as u64);
        }
        // 8 talkers, each an icon (4 rects) and a name.
        assert_eq!(draw(1024.0, 1024.0).len(), 8 * 5);
        assert!(!talker_line(&draw(1024.0, 1024.0), "P0"), "the stalest was evicted");
        assert!(talker_line(&draw(1024.0, 1024.0), "P9"));
        heard(8, "P8", "d", [0.0; 3], false, 1.0, 100);
        heard(10, "P10", "d", [0.0; 3], false, 1.0, 101);
        let draws = draw(1024.0, 1024.0);
        assert!(talker_line(&draws, "P8"), "refreshed, survived");
        assert!(!talker_line(&draws, "P1"), "the next stalest went");
    }

    #[test]
    fn whispers_show_in_their_own_colour() {
        heard(1, "Alex", "d", [0.0; 3], false, 1.0, 1);
        heard(2, "Sam", "d", [0.0; 3], true, 1.0, 2);
        let draws = draw(1024.0, 1024.0);
        let alex = draws
            .iter()
            .find(|d| matches!(d, Draw::Text { line, .. } if line == "Alex"))
            .unwrap();
        let sam = draws
            .iter()
            .find(|d| matches!(d, Draw::Text { line, .. } if line == "Sam"))
            .unwrap();
        match (alex, sam) {
            (Draw::Text { argb: a, .. }, Draw::Text { argb: s, .. }) => {
                assert_eq!(*a, COLOR_TALKING);
                assert_eq!(*s, COLOR_WHISPER);
                assert_ne!(*a, *s);
            }
            _ => panic!("both are texts"),
        }
    }

    #[test]
    fn the_local_icon_follows_the_render_priority() {
        // RenderEvents' order: disconnected → disabled → deafened → muted
        // → whispering → talking.
        set_local(true, false, false, false, false);
        set_disconnected(false);
        assert_eq!(first_color(&draw(1024.0, 1024.0)), Some(Icon::Talking.color()));
        set_local(true, true, false, false, false);
        assert_eq!(first_color(&draw(1024.0, 1024.0)), Some(Icon::Whispering.color()));
        set_local(true, false, true, false, false);
        assert_eq!(first_color(&draw(1024.0, 1024.0)), Some(Icon::Muted.color()));
        set_local(true, false, true, true, false);
        assert_eq!(first_color(&draw(1024.0, 1024.0)), Some(Icon::Deafened.color()));
        set_local(false, false, false, false, true);
        assert_eq!(first_color(&draw(1024.0, 1024.0)), Some(Icon::Off.color()));
        set_local(false, false, false, false, false);
        set_disconnected(true);
        assert_eq!(
            first_color(&draw(1024.0, 1024.0)),
            Some(Icon::Disconnected.color()),
            "disconnected wins over everything"
        );
        set_disconnected(false);
        assert_eq!(first_color(&draw(1024.0, 1024.0)), None, "all clear");
    }

    #[test]
    fn every_icon_fits_its_cell_and_keeps_its_colour() {
        for (icon, extra) in [
            (Icon::Talking, true),
            (Icon::Whispering, true),
            (Icon::Muted, true),
            (Icon::Deafened, true),
            (Icon::Off, false),
            (Icon::Disconnected, false),
        ] {
            let draws = icon.draw(0.0, 0.0);
            assert!(draws.len() >= 2, "a speaker core at least");
            for d in &draws {
                match d {
                    Draw::Rect { x, y, w, h, argb } => {
                        assert!(*x >= 0.0 && *y >= 0.0, "inside the cell");
                        assert!(*x + *w <= 7.0 && *y + *h <= 7.0, "inside the 7px cell");
                        assert_eq!(*argb, icon.color());
                    }
                    Draw::Text { .. } => panic!("icons are rects only"),
                }
            }
            assert_eq!(extra, draws.len() > 2, "the state's marks beyond the core");
        }
        assert!(Icon::Whispering.draw(0.0, 0.0).len() < Icon::Talking.draw(0.0, 0.0).len());
    }

    #[test]
    fn hidden_icons_draw_nothing() {
        heard(1, "Alex", "d", [0.0; 3], false, 1.0, 1);
        set_local(true, false, false, false, false);
        set_hide_icons(true);
        assert!(draw(1024.0, 1024.0).is_empty());
        set_hide_icons(false);
        assert!(!draw(1024.0, 1024.0).is_empty());
    }

    #[test]
    fn a_quiet_screen_draws_nothing() {
        assert!(draw(1024.0, 1024.0).is_empty(), "nothing at all");
        set_local(true, false, false, false, false);
        for i in 0..12u128 {
            heard(i, &format!("P{i}"), "d", [0.0; 3], false, 1.0, 1);
        }
        assert!(draw(1024.0, 8.0).is_empty());
        // 9 lines of 10 pixels from y=4: the icon and eight talkers end at
        // 93.
        let draws = draw(1024.0, 94.0);
        assert_eq!(draws.len(), 4 + 8 * 5, "the icon and eight talkers");
        assert_eq!(draw(1024.0, 93.0).len(), 4 + 8 * 5, "the last line just fits");
        assert_eq!(draw(1024.0, 92.0).len(), 4 + 7 * 5, "the last talker does not fit");
    }

    #[test]
    fn long_names_truncate() {
        heard(1, "Averyveryverylongplayername", "d", [0.0; 3], false, 1.0, 1);
        let draws = draw(1024.0, 1024.0);
        let name = draws
            .iter()
            .find_map(|d| match d {
                Draw::Text { line, x, .. } if *x == ICON_X => Some(line.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(name.chars().count(), MAX_NAME);
        assert!(name.ends_with('…'));
    }

    #[test]
    fn the_last_talker_is_the_most_recent() {
        assert_eq!(last_talker(), None);
        heard(1, "Alex", "d", [0.0; 3], false, 1.0, 10);
        heard(2, "Sam", "d", [0.0; 3], false, 1.0, 20);
        assert_eq!(last_talker(), Some((2, "Sam".to_owned())));
        heard(1, "Alex", "d", [0.0; 3], false, 1.0, 30);
        assert_eq!(last_talker(), Some((1, "Alex".to_owned())));
        tick(30 + FADE_TICKS + 1);
        assert_eq!(last_talker(), None);
    }
}
