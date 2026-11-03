fn speak_hud_hook(
    game: &mut crate::game::Game<'_>,
    batch: &mut horizon_client::hud::HudBatch,
    corners: &mut horizon_client::extras_hud::Corners,
) {
    use crate::game::speak_client as client;
    use crate::game::speak_frame as frame;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_icons as icons;
    use crate::game::speak_menu as menu;
    use horizon_client::extras::Corner;
    if !game.native_on(hooks::ID) || !game.native_playing() {
        return;
    }
    let get = |key: &str| game.native_setting(hooks::ID, key);
    // The local states, each frame; the settings say whether icons show.
    let (muted, deafened, disabled, hidden) = (
        client::muted(&get),
        client::deafened(&get),
        client::disabled(&get),
        get("hide_icons").as_deref() == Some("true"),
    );
    icons::set_local(
        frame::talking(),
        client::whispering(),
        muted,
        deafened,
        disabled,
    );
    icons::set_hide_icons(hidden);
    let draws = if menu::is_open() {
        // The menu, centered as a screen is. The corner boxes know the
        // screen's size: a zero box in the top-right corner lands at
        // width - 2, in the bottom-left at height - 2 (the margin the
        // boxes keep from the edge).
        let states = menu::States {
            muted,
            deafened,
            hide_icons: hidden,
            disabled,
            distance: get("distance")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(48.0),
            whisper_distance: get("whisper_distance")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(24.0),
            volume: get("volume")
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(1.0),
        };
        let (w, h) = menu::size();
        let (sw, _) = corners.place(Corner::TopRight, 0.0, 0.0);
        let (_, sh) = corners.place(Corner::BottomLeft, 0.0, 0.0);
        let origin = [(sw + 2.0 - w) / 2.0, (sh + 2.0 - h) / 2.0];
        menu::draw(origin, &states)
    } else {
        icons::draw(1024.0, 1024.0)
    };
    if draws.is_empty() {
        return;
    }
    // The box the draw list needs, placed below whatever the corner holds.
    let mut bw = 0.0f32;
    let mut bh = 0.0f32;
    for draw in &draws {
        match draw {
            icons::Draw::Rect { x, y, w, h, .. } => {
                bw = bw.max(x + w);
                bh = bh.max(y + h);
            }
            icons::Draw::Text { x, y, line, .. } => {
                bw = bw.max(x + game.native_text_width(line));
                bh = bh.max(y + 9.0);
            }
        }
    }
    let (bx, by) = if menu::is_open() {
        (0.0, 0.0)
    } else {
        corners.place(Corner::TopLeft, bw, bh)
    };
    if !menu::is_open() {
        // The indicators' translucent tag behind their glyphs.
        game.native_fill(batch, [bx, by, bw, bh], 0x7800_0000);
    }
    for draw in draws {
        match draw {
            icons::Draw::Rect { x, y, w, h, argb } => {
                game.native_fill(batch, [bx + x, by + y, w, h], argb);
            }
            icons::Draw::Text { x, y, line, argb } => {
                game.native_text(batch, &line, [bx + x, by + y], argb & 0xFF_FFFF, true);
            }
        }
    }
}
