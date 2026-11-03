use std::cell::RefCell;

use super::icons::Draw;
use super::packets::NativeCommand;

/// The menu key, as Options > Mods lists it: (name, label, default).
pub const MENU_KEY: (&str, &str, &str) = ("menu", "Open the voice menu", "key.keyboard.v");

/// The panel's layout, in GUI pixels.
pub const WIDTH: f32 = 130.0;
const LINE: f32 = 9.0;
const STRIDE: f32 = 10.0;
const PAD: f32 = 6.0;
/// The rows: the title, the group line, the choices, the footer hint.
const ROWS: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    /// A navigation key; nothing to apply.
    None,
    /// The menu closes (Escape or the menu key again).
    Close,
    ToggleMute,
    ToggleDeafen,
    ToggleHideIcons,
    ToggleVoiceChat,
    /// Step the voice distance, as the settings page's button does.
    CycleDistance,
    /// Step the whisper distance.
    CycleWhisperDistance,
    /// Step the output volume.
    CycleVolume,
    /// Ask the server to leave the group, by a control message.
    LeaveGroup,
}

pub struct States {
    pub muted: bool,
    pub deafened: bool,
    pub hide_icons: bool,
    pub disabled: bool,
    /// The voice distance, in blocks (`settings.distance`).
    pub distance: f32,
    /// The whisper distance, in blocks.
    pub whisper_distance: f32,
    /// The output volume, 0..2 (`settings.volume * 100` as the label).
    pub volume: f32,
}

/// The menu's state: open or closed, the selected row, the group the
/// server last told of. One per thread (the window's).
#[derive(Default)]
struct State {
    open: bool,
    selected: usize,
    group: Option<String>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// Open or close the menu (the menu key, the world closing).
pub fn set_open(open: bool) {
    STATE.with_borrow_mut(|s| s.open = open);
}

/// Whether the menu is open.
pub fn is_open() -> bool {
    STATE.with_borrow(|s| s.open)
}

/// The group the server last told of, as the menu's header shows.
pub fn set_group(name: Option<String>) {
    STATE.with_borrow_mut(|s| s.group = name);
}

/// The group's name, if the player is in one.
pub fn group() -> Option<String> {
    STATE.with_borrow(|s| s.group.clone())
}

/// The world closed: the menu and its selection go.
pub fn reset() {
    STATE.with_borrow_mut(|s| {
        s.open = false;
        s.selected = 0;
        s.group = None;
    });
}

/// The menu's panel size, for centering it on the screen.
pub fn size() -> (f32, f32) {
    let lines = 3 + ROWS + 1; // title, group, a gap, the rows, the footer
    (WIDTH, PAD * 2.0 + lines as f32 * STRIDE)
}

/// The menu's rows, as the header and the footer frame them: what each
/// row is called and what it shows.
pub fn rows(states: &States) -> Vec<(String, String)> {
    let on = |b: bool| if b { "ON" } else { "OFF" };
    vec![
        ("Mute microphone".into(), on(states.muted).into()),
        ("Deafen".into(), on(states.deafened).into()),
        ("Hide icons".into(), on(states.hide_icons).into()),
        ("Voice chat".into(), on(!states.disabled).into()),
        ("Voice distance".into(), format!("{:.0} blocks", states.distance)),
        ("Whisper distance".into(), format!("{:.0} blocks", states.whisper_distance)),
        ("Volume".into(), format!("{:.0}", states.volume * 100.0)),
        ("Leave group".into(), String::new()),
    ]
}

/// A key while the menu is open: the arrows move, the number keys choose
/// and use a row, Enter uses the selected row, Escape closes. Releases
/// change nothing.
pub fn key(key: &str, pressed: bool) -> MenuAction {
    if !pressed {
        return MenuAction::None;
    }
    let action_of = |row: usize| match row {
        0 => MenuAction::ToggleMute,
        1 => MenuAction::ToggleDeafen,
        2 => MenuAction::ToggleHideIcons,
        3 => MenuAction::ToggleVoiceChat,
        4 => MenuAction::CycleDistance,
        5 => MenuAction::CycleWhisperDistance,
        6 => MenuAction::CycleVolume,
        _ => MenuAction::LeaveGroup,
    };
    STATE.with_borrow_mut(|s| {
        if !s.open {
            return MenuAction::None;
        }
        match key {
            "key.keyboard.up" => {
                s.selected = s.selected.wrapping_sub(1) % ROWS;
                MenuAction::None
            }
            "key.keyboard.down" => {
                s.selected = (s.selected + 1) % ROWS;
                MenuAction::None
            }
            "key.keyboard.escape" => {
                s.open = false;
                MenuAction::Close
            }
            "key.keyboard.enter" => action_of(s.selected),
            k => {
                let Some(row) = k
                    .strip_prefix("key.keyboard.")
                    .and_then(|d| d.parse::<usize>().ok())
                    .filter(|&r| (1..=ROWS).contains(&r))
                else {
                    return MenuAction::None;
                };
                s.selected = row - 1;
                action_of(s.selected)
            }
        }
    })
}

/// The menu's draw list, at `origin` (its top-left, GUI pixels): a panel
/// darker than the indicators' tag, the rows with their values, the
/// selected row's bar and caret, and the footer's hint.
pub fn draw(origin: [f32; 2], states: &States) -> Vec<Draw> {
    let (x, y) = (origin[0], origin[1]);
    let (_, h) = size();
    let mut out = vec![Draw::Rect { x, y, w: WIDTH, h, argb: 0xB000_0000 }];
    let line = |out: &mut Vec<Draw>, x: f32, y: f32, text: &str, argb: u32| {
        out.push(Draw::Text { x, y, line: text.to_owned(), argb });
    };
    line(&mut out, x + PAD, y + PAD, "Speak.", 0xFFFF_FFFF);
    let group = match group() {
        Some(name) => format!("Group: {name}"),
        None => "No group".to_owned(),
    };
    line(&mut out, x + PAD, y + PAD + STRIDE, &group, 0xFF99_9999);
    let rows = rows(states);
    let selected = STATE.with_borrow(|s| s.selected);
    for (i, (label, value)) in rows.iter().enumerate() {
        let ry = y + PAD + (3 + i) as f32 * STRIDE;
        if i == selected {
            out.push(Draw::Rect { x, y: ry - 1.0, w: WIDTH, h: STRIDE, argb: 0x30FF_FFFF });
            out.push(Draw::Rect {
                x: x + 1.0,
                y: ry + 1.0,
                w: 2.0,
                h: LINE - 2.0,
                argb: 0xFFFF_FFFF,
            });
        }
        line(&mut out, x + PAD + 3.0, ry, label, 0xFFFF_FFFF);
        if !value.is_empty() {
            line(&mut out, x + WIDTH - PAD - 34.0, ry, value, 0xFF99_9999);
        }
    }
    let fy = y + h - PAD - LINE;
    line(&mut out, x + PAD, fy, "Enter use   Esc close", 0xFF99_9999);
    out
}

pub fn commands() -> Vec<NativeCommand> {
    vec![NativeCommand::new("speakmenu")]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn states() -> States {
        States {
            muted: false,
            deafened: false,
            hide_icons: false,
            disabled: false,
            distance: 48.0,
            whisper_distance: 24.0,
            volume: 1.0,
        }
    }

    #[test]
    fn the_menu_opens_moves_chooses_and_closes() {
        reset();
        set_open(true);
        assert!(is_open());
        // Move with wrap: up from 0 wraps to the last row.
        key("key.keyboard.up", true);
        assert_eq!(key("key.keyboard.enter", true), MenuAction::LeaveGroup);
        // Down from the last wraps to the first.
        key("key.keyboard.down", true);
        assert_eq!(key("key.keyboard.enter", true), MenuAction::ToggleMute);
        // Digits choose and use a row directly.
        assert_eq!(key("key.keyboard.8", true), MenuAction::LeaveGroup);
        // Releases change nothing.
        assert_eq!(key("key.keyboard.enter", false), MenuAction::None);
        // Out-of-range digits and other keys are not the menu's.
        assert_eq!(key("key.keyboard.9", true), MenuAction::None);
        assert_eq!(key("key.keyboard.w", true), MenuAction::None);
        // Escape closes.
        assert_eq!(key("key.keyboard.escape", true), MenuAction::Close);
        assert!(!is_open());
        // A closed menu answers nothing.
        assert_eq!(key("key.keyboard.1", true), MenuAction::None);
        reset();
    }

    #[test]
    fn every_row_has_its_action() {
        reset();
        set_open(true);
        for row in 1..=ROWS {
            let expected = match row {
                1 => MenuAction::ToggleMute,
                2 => MenuAction::ToggleDeafen,
                3 => MenuAction::ToggleHideIcons,
                4 => MenuAction::ToggleVoiceChat,
                5 => MenuAction::CycleDistance,
                6 => MenuAction::CycleWhisperDistance,
                7 => MenuAction::CycleVolume,
                _ => MenuAction::LeaveGroup,
            };
            assert_eq!(key(&format!("key.keyboard.{row}"), true), expected);
        }
        reset();
    }

    #[test]
    fn the_rows_show_the_states() {
        let shown = States {
            muted: true,
            deafened: false,
            hide_icons: false,
            disabled: true,
            distance: 250.0,
            whisper_distance: 20.0,
            volume: 1.5,
        };
        let rows = rows(&shown);
        assert_eq!(rows[0], ("Mute microphone".into(), "ON".into()));
        assert_eq!(rows[1], ("Deafen".into(), "OFF".into()));
        assert_eq!(rows[3], ("Voice chat".into(), "OFF".into()));
        assert_eq!(rows[4], ("Voice distance".into(), "250 blocks".into()));
        assert_eq!(rows[5], ("Whisper distance".into(), "20 blocks".into()));
        assert_eq!(rows[6], ("Volume".into(), "150".into()));
        assert_eq!(rows[7], ("Leave group".into(), "".into()));
    }

    #[test]
    fn the_group_line_follows_the_server() {
        reset();
        assert_eq!(group(), None);
        set_group(Some("team".into()));
        assert_eq!(group().as_deref(), Some("team"));
        set_group(None);
        assert_eq!(group(), None);
        reset();
        assert_eq!(group(), None, "the world closed");
    }

    #[test]
    fn the_panel_draws_within_itself() {
        reset();
        set_open(true);
        set_group(Some("team".into()));
        let draws = draw([100.0, 50.0], &states());
        let (w, h) = size();
        assert!(draws.len() > 2 + ROWS * 2, "a panel, a caret, rows");
        for d in &draws {
            match d {
                Draw::Rect { x, y, w: rw, h: rh, .. } => {
                    assert!(*x >= 100.0 && *y >= 50.0, "inside the panel's origin");
                    assert!(*x + *rw <= 100.0 + w + 0.5, "inside the width");
                    assert!(*y + *rh <= 50.0 + h + 0.5, "inside the height");
                }
                Draw::Text { x, y, .. } => {
                    assert!(*x >= 100.0 && *x < 100.0 + w);
                    assert!(*y >= 50.0 && *y + LINE <= 50.0 + h + 0.5);
                }
            }
        }
        let texts: Vec<&str> = draws
            .iter()
            .filter_map(|d| match d {
                Draw::Text { line, .. } => Some(line.as_str()),
                _ => None,
            })
            .collect();
        assert!(texts.contains(&"Speak."));
        assert!(texts.contains(&"Group: team"));
        assert!(texts.contains(&"Enter use   Esc close"));
        assert!(texts.contains(&"Leave group"));
        reset();
    }

    #[test]
    fn the_menu_registers_its_command() {
        assert_eq!(commands().len(), 1);
        assert_eq!(commands()[0].usage, "speakmenu");
    }
}
