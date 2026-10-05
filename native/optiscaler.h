#pragma once
// Spatpit's SDR capture host for the OptiScaler_DLSSNR forwarder/composition.
// The caller fences its queue before prepare(), resize, and destruction.
#include "DlssNr_Common.h"
#include "DlssNr_Shader.h"
#include "neural_compare.h"
#include "neural_detail.h"
#include "neural_lighting.h"
#include <array>
#include <d3dcompiler.h>
#include <cmath>
#include <filesystem>
#include <fstream>
#include <memory>

// Optional settings for the 2nd and 3rd neural passes. When `custom` is 0
// the pass follows the main pass settings.
struct SpatpitNrPassTuning {
  uint32_t custom = 0, style = 0;
  float model_scale = .5f, intensity = 1, blend = 1;
};
struct SpatpitNrOptions {
  uint32_t enabled = 1, preset = 0, style = 0, auto_mask = 0;
  float model_scale = .5f, intensity = 1, structure = 1, tone = 1, skin = -1;
  float blend = 1, colour = 1, max_ratio = 4;
  uint32_t transfer = 1, compare = 0, swap = 0, debug = 0;
  float split = .5f, zoom = 1, mv_x = 1, mv_y = 1;
  uint32_t motion_backend = 1, motion_protection = 0;
  uint32_t passes = 1, lighting_stability = 1;
  SpatpitNrPassTuning pass_tuning[2]{};
};
struct SpatpitNrStatus {
  uint64_t evaluations = 0, builds = 0;
  uint32_t width = 0, height = 0, pending = 0, active = 0;
  char message[384]{};
  uint64_t motion_frames = 0;
};
// The Rust side mirrors these layouts (src/optiscaler.rs); keep both in sync.
static_assert(sizeof(SpatpitNrPassTuning) == 20, "SpatpitNrPassTuning FFI layout");
static_assert(sizeof(SpatpitNrOptions) == 136, "SpatpitNrOptions FFI layout");
static_assert(sizeof(SpatpitNrStatus) == 424, "SpatpitNrStatus FFI layout");
// Comparison settings only change what is presented, never what is processed.
inline SpatpitNrOptions nr_processing_options(SpatpitNrOptions o) {
  o.compare = 0;
  o.swap = 0;
  o.split = 0;
  o.zoom = 1;
  return o;
}
inline bool nr_processing_changed(const SpatpitNrOptions &a, const SpatpitNrOptions &b) {
  auto x = nr_processing_options(a), y = nr_processing_options(b);
  return memcmp(&x, &y, sizeof(x)) != 0;
}

class CaptureNr {
  static void check(HRESULT hr, const char *action) {
    if (FAILED(hr)) {
      char text[300];
      sprintf_s(text, "%s (0x%08X)", action, unsigned(hr));
      throw std::runtime_error(text);
    }
  }
  static D3D12_HEAP_PROPERTIES heap(D3D12_HEAP_TYPE type) {
    D3D12_HEAP_PROPERTIES p{};
    p.Type = type;
    p.CreationNodeMask = p.VisibleNodeMask = 1;
    return p;
  }
  static D3D12_RESOURCE_DESC buffer_desc(uint64_t size) {
    D3D12_RESOURCE_DESC d{};
    d.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
    d.Width = std::max<uint64_t>(size, 256);
    d.Height = d.DepthOrArraySize = d.MipLevels = d.SampleDesc.Count = 1;
    d.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
    return d;
  }
  static void transition(ID3D12GraphicsCommandList *list,
                         ID3D12Resource *resource, D3D12_RESOURCE_STATES before,
                         D3D12_RESOURCE_STATES after) {
    D3D12_RESOURCE_BARRIER b{};
    b.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
    b.Transition = {resource, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, before,
                    after};
    list->ResourceBarrier(1, &b);
  }
  using Create = void *(*)(const wchar_t *, const wchar_t *, ID3D12Device *,
                           ID3D12GraphicsCommandList *, void *, unsigned,
                           unsigned, int, float, int, float, float, float, int,
                           int);
  using Evaluate = int (*)(ID3D12GraphicsCommandList *, void *, void *,
                           ID3D12Resource *, ID3D12Resource *, ID3D12Resource *,
                           ID3D12Resource *, unsigned, unsigned, unsigned,
                           unsigned, int, int, float, int, float, float, float,
                           int, float, float);
  Create create{};
  Evaluate evaluate{};
  void (*release)(void *){};
  int (*destroy_params)(void *){};
  HMODULE core{}, forwarder{};
  void *params{}, *feature{};
  ID3D12Device *device{};
  std::filesystem::path folder;
  SpatpitNrOptions desired{}, applied{};
  SpatpitNrStatus status{};
  uint64_t changed = 0;
  bool failed = false, reset = true, depth_clear = true, initialized = false,
       rebuild = false;
  uint32_t full_w = 0, full_h = 0, guide_w = 0, guide_h = 0, step = 0;
  ComPtr<ID3D12RootSignature> root;
  ComPtr<ID3D12PipelineState> pso;
  ComPtr<ID3D12PipelineState> compare_pso;
  ComPtr<ID3D12PipelineState> detail_pso, lighting_pso;
  std::array<ComPtr<ID3D12Resource>, 2> lighting_history;
  ComPtr<ID3D12Resource> lighting_output;
  unsigned lighting_index = 0;
  bool lighting_valid = false;
  std::array<std::unique_ptr<CaptureNr>, 2> extra;
  ComPtr<ID3D12DescriptorHeap> descriptors;
  ComPtr<ID3D12Resource> constants, source, original, proxy, model_input, model,
      output, depth;
  ComPtr<ID3D12CommandQueue> init_queue;
  ComPtr<ID3D12CommandAllocator> init_alloc;
  ComPtr<ID3D12GraphicsCommandList> init_list;
  ComPtr<ID3D12Fence> init_fence;
  uint64_t init_value = 0;
  HANDLE init_event{};
  void message(const std::string &s) {
    strncpy_s(status.message, s.c_str(), _TRUNCATE);
  }
  void log(const std::string &s) {
    std::ofstream(folder / "SpatpitOptiScaler.log", std::ios::app) << s << "\n";
  }
  static bool model_diff(const SpatpitNrOptions &a, const SpatpitNrOptions &b) {
    return a.preset != b.preset || a.style != b.style ||
           a.auto_mask != b.auto_mask || a.model_scale != b.model_scale ||
           a.intensity != b.intensity || a.structure != b.structure ||
           a.tone != b.tone || a.skin != b.skin;
  }
  template <class T> static T symbol(HMODULE m, const char *name) {
    auto p = reinterpret_cast<T>(GetProcAddress(m, name));
    if (!p)
      throw std::runtime_error(std::string("Missing neural export: ") + name);
    return p;
  }
  void initialize() {
    if (core)
      throw std::runtime_error("NGX initialization previously failed; reopen "
                               "this backend to retry initialization");
    // Driver-owned capability parameters include callbacks the model requires.
    wchar_t path[32768]{};
    DWORD bytes = sizeof(path);
    if (RegGetValueW(HKEY_LOCAL_MACHINE,
                     L"SYSTEM\\CurrentControlSet\\Services\\nvlddmkm\\NGXCore",
                     L"NGXPath", RRF_RT_REG_SZ, nullptr, path,
                     &bytes) != ERROR_SUCCESS)
      throw std::runtime_error("NVIDIA NGX driver path unavailable");
    auto core_path = std::filesystem::path(path) / L"_nvngx.dll";
    if (!std::filesystem::exists(core_path))
      core_path = std::filesystem::path(path) / L"nvngx.dll";
    core = LoadLibraryExW(core_path.c_str(), nullptr,
                          LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                              LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!core)
      throw std::runtime_error("Could not load NVIDIA NGX core");
    struct Paths {
      const wchar_t *const *path;
      unsigned length;
    };
    struct Logging {
      void (*callback)(const char *, int, int);
      int minimum;
      bool disable;
    };
    struct Common {
      Paths paths;
      void *internal;
      Logging logging;
    };
    const wchar_t *paths[] = {folder.c_str()};
    Common common{{paths, 1}, nullptr, {}};
    auto init =
        symbol<int (*)(unsigned long long, const wchar_t *, ID3D12Device *, int,
                       const Common *)>(core, "NVSDK_NGX_D3D12_Init_Ext");
    int result = init(0x24480451ull, folder.c_str(), device, 0x15, &common);
    if (result != 1) {
      char text[96];
      sprintf_s(text, "NGX initialization failed: 0x%08X", result);
      throw std::runtime_error(text);
    }
    destroy_params =
        symbol<int (*)(void *)>(core, "NVSDK_NGX_D3D12_DestroyParameters");
    if (symbol<int (*)(void **)>(
            core, "NVSDK_NGX_D3D12_GetCapabilityParameters")(&params) != 1 ||
        !params)
      throw std::runtime_error("NGX capability parameters unavailable");
    wchar_t exe[32768]{};
    GetModuleFileNameW(nullptr, exe, 32768);
    auto forwarder_path =
        std::filesystem::path(exe).parent_path() / L"nvngx.dll_dlssnr.dll";
    forwarder = LoadLibraryExW(forwarder_path.c_str(), nullptr,
                               LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                                   LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!forwarder)
      throw std::runtime_error(
          "Missing nvngx.dll_dlssnr.dll beside NeuralLayer.exe");
    create = symbol<Create>(forwarder, "dlssnr_call_create");
    evaluate = symbol<Evaluate>(forwarder, "dlssnr_call_evaluate");
    release = symbol<void (*)(void *)>(forwarder, "dlssnr_call_release");
    // This is the upstream driver's parameter ABI, verified by a round-trip.
    // No RenoDX private state or addresses are used.
    symbol<void (*)(void *, const char *, float, int)>(
        forwarder, "dlssnr_call_probe_float")(params, "Spatpit.FloatProbe",
                                              .375f, 6);
    float probe = 0;
    auto get_float = reinterpret_cast<int (*)(void *, const char *, float *)>(
        (*reinterpret_cast<void ***>(params))[14]);
    if (get_float(params, "Spatpit.FloatProbe", &probe) != 1 || probe != .375f)
      throw std::runtime_error("Unsupported NGX float parameter ABI");
    symbol<void (*)(int)>(forwarder, "dlssnr_call_set_float_slot")(6);
    D3D12_COMMAND_QUEUE_DESC q{};
    check(device->CreateCommandQueue(&q, IID_PPV_ARGS(&init_queue)),
          "Create neural initialization queue");
    check(device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                         IID_PPV_ARGS(&init_alloc)),
          "Create neural allocator");
    check(device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT,
                                    init_alloc.Get(), nullptr,
                                    IID_PPV_ARGS(&init_list)),
          "Create neural initialization list");
    check(init_list->Close(), "Close neural list");
    check(device->CreateFence(0, D3D12_FENCE_FLAG_NONE,
                              IID_PPV_ARGS(&init_fence)),
          "Create neural fence");
    init_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!init_event)
      throw std::runtime_error("Create neural wait event failed");
    D3D12_DESCRIPTOR_RANGE ranges[2] = {
        {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 5, 0, 0, 0},
        {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 2, 0, 0, 5}};
    D3D12_ROOT_PARAMETER rp[2]{};
    rp[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    rp[0].DescriptorTable = {2, ranges};
    rp[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_CBV;
    D3D12_STATIC_SAMPLER_DESC sampler{};
    sampler.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
    sampler.AddressU = sampler.AddressV = sampler.AddressW =
        D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
    sampler.MaxLOD = D3D12_FLOAT32_MAX;
    D3D12_ROOT_SIGNATURE_DESC rd{2, rp, 1, &sampler};
    ComPtr<ID3DBlob> blob, errors;
    check(D3D12SerializeRootSignature(&rd, D3D_ROOT_SIGNATURE_VERSION_1, &blob,
                                      &errors),
          "Serialize neural root");
    check(device->CreateRootSignature(0, blob->GetBufferPointer(),
                                      blob->GetBufferSize(),
                                      IID_PPV_ARGS(&root)),
          "Create neural root");
    D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = root.Get();
    pd.CS = {DlssNr_cso, sizeof(DlssNr_cso)};
    check(device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pso)),
          "Create OptiScaler composition shader");
    D3D12_DESCRIPTOR_HEAP_DESC hd{D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 56,
                                  D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, 0};
    check(device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&descriptors)),
          "Create neural descriptors");
    step = device->GetDescriptorHandleIncrementSize(hd.Type);
    auto hp = heap(D3D12_HEAP_TYPE_UPLOAD);
    auto bd = buffer_desc(2048);
    check(device->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &bd,
                                          D3D12_RESOURCE_STATE_GENERIC_READ,
                                          nullptr, IID_PPV_ARGS(&constants)),
          "Create neural constants");
    log("OptiScaler NR initialized; verified NGX float ABI");
    initialized = true;
  }
  ComPtr<ID3D12Resource> texture(unsigned w, unsigned h, DXGI_FORMAT format) {
    D3D12_RESOURCE_DESC d{};
    d.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    d.Width = w;
    d.Height = h;
    d.DepthOrArraySize = d.MipLevels = d.SampleDesc.Count = 1;
    d.Format = format;
    d.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    auto hp = heap(D3D12_HEAP_TYPE_DEFAULT);
    ComPtr<ID3D12Resource> t;
    check(device->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &d,
                                          D3D12_RESOURCE_STATE_COMMON, nullptr,
                                          IID_PPV_ARGS(&t)),
          "Allocate neural texture");
    return t;
  }
  D3D12_CPU_DESCRIPTOR_HANDLE cpu(unsigned n) {
    auto h = descriptors->GetCPUDescriptorHandleForHeapStart();
    h.ptr += n * step;
    return h;
  }
  D3D12_GPU_DESCRIPTOR_HANDLE gpu(unsigned n) {
    auto h = descriptors->GetGPUDescriptorHandleForHeapStart();
    h.ptr += n * step;
    return h;
  }
  void srv(ID3D12Resource *r, unsigned n) {
    D3D12_SHADER_RESOURCE_VIEW_DESC d{};
    d.Format = r->GetDesc().Format;
    d.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    d.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    d.Texture2D.MipLevels = 1;
    device->CreateShaderResourceView(r, &d, cpu(n));
  }
  void uav(ID3D12Resource *r, unsigned n) {
    D3D12_UNORDERED_ACCESS_VIEW_DESC d{};
    d.Format = r->GetDesc().Format;
    d.ViewDimension = D3D12_UAV_DIMENSION_TEXTURE2D;
    device->CreateUnorderedAccessView(r, nullptr, &d, cpu(n));
  }
  void dispatch(ID3D12GraphicsCommandList *l, unsigned slot,
                const DlssNrConstants &c, ID3D12Resource *src,
                ID3D12Resource *mdl, ID3D12Resource *orig,
                ID3D12Resource *motion, ID3D12Resource *out,
                ID3D12Resource *keep = nullptr,
                ID3D12PipelineState *pipeline = nullptr) {
    unsigned n = slot * 7;
    srv(src, n);
    srv(mdl ? mdl : src, n + 1);
    srv(orig ? orig : src, n + 2);
    srv(motion ? motion : src, n + 3);
    srv(src, n + 4);
    uav(out, n + 5);
    uav(keep ? keep : out, n + 6);
    void *p{};
    check(constants->Map(0, nullptr, &p), "Map neural constants");
    memcpy(static_cast<char *>(p) + slot * 256, &c, sizeof(c));
    constants->Unmap(0, nullptr);
    ID3D12DescriptorHeap *heaps[] = {descriptors.Get()};
    l->SetDescriptorHeaps(1, heaps);
    l->SetComputeRootSignature(root.Get());
    l->SetPipelineState(pipeline ? pipeline : pso.Get());
    l->SetComputeRootDescriptorTable(0, gpu(n));
    l->SetComputeRootConstantBufferView(1, constants->GetGPUVirtualAddress() +
                                               slot * 256);
    l->Dispatch((c.Width + 7) / 8, (c.Height + 7) / 8, 1);
  }

  void release_features() {
    // Only called after the host frame fence. Retire the whole chain before
    // creating any replacement: overlapping old/new model sizes can keep the
    // runtime's larger shared allocations alive after a downsize.
    for (auto &pass : extra)
      if (pass) pass->release_features();
    if (feature && release) {
      release(feature);
      feature = nullptr;
    }
  }

  SpatpitNrOptions pass_options(unsigned i) const {
    auto o = desired;
    o.passes = 1;
    o.compare = 0;
    o.lighting_stability = 0;
    if (i + 2 != desired.passes) o.debug = 0;
    if (i < 2 && desired.pass_tuning[i].custom) {
      const auto &t = desired.pass_tuning[i];
      o.style = t.style;
      o.model_scale = t.model_scale;
      o.intensity = t.intensity;
      o.blend = t.blend;
    }
    // A pass never reads later passes' tuning; keep it out of its own
    // change detection.
    for (auto &t : o.pass_tuning) t = SpatpitNrPassTuning{};
    return o;
  }
  void prepare_comparison() {
    ComPtr<ID3DBlob> code, errors;
    check(D3DCompile(neural_compare_shader, strlen(neural_compare_shader),
                     "neural_compare", nullptr, nullptr, "main", "cs_5_0",
                     D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &code, &errors),
          "Compile multi-pass comparison");
    D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = root.Get();
    pd.CS = {code->GetBufferPointer(), code->GetBufferSize()};
    check(device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&compare_pso)),
          "Create multi-pass comparison");
  }
  void prepare_detail() {
    ComPtr<ID3DBlob> code, errors;
    check(D3DCompile(neural_detail_shader, strlen(neural_detail_shader),
                     "neural_detail", nullptr, nullptr, "main", "cs_5_0",
                     D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &code, &errors),
          "Compile detail preservation");
    D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = root.Get();
    pd.CS = {code->GetBufferPointer(), code->GetBufferSize()};
    check(device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&detail_pso)),
          "Create detail preservation");
  }
  bool lighting_enabled() const {
    return desired.motion_backend == 1 && desired.lighting_stability &&
           desired.enabled && desired.blend > 0 && desired.debug == 0;
  }
  static unsigned runtime_style(const SpatpitNrOptions &o) {
    // Balanced is our composition mode, never an undocumented vendor style.
    return o.style == 3 ? 1 : o.style;
  }
  static float runtime_intensity(const SpatpitNrOptions &o) {
    // Balanced uses a gentler Natural evaluation for an intermediate look.
    // Keep the user's slider value intact; apply the mapping once at the API.
    return o.style == 3 ? o.intensity * .5f : o.intensity;
  }
  void prepare_lighting() {
    if (!lighting_pso) {
      ComPtr<ID3DBlob> code, errors;
      auto hr =
          D3DCompile(neural_lighting_shader, strlen(neural_lighting_shader),
                     "neural_lighting", nullptr, nullptr, "main", "cs_5_0",
                     D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &code, &errors);
      if (FAILED(hr) && errors)
        throw std::runtime_error(
            static_cast<const char *>(errors->GetBufferPointer()));
      check(hr, "Compile lighting stability");
      D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
      pd.pRootSignature = root.Get();
      pd.CS = {code->GetBufferPointer(), code->GetBufferSize()};
      check(
          device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&lighting_pso)),
          "Create lighting stability");
    }
    if (!lighting_output) {
      for (auto &image : lighting_history)
        image = texture((full_w + 3) / 4, (full_h + 3) / 4,
                        DXGI_FORMAT_R16G16B16A16_FLOAT);
      lighting_output = texture(full_w, full_h, DXGI_FORMAT_R8G8B8A8_UNORM);
      lighting_valid = false;
    }
  }
  void stabilize_lighting(ID3D12GraphicsCommandList *l, ID3D12Resource *back,
                          ID3D12Resource *motion, ID3D12Resource *final_output) {
    constexpr auto read = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
    constexpr auto write = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    auto *previous = lighting_history[lighting_index].Get();
    auto *current = lighting_history[1 - lighting_index].Get();
    for (auto *r : {source.Get(), final_output, previous})
      transition(l, r, D3D12_RESOURCE_STATE_COMMON, read);
    transition(l, current, D3D12_RESOURCE_STATE_COMMON, write);
    DlssNrConstants c{};
    c.Mode = 0;
    c.WhitePoint = lighting_valid ? 1.f : 0.f;
    c.Width = (full_w + 3) / 4;
    c.Height = (full_h + 3) / 4;
    c.TransferStrength = desired.style == 3 ? 1.f : 0.f;
    dispatch(l, 6, c, final_output, previous, source.Get(), motion, current,
             nullptr, lighting_pso.Get());
    transition(l, current, write, read);
    transition(l, lighting_output.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    c.Mode = 1;
    c.Width = full_w;
    c.Height = full_h;
    dispatch(l, 7, c, final_output, current, source.Get(), motion,
             lighting_output.Get(), nullptr, lighting_pso.Get());
    transition(l, lighting_output.Get(), write, D3D12_RESOURCE_STATE_COMMON);
    for (auto *r : {source.Get(), final_output, previous, current})
      transition(l, r, read, D3D12_RESOURCE_STATE_COMMON);
    lighting_index = 1 - lighting_index;
    lighting_valid = true;
    if (desired.compare)
      render_comparison(l, back, lighting_output.Get());
    else
      copy_to_back(l, back, lighting_output.Get());
  }
  void copy_to_back(ID3D12GraphicsCommandList *l, ID3D12Resource *back,
                    ID3D12Resource *image) {
    transition(l, image, D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_SOURCE);
    transition(l, back, D3D12_RESOURCE_STATE_PRESENT, D3D12_RESOURCE_STATE_COPY_DEST);
    l->CopyResource(back, image);
    transition(l, back, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_PRESENT);
    transition(l, image, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_COMMON);
  }
  void restore_source(ID3D12GraphicsCommandList *l, ID3D12Resource *back) {
    copy_to_back(l, back, source.Get());
  }
  void render_comparison(ID3D12GraphicsCommandList *l, ID3D12Resource *back,
                          ID3D12Resource *processed) {
    constexpr auto read = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
    constexpr auto write = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    transition(l, source.Get(), D3D12_RESOURCE_STATE_COMMON, read);
    transition(l, processed, D3D12_RESOURCE_STATE_COMMON, read);
    transition(l, output.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    DlssNrConstants c{};
    c.Width = full_w;
    c.Height = full_h;
    c.CompareMode = desired.compare;
    c.CompareSplit = desired.split;
    c.CompareZoom = desired.zoom;
    c.CompareSwap = desired.swap;
    dispatch(l, 5, c, processed, nullptr, source.Get(), nullptr, output.Get(),
             nullptr, compare_pso.Get());
    transition(l, source.Get(), read, D3D12_RESOURCE_STATE_COMMON);
    transition(l, processed, read, D3D12_RESOURCE_STATE_COMMON);
    transition(l, output.Get(), write, D3D12_RESOURCE_STATE_COMMON);
    copy_to_back(l, back, output.Get());
  }

public:
  void motion_error(const std::string &text) { message(text); }
  CaptureNr(ID3D12Device *d, const std::filesystem::path &f)
      : device(d), folder(f) {
    message("Ready; select a source and enable neural rendering");
  }
  ~CaptureNr() {
    if (feature && release)
      release(feature);
    if (params && destroy_params)
      destroy_params(params);
    // This backend lives until process exit. Release our feature/parameters,
    // but retain the driver core with the loaded forwarder and snippet: its
    // global Shutdown1 faults inside _nvngx.dll with this direct-module path.
    // Windows reclaims the process-owned runtime after graphics teardown.
    if (init_event)
      CloseHandle(init_event); /* NGX modules keep process-owned callbacks. */
  }
  void configure(SpatpitNrOptions o) {
    // Validate every FFI value, including malformed saved settings.
    auto clamp = [](float v, float lo, float hi, float def) {
      return std::isfinite(v) ? std::clamp(v, lo, hi) : def;
    };
    o.enabled = !!o.enabled;
    o.preset = std::min(o.preset, 3u);
    o.style = std::min(o.style, 3u);
    if (o.motion_backend != 1 && o.style == 3) o.style = 1;
    o.auto_mask = !!o.auto_mask;
    o.model_scale = clamp(o.model_scale, .25f, 1.f, .5f);
    o.intensity = clamp(o.intensity, 0, 2, 1);
    o.structure = clamp(o.structure, 0, 2, 1);
    o.tone = clamp(o.tone, 0, 2, 1);
    o.skin = clamp(o.skin, -1, 2, -1);
    o.blend = clamp(o.blend, 0, 1, 1);
    o.colour = clamp(o.colour, 0, 1, 1);
    o.max_ratio = clamp(o.max_ratio, 1, 16, 4);
    o.transfer = std::min(o.transfer, 1u);
    o.compare = std::min(o.compare, 2u);
    o.swap = !!o.swap;
    o.debug = std::min(o.debug, 6u);
    o.split = clamp(o.split, 0, 1, .5f);
    o.zoom = clamp(o.zoom, 1, 2, 1);
    o.mv_x = clamp(o.mv_x, -4, 4, 1);
    o.mv_y = clamp(o.mv_y, -4, 4, 1);
    o.passes = std::clamp(o.passes, 1u, 3u);
    o.lighting_stability = !!o.lighting_stability;
    for (auto &t : o.pass_tuning) {
      t.custom = !!t.custom;
      t.style = std::min(t.style, 3u);
      if (o.motion_backend != 1 && t.style == 3) t.style = 1;
      t.model_scale = clamp(t.model_scale, .25f, 1.f, .5f);
      t.intensity = clamp(t.intensity, 0, 2, 1);
      t.blend = clamp(t.blend, 0, 1, 1);
    }
    // Dragging the comparison divider must not restart lighting history.
    if (nr_processing_changed(o, desired)) lighting_valid = false;
    if (o.passes != desired.passes || model_diff(desired, o) ||
        o.blend != desired.blend || o.colour != desired.colour ||
        o.transfer != desired.transfer || o.max_ratio != desired.max_ratio)
      for (auto &pass : extra)
        if (pass) pass->history_reset();
    if (model_diff(desired, o))
      changed = GetTickCount64();
    if (!desired.enabled && o.enabled)
      reset = true;
    desired = o;
    for (unsigned i = 0; i + 1 < desired.passes; ++i)
      if (extra[i]) extra[i]->configure(pass_options(i));
  }
  void retry() {
    lighting_valid = false;
    failed = false;
    reset = true;
    changed = 0;
    rebuild = true;
    for (auto &pass : extra) if (pass) pass->retry();
  }
  void history_reset() {
    reset = true;
    lighting_valid = false;
    for (auto &pass : extra) if (pass) pass->history_reset();
  }
  SpatpitNrStatus get_status() const {
    auto s = status;
    s.pending = model_diff(desired, applied);
    s.active = desired.enabled && feature && !failed;
    for (unsigned i = 0; i + 1 < desired.passes; ++i) {
      if (!extra[i]) { s.active = 0; continue; }
      auto child = extra[i]->get_status();
      s.pending |= child.pending;
      s.active &= child.active;
      if (!child.active)
        sprintf_s(s.message, "Pass %u: %.340s", i + 2, child.message);
    }
    return s;
  }
  void prepare(unsigned w, unsigned h, unsigned gw, unsigned gh) {
    if (!desired.enabled || failed)
      return;
    try {
      if (!initialized)
        initialize();
      bool changed_size =
          w != full_w || h != full_h || gw != guide_w || gh != guide_h;
      if (!feature || rebuild || changed_size ||
          (model_diff(desired, applied) && GetTickCount64() - changed >= 400)) {
        rebuild = false;
        // Engine has completed all previous frame work before calling this.
        release_features();
        applied = desired;
        full_w = w;
        full_h = h;
        guide_w = gw;
        guide_h = gh;
        status.width = std::max(32u, unsigned(w * applied.model_scale) & ~7u);
        status.height = std::max(32u, unsigned(h * applied.model_scale) & ~7u);
        lighting_output.Reset();
        for (auto &image : lighting_history) image.Reset();
        lighting_valid = false;
        source = texture(w, h, DXGI_FORMAT_R8G8B8A8_UNORM);
        original = texture(w, h, DXGI_FORMAT_R16G16B16A16_FLOAT);
        proxy = texture(w, h, DXGI_FORMAT_R16G16B16A16_FLOAT);
        model_input = texture(status.width, status.height,
                              DXGI_FORMAT_R16G16B16A16_FLOAT);
        model = texture(status.width, status.height,
                        DXGI_FORMAT_R16G16B16A16_FLOAT);
        output = texture(w, h, DXGI_FORMAT_R8G8B8A8_UNORM);
        depth = texture(gw, gh, DXGI_FORMAT_R32_FLOAT);
        depth_clear = true;
        check(init_alloc->Reset(), "Reset neural allocator");
        check(init_list->Reset(init_alloc.Get(), nullptr), "Reset neural list");
        feature = create((folder / L"nvngx_dlssnr.dll").c_str(), folder.c_str(),
                         device, init_list.Get(), params, status.width,
                         status.height, applied.preset, runtime_intensity(applied),
                         runtime_style(applied), applied.structure, applied.tone,
                         applied.skin, applied.auto_mask, 0);
        check(init_list->Close(), "Close neural create list");
        ID3D12CommandList *lists[] = {init_list.Get()};
        init_queue->ExecuteCommandLists(1, lists);
        check(init_queue->Signal(init_fence.Get(), ++init_value),
              "Signal neural create fence");
        check(init_fence->SetEventOnCompletion(init_value, init_event),
              "Wait neural create fence");
        if (WaitForSingleObject(init_event, 10000) != WAIT_OBJECT_0)
          throw std::runtime_error("Neural initialization timed out");
        if (!feature) {
          char text[128];
          sprintf_s(text, "NR create failed: init=0x%08X create=0x%08X",
                    *symbol<int *>(forwarder, "dlssnr_call_last_init"),
                    *symbol<int *>(forwarder, "dlssnr_call_last_create"));
          throw std::runtime_error(text);
        }
        ++status.builds;
        reset = true;
        log("Feature built " + std::to_string(status.builds) + " at " +
            std::to_string(status.width) + "x" + std::to_string(status.height) +
            " intensity=" + std::to_string(applied.intensity));
        for (auto &pass : extra) if (pass) pass->history_reset();
      }
      // prepare runs after the host frame fence. Release inactive stages here,
      // never from configure while their GPU commands may still be executing.
      for (unsigned i = 0; i < extra.size(); ++i) {
        if (i + 1 >= desired.passes) { extra[i].reset(); continue; }
        if (!extra[i]) extra[i] = std::make_unique<CaptureNr>(device, folder);
        extra[i]->configure(pass_options(i));
        extra[i]->prepare(w, h, gw, gh);
      }
      if (lighting_enabled()) prepare_lighting();
      else {
        lighting_output.Reset();
        for (auto &image : lighting_history) image.Reset();
        lighting_valid = false;
      }
      if ((desired.passes > 1 || lighting_enabled()) && desired.compare && !compare_pso)
        prepare_comparison();
      if (desired.motion_backend == 1 && !detail_pso) prepare_detail();
    } catch (const std::exception &e) {
      failed = true;
      message(e.what());
      log(e.what());
    }
  }
  void render(ID3D12GraphicsCommandList *l, ID3D12Resource *back,
              ID3D12Resource *motion, ID3D12Resource *capture_source = nullptr) {
    if (!desired.enabled) {
      message("Off; original image. Model stays loaded for immediate resume.");
      return;
    }
    if (!get_status().active || !motion)
      return;
    // All owned resources begin and end in COMMON. Descriptor/constant slots
    // are unique for each dispatch and recycled only after the frame fence.
    constexpr auto read = D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE;
    constexpr auto write = D3D12_RESOURCE_STATE_UNORDERED_ACCESS;
    transition(l, back, D3D12_RESOURCE_STATE_PRESENT,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
    transition(l, source.Get(), D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_COPY_DEST);
    l->CopyResource(source.Get(), back);
    transition(l, source.Get(), D3D12_RESOURCE_STATE_COPY_DEST, read);
    transition(l, proxy.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    transition(l, original.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    DlssNrConstants c{};
    c.Mode = 0;
    c.WhitePoint = 1;
    c.Width = full_w;
    c.Height = full_h;
    c.Passthrough = 1;
    c.TransferStrength = desired.blend;
    c.ColourStrength = desired.colour;
    c.MaxRatio = desired.max_ratio;
    c.GuideWidth = guide_w;
    c.GuideHeight = guide_h;
    c.MvScaleX = full_w * desired.mv_x;
    c.MvScaleY = full_h * desired.mv_y;
    // Let the first evaluation develop texture before correcting the later
    // stages. Children have passes=1 and anchor to the parent's game capture.
    // A single-pass render still receives its existing detail correction.
    bool preserve = desired.motion_backend == 1 && desired.debug == 0 &&
                    desired.blend > 0 && desired.passes == 1;
    c.CompareMode = desired.passes > 1 || preserve || lighting_enabled() ? 0 : desired.compare;
    c.CompareSplit = desired.split;
    c.CompareZoom = desired.zoom;
    c.CompareSwap = desired.swap;
    c.Transfer = desired.transfer;
    c.DebugScale = 1;
    c.ApplyModel = 1;
    c.ExposurePreMul = 1;
    c.DebugView = desired.passes > 1 ? 0 : desired.debug;
    dispatch(l, 0, c, source.Get(), nullptr, nullptr, motion, proxy.Get(),
             original.Get());
    transition(l, proxy.Get(), write, read);
    transition(l, original.Get(), write, read);
    transition(l, model_input.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    c.Mode = 2;
    c.Width = status.width;
    c.Height = status.height;
    dispatch(l, 1, c, proxy.Get(), nullptr, nullptr, motion, model_input.Get());
    transition(l, model_input.Get(), write, read);
    transition(l, depth.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    if (depth_clear) {
      uav(depth.Get(), 28);
      ID3D12DescriptorHeap *hs[] = {descriptors.Get()};
      l->SetDescriptorHeaps(1, hs);
      float d[4] = {1, 1, 1, 1};
      l->ClearUnorderedAccessViewFloat(gpu(28), cpu(28), depth.Get(), d, 0,
                                       nullptr);
      depth_clear = false;
    }
    transition(l, depth.Get(), write, read);
    transition(l, model.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    int result =
        evaluate(l, feature, params, model_input.Get(), depth.Get(), motion,
                 model.Get(), status.width, status.height, guide_w, guide_h, 0,
                 reset, runtime_intensity(applied), runtime_style(applied), applied.structure,
                 applied.tone, applied.skin, applied.auto_mask,
                 status.width * desired.mv_x, status.height * desired.mv_y);
    if (reset) lighting_valid = false;
    reset = false;
    transition(l, model.Get(), write, read);
    transition(l, output.Get(), D3D12_RESOURCE_STATE_COMMON, write);
    c.Mode = 1;
    c.Width = full_w;
    c.Height = full_h;
    c.ApplyModel = result == 1;
    if (preserve) {
      // Downsampling has finished reading proxy; reuse it for the unfiltered
      // edit, avoiding another full-resolution allocation.
      transition(l, proxy.Get(), read, write);
      dispatch(l, 2, c, model_input.Get(), model.Get(), original.Get(), motion,
               proxy.Get());
      transition(l, proxy.Get(), write, read);
      c.CompareMode = desired.passes > 1 || lighting_enabled() ? 0 : desired.compare;
      // The detail shader reuses the transfer-strength constant as its radius.
      // Later passes anchor to the game capture, not already reshaped output.
      // Broaden their source-detail band without adding taps or evaluations.
      c.TransferStrength = capture_source ? 5.0f : 2.5f;
      c.Mode = desired.style == 3 ? 3 : 1;
      if (capture_source)
        transition(l, capture_source, D3D12_RESOURCE_STATE_COMMON, read);
      dispatch(l, 3, c, proxy.Get(), nullptr,
               capture_source ? capture_source : source.Get(), nullptr,
               output.Get(), nullptr, detail_pso.Get());
      if (capture_source)
        transition(l, capture_source, read, D3D12_RESOURCE_STATE_COMMON);
    } else {
      dispatch(l, 2, c, model_input.Get(), model.Get(), original.Get(), motion,
               output.Get());
    }
    if (result == 1 && desired.passes == 1 && lighting_enabled() &&
        !capture_source) {
      // Spatpit lighting reads output directly and writes the final image.
      // Do not copy an intermediate image that it would immediately replace.
      transition(l, output.Get(), write, D3D12_RESOURCE_STATE_COMMON);
      transition(l, back, D3D12_RESOURCE_STATE_COPY_SOURCE,
                 D3D12_RESOURCE_STATE_PRESENT);
    } else {
      transition(l, output.Get(), write, D3D12_RESOURCE_STATE_COPY_SOURCE);
      transition(l, back, D3D12_RESOURCE_STATE_COPY_SOURCE,
                 D3D12_RESOURCE_STATE_COPY_DEST);
      l->CopyResource(back, output.Get());
      transition(l, back, D3D12_RESOURCE_STATE_COPY_DEST,
                 D3D12_RESOURCE_STATE_PRESENT);
      transition(l, output.Get(), D3D12_RESOURCE_STATE_COPY_SOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
    }
    for (auto *r : {source.Get(), proxy.Get(), original.Get(),
                    model_input.Get(), depth.Get(), model.Get()})
      transition(l, r, read, D3D12_RESOURCE_STATE_COMMON);
    if (result == 1) {
      for (unsigned i = 0; i + 1 < desired.passes; ++i) {
        // Keep the original capture alive in COMMON between child dispatches.
        // Each child still evaluates the previous pass with its own history.
        extra[i]->render(l, back, motion, source.Get());
        if (!extra[i]->get_status().active) {
          restore_source(l, back);
          history_reset();
          return;
        }
      }
      // Stabilize once, after the complete chain, against the original capture.
      // Child stages disable lighting history; comparison is composed only after
      // the final correction so neither history nor the source side is filtered.
      auto *final_output = desired.passes > 1
                               ? extra[desired.passes - 2]->output.Get()
                               : output.Get();
      if (lighting_enabled() && !capture_source)
        stabilize_lighting(l, back, motion, final_output);
      else if (desired.passes > 1 && desired.compare)
        render_comparison(l, back, final_output);
      ++status.evaluations;
      message("Running " + std::to_string(desired.passes) +
              "x; model settings apply automatically after editing.");
      if (status.evaluations == 1)
        log("First neural evaluation succeeded");
    } else {
      restore_source(l, back);
      failed = true;
      char text[96];
      sprintf_s(text, "Neural evaluation failed: 0x%08X. Showing original.",
                result);
      message(text);
      log(text);
    }
  }
};
