#pragma once
// Optional live benchmark instrumentation; disabled in ordinary rendering.
// WGC's timestamp is the compositor's frame time, which may be ahead of the
// dequeue QPC. Only the source-to-reported-presentation interval is used for
// latency comparisons; submit/dequeue deltas are diagnostics, not input lag.
struct LatencyTrace {
  static int64_t now() {
    LARGE_INTEGER t;
    QueryPerformanceCounter(&t);
    return t.QuadPart;
  }
  struct Row {
    uint64_t capture = 0;
    int64_t source = 0, dequeue = 0, start = 0, copied = 0;
    int64_t submit = 0, returned = 0, done = 0, displayed = 0;
    UINT present = 0;
  } current;
  std::vector<Row> rows;
  int64_t frequency = 0, refresh = 0;
  HRESULT stats_result = E_PENDING;
  ComPtr<IDXGISwapChain3> chain;
  explicit LatencyTrace(IDXGISwapChain3 *swap) {
    LARGE_INTEGER f;
    QueryPerformanceFrequency(&f);
    frequency = f.QuadPart;
    DWM_TIMING_INFO timing{};
    timing.cbSize = sizeof(timing);
    if (SUCCEEDED(DwmGetCompositionTimingInfo(nullptr, &timing)))
      refresh = timing.qpcRefreshPeriod;
    chain = swap;
    rows.reserve(4096);
  }
  void poll() {
    DXGI_FRAME_STATISTICS s{};
    stats_result = chain->GetFrameStatistics(&s);
    if (FAILED(stats_result) || !s.PresentCount || !s.SyncQPCTime.QuadPart ||
        !refresh)
      return;
    for (auto i = rows.rbegin(); i != rows.rend(); ++i)
      if (i->present == s.PresentCount) {
        auto delta =
            int64_t(s.PresentRefreshCount) - int64_t(s.SyncRefreshCount);
        i->displayed = s.SyncQPCTime.QuadPart + delta * refresh;
        break;
      }
  }
  void finish(IDXGISwapChain3 *swap) {
    current.present = 0;
    swap->GetLastPresentCount(&current.present);
    rows.push_back(current);
    poll();
  }
  void write(const std::filesystem::path &path) {
    std::ofstream out(path);
    if (!out)
      throw std::runtime_error("Open latency trace");
    out << "draw,capture,present,source_qpc,dequeue_qpc,start_qpc,copy_done_"
           "qpc,submit_qpc,return_qpc,gpu_done_qpc,display_qpc,qpc_frequency,"
           "refresh_qpc\n";
    for (size_t i = 0; i < rows.size(); ++i) {
      auto &r = rows[i];
      out << i << ',' << r.capture << ',' << r.present << ',' << r.source << ','
          << r.dequeue << ',' << r.start << ',' << r.copied << ',' << r.submit
          << ',' << r.returned << ',' << r.done << ',' << r.displayed << ','
          << frequency << ',' << refresh << '\n';
    }
  }
};
