//! One-time placement and tray/shortcut events. The winit root is the only rendering surface.

use std::{
    mem::size_of,
    ptr::null_mut,
    sync::{mpsc, Arc, Mutex},
    thread,
};
use windows_sys::Win32::{
    Foundation::*,
    Graphics::{
        Dwm::*,
        Gdi::{
            ClientToScreen, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject,
            ScreenToClient, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        },
    },
    System::{
        LibraryLoader::GetModuleHandleW,
        Threading::{
            GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW,
            PROCESS_QUERY_LIMITED_INFORMATION,
        },
    },
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, Shell::*, WindowsAndMessaging::*},
};

use crate::hotkeys::{Action, Hotkeys};

pub fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

// This Shell macro is not emitted by windows-sys 0.59.
pub const TRAY_KEY_SELECT: u32 = NIN_SELECT | NINF_KEY;

extern "C" {
    fn spatpit_taskbar_identity(hwnd: HWND, exe: *const u16) -> i32;
    fn spatpit_taskbar_verify(hwnd: HWND, exe: *const u16) -> i32;
    fn spatpit_taskbar_menu_recovery(hwnd: HWND, receiver: HWND, hidden: BOOL) -> BOOL;
    fn spatpit_input_router_create(hwnd: HWND) -> *mut std::ffi::c_void;
    fn spatpit_input_router_regions(
        owner: *mut std::ffi::c_void,
        mode: u32,
        rects: *const RECT,
        count: u32,
    );
    fn spatpit_input_router_destroy(owner: *mut std::ffi::c_void);
}

pub struct InputRouter(*mut std::ffi::c_void);
pub fn cursor_client(surface: isize) -> Option<(i32, i32)> {
    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut point) == 0 || ScreenToClient(surface as HWND, &mut point) == 0 {
            None
        } else {
            Some((point.x, point.y))
        }
    }
}
impl InputRouter {
    pub fn new(surface: isize) -> Result<Self, String> {
        let owner = unsafe { spatpit_input_router_create(surface as HWND) };
        if owner.is_null() {
            Err("Could not initialize menu click-through.".into())
        } else {
            Ok(Self(owner))
        }
    }
    pub fn update(&self, ctx: &egui::Context, controls: bool, full_window: bool) {
        let mut rects = Vec::new();
        if controls && !full_window {
            let layers = ctx.memory(|m| m.areas().visible_layer_ids());
            for layer in layers {
                if !matches!(layer.order, egui::Order::Middle | egui::Order::Foreground) {
                    continue;
                }
                if let Some(area) = egui::AreaState::load(ctx, layer.id) {
                    if !area.interactable {
                        continue;
                    }
                    let mut rect = area.rect();
                    if let Some(transform) = ctx.memory(|m| m.layer_transforms.get(&layer).copied())
                    {
                        rect = transform * rect;
                    }
                    let rect = rect.intersect(ctx.screen_rect());
                    let scale = ctx.pixels_per_point();
                    if rect.is_positive() {
                        rects.push(RECT {
                            left: (rect.left() * scale).floor() as i32,
                            top: (rect.top() * scale).floor() as i32,
                            right: (rect.right() * scale).ceil() as i32,
                            bottom: (rect.bottom() * scale).ceil() as i32,
                        });
                    }
                }
            }
        }
        let mode = if !controls {
            0
        } else if full_window {
            1
        } else {
            2
        };
        unsafe {
            spatpit_input_router_regions(self.0, mode, rects.as_ptr(), rects.len() as u32);
        }
    }
}
impl Drop for InputRouter {
    fn drop(&mut self) {
        unsafe {
            spatpit_input_router_destroy(self.0);
        }
    }
}

pub fn taskbar_menu_recovery(surface: isize, receiver: isize, hidden: bool) -> bool {
    unsafe { spatpit_taskbar_menu_recovery(surface as HWND, receiver as HWND, hidden as BOOL) != 0 }
}

pub fn taskbar_identity(surface: isize) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe: Vec<u16> = exe.as_os_str().encode_wide().chain(Some(0)).collect();
    let result = unsafe { spatpit_taskbar_identity(surface as HWND, exe.as_ptr()) };
    if result < 0 {
        return Err(format!("Could not set taskbar identity: 0x{result:08X}"));
    }
    let result = unsafe { spatpit_taskbar_verify(surface as HWND, exe.as_ptr()) };
    if result < 0 {
        return Err(format!("Could not resolve taskbar icon: 0x{result:08X}"));
    }
    Ok(())
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Target {
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Bounds {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Area {
    #[default]
    Content,
    Window,
}

pub fn windows() -> Vec<Target> {
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
        let list = &mut *(param as *mut Vec<Target>);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == GetCurrentProcessId()
            || IsWindowVisible(hwnd) == 0
            || GetWindowTextLengthW(hwnd) == 0
        {
            return 1;
        }
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if style & WS_EX_TOOLWINDOW != 0 || cloaked(hwnd) {
            return 1;
        }
        let title = title(hwnd);
        if !title.trim().is_empty() {
            list.push(Target {
                hwnd: hwnd as isize,
                pid,
                title,
            });
        }
        1
    }
    let mut result: Vec<Target> = Vec::new();
    unsafe {
        EnumWindows(Some(collect), &mut result as *mut _ as LPARAM);
    }
    result.sort_by_key(|window| window.title.to_lowercase());
    result
}

/// The executable file name of a process, e.g. `game.exe`.
pub fn executable(pid: u32) -> Option<String> {
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        let ok = QueryFullProcessImageNameW(process, 0, buffer.as_mut_ptr(), &mut length);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        path.rsplit(['\\', '/']).next().map(String::from)
    }
}

/// The window's icon as `size`×`size` RGBA pixels. Asks the window with a
/// short timeout, so a hung source cannot stall the menu.
pub fn window_icon(hwnd: isize, size: i32) -> Option<Vec<u8>> {
    unsafe {
        let window = hwnd as HWND;
        let mut icon: usize = 0;
        for kind in [ICON_BIG, ICON_SMALL2] {
            if icon == 0 {
                SendMessageTimeoutW(
                    window,
                    WM_GETICON,
                    kind as usize,
                    0,
                    SMTO_ABORTIFHUNG | SMTO_BLOCK,
                    50,
                    &mut icon,
                );
            }
        }
        for index in [GCLP_HICON, GCLP_HICONSM] {
            if icon == 0 {
                icon = GetClassLongPtrW(window, index);
            }
        }
        if icon == 0 {
            return None;
        }
        let dc = CreateCompatibleDC(null_mut());
        if dc.is_null() {
            return None;
        }
        let mut info: BITMAPINFO = std::mem::zeroed();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let mut bits: *mut std::ffi::c_void = null_mut();
        let bitmap = CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if bitmap.is_null() || bits.is_null() {
            DeleteDC(dc);
            return None;
        }
        let previous = SelectObject(dc, bitmap);
        let drawn = DrawIconEx(
            dc,
            0,
            0,
            icon as HICON,
            size,
            size,
            0,
            null_mut(),
            DI_NORMAL,
        );
        let count = (size * size) as usize;
        let bgra = std::slice::from_raw_parts(bits as *const u8, count * 4);
        let mut rgba = Vec::with_capacity(count * 4);
        for p in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        SelectObject(dc, previous);
        DeleteObject(bitmap);
        DeleteDC(dc);
        if drawn == 0 {
            return None;
        }
        // Legacy icons have no alpha channel: treat drawn pixels as opaque.
        if rgba.chunks_exact(4).all(|p| p[3] == 0) {
            for p in rgba.chunks_exact_mut(4) {
                if p[0] | p[1] | p[2] != 0 {
                    p[3] = 255;
                }
            }
        }
        Some(rgba)
    }
}

unsafe fn title(hwnd: HWND) -> String {
    let len = GetWindowTextLengthW(hwnd).max(0) as usize;
    let mut buf = vec![0u16; len + 1];
    let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..copied.max(0) as usize])
}

unsafe fn cloaked(hwnd: HWND) -> bool {
    let mut value: u32 = 0;
    DwmGetWindowAttribute(
        hwnd,
        DWMWA_CLOAKED as u32,
        &mut value as *mut _ as _,
        size_of::<u32>() as u32,
    ) >= 0
        && value != 0
}

pub fn bounds(target: &Target, area: Area) -> Option<Bounds> {
    unsafe {
        let hwnd = target.hwnd as HWND;
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if IsWindow(hwnd) == 0
            || pid != target.pid
            || IsIconic(hwnd) != 0
            || IsWindowVisible(hwnd) == 0
            || cloaked(hwnd)
        {
            return None;
        }
        let mut rect: RECT = std::mem::zeroed();
        match area {
            Area::Content => {
                if GetClientRect(hwnd, &mut rect) == 0 {
                    return None;
                }
                let mut origin = POINT { x: 0, y: 0 };
                if ClientToScreen(hwnd, &mut origin) == 0 {
                    return None;
                }
                rect.right += origin.x;
                rect.bottom += origin.y;
                rect.left = origin.x;
                rect.top = origin.y;
            }
            Area::Window => {
                if DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS as u32,
                    &mut rect as *mut _ as _,
                    size_of::<RECT>() as u32,
                ) < 0
                    && GetWindowRect(hwnd, &mut rect) == 0
                {
                    return None;
                }
            }
        }
        let result = Bounds {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        };
        (result.width > 0 && result.height > 0).then_some(result)
    }
}

pub fn surface_bounds(hwnd: isize) -> Option<Bounds> {
    unsafe {
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd as HWND, &mut rect) == 0 {
            return None;
        }
        Some(Bounds {
            x: rect.left,
            y: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        })
    }
}

/// Only called on selection or Fit again, on the UI thread. No later tracking.
pub fn fit_once(surface: isize, target: &Target, area: Area) -> Result<Bounds, String> {
    let rect = bounds(target, area)
        .ok_or("That window is closed, minimized, or unavailable. Restore it and try again.")?;
    place_surface(surface, rect)?;
    Ok(rect)
}

/// Placement must preserve the user's current topmost choice.
pub fn place_surface(surface: isize, rect: Bounds) -> Result<(), String> {
    unsafe {
        if IsIconic(surface as HWND) != 0 {
            ShowWindow(surface as HWND, SW_RESTORE);
        }
        if SetWindowPos(
            surface as HWND,
            std::ptr::null_mut(),
            rect.x,
            rect.y,
            rect.width,
            rect.height,
            SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOZORDER,
        ) == 0
        {
            return Err(format!(
                "Could not place this window (Windows error {}).",
                GetLastError()
            ));
        }
    }
    Ok(())
}

pub fn set_topmost(surface: isize, enabled: bool) -> Result<(), String> {
    unsafe {
        if SetWindowPos(
            surface as HWND,
            if enabled {
                HWND_TOPMOST
            } else {
                HWND_NOTOPMOST
            },
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        ) == 0
        {
            return Err(format!(
                "Could not change Always on top (Windows error {}).",
                GetLastError()
            ));
        }
    }
    Ok(())
}

/// Recover stacking only when the selected source is focused and has moved
/// above us. Never activate the overlay or touch another application's styles.
pub fn keep_above_source(surface: isize, target: &Target) -> Result<(), String> {
    unsafe {
        let overlay = surface as HWND;
        let source = target.hwnd as HWND;
        if GetForegroundWindow() != source
            || IsWindowVisible(overlay) == 0
            || IsIconic(overlay) != 0
            || IsWindowVisible(source) == 0
            || IsIconic(source) != 0
        {
            return Ok(());
        }
        let mut above = GetWindow(overlay, GW_HWNDPREV);
        // Bound traversal because another process may change the order while
        // it is being inspected. Usually there are only a few windows above us.
        for _ in 0..256 {
            if above.is_null() {
                break;
            }
            if above == source {
                // Recover only the position immediately above the game. A
                // global HWND_TOPMOST promotion would also cover separate
                // performance HUDs that were already above it.
                let predecessor = GetWindow(source, GW_HWNDPREV);
                if predecessor == overlay {
                    return Ok(());
                }
                let insert_after = if predecessor.is_null()
                    || GetWindowLongPtrW(source, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST == 0
                {
                    HWND_TOPMOST
                } else {
                    predecessor
                };
                if SetWindowPos(
                    overlay,
                    insert_after,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                ) == 0
                {
                    return Err(format!(
                        "Could not restore the overlay above its source (Windows error {}).",
                        GetLastError()
                    ));
                }
                return Ok(());
            }
            above = GetWindow(above, GW_HWNDPREV);
        }
        Ok(())
    }
}

pub fn restore_surface(surface: isize) {
    // Internal activation (F9 comparison, pause/resume, or showing the menu)
    // is not a taskbar click. presentation() rearms recovery afterward.
    taskbar_menu_recovery(surface, 0, false);
    unsafe {
        if IsIconic(surface as HWND) != 0 || IsWindowVisible(surface as HWND) == 0 {
            ShowWindow(surface as HWND, SW_RESTORE);
        }
        SetWindowPos(
            surface as HWND,
            std::ptr::null_mut(),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
        );
        SetForegroundWindow(surface as HWND);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Event {
    ToggleMenu,
    ShowMenu,
    TogglePaused,
    ToggleNeural,
    ToggleCompare,
    /// The show/hide shortcut while the window was minimized or hidden:
    /// bring back the user's preferred controls.
    RestoreControls,
    Quit,
}
#[derive(Clone, Debug, Default)]
pub struct Status {
    pub ui_hotkey: bool,
    pub pause_hotkey: bool,
    pub neural_hotkey: bool,
    pub compare_hotkey: bool,
    pub tray: bool,
    pub error: Option<String>,
    pub receiver: isize,
}

/// Shortcuts the tray thread should register; `None` suspends them all
/// (while the user records a new shortcut).
type SharedHotkeys = Arc<Mutex<Option<Hotkeys>>>;

/// Posted to the tray thread's window when the shortcuts change.
const WM_HOTKEYS_CHANGED: u32 = WM_APP + 3;

pub struct Runtime {
    pub events: mpsc::Receiver<Event>,
    status: Arc<Mutex<Status>>,
    hotkeys: SharedHotkeys,
    shutdown: mpsc::Sender<()>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Runtime {
    pub fn start(surface: isize, ctx: egui::Context, hotkeys: Hotkeys) -> Self {
        let (tx, events) = mpsc::channel();
        let (shutdown, stop) = mpsc::channel();
        let status = Arc::new(Mutex::new(Status::default()));
        let keys: SharedHotkeys = Arc::new(Mutex::new(Some(hotkeys)));
        let shared = status.clone();
        let thread_keys = keys.clone();
        let worker = thread::spawn(move || unsafe {
            event_loop(surface, ctx, tx, stop, shared, thread_keys)
        });
        Self {
            events,
            status,
            hotkeys: keys,
            shutdown,
            worker: Some(worker),
        }
    }
    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }
    /// Register new shortcuts, or suspend all of them with `None`.
    pub fn set_hotkeys(&self, hotkeys: Option<Hotkeys>) {
        *self.hotkeys.lock().unwrap() = hotkeys;
        let receiver = self.status().receiver;
        if receiver != 0 {
            unsafe { PostMessageW(receiver as HWND, WM_HOTKEYS_CHANGED, 0, 0) };
        }
    }
}
impl Drop for Runtime {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        // Wake the event thread from its message wait.
        let receiver = self.status().receiver;
        if receiver != 0 {
            unsafe { PostMessageW(receiver as HWND, WM_NULL, 0, 0) };
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

unsafe fn tray_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = 1;
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_APP + 1;
    data.hIcon = LoadIconW(GetModuleHandleW(null_mut()), 1usize as _);
    if data.hIcon.is_null() {
        data.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
    }
    let tip = wide("NeuralLayer");
    data.szTip[..tip.len()].copy_from_slice(&tip);
    data
}

unsafe fn tray_menu(owner: HWND, keys: Option<Hotkeys>) -> Option<Event> {
    let menu = CreatePopupMenu();
    let item = |text: &str, action: Action| match keys {
        Some(keys) => wide(&format!("{text}\t{}", keys.label(action))),
        None => wide(text),
    };
    AppendMenuW(menu, MF_STRING, 100, wide("Show menu").as_ptr());
    AppendMenuW(
        menu,
        MF_STRING,
        103,
        item("Toggle neural rendering", Action::Neural).as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        104,
        item("Comparison controls", Action::Compare).as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING,
        101,
        item("Pause / resume surface", Action::Pause).as_ptr(),
    );
    AppendMenuW(menu, MF_SEPARATOR, 0, null_mut());
    AppendMenuW(menu, MF_STRING, 102, wide("Quit NeuralLayer").as_ptr());
    let mut point = std::mem::zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(owner);
    let selected = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
        point.x,
        point.y,
        0,
        owner,
        null_mut(),
    );
    PostMessageW(owner, WM_NULL, 0, 0);
    DestroyMenu(menu);
    match selected {
        100 => Some(Event::ShowMenu),
        101 => Some(Event::TogglePaused),
        102 => Some(Event::Quit),
        103 => Some(Event::ToggleNeural),
        104 => Some(Event::ToggleCompare),
        _ => None,
    }
}

/// (Re)register every shortcut and record which ones Windows accepted.
unsafe fn register_hotkeys(receiver: HWND, keys: Option<Hotkeys>, status: &mut Status) {
    for action in Action::ALL {
        UnregisterHotKey(receiver, action as i32);
    }
    let mut unavailable = Vec::new();
    for action in Action::ALL {
        let ok = match keys {
            None => true,
            Some(keys) => {
                let key = keys.get(action);
                let mut modifiers = MOD_NOREPEAT;
                if key.ctrl {
                    modifiers |= MOD_CONTROL;
                }
                if key.alt {
                    modifiers |= MOD_ALT;
                }
                if key.shift {
                    modifiers |= MOD_SHIFT;
                }
                let ok = RegisterHotKey(receiver, action as i32, modifiers, key.vk) != 0;
                if !ok && action != Action::Pause {
                    unavailable.push(key.label());
                }
                ok
            }
        };
        match action {
            Action::Menu => status.ui_hotkey = ok,
            Action::Pause => status.pause_hotkey = ok,
            Action::Neural => status.neural_hotkey = ok,
            Action::Compare => status.compare_hotkey = ok,
        }
    }
    status.error = (!unavailable.is_empty()).then(|| {
        format!(
            "{} already in use. Change it in Settings under Shortcuts, or use the menu or tray.",
            unavailable.join(", ")
        )
    });
}

unsafe fn event_loop(
    surface: isize,
    ctx: egui::Context,
    tx: mpsc::Sender<Event>,
    stop: mpsc::Receiver<()>,
    shared: Arc<Mutex<Status>>,
    hotkeys: SharedHotkeys,
) {
    SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    let instance = GetModuleHandleW(null_mut());
    let class = wide("NeuralLayer.Events");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(DefWindowProcW),
        hInstance: instance,
        lpszClassName: class.as_ptr(),
        ..std::mem::zeroed()
    };
    RegisterClassW(&wc);
    // Hidden tray message receiver only: no renderer, no overlay, no control panel window.
    let receiver = CreateWindowExW(
        WS_EX_TOOLWINDOW,
        class.as_ptr(),
        wide("NeuralLayer Events").as_ptr(),
        WS_POPUP,
        0,
        0,
        0,
        0,
        null_mut(),
        null_mut(),
        instance,
        null_mut(),
    );
    if receiver.is_null() {
        shared.lock().unwrap().error =
            Some("Tray and shortcuts could not start. The menu will stay accessible.".into());
        ctx.request_repaint();
        return;
    }
    let data = tray_data(receiver);
    let mut status = Status {
        tray: Shell_NotifyIconW(NIM_ADD, &data) != 0,
        receiver: receiver as isize,
        ..Default::default()
    };
    let keys = *hotkeys.lock().unwrap();
    register_hotkeys(receiver, keys, &mut status);
    *shared.lock().unwrap() = status.clone();
    ctx.request_repaint();
    let taskbar_created = RegisterWindowMessageW(wide("TaskbarCreated").as_ptr());
    loop {
        match stop.try_recv() {
            Ok(()) | Err(mpsc::TryRecvError::Disconnected) => break,
            Err(mpsc::TryRecvError::Empty) => {}
        }
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
            let event = match message.message {
                WM_HOTKEY if message.wParam == 1 => Some(Event::ToggleMenu),
                WM_HOTKEY if message.wParam == 2 => Some(Event::TogglePaused),
                WM_HOTKEY if message.wParam == 3 => Some(Event::ToggleNeural),
                WM_HOTKEY if message.wParam == 4 => Some(Event::ToggleCompare),
                value if value == WM_APP + 1 => match message.lParam as u32 {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK | NIN_SELECT | TRAY_KEY_SELECT => {
                        Some(Event::ShowMenu)
                    }
                    WM_RBUTTONUP => tray_menu(receiver, *hotkeys.lock().unwrap()),
                    _ => None,
                },
                value if value == WM_APP + 2 => Some(Event::ShowMenu),
                WM_HOTKEYS_CHANGED => {
                    let keys = *hotkeys.lock().unwrap();
                    register_hotkeys(receiver, keys, &mut status);
                    *shared.lock().unwrap() = status.clone();
                    ctx.request_repaint();
                    None
                }
                value if taskbar_created != 0 && value == taskbar_created => {
                    status.tray = Shell_NotifyIconW(NIM_ADD, &data) != 0;
                    *shared.lock().unwrap() = status.clone();
                    ctx.request_repaint();
                    None
                }
                _ => {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                    None
                }
            };
            if let Some(mut event) = event {
                if event == Event::Quit {
                    PostMessageW(surface as HWND, WM_CLOSE, 0, 0);
                } else if event != Event::ToggleNeural
                    && (IsIconic(surface as HWND) != 0 || IsWindowVisible(surface as HWND) == 0)
                {
                    if event == Event::ToggleMenu {
                        event = Event::RestoreControls;
                    }
                    ShowWindowAsync(surface as HWND, SW_RESTORE);
                }
                let _ = tx.send(event);
                ctx.request_repaint();
            }
        }
        // Sleep until a hotkey, tray or shutdown message arrives. The timeout
        // only bounds how long a shutdown request can go unnoticed.
        MsgWaitForMultipleObjects(0, std::ptr::null(), 0, 250, QS_ALLINPUT);
    }
    Shell_NotifyIconW(NIM_DELETE, &data);
    for action in Action::ALL {
        UnregisterHotKey(receiver, action as i32);
    }
    DestroyWindow(receiver);
    UnregisterClassW(class.as_ptr(), instance);
}

/// Give a source back the focus after the user hides our controls.
/// Click-through alone does not activate games that pause in the background.
pub fn capture_input_mode(surface: isize, target: Option<&Target>, passthrough: bool) {
    unsafe {
        let hwnd = surface as HWND;
        // InputRouter owns hit-test/activation styles. Only an explicit hide
        // hands focus back; moving the pointer outside the panel must not.
        if passthrough && GetForegroundWindow() == hwnd {
            if let Some(target) = target {
                if IsWindow(target.hwnd as HWND) != 0 && IsIconic(target.hwnd as HWND) == 0 {
                    SetForegroundWindow(target.hwnd as HWND);
                }
            }
        }
    }
}
