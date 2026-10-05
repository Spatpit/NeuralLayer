// Diagnostic color-effect replay through the production capture/composition path.
// No neural model, game injection, or changes to the installed user's presets.
extern "C" int spatpit_replay_reshade(const wchar_t *input_name,
    const wchar_t *output_name, unsigned w, unsigned h,
    const wchar_t *runtime, const wchar_t *preset_name) {
  HWND window = nullptr;
  std::ofstream log;
  try {
    if (w < 64 || h < 64 || w > 3840 || h > 2160)
      throw std::runtime_error("Invalid ReShade replay size");
    std::filesystem::path input_path(input_name), output_path(output_name), preset(preset_name);
    auto log_path = output_path; log_path += L".log";
    for (const auto &path : {output_path, log_path})
      for (const auto &source : {input_path, preset})
        if (std::filesystem::exists(path) && std::filesystem::equivalent(path, source))
          throw std::runtime_error("Replay output would overwrite an input");
    const uint64_t frame_bytes = uint64_t(w) * h * 4;
    auto bytes = std::filesystem::file_size(input_path);
    if (!bytes || bytes % frame_bytes)
      throw std::runtime_error("Expected complete RGBA input frames");
    std::ifstream ini(preset);
    std::vector<std::string> wanted;
    for (std::string line; std::getline(ini, line);) {
      if (line.rfind("Techniques=", 0) != 0) continue;
      auto value = line.substr(11);
      while (!value.empty() && (value.back() == '\r' || value.back() == ' ')) value.pop_back();
      for (size_t start = 0; start < value.size();) {
        auto end = value.find(',', start);
        auto item = value.substr(start, end == std::string::npos ? end : end - start);
        wanted.push_back(item);
        if (end == std::string::npos) break;
        start = end + 1;
      }
      break;
    }
    if (wanted.empty()) throw std::runtime_error("Preset has no enabled techniques");
    log.open(log_path);
    if (!log) throw std::runtime_error("Open ReShade replay log");
    check(CoInitializeEx(nullptr, COINIT_MULTITHREADED), "Initialize replay COM");
    WNDCLASSW wc{};
    wc.lpfnWndProc = DefWindowProcW; wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = L"SpatpitReShadeReplay";
    if (!RegisterClassW(&wc) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS)
      throw std::runtime_error("Register ReShade replay window");
    window = CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
        wc.lpszClassName, L"ReShade visual test", WS_POPUP, 0, 0, w, h,
        nullptr, nullptr, wc.hInstance, nullptr);
    if (!window) throw std::runtime_error("Create ReShade replay window");
    {
      Engine e; e.init(window, runtime);
      SpatpitOptions options{};
      options.saturation = options.contrast = 1;
      options.reshade = 1;
      auto pump = [] {
        MSG msg{};
        while (PeekMessageW(&msg, nullptr, 0, 0, PM_REMOVE)) {
          TranslateMessage(&msg); DispatchMessageW(&msg);
        }
      };
      auto find = [&](const std::string &item) {
        auto at = item.find('@');
        return e.effects->find_technique(at == std::string::npos ? nullptr : item.substr(at + 1).c_str(),
                                        item.substr(0, at).c_str());
      };
      bool set = false, ready = false;
      auto deadline = GetTickCount64() + 45000;
      while (GetTickCount64() < deadline) {
        pump(); e.render(w, h, 1, nullptr, 0, nullptr, 0, nullptr, 0, options);
        if (e.effects && !set) {
          e.effects->set_current_preset_path(preset.string().c_str()); set = true;
        }
        if (set) {
          ready = true;
          for (const auto &item : wanted) ready &= find(item).handle != 0;
          if (ready) break;
        }
        Sleep(10);
      }
      if (!ready) {
        if (e.effects) for (const auto &item : wanted)
          if (!find(item).handle) log << "Missing technique: " << item << '\n';
        throw std::runtime_error("Preset shaders did not compile/load; inspect ReShade.log");
      }
      // Allow the runtime's startup notification to expire before image checks.
      auto settle = GetTickCount64() + 5500;
      while (GetTickCount64() < settle) {
        pump(); e.render(w, h, 1, nullptr, 0, nullptr, 0, nullptr, 0, options); Sleep(10);
      }
      for (const auto &item : wanted) {
        if (!e.effects->get_technique_state(find(item)))
          throw std::runtime_error("Preset technique is disabled: " + item);
        log << "Enabled: " << item << '\n';
      }
      std::ifstream input(input_path, std::ios::binary);
      std::ofstream output(output_path, std::ios::binary);
      if (!input || !output) throw std::runtime_error("Open replay frame files");
      std::vector<uint8_t> rgba(frame_bytes), pixels(frame_bytes);
      uint64_t count = 0, changed = 0;
      double elapsed_ms = 0;
      while (input.read(reinterpret_cast<char *>(rgba.data()), rgba.size())) {
        auto start = std::chrono::steady_clock::now();
        pump(); e.texture(1, w, h, rgba.data());
        e.captured = e.textures.at(1); e.source_width = w; e.source_height = h;
        auto list = e.begin(0);
        transition(list, e.captured.resource.Get(), D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                   D3D12_RESOURCE_STATE_COMMON);
        e.execute(0); e.sync();
        e.device->CopyDescriptorsSimple(1, e.srv(0), e.srv(e.captured.slot), D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
        e.has_frame = true; ++e.frames;
        e.render(w, h, 1, nullptr, 0, nullptr, 0, nullptr, 0, options);
        if (!e.error.empty()) throw std::runtime_error(e.error);
        e.screenshot(pixels.data(), w, h);
        if (pixels != rgba) ++changed;
        output.write(reinterpret_cast<const char *>(pixels.data()), pixels.size());
        auto ms = std::chrono::duration<double, std::milli>(std::chrono::steady_clock::now() - start).count();
        if (count >= 30) elapsed_ms += ms;
        ++count;
        // Approximate the recording's 60 Hz cadence for time-dependent effects.
        if (ms < 16.667) Sleep(DWORD(16.667 - ms));
      }
      output.flush();
      if (input.gcount() || count != bytes / frame_bytes || !output || !changed)
        throw std::runtime_error("Truncated or unchanged ReShade replay");
      log << "PASS: frames=" << count << " enabled_techniques=" << wanted.size()
          << " changed_frames=" << changed << " neural=off\n";
      if (count > 30) log << "Mean upload/render/readback CPU wall ms=" << elapsed_ms / (count - 30) << '\n';
    }
    DestroyWindow(window); return 1;
  } catch (...) {
    if (log) log << "FAIL: " << exception_message() << '\n';
    if (window) DestroyWindow(window);
    return 0;
  }
}
