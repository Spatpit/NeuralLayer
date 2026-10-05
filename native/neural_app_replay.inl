// Diagnostic replay through the same Engine/ReShade/NR path used by the app.
// Inject decoded frames at the capture boundary; do not copy backend code.
#include "neural_replay_input.inl"
#include "neural_replay_tap.inl"
#include "neural_resize_probe.inl"
#include <shellapi.h>
extern "C" int spatpit_replay_app(
    const wchar_t *inputName, const wchar_t *outputName, unsigned w, unsigned h,
    const wchar_t *runtime, unsigned backend, unsigned style, int protection,
    int dumpMotion, unsigned renderRate, float modelScale, int zeroMotion,
    const wchar_t *motionInputName, unsigned passes, unsigned resizeCycles) {
  HWND window = nullptr;
  std::ofstream log;
  try {
    if (w < 64 || h < 64 || w > 3840 || h > 2160 || backend > 1 || style > 3)
      throw std::runtime_error("Invalid app replay options");
    if (passes < 1 || passes > 3)
      throw std::runtime_error("Replay passes must be 1, 2 or 3");
    if (resizeCycles > 8 ||
        (resizeCycles && (dumpMotion || (motionInputName && *motionInputName))))
      throw std::runtime_error(
          "Resize replay supports 1-8 cycles without motion dumps/inputs");
    if (renderRate != 60 && renderRate != 90 && renderRate != 120)
      throw std::runtime_error(
          "Replay render rate must be 60, 90 or 120 for a 60 FPS source");
    if (!std::isfinite(modelScale) || modelScale < .25f || modelScale > 1.f)
      throw std::runtime_error("Replay model scale must be between 0.25 and 1");
    if (zeroMotion && backend != 1)
      throw std::runtime_error(
          "Zero-motion diagnostic requires the custom backend");
    std::filesystem::path inputPath(inputName), outputPath(outputName);
    bool suppliedMotion = motionInputName && *motionInputName;
    if (suppliedMotion && (backend != 1 || zeroMotion))
      throw std::runtime_error(
          "Motion input requires custom backend without zero-motion mode");
    auto logPath = outputPath;
    logPath += L".log";
    auto timingPath = outputPath;
    timingPath += L".gpu.csv";
    auto memoryPath = outputPath;
    memoryPath += L".memory.csv";
    for (const auto &path : {outputPath, logPath, timingPath, memoryPath})
      if (std::filesystem::exists(path) &&
          std::filesystem::equivalent(inputPath, path))
        throw std::runtime_error("Replay output would overwrite input");
    auto motionPath = outputPath;
    motionPath += L".motion.f32";
    if (dumpMotion && std::filesystem::exists(motionPath) &&
        std::filesystem::equivalent(inputPath, motionPath))
      throw std::runtime_error("Motion output would overwrite input");
    if (suppliedMotion)
      for (const auto &path : {outputPath, logPath, timingPath, motionPath})
        if (std::filesystem::exists(path) &&
            std::filesystem::equivalent(motionInputName, path))
          throw std::runtime_error("Replay would overwrite motion input");
    auto bytes = std::filesystem::file_size(inputPath);
    auto frameBytes = uint64_t(w) * h * 4;
    if (!bytes || bytes % frameBytes)
      throw std::runtime_error("Replay requires complete RGBA frames");
    log.open(logPath);
    if (!log)
      throw std::runtime_error("Open replay log");
    check(CoInitializeEx(nullptr, COINIT_MULTITHREADED),
          "Initialize replay COM");
    WNDCLASSW wc{};
    wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = L"SpatpitReplay";
    if (!RegisterClassW(&wc) && GetLastError() != ERROR_CLASS_ALREADY_EXISTS)
      throw std::runtime_error("Register replay window");
    window =
        CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
                        wc.lpszClassName, L"Neural comparison replay", WS_POPUP,
                        0, 0, w, h, nullptr, nullptr, wc.hInstance, nullptr);
    if (!window)
      throw std::runtime_error("Create replay window");
    {
      Engine e;
      e.init(window, runtime);
      SpatpitOptions imageOptions{};
      imageOptions.saturation = imageOptions.contrast = 1;
      imageOptions.reshade = 1;
      auto pump = [&] {
        MSG msg{};
        while (PeekMessageW(&msg, nullptr, 0, 0, PM_REMOVE)) {
          TranslateMessage(&msg);
          DispatchMessageW(&msg);
        }
      };
      // Shader compilation is asynchronous. Wait on the real runtime rather
      // than accepting a silent passthrough as a successful Lumenite replay.
      auto deadline = GetTickCount64() + 30000;
      bool presetSet = false, ready = false;
      while (GetTickCount64() < deadline) {
        pump();
        e.render(w, h, 1, nullptr, 0, nullptr, 0, nullptr, 0, imageOptions);
        if (e.effects && !presetSet) {
          auto preset =
              (std::filesystem::path(runtime) / L"SpatpitNeural.ini").string();
          e.effects->set_current_preset_path(preset.c_str());
          presetSet = true;
        }
        if (e.effects &&
            e.effects->find_technique("lumenite_Kernel.fx", "Lumenite_Kernel")
                .handle &&
            e.effects->find_texture_variable("lumenite_Kernel.fx", "tFlow")
                .handle) {
          ready = true;
          break;
        }
        Sleep(10);
      }
      if (!ready)
        throw std::runtime_error("Lumenite runtime did not become ready");
      // Let startup notifications expire before comparing pixels or timings.
      auto settle = GetTickCount64() + 5500;
      while (GetTickCount64() < settle) {
        pump();
        e.render(w, h, 1, nullptr, 0, nullptr, 0, nullptr, 0, imageOptions);
        Sleep(10);
      }
      e.effects->enumerate_techniques(
          nullptr, [&](auto *r, auto t) { r->set_technique_state(t, false); });
      SpatpitNrOptions options{};
      options.motion_backend = backend;
      options.motion_protection = protection != 0;
      options.style = style;
      options.model_scale = modelScale;
      options.passes = passes;
      options.lighting_stability = wcsstr(GetCommandLineW(), L"--no-lighting-stability") ? 0 : 1;
      int argc = 0;
      auto argv = CommandLineToArgvW(GetCommandLineW(), &argc);
      if (!argv) throw std::runtime_error("Read replay arguments");
      bool invalidParameter = false;
      for (int i = 1; i < argc; ++i) {
        float *parameter = nullptr;
        if (wcscmp(argv[i], L"--nr-intensity") == 0) parameter = &options.intensity;
        if (wcscmp(argv[i], L"--nr-structure") == 0) parameter = &options.structure;
        if (wcscmp(argv[i], L"--nr-tone") == 0) parameter = &options.tone;
        if (!parameter) continue;
        if (++i >= argc) { invalidParameter = true; break; }
        wchar_t *end = nullptr;
        float value = wcstof(argv[i], &end);
        if (end == argv[i] || *end || !std::isfinite(value) || value < 0 || value > 2) {
          invalidParameter = true;
          break;
        }
        *parameter = value;
      }
      LocalFree(argv);
      if (invalidParameter) throw std::runtime_error("Replay NR parameters require a number from 0 to 2");
      log << "NR intensity=" << options.intensity << " structure=" << options.structure
          << " tone=" << options.tone << '\n';
      spatpit_nr_configure(&e, &options);
      imageOptions.neural = 1;
      std::unique_ptr<ReplayMotionTap> motionTap;
      std::unique_ptr<ReplayMotionInput> motionInput;
      if (suppliedMotion)
        motionInput = std::make_unique<ReplayMotionInput>(
            e, motionInputName, w, h, bytes / frameBytes);
      if (dumpMotion) {
        motionTap = std::make_unique<ReplayMotionTap>(e, motionPath);
      }
      if (motionTap || motionInput) {
        e.replay_motion_hook = [&](auto *list, auto *guide) {
          if (motionInput)
            guide = motionInput->record(list, guide);
          if (motionTap)
            motionTap->record(list, guide);
          return guide;
        };
      }
      ComPtr<ID3D12QueryHeap> queries;
      D3D12_QUERY_HEAP_DESC qd{};
      qd.Type = D3D12_QUERY_HEAP_TYPE_TIMESTAMP;
      qd.Count = 10;
      check(e.device->CreateQueryHeap(&qd, IID_PPV_ARGS(&queries)),
            "Replay timestamps");
      ComPtr<ID3D12Resource> ticks;
      auto hp = heap(D3D12_HEAP_TYPE_READBACK);
      auto bd = buffer_desc(80);
      check(e.device->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &bd,
                                              D3D12_RESOURCE_STATE_COPY_DEST,
                                              nullptr, IID_PPV_ARGS(&ticks)),
            "Replay timing buffer");
      uint64_t frequency;
      check(e.queue->GetTimestampFrequency(&frequency),
            "Replay timestamp frequency");
      unsigned stamped = 0;
      e.profile_stamp = [&](auto *list, unsigned index) {
        list->EndQuery(queries.Get(), D3D12_QUERY_TYPE_TIMESTAMP, index);
        stamped |= 1u << index;
      };
      auto registerEvent =
          reinterpret_cast<void (*)(void *, reshade::addon_event, void *)>(
              GetProcAddress(e.reshade_module, "ReShadeRegisterEventForAddon"));
      if (!registerEvent)
        throw std::runtime_error("Replay profiling event registration");
      using ProfileEvent =
          void (*)(reshade::api::effect_runtime *, reshade::api::command_list *,
                   reshade::api::resource_view, reshade::api::resource_view);
      ProfileEvent beginEffects = [](auto *, auto *list, auto, auto) {
        if (active_engine)
          active_engine->stamp(
              reinterpret_cast<ID3D12GraphicsCommandList *>(list->get_native()),
              2);
      };
      ProfileEvent finishEffects = [](auto *, auto *list, auto, auto) {
        if (active_engine)
          active_engine->stamp(
              reinterpret_cast<ID3D12GraphicsCommandList *>(list->get_native()),
              3);
      };
      registerEvent(GetModuleHandleW(nullptr),
                    reshade::addon_event::reshade_begin_effects,
                    reinterpret_cast<void *>(beginEffects));
      registerEvent(GetModuleHandleW(nullptr),
                    reshade::addon_event::reshade_finish_effects,
                    reinterpret_cast<void *>(finishEffects));
      std::ofstream timing(timingPath);
      if (!timing)
        throw std::runtime_error("Open replay timings");
      timing << "frame,image_ms,reshade_ms,motion_ms,neural_ms,protection_ms,"
                "total_ms,source_frame,fresh,phase,phase_frame,model_scale\n";
      std::unique_ptr<ReplayResizeProbe> resizeProbe;
      if (resizeCycles)
        resizeProbe = std::make_unique<ReplayResizeProbe>(e, memoryPath);
      std::ifstream input(inputPath, std::ios::binary);
      std::ofstream output(outputPath, std::ios::binary);
      if (!input || !output)
        throw std::runtime_error("Open replay frames");
      std::vector<uint8_t> rgba(frameBytes), pixels(frameBytes), freshPixels;
      uint64_t count = 0, rendered = 0, cachedFrames = 0;
      for (unsigned phase = 0; phase <= resizeCycles * 2; ++phase) {
        auto phaseStart = count;
        input.clear();
        input.seekg(0);
        if (resizeCycles) {
          options.model_scale = phase % 2 ? 1.f : .5f;
          spatpit_nr_configure(&e, &options);
          // Match the interactive debounce, then let the next actual frame
          // rebuild through Engine::render/prepare after its normal fence.
          Sleep(450);
        }
        while (input.read(reinterpret_cast<char *>(rgba.data()), rgba.size())) {
          pump();
          e.texture(1, w, h, rgba.data());
          e.captured = e.textures.at(1);
          e.source_width = w;
          e.source_height = h;
          auto list = e.begin(0);
          transition(list, e.captured.resource.Get(),
                     D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                     D3D12_RESOURCE_STATE_COMMON);
          e.execute(0);
          e.sync();
          e.device->CopyDescriptorsSimple(
              1, e.srv(0), e.srv(e.captured.slot),
              D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
          e.has_frame = true;
          ++e.frames;
          // Diagnostic isolation: reset only the custom guide on a new capture,
          // keeping neural-model history and repeated-draw behavior intact.
          if (zeroMotion && e.motion)
            e.motion->reset();
          // Simulate overlay draws between 60 Hz capture arrivals, without
          // inventing/interpolating source motion or claiming real-time pacing.
          auto draws = ((count + 1) * renderRate + 59) / 60 -
                       (count * renderRate + 59) / 60;
          for (uint64_t draw = 0; draw < draws; ++draw) {
            stamped = 0;
            e.render(w, h, 1, nullptr, 0, nullptr, 0, nullptr, 0, imageOptions);
            if (motionTap)
              motionTap->write();
            if (stamped != 1023)
              throw std::runtime_error("Incomplete replay GPU timestamps");
            list = e.begin(0);
            list->ResolveQueryData(queries.Get(), D3D12_QUERY_TYPE_TIMESTAMP, 0,
                                   10, ticks.Get(), 0);
            e.execute(0);
            e.sync();
            uint64_t *times;
            check(ticks->Map(0, nullptr, reinterpret_cast<void **>(&times)),
                  "Map replay timestamps");
            double totalMs = 0;
            timing << rendered;
            for (unsigned j = 0; j < 10; j += 2) {
              double ms = double(times[j + 1] - times[j]) * 1000 / frequency;
              totalMs += ms;
              timing << ',' << ms;
            }
            timing << ',' << totalMs << ',' << count << ',' << (draw == 0)
                   << ',' << phase << ',' << (count - phaseStart) << ','
                   << options.model_scale << '\n';
            ticks->Unmap(0, nullptr);
            auto status = e.nr->get_status();
            if (resizeCycles &&
                (status.pending ||
                 status.width !=
                     std::max(32u, unsigned(w * options.model_scale) & ~7u) ||
                 status.height !=
                     std::max(32u, unsigned(h * options.model_scale) & ~7u) ||
                 status.builds != phase + 1))
              throw std::runtime_error("Resize replay did not apply expected "
                                       "model dimensions/build");
            if (rendered == 0) {
              ID3D12Resource *guide = e.motion ? e.motion->output() : nullptr;
              if (backend == 0) {
                auto texture = e.effects->find_texture_variable(
                    "lumenite_Kernel.fx", "tFlow");
                reshade::api::resource_view view{};
                e.effects->get_texture_binding(texture, &view, nullptr);
                guide = reinterpret_cast<ID3D12Resource *>(
                    e.effects->get_device()
                        ->get_resource_from_view(view)
                        .handle);
              }
              if (!guide)
                throw std::runtime_error("Missing replay motion guide");
              auto desc = guide->GetDesc();
              log << "Guide=" << desc.Width << 'x' << desc.Height
                  << " format=" << desc.Format << " model=" << status.width
                  << 'x' << status.height << '\n';
            }
            auto expectedEvaluations = backend == 1 ? count + 1 : rendered + 1;
            if (!e.error.empty() || status.evaluations != expectedEvaluations)
              throw std::runtime_error("Replay evaluation failed: " + e.error +
                                       " " + status.message);
            if (backend == 0 &&
                !e.effects->get_technique_state(e.effects->find_technique(
                    "lumenite_Kernel.fx", "Lumenite_Kernel")))
              throw std::runtime_error("Lumenite kernel was not enabled");
            e.screenshot(pixels.data(), w, h);
            if (backend == 1 && renderRate > 60) {
              if (draw == 0)
                freshPixels = pixels;
              else {
                if (pixels != freshPixels)
                  throw std::runtime_error(
                      "Cached overlay draw changed rendered pixels");
                ++cachedFrames;
              }
              if (e.motion->dispatches() != count + 1)
                throw std::runtime_error(
                    "Overlay redraw advanced motion history");
            }
            // Resize mode retains timings/memory, avoiding many GB of repeated
            // raw video. Pixel readback still exercises cache identity checks.
            if (!resizeCycles)
              output.write(reinterpret_cast<const char *>(pixels.data()),
                           pixels.size());
            if (!output)
              throw std::runtime_error("Write replay frame");
            ++rendered;
          }
          ++count;
          if (count % 30 == 0) {
            log << "Frames=" << count
                << " evaluations=" << e.nr->get_status().evaluations << "\n";
            log.flush();
          }
        }
        if (input.gcount() != 0 || count - phaseStart != bytes / frameBytes)
          throw std::runtime_error("Truncated replay frame");
        if (resizeProbe) {
          e.sync();
          resizeProbe->sample(phase, options.model_scale, *e.nr);
          log << "Resize phase=" << phase << " scale=" << options.model_scale
              << " model=" << e.nr->get_status().width << 'x'
              << e.nr->get_status().height << '\n';
          log.flush();
        }
      }
      output.flush();
      timing.flush();
      if (!output || !timing)
        throw std::runtime_error("Flush replay output");
      e.profile_stamp = nullptr;
      e.replay_motion_hook = nullptr;
      if (motionTap) {
        motionTap->file.flush();
        if (!motionTap->file)
          throw std::runtime_error("Flush replay motion");
      }
      log << "PASS: app pipeline frames=" << count << " size=" << w << "x" << h
          << " backend=" << backend << " style=" << style
          << " protection=" << protection << " render_rate=" << renderRate
          << " renders=" << rendered << " cached=" << cachedFrames
          << " evaluations=" << e.nr->get_status().evaluations
          << " model_scale=" << modelScale << " zero_motion=" << zeroMotion
          << " supplied_motion=" << suppliedMotion << "\n";
      log << "Neural passes=" << passes << "\n";
    }
    DestroyWindow(window);
    return 1;
  } catch (...) {
    auto message = exception_message();
    if (log)
      log << "FAIL: " << message << "\n";
    if (window)
      DestroyWindow(window);
    return 0;
  }
}
