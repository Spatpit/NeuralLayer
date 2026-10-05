//! Functional checks against the actual GPU output; no benchmarking.
mod hotkeys;
mod input_routing;
mod looks;
mod topmost;
use crate::{
    native::{self, Area, Bounds, Target},
    App,
};

use std::{
    os::windows::process::CommandExt,
    process::{Child, Command},
    ptr::null_mut,
    thread,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::Gdi::*,
    System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentProcessId},
    UI::WindowsAndMessaging::*,
};

unsafe extern "system" fn fixture_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if msg == WM_APP + 46 {
        let key = native::wide("RoutingHud");
        let mut hud = GetPropW(hwnd, key.as_ptr()) as HWND;
        if hud.is_null() {
            hud = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
                native::wide("SpatpitOverlay.ExternalTestWindow").as_ptr(),
                native::wide("External metrics fixture").as_ptr(),
                WS_POPUP,
                820,
                150,
                100,
                50,
                null_mut(),
                null_mut(),
                GetModuleHandleW(null_mut()),
                null_mut(),
            );
            SetPropW(hwnd, key.as_ptr(), hud);
        }
        SetWindowPos(
            hud,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );
        return hud as LRESULT;
    }
    if msg == WM_DESTROY {
        let hud = RemovePropW(hwnd, native::wide("RoutingHud").as_ptr()) as HWND;
        if !hud.is_null() {
            DestroyWindow(hud);
        }
    }
    if msg == WM_APP + 45 {
        RemovePropW(hwnd, native::wide("RoutingOverlay").as_ptr());
        return SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        ) as LRESULT;
    }
    if msg == WM_APP + 44 {
        return AllowSetForegroundWindow(w as u32) as LRESULT;
    }
    if msg == WM_APP + 43 {
        SetPropW(hwnd, native::wide("RoutingOverlay").as_ptr(), w as HANDLE);
        return SetWindowPos(
            hwnd,
            w as HWND,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        ) as LRESULT;
    }
    if msg == WM_WINDOWPOSCHANGING {
        let overlay = GetPropW(hwnd, native::wide("RoutingOverlay").as_ptr()) as HWND;
        if !overlay.is_null() && IsWindow(overlay) != 0 {
            let position = &mut *(l as *mut WINDOWPOS);
            position.hwndInsertAfter = overlay;
            position.flags &= !SWP_NOZORDER;
        }
    }
    let counter = match msg {
        WM_LBUTTONDOWN => Some("RoutingDown"),
        WM_LBUTTONUP => Some("RoutingUp"),
        WM_MOUSEWHEEL => Some("RoutingWheel"),
        WM_KEYDOWN if w == b'K' as usize => Some("RoutingKey"),
        _ => None,
    };
    if let Some(counter) = counter {
        let name = native::wide(counter);
        let value = GetPropW(hwnd, name.as_ptr()) as usize + 1;
        SetPropW(hwnd, name.as_ptr(), value as HANDLE);
        if msg == WM_LBUTTONDOWN {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::SetCapture(hwnd);
        }
        if msg == WM_LBUTTONUP {
            windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
        }
    }
    if msg == WM_PAINT {
        let mut paint: PAINTSTRUCT = std::mem::zeroed();
        let dc = BeginPaint(hwnd, &mut paint);
        let mut rect: RECT = std::mem::zeroed();
        GetClientRect(hwnd, &mut rect);
        for (n, color) in [0x00C88C50, 0x005AB43C, 0x008C5AB4].into_iter().enumerate() {
            let brush = CreateSolidBrush(color);
            let stripe = RECT {
                left: rect.right * n as i32 / 3,
                top: 0,
                right: rect.right * (n as i32 + 1) / 3,
                bottom: rect.bottom,
            };
            FillRect(dc, &stripe, brush);
            DeleteObject(brush);
        }
        if GetWindowLongPtrW(hwnd, GWLP_USERDATA) != 0 {
            let brush = CreateSolidBrush(0x00FFFFFF);
            let patch = RECT {
                left: rect.right / 3,
                top: rect.bottom / 8,
                right: rect.right * 2 / 3,
                bottom: rect.bottom * 3 / 8,
            };
            FillRect(dc, &patch, brush);
            DeleteObject(brush);
        }
        EndPaint(hwnd, &paint);
        return 0;
    }
    if msg == WM_TIMER {
        InvalidateRect(hwnd, null_mut(), 0);
        return 0;
    }
    if msg == WM_APP + 42 {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, w as isize);
        InvalidateRect(hwnd, null_mut(), 0);
        return 0;
    }
    DefWindowProcW(hwnd, msg, w, l)
}

pub fn fixture() {
    unsafe {
        let instance = GetModuleHandleW(null_mut());
        let class = native::wide("SpatpitOverlay.ExternalTestWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(fixture_proc),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            native::wide("Overlay test — content window").as_ptr(),
            WS_OVERLAPPEDWINDOW,
            100,
            100,
            960,
            740,
            null_mut(),
            null_mut(),
            instance,
            null_mut(),
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetTimer(hwnd, 1, 100, None);
        let start = Instant::now();
        while IsWindow(hwnd) != 0 && start.elapsed() < Duration::from_secs(120) {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            thread::sleep(Duration::from_millis(10));
        }
        DestroyWindow(hwnd);
        UnregisterClassW(class.as_ptr(), instance);
    }
}

unsafe fn test_mouse(hwnd: HWND, position: egui::Pos2, flags: u32) {
    input_routing::routed_mouse(hwnd, position, flags);
}

pub fn capture_probe() {
    extern "C" {
        fn spatpit_stream_probe(hwnd: *mut std::ffi::c_void, pixels: *mut u8) -> i32;
    }
    let args: Vec<_> = std::env::args().collect();
    let hwnd: isize = args[2].parse().unwrap();
    let mut pixels = [0u8; 8];
    let result = unsafe { spatpit_stream_probe(hwnd as _, pixels.as_mut_ptr()) };
    if result == 0 {
        std::process::exit(1);
    }
    std::fs::write(&args[3], pixels).unwrap();
}

unsafe fn test_key(key: u16) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    let mut input: [INPUT; 2] = std::mem::zeroed();
    for i in &mut input {
        i.r#type = INPUT_KEYBOARD;
        i.Anonymous.ki.wVk = key;
    }
    input[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
    assert_eq!(
        SendInput(2, input.as_ptr(), std::mem::size_of::<INPUT>() as i32),
        2
    );
}

pub struct Scenario {
    child: Child,
    step: u8,
    since: Instant,
    started: Instant,
    target: Option<Target>,
    fitted: Option<Bounds>,
    hwnd: isize,
    motion_pixel: [u8; 4],
    motion_frames: u64,
    nr_builds: u64,
    nr_evaluations: u64,
    menu_before: egui::Pos2,
    menu_header_before: egui::Pos2,
    drag_point: egui::Pos2,
    cursor_before: POINT,
    idle_before: Bounds,
    idle_resize_step: u32,
    probe: Option<Child>,
    stream_pixel: [u8; 4],
    source_close_round: u8,
    menu_recovery_case: u8,
    routing_choice: i32,
    routing_combo: egui::Rect,
    routing_option: egui::Rect,
    /// When the active look last became ready (it renders on later frames).
    look_ready_at: Option<Instant>,
}
impl Scenario {
    pub fn new() -> Self {
        std::fs::create_dir_all("artifacts").unwrap();
        for path in [
            "artifacts/self-test.txt",
            "artifacts/self-test-error.txt",
            "artifacts/self-test-progress.txt",
            "artifacts/source-close-check.txt",
            "artifacts/menu-recovery-check.txt",
        ] {
            let _ = std::fs::remove_file(path);
        }
        std::panic::set_hook(Box::new(|info| {
            let _ = std::fs::write("artifacts/self-test-error.txt", info.to_string());
            eprintln!("{info}");
        }));
        Self {
            child: Command::new(std::env::current_exe().unwrap())
                .arg("--test-window")
                .creation_flags(0x08000000)
                .spawn()
                .unwrap(),
            step: 0,
            since: Instant::now(),
            started: Instant::now(),
            target: None,
            fitted: None,
            hwnd: 0,
            motion_pixel: [0; 4],
            motion_frames: 0,
            nr_builds: 0,
            nr_evaluations: 0,
            menu_before: egui::Pos2::ZERO,
            menu_header_before: egui::Pos2::ZERO,
            drag_point: egui::Pos2::ZERO,
            cursor_before: POINT { x: 0, y: 0 },
            idle_before: Bounds::default(),
            idle_resize_step: 0,
            probe: None,
            stream_pixel: [0; 4],
            source_close_round: 0,
            menu_recovery_case: 0,
            routing_choice: 0,
            routing_combo: egui::Rect::NOTHING,
            routing_option: egui::Rect::NOTHING,
            look_ready_at: None,
        }
    }
    fn advance(&mut self) {
        self.step += 1;
        self.since = Instant::now();
        std::fs::write(
            "artifacts/self-test-progress.txt",
            format!("Step {}", self.step),
        )
        .unwrap();
    }
    fn poll_probe(&mut self, hwnd: isize, path: &str) -> Option<Vec<u8>> {
        if let Some(probe) = &mut self.probe {
            let result = probe.try_wait().unwrap()?;
            assert!(result.success(), "External Windows Graphics Capture failed");
            self.probe = None;
            let pixels = std::fs::read(path).unwrap();
            assert_eq!(pixels.len(), 8);
            Some(pixels)
        } else {
            self.probe = Some(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--capture-probe", &hwnd.to_string(), path])
                    .creation_flags(0x08000000)
                    .spawn()
                    .unwrap(),
            );
            None
        }
    }
    pub fn tick(&mut self, app: &mut App, ctx: &egui::Context) {
        if self.step == 99 {
            return;
        }
        assert!(
            self.started.elapsed() < Duration::from_secs(100),
            "Self-test timed out at step {}",
            self.step
        );
        if self.step != 0 && std::env::args().any(|a| a == "--test-topmost") {
            self.tick_topmost(app, ctx);
            return;
        }
        if self.step != 0 && std::env::args().any(|a| a == "--test-hotkeys") {
            self.tick_hotkeys(app, ctx);
            return;
        }
        if self.step != 0 && std::env::args().any(|a| a == "--test-input-routing") {
            self.tick_input_routing(app, ctx);
            return;
        }
        if self.since.elapsed() < Duration::from_millis(350) && ![2, 4, 10].contains(&self.step) {
            return;
        }
        let screenshots: Vec<_> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
                .collect()
        });
        if self.step != 0 && std::env::args().any(|a| a == "--test-source-close") {
            self.tick_source_close(app, ctx);
            return;
        }
        if self.step != 0 && std::env::args().any(|a| a == "--test-menu-recovery") {
            self.tick_menu_recovery(app, ctx);
            return;
        }
        unsafe {
            match self.step {
                0 => {
                    if app.runtime.status().receiver == 0 {
                        return;
                    }
                    let Some(target) = native::windows()
                        .into_iter()
                        .find(|t| t.pid == self.child.id())
                    else {
                        return;
                    };
                    assert_eq!(target.title, "Overlay test — content window");
                    for kind in [ICON_SMALL, ICON_BIG] {
                        assert_ne!(
                            SendMessageW(app.hwnd as HWND, WM_GETICON, kind as usize, 0),
                            0,
                            "Window and taskbar icons must both be set"
                        );
                    }
                    let mut affinity = 0;
                    // The query requires a layered window; the external probe
                    // below is the actual capture compatibility check.
                    if GetWindowDisplayAffinity(app.hwnd as HWND, &mut affinity) != 0 {
                        assert_eq!(affinity, WDA_NONE, "Output must allow Windows capture");
                    }
                    assert!(
                        app.graphics.capture(app.hwnd, true).is_err(),
                        "Self-capture must remain blocked"
                    );
                    self.idle_before = native::surface_bounds(app.hwnd).unwrap();
                    app.fit(target.clone());
                    // Menu input checks below exercise the compact size. Keep
                    // its grip inside the fixture even if app defaults grow.
                    if std::env::args().any(|a| a == "--test-menu") {
                        app.menu_size = egui::vec2(660., 560.);
                    }
                    app.windows = native::windows();
                    assert!(app.error.is_none(), "{:?}", app.error);
                    self.target = Some(target);
                    self.hwnd = app.hwnd;
                    self.fitted = app.fitted;
                    self.advance();
                }
                1 => {
                    assert!(app.error.is_none(), "{:?}", app.error);
                    if app.graphics.status().has_frame == 0 {
                        return;
                    }
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        self.fitted,
                        "Actual root must fit the target"
                    );
                    assert_eq!(
                        visible_surfaces(),
                        1,
                        "Only one visible window may belong to the app"
                    );
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    self.advance();
                }
                2 => {
                    let Some(image) = screenshots.first() else {
                        return;
                    };
                    let opaque = image.pixels.iter().filter(|p| p.a() == 255).count();
                    assert!(
                        opaque > image.pixels.len() / 2,
                        "Menu must share a surface with captured pixels"
                    );
                    crate::save_image("artifacts/unified-menu.png", image);
                    if std::env::args().any(|a| a == "--test-menu") {
                        assert!(
                            app.runtime.status().ui_hotkey,
                            "F8 must register successfully"
                        );
                        self.menu_before = app.menu_position.unwrap();
                        self.menu_header_before = app.rects.menu_header.min;
                        self.drag_point = egui::pos2(
                            app.rects.menu_header.right() - 20.,
                            app.rects.menu_header.center().y,
                        ) * ctx.pixels_per_point();
                        GetCursorPos(&mut self.cursor_before);
                        SetForegroundWindow(app.hwnd as HWND);
                        test_mouse(app.hwnd as HWND, self.drag_point, 0);
                        self.step = 39;
                        self.since = Instant::now();
                        return;
                    }
                    PostMessageW(app.runtime.status().receiver as HWND, WM_HOTKEY, 1, 0);
                    self.advance();
                }
                39 => {
                    // Give native activation and pointer hover a frame before
                    // pressing, instead of racing both against SendInput.
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    self.advance();
                }
                40 => {
                    let point = self.drag_point + egui::vec2(90., 45.) * ctx.pixels_per_point();
                    test_mouse(app.hwnd as HWND, point, 0);
                    self.advance();
                }
                41 => {
                    let point = self.drag_point + egui::vec2(110., 55.) * ctx.pixels_per_point();
                    test_mouse(
                        app.hwnd as HWND,
                        point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                42 => {
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    assert!(
                        app.rects.menu_header.min.distance(self.menu_header_before) > 30.,
                        "Rendered title must actually move: {:?} -> {:?}, position {:?}, pointer {:?}",self.menu_header_before,app.rects.menu_header.min,app.menu_position,ctx.input(|i|(i.pointer.interact_pos(),i.pointer.primary_down()))
                    );
                    assert!(
                        app.menu_position.unwrap().distance(self.menu_before) > 30.,
                        "Left-button drag must move the embedded menu: {:?} -> {:?}",
                        self.menu_before,
                        app.menu_position
                    );
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        self.fitted,
                        "Dragging menu must not move the capture canvas"
                    );
                    let b = self.fitted.unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    crate::save_image("artifacts/draggable-menu.png", &image);
                    // Exercise Windows' actual registered F8 hotkey, rather than
                    // posting WM_HOTKEY directly to its receiver.
                    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
                    let mut input: [INPUT; 2] = std::mem::zeroed();
                    for i in &mut input {
                        i.r#type = INPUT_KEYBOARD;
                        i.Anonymous.ki.wVk = VK_F8;
                    }
                    input[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
                    assert_eq!(
                        SendInput(2, input.as_ptr(), std::mem::size_of::<INPUT>() as i32),
                        2
                    );
                    self.advance();
                }
                43 => {
                    assert!(!app.menu_visible, "F8 must hide the menu");
                    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
                    let mut input: [INPUT; 2] = std::mem::zeroed();
                    for i in &mut input {
                        i.r#type = INPUT_KEYBOARD;
                        i.Anonymous.ki.wVk = VK_F8;
                    }
                    input[1].Anonymous.ki.dwFlags = KEYEVENTF_KEYUP;
                    assert_eq!(
                        SendInput(2, input.as_ptr(), std::mem::size_of::<INPUT>() as i32),
                        2
                    );
                    self.advance();
                }
                44 => {
                    assert!(app.menu_visible, "F8 must restore a click-through menu");
                    assert_eq!(native::surface_bounds(app.hwnd), self.fitted);
                    assert!(
                        app.menu_position.unwrap().distance(self.menu_before) > 30.,
                        "Menu placement must survive hide/show"
                    );
                    std::fs::write("artifacts/menu-input-check.txt","PASS: left-button drag in empty header space moves menu independently; real F8 input hides and restores click-through menu; position preserved.\n").unwrap();
                    app.menu_position = None;
                    // Leave room to grow inside the fixture at display scaling
                    // above 100%; a clipped panel cannot grow on both axes.
                    app.menu_size = egui::vec2(500., 350.);
                    self.step = 54;
                    self.since = Instant::now();
                }
                54 => {
                    assert!(
                        app.runtime.status().neural_hotkey && app.runtime.status().compare_hotkey,
                        "F6 and F9 must register"
                    );
                    self.drag_point = app.rects.menu_resize.center() * ctx.pixels_per_point();
                    GetCursorPos(&mut self.cursor_before);
                    SetForegroundWindow(app.hwnd as HWND);
                    test_mouse(app.hwnd as HWND, self.drag_point, 0);
                    self.step = 200;
                    self.since = Instant::now();
                }
                200 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    self.step = 55;
                    self.since = Instant::now();
                }
                55 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point + egui::vec2(80., 40.) * ctx.pixels_per_point(),
                        0,
                    );
                    self.advance();
                }
                56 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point + egui::vec2(80., 40.) * ctx.pixels_per_point(),
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                57 => {
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    assert!(
                        app.menu_size.x > 540. && app.menu_size.y > 370.,
                        "Corner drag must resize menu: {:?}",
                        app.menu_size
                    );
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        self.fitted,
                        "Menu resize must not resize the canvas"
                    );
                    std::fs::write("artifacts/menu-resize-check.txt","PASS: real left-button corner drag resizes the compact menu without changing canvas bounds.\n").unwrap();
                    app.menu_size = egui::vec2(660., 560.);
                    self.step = 50;
                    self.since = Instant::now();
                }
                50 => {
                    app.set_always_on_top(false);
                    assert!(!app.always_on_top);
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                        0
                    );
                    app.fit(self.target.clone().unwrap());
                    app.show_menu();
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                        0,
                        "Fit and menu recovery must preserve normal z-order"
                    );
                    self.step = 88;
                    self.since = Instant::now();
                }
                88 => {
                    assert!(
                        ctx.screen_rect().contains_rect(app.rects.stop_button),
                        "Stop must be reachable above the page controls"
                    );
                    self.drag_point = app.rects.stop_button.center() * ctx.pixels_per_point();
                    GetCursorPos(&mut self.cursor_before);
                    SetForegroundWindow(app.hwnd as HWND);
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    self.advance();
                }
                89 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                90 => {
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    assert!(app.error.is_none(), "{:?}", app.error);
                    assert!(app.target.is_none() && app.fitted.is_none());
                    assert_eq!(app.graphics.status().has_frame, 0);
                    let idle = native::surface_bounds(app.hwnd).unwrap();
                    assert_eq!((idle.x, idle.y), (self.idle_before.x, self.idle_before.y));
                    let expected = (app.menu_size + egui::Vec2::splat(8.)) * ctx.pixels_per_point();
                    assert!((idle.width as f32 - expected.x).abs() <= 1.
                        && (idle.height as f32 - expected.y).abs() <= 1.,
                        "Idle canvas must shrink around the current menu: {idle:?}, expected {expected:?}");
                    self.idle_before = idle;
                    assert!(app.menu_visible && !app.paused);
                    std::fs::write("artifacts/stop-button-check.txt", "PASS: clicking Stop clears capture and selection, restores pre-capture position, shrinks the canvas to the current menu, and keeps the menu interactive.\n").unwrap();
                    self.step = 210;
                    self.since = Instant::now();
                }
                210 => {
                    self.drag_point = app.rects.menu_resize.center() * ctx.pixels_per_point();
                    self.menu_before = egui::pos2(app.menu_size.x, app.menu_size.y);
                    GetCursorPos(&mut self.cursor_before);
                    SetForegroundWindow(app.hwnd as HWND);
                    test_mouse(app.hwnd as HWND, self.drag_point, 0);
                    self.idle_resize_step = 0;
                    self.advance();
                }
                211 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    self.advance();
                }
                212 => {
                    self.idle_resize_step += 1;
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point
                            + egui::vec2(8., 5.)
                                * self.idle_resize_step as f32
                                * ctx.pixels_per_point(),
                        0,
                    );
                    if self.idle_resize_step == 20 {
                        self.advance();
                    }
                }
                213 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point + egui::vec2(160., 100.) * ctx.pixels_per_point(),
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                214 => {
                    if self.since.elapsed() < Duration::from_millis(150) {
                        return;
                    }
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    let expected = self.menu_before.to_vec2() + egui::vec2(160., 100.);
                    assert!(
                        (app.menu_size - expected).length() < 3.,
                        "Idle drag lost mouse movement: {:?} vs {:?}",
                        app.menu_size,
                        expected
                    );
                    let bounds = native::surface_bounds(app.hwnd).unwrap();
                    let physical = (expected + egui::Vec2::splat(8.)) * ctx.pixels_per_point();
                    assert!(
                        (bounds.width as f32 - physical.x).abs() < 2.
                            && (bounds.height as f32 - physical.y).abs() < 2.,
                        "Idle window no longer hugs resized menu"
                    );
                    assert!(!app.menu_resizing && app.error.is_none());
                    self.idle_before = bounds;
                    std::fs::write("artifacts/idle-menu-resize-check.txt", "PASS: 20 real drag updates retain full 160x100 logical movement; native window follows menu; release restores exact canvas.\n").unwrap();
                    app.tab = crate::Page::Look;
                    self.advance();
                }
                215 => {
                    let bounds = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([bounds.width as u32, bounds.height as u32])
                        .unwrap();
                    crate::save_image("artifacts/reshade-cleanup.png", &image);
                    // Later menu-layout checks use the fixture's original size.
                    app.menu_size = self.menu_before.to_vec2();
                    self.advance();
                }
                216 => {
                    if self.since.elapsed() < Duration::from_millis(150) {
                        return;
                    }
                    self.idle_before = native::surface_bounds(app.hwnd).unwrap();
                    self.step = 51;
                    self.since = Instant::now();
                }
                51 => {
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32
                            & (WS_EX_TOPMOST | WS_EX_TRANSPARENT),
                        0,
                        "Idle window must remain normal and interactive"
                    );
                    let b = self.idle_before;
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    assert_eq!(
                        image.pixels[0].a(),
                        0,
                        "Stopping must clear the captured image"
                    );
                    crate::save_image("artifacts/idle-window-controls.png", &image);
                    self.menu_before = app.menu_position.unwrap();
                    self.menu_header_before = app.rects.menu_header.min;
                    self.drag_point = egui::pos2(
                        app.rects.menu_header.right() - 20.,
                        app.rects.menu_header.center().y,
                    ) * ctx.pixels_per_point();
                    GetCursorPos(&mut self.cursor_before);
                    SetForegroundWindow(app.hwnd as HWND);
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    let mut point = POINT {
                        x: self.drag_point.x as i32,
                        y: self.drag_point.y as i32,
                    };
                    ClientToScreen(app.hwnd as HWND, &mut point);
                    // Windows' native drag runs a modal loop. Deliver move/release
                    // independently so the UI thread can complete that loop.
                    thread::spawn(move || {
                        use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
                        thread::sleep(Duration::from_millis(350));
                        SetCursorPos(point.x + 20, point.y + 10);
                        thread::sleep(Duration::from_millis(350));
                        SetCursorPos(point.x + 130, point.y + 70);
                        thread::sleep(Duration::from_millis(350));
                        let mut input: INPUT = std::mem::zeroed();
                        input.r#type = INPUT_MOUSE;
                        input.Anonymous.mi.dwFlags = MOUSEEVENTF_LEFTUP;
                        SendInput(1, &input, std::mem::size_of::<INPUT>() as i32);
                    });
                    self.advance();
                }
                52 => {
                    if self.since.elapsed() < Duration::from_millis(1400) {
                        return;
                    }
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    let moved = native::surface_bounds(app.hwnd).unwrap();
                    assert!(
                        (moved.x - self.idle_before.x).abs() > 30
                            || (moved.y - self.idle_before.y).abs() > 30,
                        "Idle title drag must move the native app: {:?} -> {:?}",
                        self.idle_before,
                        moved
                    );
                    assert_eq!(
                        (moved.width, moved.height),
                        (self.idle_before.width, self.idle_before.height)
                    );
                    assert!(
                        app.rects.menu_header.min.distance(self.menu_header_before) < 2.,
                        "Idle drag must leave menu attached to the app"
                    );
                    app.set_always_on_top(true);
                    assert_ne!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                        0
                    );
                    app.set_always_on_top(false);
                    self.idle_before = moved;
                    app.fit(self.target.clone().unwrap());
                    assert!(app.error.is_none(), "{:?}", app.error);
                    assert_eq!(app.idle_bounds, Some(moved));
                    self.fitted = app.fitted;
                    self.advance();
                }
                53 => {
                    if app.graphics.status().has_frame == 0 {
                        return;
                    }
                    assert_eq!(native::surface_bounds(app.hwnd), self.fitted);
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                        0
                    );
                    app.stop_rendering();
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        Some(self.idle_before),
                        "A second stop must restore the newly dragged idle placement"
                    );
                    app.fit(self.target.clone().unwrap());
                    assert!(app.error.is_none(), "{:?}", app.error);
                    std::fs::write("artifacts/window-controls-check.txt", "PASS: Always on top toggles the native window level; fit/menu recovery preserve off; stopping clears capture and selection and restores idle bounds; real left-button idle drag moves the native app with its menu attached; reselect captures again; second stop restores new idle position.\n").unwrap();
                    PostMessageW(app.runtime.status().receiver as HWND, WM_HOTKEY, 1, 0);
                    self.step = 3;
                    self.since = Instant::now();
                }
                3 => {
                    if app.menu_visible {
                        return;
                    }
                    assert_ne!(
                        IsWindowVisible(app.hwnd as HWND),
                        0,
                        "Hiding menu must not hide the root"
                    );
                    assert_eq!(app.hwnd, self.hwnd);
                    assert_eq!(visible_surfaces(), 1);
                    let flags = GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32;
                    assert_eq!(
                        flags & (WS_EX_LAYERED | WS_EX_TRANSPARENT),
                        WS_EX_LAYERED | WS_EX_TRANSPARENT
                    );
                    let b = self.fitted.unwrap();
                    assert_ne!(
                        WindowFromPoint(POINT {
                            x: b.x + b.width / 2,
                            y: b.y + b.height / 2
                        }),
                        app.hwnd as HWND
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    self.advance();
                }
                4 => {
                    let Some(image) = screenshots.first() else {
                        return;
                    };
                    let pixel = image.pixels
                        [image.width() * (image.height() / 2) + image.width() / 6]
                        .to_array();
                    assert!(
                        pixel[0].abs_diff(80) < 4
                            && pixel[1].abs_diff(140) < 4
                            && pixel[2].abs_diff(200) < 4
                            && pixel[3] == 255,
                        "Captured RGB pixel: {pixel:?}"
                    );
                    crate::save_image("artifacts/live-original.png", image);
                    app.options.effect = 1;
                    app.options.saturation = 0.0;
                    self.step = 20;
                    self.since = Instant::now();
                }
                20 => {
                    if std::env::args().any(|a| a == "--test-stream") {
                        app.set_always_on_top(true);
                    }
                    let b = app.fitted.unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    let pixel =
                        image.pixels[image.width() * (image.height() / 2) + image.width() / 6];
                    assert!(
                        pixel.r().abs_diff(pixel.g()) < 2 && pixel.g().abs_diff(pixel.b()) < 2,
                        "Adjustment must process captured pixels"
                    );
                    crate::save_image("artifacts/live-adjusted.png", &image);
                    app.options.effect = 0;
                    if std::env::args().any(|a| a == "--test-looks") {
                        self.step = 230;
                        self.since = Instant::now();
                        return;
                    }
                    if std::env::args().any(|a| a == "--test-neural") {
                        if std::env::args().any(|a| a == "--test-balanced") {
                            app.nr_options.motion_backend = 1;
                            app.nr_options.style = 3;
                            app.save_nr_options();
                            assert_eq!(crate::optiscaler::load_options().style, 3);
                        }
                        app.show_menu();
                        app.tab = crate::Page::Neural;
                        self.step = 80;
                        self.since = Instant::now();
                        return;
                    } else if std::env::args().any(|a| a == "--test-reshade") {
                        app.load_effects(false);
                    }
                    self.advance();
                }
                21 => {
                    assert!(app.error.is_none(), "{:?}", app.error);
                    if std::env::args().any(|a| a == "--test-neural") {
                        assert!(
                            app.neural_enabled(),
                            "The checkbox must start neural processing from Original mode"
                        );
                    }
                    if app.options.reshade != 0 {
                        if self.since.elapsed() < Duration::from_secs(20) {
                            return;
                        }
                        assert!(
                            app.graphics.status().techniques > 0,
                            "ReShade shaders must compile"
                        );
                        let b = app.fitted.unwrap();
                        let image = app
                            .graphics
                            .screenshot([b.width as u32, b.height as u32])
                            .unwrap();
                        crate::save_image("artifacts/live-reshade.png", &image);
                        let pixel =
                            image.pixels[image.width() * (image.height() / 2) + image.width() / 6];
                        assert!(
                            pixel.r() != 80 || pixel.g() != 140 || pixel.b() != 200,
                            "ReShade must change captured pixels"
                        );
                        if std::env::args().any(|a| a == "--test-neural") {
                            let s = app.graphics.nr_status();
                            assert!(
                                s.evaluations > 0,
                                "Neural frames must evaluate: {}",
                                s.message()
                            );
                        }
                    }
                    let b = app.fitted.unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    self.motion_pixel = image.pixels
                        [image.width() * (image.height() / 4) + image.width() / 2]
                        .to_array();
                    if std::env::args().any(|a| a == "--test-stream") {
                        let Some(pixels) =
                            self.poll_probe(app.hwnd, "artifacts/stream-before.rgba")
                        else {
                            return;
                        };
                        self.stream_pixel.copy_from_slice(&pixels[4..8]);
                        assert!(
                            self.stream_pixel[..3]
                                .iter()
                                .zip(&self.motion_pixel[..3])
                                .all(|(a, b)| a.abs_diff(*b) < 12),
                            "External capture must see processed output: {:?} vs {:?}",
                            self.stream_pixel,
                            self.motion_pixel
                        );
                        assert_eq!(self.stream_pixel[3], 255);
                    }
                    self.motion_frames = app.graphics.status().frames;
                    crate::save_image("artifacts/motion-before.png", &image);
                    if std::env::args().any(|a| a == "--test-reshade") {
                        assert!(
                            app.graphics
                                .uniforms("SpatpitClarity.fx")
                                .iter()
                                .any(|u| u.name == "Contrast"),
                            "Shader controls must expose editable uniforms"
                        );
                        app.graphics
                            .set_uniform(
                                "SpatpitClarity.fx",
                                "Contrast",
                                &[0.5, 0.0, 0.0, 0.0],
                                false,
                            )
                            .unwrap();
                    }
                    PostMessageW(
                        self.target.as_ref().unwrap().hwnd as HWND,
                        WM_APP + 42,
                        1,
                        0,
                    );
                    self.advance();
                }
                22 => {
                    if self.since.elapsed() < Duration::from_secs(2) {
                        return;
                    }
                    let b = app.fitted.unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    crate::save_image("artifacts/motion-after.png", &image);
                    if std::env::args().any(|a| a == "--test-reshade") {
                        let left = image.pixels
                            [image.width() * (image.height() / 2) + image.width() / 6]
                            .to_array();
                        assert!(left[0].abs_diff(104)<=3 && left[1].abs_diff(134)<=3 && left[2].abs_diff(164)<=3,"Editing the ReShade contrast control must change output pixels: {left:?}");
                        app.graphics
                            .set_uniform(
                                "SpatpitClarity.fx",
                                "Contrast",
                                &[1.08, 0.0, 0.0, 0.0],
                                false,
                            )
                            .unwrap();
                    }
                    let pixel = image.pixels
                        [image.width() * (image.height() / 4) + image.width() / 2]
                        .to_array();
                    assert!(
                        app.graphics.status().frames > self.motion_frames,
                        "Capture must receive new source frames after neural activation"
                    );
                    assert!(
                        pixel[..3]
                            .iter()
                            .zip(&self.motion_pixel[..3])
                            .map(|(a, b)| a.abs_diff(*b) as u32)
                            .sum::<u32>()
                            > 80,
                        "Live processed output must follow a changed source: {:?} -> {:?}",
                        self.motion_pixel,
                        pixel
                    );
                    if std::env::args().any(|a| a == "--test-stream") {
                        let Some(pixels) = self.poll_probe(app.hwnd, "artifacts/stream-after.rgba")
                        else {
                            return;
                        };
                        assert!(
                            pixels[4..7]
                                .iter()
                                .zip(&pixel[..3])
                                .all(|(a, b)| a.abs_diff(*b) < 12),
                            "External capture must follow processed output: {:?} vs {:?}",
                            &pixels[4..8],
                            pixel
                        );
                        assert!(
                            pixels[4..7]
                                .iter()
                                .zip(&self.stream_pixel[..3])
                                .map(|(a, b)| a.abs_diff(*b) as u32)
                                .sum::<u32>()
                                > 80,
                            "Stream consumer must receive changed frames"
                        );
                        assert_eq!(pixels[7], 255);
                        std::fs::write("artifacts/stream-capture-check.txt", "PASS: external-process Windows Graphics Capture receives the overlay's processed pixels and follows source changes while the canvas covers the source; no source/output feedback observed; direct self-capture remains rejected. Discord itself was not exercised.\n").unwrap();
                        if std::env::args().any(|a| a == "--test-menu") {
                            app.set_always_on_top(false);
                        }
                    }
                    if std::env::args().any(|a| a == "--test-neural") {
                        assert!(app.runtime.status().neural_hotkey);
                        test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F6);
                        self.nr_evaluations = app.graphics.nr_status().evaluations;
                        self.nr_builds = app.graphics.nr_status().builds;
                        self.step = 70;
                        self.since = Instant::now();
                        return;
                    }
                    app.options.reshade = 0;
                    let target = self.target.as_ref().unwrap();
                    SetWindowPos(
                        target.hwnd as HWND,
                        null_mut(),
                        160,
                        140,
                        840,
                        640,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    self.step = 5;
                    self.since = Instant::now();
                }
                230..=236 => self.tick_looks(app, ctx),
                80 => {
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    assert!(!app.neural_enabled());
                    GetCursorPos(&mut self.cursor_before);
                    SetForegroundWindow(app.hwnd as HWND);
                    self.drag_point = app.rects.neural_toggle.center() * ctx.pixels_per_point();
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    self.advance();
                }
                81 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                82 => {
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    assert!(
                        app.neural_enabled(),
                        "One checkbox click must start the pipeline: {:?}",
                        app.error
                    );
                    app.menu_visible = false;
                    self.step = 21;
                    self.since = Instant::now();
                }
                70 => {
                    assert_eq!(
                        app.nr_options.enabled, 0,
                        "Actual F6 input must bypass neural processing"
                    );
                    assert!(!app.menu_visible, "F6 must not open the menu");
                    self.nr_evaluations = app.graphics.nr_status().evaluations;
                    self.step = 30;
                    self.since = Instant::now();
                }
                30 => {
                    assert_eq!(
                        app.graphics.nr_status().evaluations,
                        self.nr_evaluations,
                        "Disabling must stop neural work immediately"
                    );
                    assert_eq!(app.graphics.nr_status().builds, self.nr_builds);
                    app.nr_options.blend = 0.;
                    app.graphics.nr_configure(&app.nr_options);
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F6);
                    self.advance();
                }
                31 => {
                    let s = app.graphics.nr_status();
                    assert!(s.evaluations > self.nr_evaluations, "Re-enable must resume");
                    assert_eq!(s.builds, self.nr_builds, "Toggle must reuse model");
                    let b = app.fitted.unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    let pixel =
                        image.pixels[image.width() * (image.height() / 2) + image.width() / 6];
                    assert!(
                        pixel.r().abs_diff(80) < 3
                            && pixel.g().abs_diff(140) < 3
                            && pixel.b().abs_diff(200) < 3,
                        "Zero blend should reproduce original: {:?}",
                        pixel
                    );
                    crate::save_image("artifacts/optiscaler-zero-blend.png", &image);
                    app.nr_options.intensity = 0.37;
                    app.graphics.nr_configure(&app.nr_options);
                    self.advance();
                }
                32 => {
                    if self.since.elapsed() < Duration::from_secs(3) {
                        return;
                    }
                    let s = app.graphics.nr_status();
                    assert_eq!(
                        s.builds,
                        self.nr_builds + 1,
                        "One automatic model rebuild for intensity: {}",
                        s.message()
                    );
                    assert!(s.active != 0);
                    assert_eq!(app.hwnd, self.hwnd);
                    self.nr_builds = s.builds;
                    app.nr_options.model_scale = 0.75;
                    app.nr_options.blend = 1.;
                    app.graphics.nr_configure(&app.nr_options);
                    self.advance();
                }
                33 => {
                    if self.since.elapsed() < Duration::from_secs(3) {
                        return;
                    }
                    let s = app.graphics.nr_status();
                    if std::env::args().any(|a| a == "--test-custom") {
                        assert!(s.motion_frames > 0, "Custom motion did not run");
                        std::fs::write(
                            "artifacts/custom-neural-check.txt",
                            format!(
                                "PASS: custom motion frames={} neural evaluations={}\n",
                                s.motion_frames, s.evaluations
                            ),
                        )
                        .unwrap();
                    }
                    assert_eq!(s.builds, self.nr_builds + 1);
                    let b = app.fitted.unwrap();
                    assert_eq!(s.width, ((b.width as f32 * 0.75) as u32) & !7);
                    assert!(s.active != 0, "{}", s.message());
                    std::fs::write("artifacts/optiscaler-functional-check.txt","PASS: one enable-checkbox click starts processing; successful neural evaluation; moving input changes processed pixels; immediate bypass and resume without feature rebuild; zero blend matches original; intensity and model resolution apply automatically in the same process and HWND. No benchmarks run.\n").unwrap();
                    let passes = app.nr_options.passes;
                    app.nr_options = crate::optiscaler::Options {
                        passes,
                        ..Default::default()
                    };
                    if std::env::args().any(|a| a == "--test-custom") {
                        app.nr_options.motion_backend = 1;
                    }
                    if std::env::args().any(|a| a == "--test-balanced") {
                        app.nr_options.motion_backend = 1;
                        app.nr_options.style = 3;
                        std::fs::write("artifacts/balanced-check.txt", "PASS: Balanced preference roundtrip, activation, moving output, bypass, intensity and model resize.\n").unwrap();
                    }
                    app.graphics.nr_configure(&app.nr_options);
                    app.menu_visible = true;
                    app.tab = crate::Page::Neural;
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                        1160., 860.,
                    )));
                    self.advance();
                    if std::env::args().any(|a| a == "--test-multipass") {
                        app.nr_options.passes = 2;
                        app.graphics.nr_configure(&app.nr_options);
                        self.nr_evaluations = s.evaluations;
                        self.step = 182;
                    }
                }
                186 => {
                    if self.since.elapsed() < Duration::from_secs(5) {
                        return;
                    }
                    let s = app.graphics.nr_status();
                    assert!(
                        s.active != 0 && s.evaluations > self.nr_evaluations,
                        "Per-pass settings stopped the chain: {}",
                        s.message()
                    );
                    std::fs::write(
                        "artifacts/multipass-tuning-check.txt",
                        "PASS: 3x with own settings for passes 2 and 3 (35% and 25% model resolution, different styles, intensity and blend) keeps producing successful full-chain frames.\n",
                    )
                    .unwrap();
                    app.nr_options.pass_tuning = Default::default();
                    app.graphics.nr_configure(&app.nr_options);
                    self.step = 34;
                    self.since = Instant::now();
                }
                182..=185 => {
                    if self.since.elapsed() < Duration::from_secs(4) {
                        return;
                    }
                    let s = app.graphics.nr_status();
                    assert!(
                        s.active != 0 && s.evaluations > self.nr_evaluations,
                        "Pass switch to {} failed: {}",
                        app.nr_options.passes,
                        s.message()
                    );
                    self.nr_evaluations = s.evaluations;
                    if self.step == 185 {
                        std::fs::write("artifacts/multipass-switch-check.txt",
                            "PASS: live 3x -> 2x -> 3x -> 1x -> 3x switching resumes successful full-chain frames.\n").unwrap();
                        // Own settings for the extra passes: lower model
                        // resolutions and a different style per pass.
                        app.nr_options.pass_tuning = [
                            crate::optiscaler::PassTuning {
                                custom: 1,
                                style: 1,
                                model_scale: 0.35,
                                intensity: 0.8,
                                blend: 0.9,
                            },
                            crate::optiscaler::PassTuning {
                                custom: 1,
                                style: 0,
                                model_scale: 0.25,
                                intensity: 0.6,
                                blend: 0.7,
                            },
                        ];
                        app.graphics.nr_configure(&app.nr_options);
                        self.step = 186;
                        self.since = Instant::now();
                    } else {
                        app.nr_options.passes = if self.step == 183 { 1 } else { 3 };
                        app.graphics.nr_configure(&app.nr_options);
                        self.advance();
                    }
                }
                34 => {
                    if self.since.elapsed() < Duration::from_secs(3) {
                        return;
                    }
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    crate::save_image("artifacts/optiscaler-menu.png", &image);
                    if std::env::args().any(|a| a == "--test-menu") {
                        app.menu_position = None;
                        app.menu_size = egui::vec2(1000., 760.);
                        self.step = 86;
                        self.since = Instant::now();
                        return;
                    }
                    app.stop_rendering();
                    assert_eq!(
                        app.options.neural, 1,
                        "Stopping preserves processing preferences"
                    );
                    assert!(app.target.is_none());
                    self.advance();
                }
                86 => {
                    assert!(app.rects.output_limit.left() > app.rects.neural_toggle.right());
                    assert!(
                        (app.rects.output_limit.top() - app.rects.neural_toggle.top()).abs() < 8.
                    );
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    crate::save_image("artifacts/neural-menu-wide.png", &image);
                    app.menu_size = egui::vec2(340., 560.);
                    self.advance();
                }
                87 => {
                    assert!(app.rects.output_limit.top() > app.rects.neural_toggle.bottom());
                    assert!(
                        app.rects.output_limit.right()
                            < app.menu_position.unwrap().x + app.menu_size.x
                    );
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    crate::save_image("artifacts/neural-menu-narrow.png", &image);
                    app.menu_size = egui::vec2(660., 560.);
                    self.nr_builds = app.graphics.nr_status().builds;
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F9);
                    self.step = 60;
                    self.since = Instant::now();
                }
                60 => {
                    assert!(
                        app.compare_visible && !app.menu_visible,
                        "F9 opens only the mini panel"
                    );
                    assert_eq!(app.nr_options.compare, 2);
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0
                    );
                    GetCursorPos(&mut self.cursor_before);
                    self.drag_point = egui::pos2(
                        app.rects.compare_slider.left() + 200.,
                        app.rects.compare_slider.center().y,
                    ) * ctx.pixels_per_point();
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    self.advance();
                }
                61 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                62 => {
                    assert!(
                        app.nr_options.split > 0.3,
                        "Mini slider must change OptiScaler's shared split"
                    );
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    let pixel =
                        image.pixels[image.width() * (image.height() / 2) + image.width() / 6];
                    crate::save_image("artifacts/compare-mini-menu.png", &image);
                    assert!(
                        pixel.r().abs_diff(80) < 3
                            && pixel.g().abs_diff(140) < 3
                            && pixel.b().abs_diff(200) < 3,
                        "Wipe original side: {:?}",
                        pixel
                    );
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                    );
                    self.advance();
                }
                63 => {
                    self.drag_point = egui::pos2(
                        app.rects.compare_slider.left() + 2.,
                        app.rects.compare_slider.center().y,
                    ) * ctx.pixels_per_point();
                    test_mouse(app.hwnd as HWND, self.drag_point, 0);
                    self.advance();
                }
                64 => {
                    test_mouse(
                        app.hwnd as HWND,
                        self.drag_point,
                        windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                    );
                    self.advance();
                }
                65 => {
                    SetCursorPos(self.cursor_before.x, self.cursor_before.y);
                    assert!(
                        app.nr_options.split < 0.1,
                        "Dragging mini slider must move the wipe"
                    );
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    let pixel =
                        image.pixels[image.width() * (image.height() / 2) + image.width() / 6];
                    assert!(
                        pixel.r() != 80 || pixel.g() != 140 || pixel.b() != 200,
                        "Wipe must reveal neural output at the same pixel"
                    );
                    assert_eq!(
                        app.graphics.nr_status().builds,
                        self.nr_builds,
                        "Comparison must not rebuild the model"
                    );
                    assert_eq!(app.hwnd, self.hwnd);
                    app.nr_options.split = 0.7;
                    app.graphics.nr_configure(&app.nr_options);
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F9);
                    self.advance();
                }
                66 => {
                    assert!(!app.compare_visible && !app.menu_visible);
                    assert_ne!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Closing mini controls restores click-through"
                    );
                    assert_eq!(
                        app.nr_options.compare, 0,
                        "Closing controls disables comparison"
                    );
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    let pixel =
                        image.pixels[image.width() * (image.height() / 2) + image.width() / 6];
                    assert!(
                        pixel.r() != 80 || pixel.g() != 140 || pixel.b() != 200,
                        "F9 off must restore neural output on the original side"
                    );
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F9);
                    self.step = 83;
                    self.since = Instant::now();
                }
                83 => {
                    assert!(app.compare_visible);
                    assert_eq!(app.nr_options.compare, 2);
                    assert_eq!(app.nr_options.split, 0.7);
                    app.nr_options.compare = 1;
                    app.graphics.nr_configure(&app.nr_options);
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F9);
                    self.advance();
                }
                84 => {
                    assert!(!app.compare_visible);
                    assert_eq!(app.nr_options.compare, 0);
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F9);
                    self.advance();
                }
                85 => {
                    assert!(app.compare_visible);
                    assert_eq!(
                        app.nr_options.compare, 1,
                        "Reopen restores side-by-side mode"
                    );
                    app.close_compare();
                    assert_eq!(app.nr_options.compare, 0);
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F8);
                    self.step = 67;
                    self.since = Instant::now();
                }
                67 => {
                    assert!(app.menu_visible && !app.compare_visible);
                    assert_eq!(app.nr_options.split, 0.7);
                    app.nr_options.compare = 0;
                    app.nr_options.split = 0.5;
                    app.graphics.nr_configure(&app.nr_options);
                    app.save_nr_options();
                    std::fs::write("artifacts/neural-shortcuts-check.txt","PASS: actual F6 input bypasses/resumes neural processing without a model restart; actual F9 opens/closes the mini panel and disables the comparison effect; reopening restores wipe or side-by-side mode; real slider drag updates shared OptiScaler wipe and changes output pixels; F9 close restores click-through; F8 restores full menu.\n").unwrap();
                    app.stop_rendering();
                    self.step = 35;
                    self.since = Instant::now();
                }
                35 => {
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    assert_eq!(
                        image.pixels[0].a(),
                        0,
                        "Neural output must also clear on stop"
                    );
                    assert_eq!(app.graphics.status().has_frame, 0);
                    assert_eq!(app.graphics.status().capture_active, 0);
                    self.nr_evaluations = app.graphics.nr_status().evaluations;
                    app.fit(self.target.clone().unwrap());
                    assert!(app.error.is_none(), "{:?}", app.error);
                    self.advance();
                }
                36 => {
                    if app.graphics.nr_status().evaluations <= self.nr_evaluations
                        || app.graphics.nr_status().active == 0
                    {
                        return;
                    }
                    assert_ne!(app.graphics.status().has_frame, 0);
                    std::fs::write("artifacts/neural-stop-check.txt", "PASS: stopping with OptiScaler enabled clears capture and output; neural preferences are retained; selecting a source again resumes successful neural evaluation.\n").unwrap();
                    app.menu_visible = false;
                    app.options.reshade = 0;
                    app.fit(self.target.clone().unwrap());
                    self.fitted = app.fitted;
                    let target = self.target.as_ref().unwrap();
                    SetWindowPos(
                        target.hwnd as HWND,
                        null_mut(),
                        160,
                        140,
                        840,
                        640,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    self.step = 5;
                    self.since = Instant::now();
                }
                5 => {
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        self.fitted,
                        "Target movement/resizing must not be followed"
                    );
                    app.fit(self.target.clone().unwrap());
                    assert!(app.error.is_none());
                    assert_ne!(
                        app.fitted, self.fitted,
                        "Fit again should explicitly update bounds"
                    );
                    self.fitted = app.fitted;
                    self.advance();
                }
                6 => {
                    assert_eq!(native::surface_bounds(app.hwnd), self.fitted);
                    ShowWindow(self.target.as_ref().unwrap().hwnd as HWND, SW_MINIMIZE);
                    self.advance();
                }
                7 => {
                    assert_ne!(IsWindowVisible(app.hwnd as HWND), 0);
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        self.fitted,
                        "Target minimizing must not change the canvas"
                    );
                    // Exercise tray restore, rather than directly editing the menu state.
                    PostMessageW(
                        app.runtime.status().receiver as HWND,
                        WM_APP + 1,
                        1,
                        WM_LBUTTONUP as isize,
                    );
                    self.advance();
                }
                8 => {
                    if !app.menu_visible {
                        return;
                    }
                    assert_eq!(app.hwnd, self.hwnd);
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Restored menu must accept input"
                    );
                    ShowWindow(self.target.as_ref().unwrap().hwnd as HWND, SW_RESTORE);
                    SetWindowPos(
                        self.target.as_ref().unwrap().hwnd as HWND,
                        null_mut(),
                        120,
                        120,
                        380,
                        330,
                        SWP_NOACTIVATE | SWP_NOZORDER,
                    );
                    app.area = Area::Window;
                    app.fit(self.target.clone().unwrap());
                    self.fitted = app.fitted;
                    self.advance();
                }
                9 => {
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        self.fitted,
                        "Small targets must not be clamped to menu minimum size"
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot);
                    self.advance();
                }
                10 => {
                    let Some(image) = screenshots.first() else {
                        return;
                    };
                    crate::save_image("artifacts/small-menu.png", image);
                    app.menu_visible = false;
                    PostMessageW(self.target.as_ref().unwrap().hwnd as HWND, WM_CLOSE, 0, 0);
                    self.advance();
                }
                11 => {
                    if app.applied_presentation != Some((true, false)) {
                        return;
                    }
                    assert_eq!(
                        app.graphics.status().capture_active,
                        0,
                        "Closed source must stop capture"
                    );
                    assert_eq!(IsWindow(self.target.as_ref().unwrap().hwnd as HWND), 0);
                    assert_ne!(IsWindowVisible(app.hwnd as HWND), 0);
                    assert!(app.target.is_none() && app.fitted.is_none());
                    assert!(app.menu_visible && !app.paused && !app.compare_visible);
                    assert_eq!(app.graphics.status().has_frame, 0);
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Source closure must restore mouse input"
                    );
                    assert_eq!(
                        native::surface_bounds(app.hwnd),
                        Some(self.idle_before),
                        "Closing target must restore the app's idle bounds"
                    );
                    self.fitted = native::surface_bounds(app.hwnd);
                    app.fit(self.target.clone().unwrap());
                    assert!(
                        app.error.is_some(),
                        "Fit again must report an unavailable target"
                    );
                    assert_eq!(native::surface_bounds(app.hwnd), self.fitted);
                    PostMessageW(app.runtime.status().receiver as HWND, WM_HOTKEY, 2, 0);
                    self.advance();
                }
                12 => {
                    if !app.paused {
                        return;
                    }
                    let b = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([b.width as u32, b.height as u32])
                        .unwrap();
                    assert!(
                        image.pixels.iter().all(|p| p.a() == 0),
                        "Paused canvas must be fully transparent"
                    );
                    crate::save_image("artifacts/hidden-surface.png", &image);
                    assert_ne!(
                        IsWindowVisible(app.hwnd as HWND),
                        0,
                        "Pausing clears the surface without hiding it"
                    );
                    PostMessageW(app.runtime.status().receiver as HWND, WM_HOTKEY, 2, 0);
                    self.advance();
                }
                13 => {
                    if app.paused {
                        return;
                    }
                    assert_ne!(IsWindowVisible(app.hwnd as HWND), 0);
                    assert_eq!(native::surface_bounds(app.hwnd), self.fitted);
                    ShowWindowAsync(app.hwnd as HWND, SW_MINIMIZE);
                    let receiver = app.runtime.status().receiver;
                    thread::spawn(move || {
                        thread::sleep(Duration::from_millis(200));
                        PostMessageW(receiver as HWND, WM_HOTKEY, 1, 0);
                    });
                    self.advance();
                }
                14 => {
                    if std::env::args().any(|a| a == "--test-menu") {
                        assert_eq!(
                            GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST,
                            0,
                            "Pause, tray and minimized recovery must preserve Always on top off"
                        );
                    }
                    assert_eq!(
                        IsIconic(app.hwnd as HWND),
                        0,
                        "Shortcut must restore a minimized canvas"
                    );
                    assert!(
                        app.menu_visible,
                        "Restoring a minimized canvas must show the menu"
                    );
                    assert_eq!(native::surface_bounds(app.hwnd), self.fitted);
                    std::fs::write("artifacts/self-test.txt", "PASS: cross-process GPU capture with checked RGB output; image adjustment changes captured pixels; one DirectX rendering HWND; hidden-menu click-through; explicit fit/refit; no movement following; transparent pause; tray/input restore; small-window fit; source minimize/close; pause/resume; shortcut recovery. No benchmarks run.\n").unwrap();
                    self.step = 99;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => unreachable!(),
            }
        }
    }

    fn tick_menu_recovery(&mut self, app: &mut App, ctx: &egui::Context) {
        unsafe {
            match self.step {
                1 => {
                    assert!(app.error.is_none(), "{:?}", app.error);
                    if app.graphics.status().has_frame == 0 {
                        return;
                    }
                    app.tab = crate::Page::Neural;
                    app.menu_visible = false;
                    app.paused = self.menu_recovery_case == 4;
                    self.advance();
                }
                2 => {
                    assert!(!app.menu_visible);
                    assert_ne!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Hide controls before exercising shell recovery"
                    );
                    let hwnd = app.hwnd;
                    let source = self.target.as_ref().unwrap().hwnd as HWND;
                    // Explorer is allowed to activate windows. Give this test
                    // the same right through its foreground source process.
                    if GetForegroundWindow() != source {
                        SendMessageW(source, WM_APP + 43, hwnd as usize, 0);
                        let point = ctx.screen_rect().center() * ctx.pixels_per_point();
                        input_routing::routed_mouse(
                            hwnd as HWND,
                            point,
                            windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTDOWN,
                        );
                        input_routing::routed_mouse(
                            hwnd as HWND,
                            point,
                            windows_sys::Win32::UI::Input::KeyboardAndMouse::MOUSEEVENTF_LEFTUP,
                        );
                        assert!(
                            self.since.elapsed() < Duration::from_secs(3),
                            "Fixture must receive focus before simulating Explorer"
                        );
                        return;
                    }
                    assert_ne!(
                        SendMessageW(source, WM_APP + 44, GetCurrentProcessId() as usize, 0),
                        0
                    );
                    let receiver = app.runtime.status().receiver;
                    match self.menu_recovery_case {
                        0 => {
                            // Explorer activates a running window on taskbar selection.
                            assert_ne!(SetForegroundWindow(hwnd as HWND), 0);
                        }
                        1 => {
                            // An already-active taskbar button requests minimize.
                            SendMessageW(hwnd as HWND, WM_SYSCOMMAND, SC_MINIMIZE as usize, 0);
                        }
                        2 | 5 => {
                            let tray = self.menu_recovery_case == 5;
                            thread::spawn(move || {
                                thread::sleep(Duration::from_millis(200));
                                if tray {
                                    PostMessageW(
                                        receiver as HWND,
                                        WM_APP + 1,
                                        1,
                                        WM_LBUTTONUP as isize,
                                    );
                                } else {
                                    PostMessageW(
                                        hwnd as HWND,
                                        WM_SYSCOMMAND,
                                        SC_RESTORE as usize,
                                        0,
                                    );
                                }
                            });
                            ShowWindowAsync(hwnd as HWND, SW_MINIMIZE);
                        }
                        3 | 4 | 6 | 7 => {
                            let event = match self.menu_recovery_case {
                                6 => native::TRAY_KEY_SELECT,
                                7 => WM_LBUTTONDBLCLK,
                                _ => WM_LBUTTONUP,
                            };
                            PostMessageW(receiver as HWND, WM_APP + 1, 1, event as isize);
                        }
                        _ => unreachable!(),
                    }
                    self.advance();
                }
                3 => {
                    let flags = GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32;
                    if app.menu_visible
                        && !app.paused
                        && IsIconic(app.hwnd as HWND) == 0
                        && !self.hover_controls(app, ctx)
                    {
                        return;
                    }
                    if !app.menu_visible
                        || app.paused
                        || IsIconic(app.hwnd as HWND) != 0
                        || flags & (WS_EX_TRANSPARENT | WS_EX_NOACTIVATE) != 0
                    {
                        assert!(
                            self.since.elapsed() < Duration::from_secs(3),
                            "Menu recovery failed for case {}",
                            self.menu_recovery_case
                        );
                        return;
                    }
                    assert!(app.error.is_none(), "{:?}", app.error);
                    assert_eq!(
                        app.tab,
                        crate::Page::Neural,
                        "Keep the previously selected menu page"
                    );
                    assert_eq!(native::surface_bounds(app.hwnd), self.fitted);
                    assert_eq!(app.target, self.target);
                    assert_ne!(app.graphics.status().capture_active, 0);
                    assert_ne!(app.graphics.status().has_frame, 0);
                    self.menu_recovery_case += 1;
                    if self.menu_recovery_case == 8 {
                        app.menu_visible = false;
                        self.advance();
                    } else {
                        self.step = 1;
                        self.since = Instant::now();
                    }
                }
                4 => {
                    test_key(windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_F9);
                    self.advance();
                }
                5 => {
                    assert!(
                        app.compare_visible && !app.menu_visible,
                        "F9 must open only the comparison panel"
                    );
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    SetForegroundWindow(self.target.as_ref().unwrap().hwnd as HWND);
                    assert_ne!(SetForegroundWindow(app.hwnd as HWND), 0);
                    self.advance();
                }
                6 => {
                    assert!(
                        app.compare_visible && !app.menu_visible,
                        "Activating the interactive comparison panel must not open the full menu"
                    );
                    app.close_compare();
                    app.paused = true;
                    self.advance();
                }
                7 => {
                    PostMessageW(app.runtime.status().receiver as HWND, WM_HOTKEY, 2, 0);
                    self.advance();
                }
                8 => {
                    assert!(
                        !app.menu_visible && !app.paused,
                        "Resuming via shortcut must preserve hidden controls"
                    );
                    assert_ne!(app.graphics.status().capture_active, 0);
                    std::fs::write("artifacts/menu-recovery-check.txt", "PASS: taskbar activation, minimize command while controls are hidden, minimized taskbar restore, tray click, paused tray click, minimized tray click, tray keyboard selection and double-click restore the menu and mouse input without shortcuts; capture, canvas bounds and selected page are retained. F9 panel activation and pause/resume do not spuriously open the full menu. Shell message paths tested; Explorer UI was not clicked.\n").unwrap();
                    self.step = 99;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => unreachable!(),
            }
        }
    }

    fn tick_source_close(&mut self, app: &mut App, ctx: &egui::Context) {
        let neural = std::env::args().any(|a| a == "--test-neural");
        unsafe {
            match self.step {
                1 => {
                    assert!(app.error.is_none(), "{:?}", app.error);
                    if app.graphics.status().has_frame == 0 {
                        return;
                    }
                    if neural && app.options.neural == 0 {
                        app.load_effects(true);
                        assert!(app.error.is_none(), "{:?}", app.error);
                    }
                    self.advance();
                }
                2 => {
                    if neural && app.graphics.nr_status().evaluations <= self.nr_evaluations {
                        return;
                    }
                    assert!(app.error.is_none(), "{:?}", app.error);
                    app.menu_visible = false;
                    // Also exercise source loss while processing is paused.
                    app.paused = self.source_close_round == 1;
                    self.advance();
                }
                3 => {
                    assert_ne!(app.graphics.status().capture_active, 0);
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        WS_EX_TRANSPARENT,
                        "The source must close while controls are click-through"
                    );
                    if self.source_close_round == 2 {
                        self.child.kill().unwrap();
                    } else {
                        PostMessageW(self.target.as_ref().unwrap().hwnd as HWND, WM_CLOSE, 0, 0);
                    }
                    self.advance();
                }
                4 => {
                    if app.graphics.status().capture_active != 0
                        || app.target.is_some()
                        || app.applied_presentation != Some((true, false))
                    {
                        assert!(
                            self.since.elapsed() < Duration::from_secs(5),
                            "Source closure did not recover"
                        );
                        return;
                    }
                    assert_eq!(IsWindow(self.target.as_ref().unwrap().hwnd as HWND), 0);
                    assert_eq!(app.graphics.status().has_frame, 0);
                    assert!(app.menu_visible && !app.paused && !app.compare_visible);
                    assert!(app.fitted.is_none());
                    assert_eq!(app.tab, crate::Page::Source);
                    assert_eq!(native::surface_bounds(app.hwnd), Some(self.idle_before));
                    if !self.hover_controls(app, ctx) {
                        return;
                    }
                    assert_eq!(
                        GetWindowLongPtrW(app.hwnd as HWND, GWL_EXSTYLE) as u32 & WS_EX_TRANSPARENT,
                        0,
                        "Source loss must restore input without a shortcut"
                    );
                    let bounds = native::surface_bounds(app.hwnd).unwrap();
                    let image = app
                        .graphics
                        .screenshot([bounds.width as u32, bounds.height as u32]);
                    let image = match image {
                        Ok(image) => image,
                        Err(error) if error == "Screenshot dimensions changed" => {
                            assert!(
                                self.since.elapsed() < Duration::from_secs(5),
                                "Canvas resize did not finish"
                            );
                            return;
                        }
                        Err(error) => panic!("{error}"),
                    };
                    assert_eq!(
                        image.pixels[0].a(),
                        0,
                        "Source loss must clear captured output"
                    );
                    if neural {
                        assert_eq!(app.options.neural, 1, "Keep neural processing configured");
                    }
                    self.nr_evaluations = app.graphics.nr_status().evaluations;
                    self.motion_frames = app.graphics.status().frames;
                    assert!(self.child.try_wait().unwrap().is_some());
                    self.child = Command::new(std::env::current_exe().unwrap())
                        .arg("--test-window")
                        .creation_flags(0x08000000)
                        .spawn()
                        .unwrap();
                    self.advance();
                }
                5 => {
                    let Some(target) = native::windows()
                        .into_iter()
                        .find(|t| t.pid == self.child.id())
                    else {
                        return;
                    };
                    app.fit(target.clone());
                    assert!(app.error.is_none(), "{:?}", app.error);
                    self.target = Some(target);
                    self.advance();
                }
                6 => {
                    assert!(app.error.is_none(), "{:?}", app.error);
                    if app.graphics.status().frames <= self.motion_frames
                        || (neural && app.graphics.nr_status().evaluations <= self.nr_evaluations)
                    {
                        return;
                    }
                    assert_ne!(app.graphics.status().has_frame, 0);
                    self.source_close_round += 1;
                    if self.source_close_round == 3 {
                        std::fs::write("artifacts/source-close-check.txt", format!(
                            "PASS: graceful close with hidden controls, close while paused, forced source process exit; capture/output cleared, source menu and mouse input restored, idle bounds restored; replacement source renders after each closure. Neural enabled: {neural}.\n"
                        )).unwrap();
                        self.step = 99;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    } else {
                        self.step = 2;
                        self.since = Instant::now();
                    }
                }
                _ => unreachable!(),
            }
        }
    }
}
impl Drop for Scenario {
    fn drop(&mut self) {
        if let Some(probe) = &mut self.probe {
            let _ = probe.kill();
            let _ = probe.wait();
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
unsafe fn visible_surfaces() -> usize {
    unsafe extern "system" fn count(hwnd: HWND, param: LPARAM) -> BOOL {
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId() && IsWindowVisible(hwnd) != 0 {
            let mut class = [0u16; 256];
            let len = GetClassNameW(hwnd, class.as_mut_ptr(), 256);
            let class = String::from_utf16_lossy(&class[..len as usize]);
            // Windows input indicators and winit event plumbing are not app rendering surfaces.
            if class.starts_with("UAC") || class == "Winit Thread Event Target" {
                return 1;
            }
            *(param as *mut usize) += 1;
        }
        1
    }
    let mut total = 0usize;
    EnumWindows(Some(count), &mut total as *mut _ as LPARAM);
    total
}
