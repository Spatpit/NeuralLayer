use super::*;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

// Exercise input-stream movement and the immediately following click in one
// SendInput batch, in addition to the suite's cursor-warp based checks.
pub(super) unsafe fn routed_mouse(hwnd: HWND, position: egui::Pos2, flags: u32) {
    let mut point = POINT {
        x: position.x as i32,
        y: position.y as i32,
    };
    ClientToScreen(hwnd, &mut point);
    let mut inputs: [INPUT; 2] = std::mem::zeroed();
    inputs[0].r#type = INPUT_MOUSE;
    inputs[0].Anonymous.mi.dx = ((point.x - GetSystemMetrics(SM_XVIRTUALSCREEN)) as i64 * 65535
        / (GetSystemMetrics(SM_CXVIRTUALSCREEN) - 1) as i64) as i32;
    inputs[0].Anonymous.mi.dy = ((point.y - GetSystemMetrics(SM_YVIRTUALSCREEN)) as i64 * 65535
        / (GetSystemMetrics(SM_CYVIRTUALSCREEN) - 1) as i64) as i32;
    inputs[0].Anonymous.mi.dwFlags =
        MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK;
    inputs[1].r#type = INPUT_MOUSE;
    inputs[1].Anonymous.mi.dwFlags = flags;
    let count = if flags == 0 { 1 } else { 2 };
    assert_eq!(
        SendInput(count, inputs.as_ptr(), std::mem::size_of::<INPUT>() as i32),
        count
    );
}

impl Scenario {
    pub(super) fn hover_controls(&self, app: &App, ctx: &egui::Context) -> bool {
        let point = if app.menu_visible {
            app.rects.menu_header.center()
        } else {
            app.rects.compare_slider.center()
        } * ctx.pixels_per_point();
        unsafe {
            if native::cursor_client(app.hwnd) != Some((point.x as i32, point.y as i32)) {
                if let Some(target) = &self.target {
                    SendMessageW(
                        target.hwnd as HWND,
                        WM_APP + 44,
                        GetCurrentProcessId() as usize,
                        0,
                    );
                }
                SetForegroundWindow(app.hwnd as HWND);
                test_mouse(app.hwnd as HWND, point, 0);
                let mut physical = POINT { x: 0, y: 0 };
                GetPhysicalCursorPos(&mut physical);
                ScreenToClient(app.hwnd as HWND, &mut physical);
                let mut clip: RECT = std::mem::zeroed();
                GetClipCursor(&mut clip);
                assert!(
                    self.since.elapsed() < Duration::from_secs(3),
                    "Hover failed step {}: requested={point:?} actual={:?} physical={},{} clip={},{},{},{} foreground={:?}",
                    self.step, native::cursor_client(app.hwnd), physical.x,physical.y,clip.left,clip.top,clip.right,clip.bottom,GetForegroundWindow()
                );
                return false;
            }
            let interactive = GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32
                & (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE)
                == 0;
            assert!(
                interactive || self.since.elapsed() < Duration::from_secs(3),
                "Visible controls must accept the pointer at step {}",
                self.step
            );
            interactive
        }
    }

    pub(super) fn tick_input_routing(&mut self, app: &mut App, ctx: &egui::Context) {
        if (16..=19).contains(&self.step) {
            egui::Area::new(egui::Id::new("routing_popup_fixture"))
                .order(egui::Order::Foreground)
                .fixed_pos(egui::pos2(ctx.screen_rect().right() - 200., 25.))
                .show(ctx, |ui| {
                    self.routing_combo = egui::ComboBox::from_id_salt("routing_combo")
                        .selected_text("Popup routing test")
                        .show_ui(ui, |ui| {
                            self.routing_option = ui
                                .selectable_value(&mut self.routing_choice, 1, "Select this option")
                                .rect;
                        })
                        .response
                        .rect;
                });
        }
        if self.since.elapsed() < Duration::from_millis(220) {
            return;
        }
        let hwnd = app.hwnd as HWND;
        let source = self.target.as_ref().unwrap().hwnd as HWND;
        let scale = ctx.pixels_per_point();
        let outside = (ctx.screen_rect().max - egui::vec2(30., 30.)) * scale;
        let header = egui::pos2(
            app.rects.menu_header.right() - 20.,
            app.rects.menu_header.center().y,
        ) * scale;
        unsafe {
            let count = |name: &str| GetPropW(source, native::wide(name).as_ptr()) as usize;
            match self.step {
                1 => {
                    if app.graphics.status().has_frame == 0 {
                        return;
                    }
                    GetCursorPos(&mut self.cursor_before);
                    app.menu_size = egui::vec2(660., 560.);
                    SetWindowPos(
                        hwnd,
                        HWND_TOPMOST,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                    assert_ne!(
                        SendMessageW(source, WM_APP + 43, hwnd as usize, 0),
                        0,
                        "Place fixture directly under test overlay"
                    );
                    app.menu_position = Some(egui::pos2(18., 18.));
                    self.advance();
                }
                2 => {
                    SetForegroundWindow(hwnd);
                    routed_mouse(hwnd, header, 0);
                    // Hidden controls restore cross-process input.
                    app.menu_visible = false;
                    self.advance();
                }
                3 => {
                    assert_ne!(
                        GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Hidden menu must pass through"
                    );
                    // No frame wait between crossing the edge and clicking.
                    routed_mouse(hwnd, outside, MOUSEEVENTF_LEFTDOWN);
                    routed_mouse(hwnd, outside, MOUSEEVENTF_LEFTUP);
                    self.advance();
                }
                4 => {
                    assert_eq!(
                        count("RoutingDown"),
                        1,
                        "First outside click must reach the other process"
                    );
                    assert_eq!(count("RoutingUp"), 1);
                    assert_eq!(
                        GetForegroundWindow(),
                        source,
                        "Outside click must focus the source"
                    );
                    test_key(b'K' as u16);
                    let mut wheel: INPUT = std::mem::zeroed();
                    wheel.r#type = INPUT_MOUSE;
                    wheel.Anonymous.mi.dwFlags = MOUSEEVENTF_WHEEL;
                    wheel.Anonymous.mi.mouseData = 120;
                    assert_eq!(SendInput(1, &wheel, std::mem::size_of::<INPUT>() as i32), 1);
                    self.advance();
                }
                5 => {
                    assert_eq!(
                        count("RoutingKey"),
                        1,
                        "Keyboard focus stays with the clicked source"
                    );
                    assert_eq!(
                        count("RoutingWheel"),
                        1,
                        "Wheel must pass through outside controls"
                    );
                    self.menu_before = app.menu_position.unwrap();
                    app.show_menu();
                    self.advance();
                    self.step = 50;
                }
                50 => {
                    routed_mouse(hwnd, header, MOUSEEVENTF_LEFTDOWN);
                    self.advance();
                    self.step = 6;
                }
                6 => {
                    assert_eq!(
                        GetForegroundWindow(),
                        hwnd,
                        "Clicking controls must activate them"
                    );
                    assert!(
                        ctx.input(|i| i.pointer.primary_down()),
                        "Menu press must be delivered to egui: {:?}",
                        ctx.input(|i| i.pointer.interact_pos())
                    );
                    assert_eq!(
                        count("RoutingDown"),
                        1,
                        "First menu click must not leak to the source"
                    );
                    routed_mouse(hwnd, outside, 0);
                    self.advance();
                }
                7 => {
                    assert_eq!(
                        GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Keep a menu drag captured across the edge"
                    );
                    routed_mouse(hwnd, outside, MOUSEEVENTF_LEFTUP);
                    self.advance();
                }
                8 => {
                    assert_ne!(
                        app.menu_position.unwrap(),
                        self.menu_before,
                        "Menu header must remain draggable"
                    );
                    assert_eq!(
                        count("RoutingUp"),
                        1,
                        "Menu drag release must not leak to source"
                    );
                    app.menu_position = Some(egui::pos2(18., 18.));
                    app.toggle_compare();
                    self.advance();
                }
                9 => {
                    routed_mouse(hwnd, outside, MOUSEEVENTF_LEFTDOWN);
                    self.advance();
                }
                10 => {
                    routed_mouse(hwnd, app.rects.compare_slider.center() * scale, 0);
                    self.advance();
                }
                11 => {
                    assert_ne!(
                        GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "A source drag must not be stolen by the mini panel"
                    );
                    routed_mouse(
                        hwnd,
                        app.rects.compare_slider.center() * scale,
                        MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                12 => {
                    assert_eq!(count("RoutingDown"), 2);
                    assert_eq!(
                        count("RoutingUp"),
                        2,
                        "Source drag release must stay with source"
                    );
                    self.advance();
                }
                13 => {
                    assert!(app.compare_visible && !app.menu_visible);
                    routed_mouse(hwnd, outside, MOUSEEVENTF_LEFTDOWN);
                    routed_mouse(hwnd, outside, MOUSEEVENTF_LEFTUP);
                    self.advance();
                }
                14 => {
                    assert_eq!(
                        count("RoutingDown"),
                        3,
                        "Mini panel background must pass through"
                    );
                    routed_mouse(hwnd, app.rects.compare_slider.center() * scale, 0);
                    self.advance();
                }
                15 => {
                    assert_eq!(
                        GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Mini panel must accept input"
                    );
                    self.advance();
                }
                16 => {
                    self.advance();
                }
                17 => {
                    routed_mouse(
                        hwnd,
                        self.routing_combo.center() * scale,
                        MOUSEEVENTF_LEFTDOWN,
                    );
                    routed_mouse(
                        hwnd,
                        self.routing_combo.center() * scale,
                        MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                18 => {
                    assert!(
                        self.routing_option.is_positive(),
                        "Dropdown must open outside the menu"
                    );
                    routed_mouse(
                        hwnd,
                        self.routing_option.center() * scale,
                        MOUSEEVENTF_LEFTDOWN,
                    );
                    routed_mouse(
                        hwnd,
                        self.routing_option.center() * scale,
                        MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                19 => {
                    assert_eq!(
                        self.routing_choice, 1,
                        "Popup option must receive its click"
                    );
                    assert_eq!(
                        count("RoutingDown"),
                        3,
                        "Controls/popups must not leak clicks"
                    );
                    assert_eq!(visible_surfaces(), 1);
                    std::fs::write("artifacts/input-routing-check.txt","PASS: hidden-menu cross-process first-click passthrough; source focus, keyboard and wheel; menu drag across edge; source drag across mini panel; F9 panel inside/outside; real egui dropdown outside mini panel; one rendering HWND.\n").unwrap();
                    app.menu_visible = true;
                    app.compare_visible = false;
                    self.advance();
                }
                20 => {
                    routed_mouse(hwnd, outside, 0);
                    self.advance();
                }
                21 => {
                    let flags = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                    assert_eq!(flags & (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW), 0,
                        "Open menu must keep the legacy non-layered window when the pointer leaves the menu");
                    assert_ne!(flags & WS_EX_APPWINDOW, 0);
                    test_key(VK_F8);
                    self.advance();
                }
                22 => {
                    assert!(!app.menu_visible);
                    let flags = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                    assert_eq!(
                        flags & (WS_EX_LAYERED | WS_EX_TRANSPARENT),
                        WS_EX_LAYERED | WS_EX_TRANSPARENT,
                        "Hiding the menu must restore cross-process click-through"
                    );
                    std::fs::write("artifacts/stream-picker-check.txt", "PASS: legacy application-window flags stay non-layered with the pointer outside the menu; F8 hides the menu and restores click-through. Discord picker not exercised.\n").unwrap();
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    self.step = 99;
                }
                _ => {}
            }
        }
    }
}
