// Live WGC -> the application's Engine -> DirectComposition benchmark.
// Separate animated source process; timestamps do not measure physical scanout.
struct LatencyTimer {
  HANDLE timer = CreateWaitableTimerExW(nullptr, nullptr, 2, TIMER_ALL_ACCESS);
  LatencyTimer() {
    if (!timer)
      throw std::runtime_error("Create high-resolution latency timer");
  }
  ~LatencyTimer() { CloseHandle(timer); }
  void wait(int64_t deadline, int64_t frequency) {
    for (;;) {
      MSG msg{};
      while (PeekMessageW(&msg, nullptr, 0, 0, PM_REMOVE)) {
        if (msg.message == WM_QUIT)
          throw std::runtime_error("Latency window closed");
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
      }
      auto remaining = deadline - LatencyTrace::now();
      if (remaining <= 0)
        return;
      LARGE_INTEGER due;
      due.QuadPart = -std::max<int64_t>(
          1, int64_t(double(remaining) * 10000000 / frequency));
      if (!SetWaitableTimer(timer, &due, 0, nullptr, nullptr, FALSE))
        throw std::runtime_error("Arm latency timer");
      MsgWaitForMultipleObjectsEx(1, &timer, 1000, QS_ALLINPUT,
                                  MWMO_INPUTAVAILABLE);
    }
  }
};
static uint64_t latency_source_sequence = 0;
static LRESULT CALLBACK latency_source_proc(HWND window, UINT message,
                                            WPARAM wp, LPARAM lp) {
  if (message == WM_CLOSE) {
    DestroyWindow(window);
    return 0;
  }
  if (message == WM_DESTROY) {
    // The source loop exits on IsWindow; retain control to flush its trace.
    return 0;
  }
  if (message == WM_ERASEBKGND)
    return 1;
  if (message == WM_PAINT) {
    PAINTSTRUCT ps;
    HDC dc = BeginPaint(window, &ps);
    RECT r;
    GetClientRect(window, &r);
    auto phase = GetTickCount64();
    for (int y = 0; y < 12; ++y)
      for (int x = 0; x < 20; ++x) {
        HBRUSH b = CreateSolidBrush(
            RGB(40 + x * 7, 40 + y * 12, 90 + (x + y) % 6 * 20));
        RECT tile{x * r.right / 20, y * r.bottom / 12, (x + 1) * r.right / 20,
                  (y + 1) * r.bottom / 12};
        FillRect(dc, &tile, b);
        DeleteObject(b);
      }
    int x = int((phase / 4) % std::max(1L, r.right - 160));
    HBRUSH b = CreateSolidBrush(RGB(225, 210, 180));
    RECT moving{x, r.bottom / 4, x + 160, r.bottom * 3 / 4};
    FillRect(dc, &moving, b);
    DeleteObject(b);
    // Encode a changing source ID as visible black/white tiles.
    for (int bit = 0; bit < 24; ++bit) {
      RECT marker{bit * 8, 0, bit * 8 + 8, 8};
      FillRect(dc, &marker, static_cast<HBRUSH>(GetStockObject(
          (latency_source_sequence & (uint64_t(1) << bit)) ? WHITE_BRUSH : BLACK_BRUSH)));
    }
    EndPaint(window, &ps);
    return 0;
  }
  return DefWindowProcW(window, message, wp, lp);
}
extern "C" int spatpit_latency_source(unsigned rate, const wchar_t *output) {
  try {
    if (rate != 60 && rate != 120) return 0;
    std::vector<std::pair<uint64_t, int64_t>> updates;
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    WNDCLASSW wc{};
    wc.lpfnWndProc = latency_source_proc;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = L"SpatpitLatencySource";
    RegisterClassW(&wc);
    HWND w = CreateWindowExW(WS_EX_TOOLWINDOW, wc.lpszClassName,
                             L"Spatpit latency source", WS_POPUP, 0, 0, 2560,
                             1440, nullptr, nullptr, wc.hInstance, nullptr);
    if (!w)
      return 0;
    ShowWindow(w, SW_SHOWNOACTIVATE);
    LARGE_INTEGER f;
    QueryPerformanceFrequency(&f);
    LatencyTimer timer;
    auto begin = LatencyTrace::now(), next = begin;
    while (IsWindow(w) && LatencyTrace::now() - begin < f.QuadPart * 180) {
      timer.wait(next, f.QuadPart);
      if (!IsWindow(w)) break;
      next += f.QuadPart / rate;
      ++latency_source_sequence;
      InvalidateRect(w, nullptr, FALSE);
      UpdateWindow(w);
      GdiFlush();
      updates.emplace_back(latency_source_sequence, LatencyTrace::now());
      if (next < LatencyTrace::now() - f.QuadPart / rate)
        next = LatencyTrace::now();
    }
    if (output && *output) {
      std::ofstream out{std::filesystem::path(output)};
      if (!out) throw std::runtime_error("Open source timing trace");
      out << "source_id,update_done_qpc,qpc_frequency\n";
      for (auto &row : updates)
        out << row.first << "," << row.second << "," << f.QuadPart << "\n";
    }
    return 1;
  } catch (...) {
    return 0;
  }
}
struct LatencyChild {
  PROCESS_INFORMATION info{};
  HWND window = nullptr;
  ~LatencyChild() {
    if (window)
      PostMessageW(window, WM_CLOSE, 0, 0);
    if (info.hProcess) {
      if (WaitForSingleObject(info.hProcess, 3000) == WAIT_TIMEOUT)
        TerminateProcess(info.hProcess, 1);
      CloseHandle(info.hProcess);
      CloseHandle(info.hThread);
    }
  }
};
extern "C" int spatpit_latency_bench(const wchar_t *output,
                                    const wchar_t *runtime, unsigned rate,
                                    unsigned seconds, unsigned neural_enabled,
                                    unsigned source_rate) {
  std::ofstream log(std::filesystem::path(std::wstring(output) + L".log"));
  HWND window = nullptr;
  try {
    if ((rate != 60 && rate != 120) || seconds < 5 || seconds > 60 ||
        neural_enabled > 1 || (source_rate != 60 && source_rate != 120))
      throw std::runtime_error("Invalid latency options");
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    check(CoInitializeEx(nullptr, COINIT_MULTITHREADED), "Latency COM");
    LatencyChild child;
    wchar_t executable[32768];
    GetModuleFileNameW(nullptr, executable, 32768);
    std::wstring command =
        L"\"" + std::wstring(executable) + L"\" --latency-source " +
        std::to_wstring(source_rate) + L" \"" + std::wstring(output) + L".source.csv\"";
    STARTUPINFOW startup{};
    startup.cb = sizeof(startup);
    startup.dwFlags = STARTF_USESHOWWINDOW;
    startup.wShowWindow = SW_HIDE;
    if (!CreateProcessW(nullptr, command.data(), nullptr, nullptr, FALSE,
                        CREATE_NO_WINDOW, nullptr, nullptr, &startup,
                        &child.info))
      throw std::runtime_error("Start latency source");
    auto until = GetTickCount64() + 5000;
    while (!child.window && GetTickCount64() < until) {
      EnumWindows(
          [](HWND w, LPARAM data) -> BOOL {
            auto *child = reinterpret_cast<LatencyChild *>(data);
            DWORD pid = 0;
            GetWindowThreadProcessId(w, &pid);
            if (pid == child->info.dwProcessId && IsWindowVisible(w)) {
              child->window = w;
              return FALSE;
            }
            return TRUE;
          },
          reinterpret_cast<LPARAM>(&child));
      Sleep(10);
    }
    if (!child.window)
      throw std::runtime_error("Find latency source");
    WNDCLASSW wc{};
    wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = L"SpatpitLatencyOutput";
    RegisterClassW(&wc);
    window = CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW |
                                 WS_EX_TOPMOST | WS_EX_NOACTIVATE,
                             wc.lpszClassName, L"Overlay latency benchmark",
                             WS_POPUP, 0, 0, 2560, 1440, nullptr, nullptr,
                             wc.hInstance, nullptr);
    if (!window)
      throw std::runtime_error("Create latency output");
    {
      Engine e;
      e.init(window, runtime);
      ShowWindow(window, SW_SHOWNOACTIVATE);
      e.capture(child.window, false);
      auto arrivals = std::make_shared<std::atomic<uint64_t>>(0);
      auto arrival_token = e.pool.FrameArrived([arrivals](auto const &, auto const &) {
        arrivals->fetch_add(1, std::memory_order_relaxed);
      });
      SpatpitOptions image{};
      image.saturation = image.contrast = 1;
      image.reshade = 1;
      SpatpitNrOptions nr{};
      nr.enabled = neural_enabled;
      nr.motion_backend = 1;
      nr.passes = 1;
      nr.model_scale = .5f;
      nr.style = 0;
      LatencyTimer timer;
      LARGE_INTEGER f;
      QueryPerformanceFrequency(&f);
      bool preset = false, configured = false;
      auto warm = LatencyTrace::now(), next = warm;
      while (LatencyTrace::now() - warm < f.QuadPart * 10) {
        timer.wait(next, f.QuadPart);
        next = LatencyTrace::now() + f.QuadPart / rate;
        if (e.effects && !preset) {
          e.effects->set_current_preset_path(
              (std::filesystem::path(runtime) / L"SpatpitNeural.ini")
                  .string()
                  .c_str());
          preset = true;
        }
        if (e.effects &&
            e.effects->find_technique("lumenite_Kernel.fx", "Lumenite_Kernel")
                .handle &&
            !configured) {
          e.effects->enumerate_techniques(nullptr, [](auto *r, auto t) {
            r->set_technique_state(t, false);
          });
          spatpit_nr_configure(&e, &nr);
          image.neural = neural_enabled;
          configured = true;
        }
        e.render(2560, 1440, 1, nullptr, 0, nullptr, 0, nullptr, 0, image);
      }
      if (!configured ||
          (neural_enabled && (!e.nr || !e.nr->get_status().active)) ||
          e.frames < 60)
        throw std::runtime_error("Latency pipeline not ready");
      auto initial_evaluations = e.nr ? e.nr->get_status().evaluations : 0;
      LatencyTrace trace(e.swap.Get());
      e.latency = &trace;
      auto initial_frames = e.frames;
      auto initial_arrivals = arrivals->load();
      auto started = LatencyTrace::now();
      next = started;
      while (LatencyTrace::now() - started < f.QuadPart * seconds) {
        timer.wait(next, f.QuadPart);
        next = LatencyTrace::now() + f.QuadPart / rate;
        e.render(2560, 1440, 1, nullptr, 0, nullptr, 0, nullptr, 0, image);
      }
      auto ended = LatencyTrace::now();
      auto measured_arrivals = arrivals->load() - initial_arrivals;
      e.pool.FrameArrived(arrival_token);
      e.latency = nullptr;
      for (int i = 0; i < 20; ++i) {
        Sleep(5);
        trace.poll();
      }
      trace.write(output);
      auto status = e.nr ? e.nr->get_status() : SpatpitNrStatus{};
      if ((neural_enabled && !status.active) ||
          e.frames - initial_frames < seconds * 10)
        throw std::runtime_error(
            "Live capture/neural processing stalled during measurement");
      if (!neural_enabled && status.evaluations)
        throw std::runtime_error(
            "Neural-off benchmark unexpectedly evaluated the model");
      auto observed = std::count_if(trace.rows.begin(), trace.rows.end(),
                                    [](auto &r) { return r.displayed != 0; });
      if (!observed)
        throw std::runtime_error(
            "No DXGI presentation statistics available; see raw trace");
      log << "PASS: live WGC benchmark; rate=" << rate << " seconds=" << seconds
          << " source_rate=" << source_rate
          << " elapsed_seconds=" << double(ended - started) / f.QuadPart
          << " arrival_events=" << measured_arrivals
          << " interval_before_100ns=" << e.capture_interval_before
          << " interval_after_100ns=" << e.capture_interval_after
          << " renders=" << trace.rows.size() << " source_frames=" << e.frames
          << " neural=" << neural_enabled
          << " evaluations=" << status.evaluations - initial_evaluations
          << " model=" << status.width << 'x' << status.height
          << " passes=1 backend=1 style=0 scale=0.5 protection=1\n"
          << "desktop=" << GetSystemMetrics(SM_CXSCREEN) << 'x'
          << GetSystemMetrics(SM_CYSCREEN)
          << " measured_source_frames=" << e.frames - initial_frames
          << " observed_presents=" << observed << " refresh_hz="
          << (trace.refresh ? double(trace.frequency) / trace.refresh : 0)
          << " stats_hresult=" << std::hex << unsigned(trace.stats_result)
          << '\n';
    }
    DestroyWindow(window);
    return 1;
  } catch (...) {
    log << "FAIL: " << exception_message() << '\n';
    if (window)
      DestroyWindow(window);
    return 0;
  }
}
