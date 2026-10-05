// Offline replay diagnostics only; no memory queries in interactive rendering.
#include <psapi.h>
#pragma comment(lib, "psapi.lib")
struct ReplayResizeProbe {
  ComPtr<IDXGIAdapter3> adapter;
  std::ofstream file;
  ReplayResizeProbe(Engine &engine, const std::filesystem::path &path)
      : file(path) {
    if (!file)
      throw std::runtime_error("Open resize memory report");
    ComPtr<IDXGIFactory4> factory;
    check(CreateDXGIFactory1(IID_PPV_ARGS(&factory)), "Resize probe factory");
    check(factory->EnumAdapterByLuid(engine.device->GetAdapterLuid(),
                                     IID_PPV_ARGS(&adapter)),
          "Resize probe adapter");
    file << "phase,scale,model_width,model_height,builds,evaluations,"
            "local_bytes,local_budget,nonlocal_bytes,private_bytes,working_"
            "set\n";
  }
  void sample(unsigned phase, float scale, const CaptureNr &nr) {
    DXGI_QUERY_VIDEO_MEMORY_INFO local{}, nonlocal{};
    check(adapter->QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_LOCAL,
                                        &local),
          "Query local GPU memory");
    check(adapter->QueryVideoMemoryInfo(0, DXGI_MEMORY_SEGMENT_GROUP_NON_LOCAL,
                                        &nonlocal),
          "Query nonlocal GPU memory");
    PROCESS_MEMORY_COUNTERS_EX process{};
    process.cb = sizeof(process);
    if (!GetProcessMemoryInfo(
            GetCurrentProcess(),
            reinterpret_cast<PROCESS_MEMORY_COUNTERS *>(&process),
            sizeof(process)))
      throw std::runtime_error("Query process memory");
    auto s = nr.get_status();
    file << phase << ',' << scale << ',' << s.width << ',' << s.height << ','
         << s.builds << ',' << s.evaluations << ',' << local.CurrentUsage << ','
         << local.Budget << ',' << nonlocal.CurrentUsage << ','
         << process.PrivateUsage << ',' << process.WorkingSetSize << '\n';
    file.flush();
    if (!file)
      throw std::runtime_error("Write resize memory report");
  }
};
