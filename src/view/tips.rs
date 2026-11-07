use crate::view::*;

pub fn content_tip(pill: Rect, scale: f32, ui: &UiState, clips: usize, px: f32, py: f32) -> Option<(String, Rect)> {
    match ui.tab {
        Tab::Home => home_tip(pill, scale, ui, px, py),
        Tab::Yandex => yandex_tip(pill, scale, &ui.yandex, px, py),
        Tab::Note => {
            let tree = note_tree(pill, scale);
            let reminder = ui.notes.remind.is_some();
            let hit = hit_note_tree(pill, scale, px, py, ui).or_else(|| hit_note_panel(pill, scale, px, py, ui))?;
            let (text, rect) = match hit {
                NoteHit::AddNote => ("New note", tool_button(tree, scale, 0)),
                NoteHit::AddFolder => ("New folder", tool_button(tree, scale, 1)),
                NoteHit::Rename => ("Rename", tool_button(tree, scale, 2)),
                NoteHit::Delete => ("Delete (press twice)", tool_button(tree, scale, 3)),
                NoteHit::TypeToggle => (
                    if reminder { "Make it a plain note" } else { "Make it a reminder" },
                    type_button(pill, scale, reminder),
                ),
                NoteHit::RemindOpen => ("Pick date and time", remind_line(pill, scale)),
                _ => return None,
            };
            Some((text.to_string(), rect))
        }
        Tab::Clipboard => {
            let visible = clips.saturating_sub(ui.clip.scroll);
            match hit_clip(pill, scale, px, py, visible)? {
                ClipHit::Pin(i) => Some(("Pin or unpin".into(), clip_button(pill, scale, i, false))),
                ClipHit::Delete(i) => Some(("Delete".into(), clip_button(pill, scale, i, true))),
                ClipHit::Row(_) => None,
            }
        }
        Tab::Chat => {
            let (new, chats, attach, pet) = chat_buttons(pill, scale);
            let (text, rect) = match hit_chat(pill, scale, px, py, ui)? {
                ChatHit::Pet => (if ui.chat.pet.enabled { "Back to chat" } else { "Create or change your pet" }, pet),
                ChatHit::New => ("New chat", new),
                ChatHit::Chats => ("Saved chats", chats),
                ChatHit::Attach => ("Attach a file", attach),
                ChatHit::Send => ("Send", agy_send_button(pill, scale)),
                ChatHit::RowDelete(i) => ("Delete the chat", row_delete(agy_row(pill, scale, i), scale)),
                _ => return None,
            };
            Some((text.to_string(), rect))
        }
        Tab::Agy => {
            let hit =
                hit_agy(pill, scale, px, py, ui.agy.chats.len(), ui.agy.scroll, ui.agy.open, ui.agy.input_h.get())?;
            match hit {
                AgyHit::Back => Some(("Back to the list".into(), agy_back(pill, scale))),
                AgyHit::Send => Some(("Send".into(), agy_send_button(pill, scale))),
                _ => None,
            }
        }
        _ => None,
    }
}

fn clip_button(pill: Rect, scale: f32, i: usize, delete: bool) -> Rect {
    button_rect(row_rect(pill, scale, i), scale, delete)
}

fn home_tip(pill: Rect, scale: f32, ui: &UiState, px: f32, py: f32) -> Option<(String, Rect)> {
    let wide = home_wide(ui);
    let layout = dock_layout(pill, scale, ui);
    let (text, rect): (String, Rect) = match hit_home(pill, scale, px, py, ui)? {
        HomeHit::Media(i) => (["Previous", "Play or pause", "Next"][i].into(), media_btn(pill, scale, i, wide)),
        HomeHit::Extra(i) => (["Shuffle", "Repeat", "Like"][i].into(), extra_btn(pill, scale, i, wide)),
        HomeHit::Wave => ("Open My Wave settings".into(), wave_link(pill, scale, ui)),
        HomeHit::Notification | HomeHit::NotificationExpand => (
            if matches!(hit_home(pill, scale, px, py, ui), Some(HomeHit::Notification)) && ui.cfg.notification_open_app
            {
                "Open sender app"
            } else {
                "Expand notification"
            }
            .into(),
            home_notification_rect(pill, scale, ui)?,
        ),
        HomeHit::Player => ("Next player".into(), player_btn(pill, scale, wide)),
        HomeHit::Edit => {
            (if ui.home.edit { "Done" } else { "Choose the tiles" }.into(), edit_button(pill, scale, wide))
        }
        HomeHit::VolumeIcon => {
            let row = volume_row(pill, scale, wide);
            (
                if ui.home.vol_track {
                    "Player volume: left-click for system, right-click for mixer"
                } else {
                    "System volume: left-click for player, right-click for mixer"
                }
                .into(),
                (row.0 + 2.0 * scale, row.1, row.0 + 42.0 * scale, row.3),
            )
        }
        HomeHit::Device => ("Sound output".into(), device_chip(pill, scale, wide)),
        HomeHit::Mixer(_) => return None,
        HomeHit::DiscordMute => ("Mute".into(), strip_toggle(pill, scale, 1)),
        HomeHit::DiscordDeaf => ("Deafen".into(), strip_toggle(pill, scale, 0)),
        HomeHit::DiscordOverlay => ("Voice overlay".into(), strip_toggle(pill, scale, 2)),
        HomeHit::DockAdd => ("Add to the dock".into(), layout.button),
        HomeHit::DockMore => (
            if layout.open { "Fold the dock" } else { "Show the whole dock" }.into(),
            layout.close.unwrap_or(layout.button),
        ),
        HomeHit::DockUnpin(i) => ("Unpin".into(), dock_cross(*layout.slots.get(i)?, scale)),
        HomeHit::Dock(i) => (dock_name(&ui.home.dock.get(i)?.path), *layout.slots.get(i)?),
        _ => return None,
    };
    Some((text, rect))
}
