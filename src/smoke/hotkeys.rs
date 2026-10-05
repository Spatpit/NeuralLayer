use super::*;
use crate::hotkeys::{Action, Hotkey, Hotkeys};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

impl Scenario {
    /// Record a new shortcut through the menu, check that the new key works
    /// globally and the old one is released, then check the command-bar
    /// preference for the show/hide shortcut.
    pub(super) fn tick_hotkeys(&mut self, app: &mut App, ctx: &egui::Context) {
        if self.since.elapsed() < Duration::from_millis(350) {
            return;
        }
        unsafe {
            match self.step {
                1 => {
                    if app.graphics.status().has_frame == 0 {
                        return;
                    }
                    assert_eq!(app.hotkeys, Hotkeys::default());
                    app.tab = crate::Page::Settings;
                    // The focused fixture must permit us to take the foreground.
                    if let Some(target) = &self.target {
                        SendMessageW(
                            target.hwnd as HWND,
                            WM_APP + 44,
                            GetCurrentProcessId() as usize,
                            0,
                        );
                    }
                    app.show_menu();
                    assert_ne!(SetForegroundWindow(app.hwnd as HWND), 0);
                    // Same as clicking the Comparison controls shortcut button.
                    app.rebinding = Some(Action::Compare);
                    app.runtime.set_hotkeys(None);
                    self.advance();
                }
                2 => {
                    // F9 is suspended while recording, so the menu receives it
                    // as an ordinary key; a plain letter must be refused.
                    test_key(b'K' as u16);
                    self.advance();
                }
                3 => {
                    assert_eq!(app.rebinding, Some(Action::Compare));
                    assert!(
                        app.hotkey_message
                            .as_deref()
                            .is_some_and(|m| m.contains("Add Ctrl")),
                        "A plain letter must be refused: {:?}",
                        app.hotkey_message
                    );
                    test_key(VK_F7);
                    self.advance();
                }
                4 => {
                    assert_eq!(app.rebinding, None, "F7 must complete recording");
                    assert_eq!(app.hotkeys.compare, Hotkey::key(VK_F7 as u32));
                    assert!(app.runtime.status().compare_hotkey);
                    app.hide_controls();
                    assert!(!app.compare_visible);
                    test_key(VK_F7);
                    self.advance();
                }
                5 => {
                    assert!(app.compare_visible, "The new shortcut must work globally");
                    test_key(VK_F9);
                    self.advance();
                }
                6 => {
                    assert!(
                        app.compare_visible,
                        "The old shortcut must be released after rebinding"
                    );
                    test_key(VK_F7);
                    self.advance();
                }
                7 => {
                    assert!(!app.compare_visible);
                    // Hiding the command bar makes F8 bring the bar back.
                    app.show_bar();
                    app.hide_controls();
                    assert!(app.prefer_bar);
                    test_key(VK_F8);
                    self.advance();
                }
                8 => {
                    assert!(
                        app.bar_visible && !app.menu_visible,
                        "F8 must bring back the command bar"
                    );
                    test_key(VK_F8);
                    self.advance();
                }
                9 => {
                    assert!(
                        !app.bar_visible && !app.menu_visible,
                        "F8 must hide the command bar"
                    );
                    // Switching to the full menu makes F8 follow the menu.
                    app.show_bar();
                    app.show_menu();
                    test_key(VK_F8);
                    self.advance();
                }
                10 => {
                    assert!(
                        !app.bar_visible && !app.menu_visible && !app.prefer_bar,
                        "F8 must hide the full menu and remember it"
                    );
                    test_key(VK_F8);
                    self.advance();
                }
                11 => {
                    assert!(
                        app.menu_visible && !app.bar_visible,
                        "F8 must bring back the full menu"
                    );
                    app.hotkeys = Hotkeys::default();
                    app.finish_rebinding();
                    self.advance();
                }
                12 => {
                    assert!(app.runtime.status().compare_hotkey);
                    std::fs::write(
                        "artifacts/hotkeys-check.txt",
                        "PASS: recording suspends global shortcuts; plain letters are refused; F7 recorded for comparison and works globally; F9 released; F8 hides and restores whichever controls were last in use (command bar or full menu); defaults restored.\n",
                    )
                    .unwrap();
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    self.step = 99;
                }
                _ => unreachable!(),
            }
        }
    }
}
