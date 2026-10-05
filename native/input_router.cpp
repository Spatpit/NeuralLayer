#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <commctrl.h>
#include <atomic>
#include <array>
#include <memory>
#include <mutex>
#include <thread>

static UINT style_message() {
  static UINT message = RegisterWindowMessageW(L"SpatpitOverlay.InputRegionStyle");
  return message;
}
static LRESULT CALLBACK input_style(HWND hwnd, UINT message, WPARAM pass,
                                    LPARAM point_data, UINT_PTR id, DWORD_PTR) {
  if (message == style_message()) {
    auto flags = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    constexpr LONG_PTR mask = WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE;
    auto next = pass ? flags | mask : flags & ~mask;
    if (next != flags) {
      SetWindowLongPtrW(hwnd, GWL_EXSTYLE, next);
      SetWindowPos(hwnd, nullptr, 0, 0, 0, 0,
                   SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED);
      if (pass) PostMessageW(hwnd, WM_MOUSELEAVE, 0, 0);
      else {
        POINT point{static_cast<short>(LOWORD(point_data)), static_cast<short>(HIWORD(point_data))};
        ScreenToClient(hwnd, &point);
        PostMessageW(hwnd, WM_MOUSEMOVE, 0, MAKELPARAM(point.x, point.y));
      }
    }
    return 1;
  }
  if (message == WM_NCDESTROY) RemoveWindowSubclass(hwnd, input_style, id);
  return DefSubclassProc(hwnd, message, pass, point_data);
}

// Keep one rendering HWND. A dedicated mouse-hook thread changes the normal
// Windows hit-test styles before input is dispatched, including the first click
// after crossing a panel edge. No input is swallowed, forwarded or recorded.
struct InputRouter {
  HWND window{};
  std::mutex mutex;
  std::array<RECT, 64> regions{};
  unsigned count = 0;
  unsigned mode = 1; // 0: passthrough, 1: whole window, 2: UI regions
  std::atomic<bool> stopping{false};
  std::atomic<DWORD> thread{0};
  HANDLE ready = CreateEventW(nullptr, TRUE, FALSE, nullptr);
  HHOOK hook{};
  std::atomic<unsigned> buttons{0};
  std::atomic<bool> drag_pass{false};
  ULONGLONG retry_after = 0;
  ~InputRouter() { if (ready) CloseHandle(ready); }

  bool desired(POINT point) {
    unsigned current_mode;
    std::array<RECT, 64> current_regions;
    unsigned current_count;
    {
      std::lock_guard lock(mutex);
      current_mode = mode;
      current_regions = regions;
      current_count = count;
    }
    if (current_mode == 0) return true;
    if (buttons) return drag_pass.load();
    if (current_mode == 1) return false;
    ScreenToClient(window, &point);
    for (unsigned i = 0; i < current_count; ++i)
      if (PtInRect(&current_regions[i], point)) return false;
    return true;
  }
  void apply(POINT point, bool pass) {
    if (stopping || !IsWindow(window)) return;
    auto flags = GetWindowLongPtrW(window, GWL_EXSTYLE);
    constexpr LONG_PTR mask = WS_EX_TRANSPARENT | WS_EX_LAYERED | WS_EX_NOACTIVATE;
    auto next = pass ? flags | mask : flags & ~mask;
    if (next == flags) return;
    // A slow model rebuild must not hang the global input hook. Mutate styles
    // on the window thread with a short bounded wait, then retry on a later
    // event if it is busy. No router lock is held across the call.
    if (GetTickCount64() < retry_after) return;
    DWORD_PTR result{};
    if (!SendMessageTimeoutW(window, style_message(), pass, MAKELPARAM(point.x,point.y),
                             SMTO_ABORTIFHUNG | SMTO_BLOCK, 8, &result))
      retry_after = GetTickCount64() + 32;
  }
};
static thread_local InputRouter *current_router = nullptr;
static unsigned button_bit(WPARAM message, DWORD data) {
  switch (message) {
  case WM_LBUTTONDOWN: case WM_LBUTTONUP: return 1;
  case WM_RBUTTONDOWN: case WM_RBUTTONUP: return 2;
  case WM_MBUTTONDOWN: case WM_MBUTTONUP: return 4;
  case WM_XBUTTONDOWN: case WM_XBUTTONUP: return HIWORD(data) == XBUTTON1 ? 8 : 16;
  default: return 0;
  }
}
static LRESULT CALLBACK route_mouse(int code, WPARAM message, LPARAM data) {
  if (code == HC_ACTION && current_router && !current_router->stopping) {
    auto &router = *current_router;
    auto &mouse = *reinterpret_cast<MSLLHOOKSTRUCT *>(data);
    bool pass = router.desired(mouse.pt);
    auto bit = button_bit(message, mouse.mouseData);
    bool down = message == WM_LBUTTONDOWN || message == WM_RBUTTONDOWN ||
                message == WM_MBUTTONDOWN || message == WM_XBUTTONDOWN;
    router.apply(mouse.pt, pass);
    if (bit) {
      if (down) {
        if (!router.buttons) router.drag_pass = pass;
        router.buttons |= bit;
      } else router.buttons &= ~bit;
    }
  }
  return CallNextHookEx(nullptr, code, message, data);
}
using RouterOwner = std::shared_ptr<InputRouter>;
extern "C" void *spatpit_input_router_create(HWND window) {
  RouterOwner state;
  try {
    state = std::make_shared<InputRouter>();
    if (!state->ready) return nullptr;
    state->window = window;
    if (!style_message() || !SetWindowSubclass(window, input_style, 2, 0)) return nullptr;
    std::thread([state] {
      state->thread = GetCurrentThreadId();
      MSG message{};
      PeekMessageW(&message, nullptr, 0, 0, PM_NOREMOVE);
      current_router = state.get();
      state->hook = SetWindowsHookExW(WH_MOUSE_LL, route_mouse, GetModuleHandleW(nullptr), 0);
      SetEvent(state->ready);
      if (state->hook) {
        while (!state->stopping && GetMessageW(&message, nullptr, 0, 0) > 0) {
          if (message.message == WM_APP) {
            POINT point{};
            if (GetCursorPos(&point)) state->apply(point, state->desired(point));
          }
        }
        UnhookWindowsHookEx(state->hook);
      }
      current_router = nullptr;
    }).detach();
    if (WaitForSingleObject(state->ready, 2000) != WAIT_OBJECT_0 || !state->hook) {
      state->stopping = true;
      PostThreadMessageW(state->thread, WM_QUIT, 0, 0);
      RemoveWindowSubclass(window, input_style, 2);
      return nullptr;
    }
    return new RouterOwner(std::move(state));
  } catch (...) {
    if (state) {
      state->stopping = true;
      PostThreadMessageW(state->thread, WM_QUIT, 0, 0);
      RemoveWindowSubclass(window, input_style, 2);
    }
    return nullptr;
  }
}
extern "C" void spatpit_input_router_regions(void *owner, unsigned mode, const RECT *rects, unsigned count) {
  if (!owner) return;
  auto &state = *static_cast<RouterOwner *>(owner);
  {
    std::lock_guard lock(state->mutex);
    state->mode = count > state->regions.size() ? 1 : mode;
    state->count = count > state->regions.size() ? 0 : count;
    for (unsigned i = 0; i < state->count; ++i) state->regions[i] = rects[i];
  }
  // Publish styles synchronously when already on the window thread. Waiting
  // for the hook's 8 ms request can leave stale hit-test styles during rendering.
  // The hook still handles first-click routing, and its atomic drag ownership
  // also governs this per-frame reconciliation.
  if (GetCurrentThreadId() == GetWindowThreadProcessId(state->window, nullptr)) {
    POINT point{};
    if (GetCursorPos(&point))
      SendMessageW(state->window, style_message(), state->desired(point), MAKELPARAM(point.x, point.y));
  }
  PostThreadMessageW(state->thread, WM_APP, 0, 0);
}
extern "C" void spatpit_input_router_destroy(void *owner) {
  if (!owner) return;
  auto &state = *static_cast<RouterOwner *>(owner);
  state->stopping = true;
  RemoveWindowSubclass(state->window, input_style, 2);
  PostThreadMessageW(state->thread, WM_QUIT, 0, 0);
  // Do not join on the window thread: a style-change call may be in flight.
  delete static_cast<RouterOwner *>(owner);
}
