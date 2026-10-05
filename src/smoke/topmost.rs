use super::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

unsafe fn above(window: HWND, other: HWND) -> bool {
    let mut next = GetWindow(other, GW_HWNDPREV);
    for _ in 0..256 {
        if next.is_null() {
            return false;
        }
        if next == window {
            return true;
        }
        next = GetWindow(next, GW_HWNDPREV);
    }
    false
}
impl Scenario {
    pub(super) fn tick_topmost(&mut self, app: &mut App, ctx: &egui::Context) {
        if self.since.elapsed() < Duration::from_millis(350) {
            return;
        }
        unsafe {
            let overlay = app.hwnd as HWND;
            let source = self.target.as_ref().unwrap().hwnd as HWND;
            match self.step {
                1 => {
                    if app.graphics.status().has_frame == 0 {
                        return;
                    }
                    GetCursorPos(&mut self.cursor_before);
                    app.set_always_on_top(true);
                    // Isolate the source from unrelated desktop windows before
                    // removing the fixture's ordering constraint in step 2.
                    assert_ne!(SendMessageW(source, WM_APP + 43, app.hwnd as usize, 0), 0);
                    app.menu_visible = false;
                    // Keep the native topmost flag, but defer recovery until
                    // the fixture has reproduced the activation ordering.
                    app.always_on_top = false;
                    self.advance();
                }
                2 => {
                    assert_ne!(SendMessageW(source, WM_APP + 45, 0, 0), 0);
                    input_routing::routed_mouse(
                        source,
                        egui::pos2(100., 100.),
                        MOUSEEVENTF_LEFTDOWN,
                    );
                    input_routing::routed_mouse(source, egui::pos2(100., 100.), MOUSEEVENTF_LEFTUP);
                    self.advance();
                }
                3 => {
                    assert_eq!(GetForegroundWindow(), source);
                    assert!(
                        above(source, overlay),
                        "Reproduce topmost game activation covering a topmost overlay"
                    );
                    assert_ne!(
                        GetWindowLongPtrW(overlay, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                        0
                    );
                    let hud = SendMessageW(source, WM_APP + 46, 0, 0) as HWND;
                    assert!(!hud.is_null());
                    assert!(above(hud, source), "Metrics HUD starts above the game");
                    app.always_on_top = true;
                    self.advance();
                }
                4 => {
                    assert_eq!(
                        GetForegroundWindow(),
                        source,
                        "The game must retain keyboard focus"
                    );
                    assert!(
                        above(overlay, source),
                        "Overlay must recover above the focused topmost game"
                    );
                    let hud = GetPropW(source, native::wide("RoutingHud").as_ptr()) as HWND;
                    assert!(
                        above(hud, overlay),
                        "Recovery must leave the metrics HUD above our output"
                    );
                    assert_ne!(
                        GetWindowLongPtrW(overlay, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0
                    );
                    assert_eq!(visible_surfaces(), 1);
                    app.set_always_on_top(false);
                    SendMessageW(source, WM_APP + 45, 0, 0);
                    self.advance();
                }
                5 => {
                    assert_eq!(
                        GetWindowLongPtrW(overlay, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                        0,
                        "Respect Always on top being disabled"
                    );
                    assert!(above(source, overlay));
                    app.set_always_on_top(true);
                    app.paused = true;
                    SendMessageW(source, WM_APP + 45, 0, 0);
                    self.advance();
                }
                6 => {
                    assert!(
                        above(source, overlay),
                        "Paused overlay must not compete for stacking"
                    );
                    SendMessageW(source, WM_APP + 46, 0, 0);
                    app.paused = false;
                    self.advance();
                }
                7 => {
                    assert!(above(overlay, source));
                    let hud = GetPropW(source, native::wide("RoutingHud").as_ptr()) as HWND;
                    assert!(
                        above(hud, overlay),
                        "Resume must also preserve the metrics HUD"
                    );
                    assert_eq!(GetForegroundWindow(), source);
                    // A topmost source should not trigger recovery while another app has focus.
                    assert_ne!(
                        SendMessageW(source, WM_APP + 44, GetCurrentProcessId() as usize, 0),
                        0
                    );
                    app.menu_visible = true;
                    native::taskbar_menu_recovery(app.hwnd, 0, false);
                    assert_ne!(SetForegroundWindow(overlay), 0);
                    SendMessageW(source, WM_APP + 45, 0, 0);
                    self.advance();
                }
                8 => {
                    assert_eq!(GetForegroundWindow(), overlay);
                    assert!(
                        above(source, overlay),
                        "Do not reorder for an unfocused source"
                    );
                    std::fs::write("artifacts/topmost-check.txt", "PASS: a separate topmost source overtakes the overlay; automatic recovery preserves source focus, click-through and one rendering window, while leaving a separate external metrics HUD above our output; Always on top off, pause, and an unfocused source suppress recovery; resume restores stacking.\n").unwrap();
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    self.step = 99;
                }
                _ => unreachable!(),
            }
        }
    }
}
