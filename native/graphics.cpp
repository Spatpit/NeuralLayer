#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include "graphics.h"
#include "reshade_api.hpp"
#include "reshade_events.hpp"
#include "shader_source.h"
#include "neural_motion.h"
#include <algorithm>
#include <array>
#include <atomic>
#include <d3d11_4.h>
#include <d3d12.h>
#include <d3dcompiler.h>
#include <dcomp.h>
#include <dwmapi.h>
#include <dxgi1_6.h>
#include <filesystem>
#include <functional>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <thread>
#include <utility>
#include <vector>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <windows.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <winrt/Windows.Graphics.DirectX.h>
#include <wrl/client.h>
using Microsoft::WRL::ComPtr;
using namespace winrt::Windows::Graphics::Capture;
using namespace winrt::Windows::Graphics::DirectX;
using namespace winrt::Windows::Graphics::DirectX::Direct3D11;
static thread_local std::string last_creation_error;
static std::string exception_message();
static void check(HRESULT hr, const char *action) {
  if (FAILED(hr)) {
    char s[300];
    sprintf_s(s, "%s (0x%08X)", action, static_cast<unsigned>(hr));
    throw std::runtime_error(s);
  }
}
static D3D12_HEAP_PROPERTIES heap(D3D12_HEAP_TYPE t) {
  D3D12_HEAP_PROPERTIES p{};
  p.Type = t;
  p.CreationNodeMask = p.VisibleNodeMask = 1;
  return p;
}
static D3D12_RESOURCE_DESC buffer_desc(uint64_t size) {
  D3D12_RESOURCE_DESC d{};
  d.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
  d.Width = std::max<uint64_t>(size, 256);
  d.Height = 1;
  d.DepthOrArraySize = d.MipLevels = 1;
  d.SampleDesc.Count = 1;
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
#include "optiscaler.h"
#include "latency_trace.h"
struct Texture {
  ComPtr<ID3D12Resource> resource;
  uint32_t slot = 0;
};
struct Engine;
static Engine *active_engine = nullptr;
static void runtime_init(reshade::api::effect_runtime *runtime);
static void runtime_destroy(reshade::api::effect_runtime *runtime);
static void runtime_reloaded(reshade::api::effect_runtime *runtime);
static bool runtime_technique(reshade::api::effect_runtime *runtime, reshade::api::effect_technique technique, bool enabled);
static bool managed_color(const std::string &effect) {
  // Effects copied in by "Import preset"; one look runs at a time.
  return effect.rfind("SpatpitLook_", 0) == 0;
}
static void runtime_present(reshade::api::effect_runtime *runtime);
static bool runtime_overlay(reshade::api::effect_runtime *runtime, bool open,
                            reshade::api::input_source source);
struct Engine {
  HWND hwnd{}, target{};
  LatencyTrace *latency = nullptr;
  uint32_t width = 1, height = 1;
  float display_scale_x = 1, display_scale_y = 1;
  bool client_only = true, has_frame = false;
  uint64_t frames = 0;
  uint64_t last_frame_tick = 0;
  int64_t capture_interval_before = -1, capture_interval_after = -1;
  std::string error;
  // Set when the last failure was a transient ReShade effect reload.
  bool reloading = false;
  std::shared_ptr<std::atomic<bool>> source_closed;
  ComPtr<ID3D12Device> device;
  ComPtr<ID3D12CommandQueue> queue;
  // Populated only by the offline replay profiler; no queries in normal use.
  std::function<void(ID3D12GraphicsCommandList *, unsigned)> profile_stamp;
  std::function<ID3D12Resource *(ID3D12GraphicsCommandList *, ID3D12Resource *)> replay_motion_hook;
  void stamp(ID3D12GraphicsCommandList *list, unsigned index) {
    if (profile_stamp) profile_stamp(list, index);
  }
  ComPtr<IDXGISwapChain3> swap;
  ComPtr<IDCompositionDevice> composition;
  ComPtr<IDCompositionTarget> composition_target;
  ComPtr<IDCompositionVisual> visual;
  ComPtr<ID3D12DescriptorHeap> rtvs, srvs;
  uint32_t rtv_step = 0, srv_step = 0, next_slot = 8;
  std::vector<uint32_t> free_slots;
  std::array<ComPtr<ID3D12Resource>, 2> back;
  std::array<ComPtr<ID3D12CommandAllocator>, 2> alloc;
  std::array<ComPtr<ID3D12GraphicsCommandList>, 2> commands;
  ComPtr<ID3D12Fence> fence;
  HANDLE fence_event{};
  uint64_t fence_value = 0;
  ComPtr<ID3D12RootSignature> root;
  ComPtr<ID3D12PipelineState> image_pso, ui_pso, opaque_alpha_pso;
  ComPtr<ID3D12Resource> vertices, indices;
  uint64_t vertex_capacity = 0, index_capacity = 0;
  std::map<uint32_t, Texture> textures;
  ComPtr<ID3D11Device> device11;
  ComPtr<ID3D11DeviceContext> context11;
  ComPtr<ID3D11Device5> capture_device5;
  ComPtr<ID3D11DeviceContext4> capture_context4;
  ComPtr<ID3D12Fence> capture_fence12;
  ComPtr<ID3D11Fence> capture_fence11;
  uint64_t capture_fence_value = 0;
  IDirect3DDevice capture_device{nullptr};
  GraphicsCaptureItem item{nullptr};
  Direct3D11CaptureFramePool pool{nullptr};
  GraphicsCaptureSession session{nullptr};
  winrt::event_token close_token{};
  Texture captured;
  ComPtr<ID3D11Resource> wrapped;
  uint32_t source_width = 0, source_height = 0;
  DXGI_FORMAT source_format = DXGI_FORMAT_UNKNOWN;
  HMODULE reshade_module{};
  reshade::api::effect_runtime *effects{};
  std::function<void()> paint_ui;
  std::filesystem::path runtime_folder;
  bool native_overlay = false;
  uint64_t effect_generation = 0;
  std::vector<std::pair<std::string, std::string>> color_allowed;
  std::unique_ptr<CaptureNr> nr;
  std::unique_ptr<NeuralMotion> motion;
  SpatpitNrOptions motion_options{};
  SpatpitOptions previous_options{};
  uint64_t motion_capture=0, motion_model_build=0;
  ~Engine() {
    stop();
    try {
      sync();
    } catch (...) {
    }
    active_engine = nullptr;
    nr.reset();
    motion.reset();
    if (fence_event)
      CloseHandle(fence_event); /* ReShade hooks retain module code until
                                   process exit. */
  }
  D3D12_CPU_DESCRIPTOR_HANDLE rtv(uint32_t n) {
    auto h = rtvs->GetCPUDescriptorHandleForHeapStart();
    h.ptr += n * rtv_step;
    return h;
  }
  D3D12_CPU_DESCRIPTOR_HANDLE srv(uint32_t n) {
    auto h = srvs->GetCPUDescriptorHandleForHeapStart();
    h.ptr += n * srv_step;
    return h;
  }
  D3D12_GPU_DESCRIPTOR_HANDLE gpu(uint32_t n) {
    auto h = srvs->GetGPUDescriptorHandleForHeapStart();
    h.ptr += n * srv_step;
    return h;
  }
  void sync() {
    if (!queue || !fence)
      return;
    check(queue->Signal(fence.Get(), ++fence_value), "Signal graphics fence");
    if (fence->GetCompletedValue() < fence_value) {
      check(fence->SetEventOnCompletion(fence_value, fence_event),
            "Wait for graphics fence");
      if (WaitForSingleObject(fence_event, 5000) != WAIT_OBJECT_0)
        throw std::runtime_error("Graphics device stopped responding");
    }
  }
  ComPtr<ID3D12Resource> upload(uint64_t size) {
    ComPtr<ID3D12Resource> r;
    auto h = heap(D3D12_HEAP_TYPE_UPLOAD);
    auto d = buffer_desc(size);
    check(device->CreateCommittedResource(&h, D3D12_HEAP_FLAG_NONE, &d,
                                          D3D12_RESOURCE_STATE_GENERIC_READ,
                                          nullptr, IID_PPV_ARGS(&r)),
          "Create upload resource");
    return r;
  }
  ID3D12GraphicsCommandList *begin(uint32_t n) {
    check(alloc[n]->Reset(), "Reset command allocator");
    check(commands[n]->Reset(alloc[n].Get(), nullptr), "Reset command list");
    return commands[n].Get();
  }
  void execute(uint32_t n) {
    check(commands[n]->Close(), "Close command list");
    ID3D12CommandList *lists[] = {commands[n].Get()};
    queue->ExecuteCommandLists(1, lists);
  }
  void make_backbuffers() {
    for (uint32_t n = 0; n < 2; n++) {
      check(swap->GetBuffer(n, IID_PPV_ARGS(&back[n])), "Get output texture");
      device->CreateRenderTargetView(back[n].Get(), nullptr, rtv(n));
    }
  }
  void load_hooks() {
    auto path = runtime_folder / L"ReShade64.dll";
    if (!std::filesystem::exists(path))
      return;
    SetEnvironmentVariableW(L"RESHADE_BASE_PATH_OVERRIDE",
                            runtime_folder.c_str());
    SetEnvironmentVariableW(L"RESHADE_DISABLE_LOADING_CHECK", L"1");
    check(SetDefaultDllDirectories(LOAD_LIBRARY_SEARCH_DEFAULT_DIRS)
              ? S_OK
              : HRESULT_FROM_WIN32(GetLastError()),
          "Set app DLL search path");
    if (!AddDllDirectory(runtime_folder.c_str()))
      throw std::runtime_error("Could not register app runtime folder");
    reshade_module = LoadLibraryExW(path.c_str(), nullptr,
                                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR |
                                        LOAD_LIBRARY_SEARCH_DEFAULT_DIRS);
    if (!reshade_module)
      throw std::runtime_error("Could not load app-local ReShade");
    // Editable shader uniforms must not be compiled away as constants.
    auto configure =
        reinterpret_cast<void (*)(void *, reshade::api::effect_runtime *,
                                  const char *, const char *, const char *)>(
            GetProcAddress(reshade_module, "ReShadeSetConfigValue"));
    if (configure)
      configure(GetModuleHandleW(nullptr), nullptr, "GENERAL",
                "PerformanceMode", "0");
    if (configure) {
      configure(nullptr, nullptr, "STYLE", "StyleIndex", "1");
      configure(nullptr, nullptr, "STYLE", "FontSize", "16");
      configure(nullptr, nullptr, "STYLE", "FrameRounding", "5");
    }
    auto register_addon = reinterpret_cast<bool (*)(void *, uint32_t)>(
        GetProcAddress(reshade_module, "ReShadeRegisterAddon"));
    auto register_event =
        reinterpret_cast<void (*)(void *, reshade::addon_event, void *)>(
            GetProcAddress(reshade_module, "ReShadeRegisterEventForAddon"));
    if (!register_addon || !register_event ||
        !register_addon(GetModuleHandleW(nullptr), 20))
      throw std::runtime_error("ReShade host API is incompatible");
    register_event(GetModuleHandleW(nullptr),
                   reshade::addon_event::init_effect_runtime,
                   reinterpret_cast<void *>(runtime_init));
    register_event(GetModuleHandleW(nullptr),
                   reshade::addon_event::destroy_effect_runtime,
                   reinterpret_cast<void *>(runtime_destroy));
    register_event(GetModuleHandleW(nullptr),
                   reshade::addon_event::reshade_reloaded_effects,
                   reinterpret_cast<void *>(runtime_reloaded));
    register_event(GetModuleHandleW(nullptr),
                   reshade::addon_event::reshade_set_technique_state,
                   reinterpret_cast<void *>(runtime_technique));
    register_event(GetModuleHandleW(nullptr),
                   reshade::addon_event::reshade_present,
                   reinterpret_cast<void *>(runtime_present));
    register_event(GetModuleHandleW(nullptr),
                   reshade::addon_event::reshade_open_overlay,
                   reinterpret_cast<void *>(runtime_overlay));
  }
  void init(HWND window, const wchar_t *folder) {
    runtime_folder = folder;
    active_engine = this;
    load_hooks();
    hwnd = window;
    RECT rc{};
    GetClientRect(hwnd, &rc);
    width = std::max(1L, rc.right);
    height = std::max(1L, rc.bottom);
    check(D3D12CreateDevice(nullptr, D3D_FEATURE_LEVEL_11_0,
                            IID_PPV_ARGS(&device)),
          "Create DirectX 12 device");
    D3D12_COMMAND_QUEUE_DESC q{};
    q.Type = D3D12_COMMAND_LIST_TYPE_DIRECT;
    check(device->CreateCommandQueue(&q, IID_PPV_ARGS(&queue)),
          "Create graphics queue");
    ComPtr<IDXGIFactory2> factory;
    check(CreateDXGIFactory2(0, IID_PPV_ARGS(&factory)), "Create DXGI factory");
    DXGI_SWAP_CHAIN_DESC1 d{};
    d.Width = width;
    d.Height = height;
    d.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    d.SampleDesc.Count = 1;
    d.BufferUsage = DXGI_USAGE_RENDER_TARGET_OUTPUT;
    d.BufferCount = 2;
    d.SwapEffect = DXGI_SWAP_EFFECT_FLIP_SEQUENTIAL;
    d.AlphaMode = DXGI_ALPHA_MODE_PREMULTIPLIED;
    d.Scaling = DXGI_SCALING_STRETCH;
    ComPtr<IDXGISwapChain1> sc;
    check(factory->CreateSwapChainForComposition(queue.Get(), &d, nullptr, &sc),
          "Create transparent DirectX swapchain");
    check(sc.As(&swap), "Get swapchain interface");
    check(DCompositionCreateDevice(nullptr, IID_PPV_ARGS(&composition)),
          "Create desktop composition device");
    check(composition->CreateTargetForHwnd(hwnd, TRUE, &composition_target),
          "Bind composition to canvas");
    check(composition->CreateVisual(&visual), "Create composition visual");
    check(visual->SetContent(swap.Get()), "Set canvas content");
    check(composition_target->SetRoot(visual.Get()), "Set canvas visual");
    check(composition->Commit(), "Commit composition");
    D3D12_DESCRIPTOR_HEAP_DESC dh{};
    dh.Type = D3D12_DESCRIPTOR_HEAP_TYPE_RTV;
    dh.NumDescriptors = 2;
    check(device->CreateDescriptorHeap(&dh, IID_PPV_ARGS(&rtvs)),
          "Create render targets");
    dh.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    dh.NumDescriptors = 2048;
    dh.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    check(device->CreateDescriptorHeap(&dh, IID_PPV_ARGS(&srvs)),
          "Create texture descriptors");
    rtv_step = device->GetDescriptorHandleIncrementSize(
        D3D12_DESCRIPTOR_HEAP_TYPE_RTV);
    srv_step = device->GetDescriptorHandleIncrementSize(
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    make_backbuffers();
    for (uint32_t n = 0; n < 2; n++) {
      check(device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                           IID_PPV_ARGS(&alloc[n])),
            "Create allocator");
      check(device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT,
                                      alloc[n].Get(), nullptr,
                                      IID_PPV_ARGS(&commands[n])),
            "Create command list");
      commands[n]->Close();
    }
    check(device->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&fence)),
          "Create fence");
    fence_event = CreateEventW(nullptr, FALSE, FALSE, nullptr);
    if (!fence_event)
      throw std::runtime_error("CreateEvent failed");
    create_pipelines();
    // WGC frame pools stall when backed by D3D11On12. Capture on a native D3D11
    // device and exchange GPU resources with D3D12 using shared handles/fences.
    ComPtr<IDXGIFactory4> factory4;
    check(factory.As(&factory4), "Get capture adapter factory");
    ComPtr<IDXGIAdapter> adapter;
    check(factory4->EnumAdapterByLuid(device->GetAdapterLuid(),
                                      IID_PPV_ARGS(&adapter)),
          "Select matching capture adapter");
    D3D_FEATURE_LEVEL level;
    check(D3D11CreateDevice(adapter.Get(), D3D_DRIVER_TYPE_UNKNOWN, nullptr,
                            D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0,
                            D3D11_SDK_VERSION, &device11, &level, &context11),
          "Create native capture device");
    ComPtr<ID3D11Multithread> multithread;
    check(context11.As(&multithread), "Protect capture context");
    multithread->SetMultithreadProtected(TRUE);
    check(device11.As(&capture_device5), "Get shared-fence capture device");
    check(context11.As(&capture_context4), "Get shared-fence capture context");
    check(device->CreateFence(0, D3D12_FENCE_FLAG_SHARED,
                              IID_PPV_ARGS(&capture_fence12)),
          "Create capture synchronization fence");
    HANDLE fence_handle{};
    check(device->CreateSharedHandle(capture_fence12.Get(), nullptr,
                                     GENERIC_ALL, nullptr, &fence_handle),
          "Share capture fence");
    HRESULT fence_result = capture_device5->OpenSharedFence(
        fence_handle, IID_PPV_ARGS(&capture_fence11));
    CloseHandle(fence_handle);
    check(fence_result, "Open capture fence");
    ComPtr<IDXGIDevice> dxgi;
    check(device11.As(&dxgi), "Get capture DXGI device");
    ComPtr<IInspectable> inspect;
    check(CreateDirect3D11DeviceFromDXGIDevice(dxgi.Get(), &inspect),
          "Create WinRT capture device");
    capture_device = winrt::Windows::Foundation::IInspectable(
                         inspect.Detach(), winrt::take_ownership_from_abi)
                         .as<IDirect3DDevice>();
    // Leave the output capturable for Discord/OBS. Source capture targets a
    // specific HWND, not the composed desktop; capture() rejects our windows.
  }
  void create_pipelines() {
    D3D12_DESCRIPTOR_RANGE range{};
    range.RangeType = D3D12_DESCRIPTOR_RANGE_TYPE_SRV;
    range.NumDescriptors = 1;
    range.BaseShaderRegister = 0;
    D3D12_ROOT_PARAMETER p[2]{};
    p[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    p[0].DescriptorTable = {1, &range};
    p[0].ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    p[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
    p[1].Constants = {0, 0, 20};
    D3D12_STATIC_SAMPLER_DESC sampler{};
    sampler.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
    sampler.AddressU = sampler.AddressV = sampler.AddressW =
        D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
    sampler.MaxLOD = D3D12_FLOAT32_MAX;
    sampler.ComparisonFunc = D3D12_COMPARISON_FUNC_ALWAYS;
    sampler.ShaderVisibility = D3D12_SHADER_VISIBILITY_PIXEL;
    D3D12_ROOT_SIGNATURE_DESC r{};
    r.NumParameters = 2;
    r.pParameters = p;
    r.NumStaticSamplers = 1;
    r.pStaticSamplers = &sampler;
    r.Flags = D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT;
    ComPtr<ID3DBlob> blob, err;
    check(D3D12SerializeRootSignature(&r, D3D_ROOT_SIGNATURE_VERSION_1, &blob,
                                      &err),
          "Serialize shaders");
    check(device->CreateRootSignature(0, blob->GetBufferPointer(),
                                      blob->GetBufferSize(),
                                      IID_PPV_ARGS(&root)),
          "Create shader root");
    auto compile = [&](const char *entry, const char *profile) {
      ComPtr<ID3DBlob> b, e;
      HRESULT hr = D3DCompile(spatpit_shaders, strlen(spatpit_shaders), "spatpit",
                              nullptr, nullptr, entry, profile,
                              D3DCOMPILE_ENABLE_STRICTNESS, 0, &b, &e);
      if (FAILED(hr))
        throw std::runtime_error(e ? static_cast<char *>(e->GetBufferPointer())
                                   : "Shader compilation failed");
      return b;
    };
    for (int pipeline = 0; pipeline < 3; pipeline++) {
      bool ui = pipeline == 1, alpha = pipeline == 2;
      auto vs = compile(ui ? "ui_vs" : "image_vs", "vs_5_0"),
           ps = compile(ui ? "ui_ps" : alpha ? "opaque_alpha_ps" : "image_ps", "ps_5_0");
      D3D12_GRAPHICS_PIPELINE_STATE_DESC d{};
      d.pRootSignature = root.Get();
      d.VS = {vs->GetBufferPointer(), vs->GetBufferSize()};
      d.PS = {ps->GetBufferPointer(), ps->GetBufferSize()};
      d.SampleMask = UINT_MAX;
      d.RasterizerState.FillMode = D3D12_FILL_MODE_SOLID;
      d.RasterizerState.CullMode = D3D12_CULL_MODE_NONE;
      d.RasterizerState.DepthClipEnable = TRUE;
      d.DepthStencilState.DepthEnable = FALSE;
      d.DepthStencilState.StencilEnable = FALSE;
      d.PrimitiveTopologyType = D3D12_PRIMITIVE_TOPOLOGY_TYPE_TRIANGLE;
      d.NumRenderTargets = 1;
      d.RTVFormats[0] = DXGI_FORMAT_R8G8B8A8_UNORM;
      d.SampleDesc.Count = 1;
      auto &b = d.BlendState.RenderTarget[0];
      b.RenderTargetWriteMask = alpha ? D3D12_COLOR_WRITE_ENABLE_ALPHA : D3D12_COLOR_WRITE_ENABLE_ALL;
      b.BlendEnable = ui;
      b.SrcBlend = D3D12_BLEND_ONE;
      b.DestBlend = D3D12_BLEND_INV_SRC_ALPHA;
      b.BlendOp = D3D12_BLEND_OP_ADD;
      b.SrcBlendAlpha = D3D12_BLEND_ONE;
      b.DestBlendAlpha = D3D12_BLEND_INV_SRC_ALPHA;
      b.BlendOpAlpha = D3D12_BLEND_OP_ADD;
      D3D12_INPUT_ELEMENT_DESC input[] = {
          {"POSITION", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 0,
           D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
          {"TEXCOORD", 0, DXGI_FORMAT_R32G32_FLOAT, 0, 8,
           D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0},
          {"COLOR", 0, DXGI_FORMAT_R8G8B8A8_UNORM, 0, 16,
           D3D12_INPUT_CLASSIFICATION_PER_VERTEX_DATA, 0}};
      if (ui)
        d.InputLayout = {input, 3};
      auto **result = ui ? ui_pso.GetAddressOf() : alpha ? opaque_alpha_pso.GetAddressOf() : image_pso.GetAddressOf();
      check(device->CreateGraphicsPipelineState(&d, IID_PPV_ARGS(result)),
            "Create graphics pipeline");
    }
  }
  void resize(uint32_t w, uint32_t h) {
    if (w == width && h == height)
      return;
    sync();
    for (auto &b : back)
      b.Reset();
    check(swap->ResizeBuffers(2, w, h, DXGI_FORMAT_R8G8B8A8_UNORM, 0),
          "Resize output canvas");
    width = w;
    height = h;
    make_backbuffers();
  }
  void stop() {
    if (nr) nr->history_reset();
    if (motion) motion->reset();
    // Detach logical capture state first. A late Closed callback owns only
    // its session's flag, never the engine or a subsequent capture session.
    target = nullptr;
    has_frame = false;
    source_closed.reset();
    if (!session && !pool && !item)
      return;
    // WGC's synchronous Close can block after a source process is killed.
    // Transfer ownership to an MTA worker so the UI keeps pumping messages
    // and can start another capture. Never wait for this worker on the UI
    // thread, and never let it access Engine or its GPU resources.
    try {
      std::thread([session = std::move(session), pool = std::move(pool),
                   item = std::move(item), token = std::exchange(close_token, {})]() mutable {
        HRESULT apartment = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
        try {
          if (item && token.value)
            item.Closed(token);
        } catch (...) {
        }
        try {
          if (session)
            session.Close();
        } catch (...) {
        }
        try {
          if (pool)
            pool.Close();
        } catch (...) {
        }
        session = nullptr;
        pool = nullptr;
        item = nullptr;
        if (SUCCEEDED(apartment))
          CoUninitialize();
      }).detach();
    } catch (...) {
      // Capture is already detached even if Windows cannot start a worker.
    }
  }
  void capture(HWND window, bool client) {
    DWORD source_pid = 0;
    if (!IsWindow(window) || !GetWindowThreadProcessId(window, &source_pid))
      throw std::runtime_error("That capture window is unavailable");
    if (source_pid == GetCurrentProcessId())
      throw std::runtime_error("The app cannot capture its own windows");
    if (!GraphicsCaptureSession::IsSupported())
      throw std::runtime_error("Windows Graphics Capture is unavailable");
    stop();
    try {
      sync();
      source_width = source_height = 0;
      wrapped.Reset();
      captured.resource.Reset();
      auto interop = winrt::get_activation_factory<GraphicsCaptureItem,
                                                   IGraphicsCaptureItemInterop>();
      check(interop->CreateForWindow(window,
                                     winrt::guid_of<GraphicsCaptureItem>(),
                                     winrt::put_abi(item)),
            "Select capture window");
      pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
          capture_device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2,
          item.Size());
      session = pool.CreateCaptureSession(item);
      session.IsCursorCaptureEnabled(false);
      capture_interval_before = capture_interval_after = -1;
      // Recent Windows builds default to a 16 ms capture interval even on a
      // 120 Hz display. Remove that OS throttle; the render loop still limits
      // consumption and drains at most two queued frames. Older Windows versions
      // without this optional interface retain their existing capture behavior.
      if (auto cadence = session.try_as<IGraphicsCaptureSession5>()) {
        capture_interval_before = cadence.MinUpdateInterval().count();
        cadence.MinUpdateInterval(winrt::Windows::Foundation::TimeSpan{0});
        capture_interval_after = cadence.MinUpdateInterval().count();
      }
      source_closed = std::make_shared<std::atomic<bool>>(false);
      // C++/WinRT delegates are agile. Do no teardown or UI work in this
      // callback: Close can wait for capture callbacks to finish.
      close_token = item.Closed([closed = source_closed](auto const &, auto const &) {
        closed->store(true);
      });
      target = window;
      client_only = client;
      session.StartCapture();
      error.clear();
    } catch (...) {
      stop();
      throw;
    }
  }
  void capture_frame(bool paused) {
    if (!session)
      return;
    if ((source_closed && source_closed->load()) || !IsWindow(target)) {
      stop();
      error = "Source closed. Select another window.";
      return;
    }
    if (paused || IsIconic(target))
      return;
    auto frame = pool.TryGetNextFrame();
    if (!frame)
      return;
    // The two-buffer pool can retain an older frame when rendering falls
    // behind capture. Prefer the newest available image. Bound the drain so
    // a continuously producing source cannot keep us here indefinitely.
    for (unsigned i = 1; i < 2; ++i) {
      auto newer = pool.TryGetNextFrame();
      if (!newer)
        break;
      frame.Close();
      frame = std::move(newer);
    }
    if (latency) {
      latency->current.source = int64_t(double(frame.SystemRelativeTime().count()) *
                                      (double(latency->frequency) / 10000000.0));
      latency->current.dequeue = LatencyTrace::now();
    }
    auto size = frame.ContentSize();
    if (size.Width <= 0 || size.Height <= 0)
      return;
    auto access = frame.Surface()
                      .as<::Windows::Graphics::DirectX::Direct3D11::
                              IDirect3DDxgiInterfaceAccess>();
    ComPtr<ID3D11Texture2D> texture;
    check(access->GetInterface(IID_PPV_ARGS(&texture)), "Read capture texture");
    D3D11_TEXTURE2D_DESC desc{};
    texture->GetDesc(&desc);
    if (desc.Width != source_width || desc.Height != source_height ||
        desc.Format != source_format) {
      sync();
      wrapped.Reset();
      captured.resource.Reset();
      source_width = desc.Width;
      source_height = desc.Height;
      source_format = desc.Format;
      D3D12_RESOURCE_DESC d{};
      d.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
      d.Width = source_width;
      d.Height = source_height;
      d.DepthOrArraySize = d.MipLevels = 1;
      d.Format = source_format;
      d.SampleDesc.Count = 1;
      d.Flags = D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS |
                D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET;
      auto h = heap(D3D12_HEAP_TYPE_DEFAULT);
      check(device->CreateCommittedResource(
                &h, D3D12_HEAP_FLAG_SHARED, &d, D3D12_RESOURCE_STATE_COMMON,
                nullptr, IID_PPV_ARGS(&captured.resource)),
            "Create capture image");
      captured.slot = 0;
      HANDLE shared_handle{};
      check(device->CreateSharedHandle(captured.resource.Get(), nullptr,
                                       GENERIC_ALL, nullptr, &shared_handle),
            "Share capture image");
      ComPtr<ID3D11Texture2D> shared_texture;
      HRESULT opened = capture_device5->OpenSharedResource1(
          shared_handle, IID_PPV_ARGS(&shared_texture));
      CloseHandle(shared_handle);
      check(opened, "Open shared capture image");
      check(shared_texture.As(&wrapped), "Get shared capture resource");
      D3D12_SHADER_RESOURCE_VIEW_DESC v{};
      v.Format = source_format;
      v.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
      v.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
      v.Texture2D.MipLevels = 1;
      device->CreateShaderResourceView(captured.resource.Get(), &v, srv(0));
    }
    ID3D11Resource *resource = wrapped.Get();
    context11->CopyResource(resource, texture.Get());
    check(
        capture_context4->Signal(capture_fence11.Get(), ++capture_fence_value),
        "Signal captured frame");
    context11->Flush();
    check(queue->Wait(capture_fence12.Get(), capture_fence_value),
          "Wait for captured frame on GPU");
    sync();
    has_frame = true;
    frames++;
    if (latency) {
      latency->current.capture = frames;
      latency->current.copied = LatencyTrace::now();
    }
    last_frame_tick = GetTickCount64();
    frame.Close();
    if (size.Width != static_cast<int>(source_width) ||
        size.Height != static_cast<int>(source_height))
      pool.Recreate(capture_device, DirectXPixelFormat::B8G8R8A8UIntNormalized,
                    2, size);
  }
  void texture(uint32_t id, uint32_t w, uint32_t h, const uint8_t *rgba) {
    sync();
    auto &t = textures[id];
    if (!t.slot) {
      if (!free_slots.empty()) {
        t.slot = free_slots.back();
        free_slots.pop_back();
      } else {
        if (next_slot >= 2048)
          throw std::runtime_error("Too many UI textures");
        t.slot = next_slot++;
      }
    }
    D3D12_RESOURCE_DESC d{};
    d.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    d.Width = w;
    d.Height = h;
    d.DepthOrArraySize = d.MipLevels = 1;
    d.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
    d.SampleDesc.Count = 1;
    auto hp = heap(D3D12_HEAP_TYPE_DEFAULT);
    t.resource.Reset();
    check(device->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &d,
                                          D3D12_RESOURCE_STATE_COPY_DEST,
                                          nullptr, IID_PPV_ARGS(&t.resource)),
          "Create UI texture");
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    uint64_t bytes;
    device->GetCopyableFootprints(&d, 0, 1, 0, &fp, nullptr, nullptr, &bytes);
    auto staging = upload(bytes);
    uint8_t *ptr;
    check(staging->Map(0, nullptr, reinterpret_cast<void **>(&ptr)),
          "Map UI texture");
    for (uint32_t row = 0; row < h; row++)
      memcpy(ptr + row * fp.Footprint.RowPitch, rgba + row * w * 4, w * 4);
    staging->Unmap(0, nullptr);
    auto list = begin(0);
    D3D12_TEXTURE_COPY_LOCATION dst{};
    dst.pResource = t.resource.Get();
    dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    D3D12_TEXTURE_COPY_LOCATION src{};
    src.pResource = staging.Get();
    src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    src.PlacedFootprint = fp;
    list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
    transition(list, t.resource.Get(), D3D12_RESOURCE_STATE_COPY_DEST,
               D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
    execute(0);
    sync();
    D3D12_SHADER_RESOURCE_VIEW_DESC v{};
    v.Format = d.Format;
    v.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    v.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    v.Texture2D.MipLevels = 1;
    device->CreateShaderResourceView(t.resource.Get(), &v, srv(t.slot));
  }
  void setup(ID3D12GraphicsCommandList *list, uint32_t output) {
    auto view = rtv(output);
    list->OMSetRenderTargets(1, &view, FALSE, nullptr);
    D3D12_VIEWPORT viewport{
        0, 0, static_cast<float>(width), static_cast<float>(height), 0, 1};
    list->RSSetViewports(1, &viewport);
    D3D12_RECT rect{0, 0, static_cast<LONG>(width), static_cast<LONG>(height)};
    list->RSSetScissorRects(1, &rect);
    list->SetGraphicsRootSignature(root.Get());
    ID3D12DescriptorHeap *heaps[] = {srvs.Get()};
    list->SetDescriptorHeaps(1, heaps);
    list->IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
  }
  void render(uint32_t w, uint32_t h, float scale, const SpatpitVertex *v,
              uint32_t nv, const uint32_t *ix, uint32_t ni,
              const SpatpitDraw *draws, uint32_t nd, SpatpitOptions options) {
    if (!w || !h)
      return;
    if (latency) latency->current.start = LatencyTrace::now();
    sync();
    // While dragging an idle menu, reuse its buffers; ReShade's resize hooks
    // otherwise rebuild effect resources on every mouse movement. Commit the
    // exact size on release. Active capture and the native panel resize normally.
    if (!(options.idle_resize && !target && !has_frame && !native_overlay))
      resize(w, h);
    // A composition swapchain has no HWND sizing relationship of its own.
    // Scale its visual explicitly, then return to identity after buffer resize.
    float sx = float(w) / width, sy = float(h) / height;
    if (sx != display_scale_x || sy != display_scale_y) {
      D2D_MATRIX_3X2_F transform{sx, 0, 0, sy, 0, 0};
      check(visual->SetTransform(transform), "Scale idle menu canvas");
      check(composition->Commit(), "Commit idle menu scale");
      display_scale_x = sx;
      display_scale_y = sy;
    }
    if (motion && memcmp(&options,&previous_options,sizeof(options))) motion->invalidate();
    previous_options=options;
    try {
      // Detect a closed source even when processing is paused. Source loss
      // can also race any frame-pool operation after the validity check.
      capture_frame(options.paused != 0);
    } catch (...) {
      auto message = exception_message();
      stop();
      error = "Capture stopped: " + message + " Select another window.";
      // Continue drawing the transparent canvas and menu this frame.
    }
    uint32_t n = swap->GetCurrentBackBufferIndex();
    auto list = begin(0);
    transition(list, back[n].Get(), D3D12_RESOURCE_STATE_PRESENT,
               D3D12_RESOURCE_STATE_RENDER_TARGET);
    stamp(list, 0);
    setup(list, n);
    float clear[4] = {0, 0, 0, 0};
    list->ClearRenderTargetView(rtv(n), clear, 0, nullptr);
    float params[20] = {static_cast<float>(w),
                        static_cast<float>(h),
                        0,
                        0,
                        0,
                        0,
                        1,
                        1,
                        options.sharpness,
                        options.saturation,
                        options.contrast,
                        options.split,
                        static_cast<float>(options.effect),
                        static_cast<float>(options.comparison),
                        0,
                        0};
    if (has_frame && !options.paused) {
      transition(list, captured.resource.Get(), D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE);
      if (client_only && target) {
        RECT window{}, client{};
        POINT origin{};
        if (SUCCEEDED(DwmGetWindowAttribute(target, DWMWA_EXTENDED_FRAME_BOUNDS,
                                            &window, sizeof(window))) &&
            GetClientRect(target, &client) && ClientToScreen(target, &origin)) {
          params[4] = std::clamp(float(origin.x - window.left) / source_width,
                                 0.f, 1.f);
          params[5] = std::clamp(float(origin.y - window.top) / source_height,
                                 0.f, 1.f);
          params[6] = std::clamp(float(client.right) / source_width, 0.f,
                                 1.f - params[4]);
          params[7] = std::clamp(float(client.bottom) / source_height, 0.f,
                                 1.f - params[5]);
        }
      }
      list->SetPipelineState(image_pso.Get());
      list->SetGraphicsRoot32BitConstants(1, 20, params, 0);
      list->SetGraphicsRootDescriptorTable(0, gpu(0));
      list->DrawInstanced(3, 1, 0, 0);
      transition(list, captured.resource.Get(),
                 D3D12_RESOURCE_STATE_PIXEL_SHADER_RESOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
    }
    transition(list, back[n].Get(), D3D12_RESOURCE_STATE_RENDER_TARGET,
               D3D12_RESOURCE_STATE_PRESENT);
    stamp(list, 1);
    execute(0);
    if (effects) {
      effects->set_effects_state(options.reshade != 0 && has_frame &&
                                 !options.paused);
      auto kernel=effects->find_technique("lumenite_Kernel.fx","Lumenite_Kernel");
      bool wanted=options.neural && motion_options.motion_backend==0;
      if(kernel.handle && effects->get_technique_state(kernel)!=wanted) effects->set_technique_state(kernel,wanted);
    }
    paint_ui = [&] {
      ID3D12Resource *guide = nullptr;
      bool custom=false,fresh=false;
      if (runtime_folder.filename() == L"runtime-optiscaler" && effects &&
          options.reshade && options.neural && motion_options.enabled && has_frame && !options.paused) {
        if (!nr) nr = std::make_unique<CaptureNr>(device.Get(),runtime_folder);
        if(motion_options.motion_backend==1) {
          custom=true;
          if(!motion)motion=std::make_unique<NeuralMotion>(device.Get());
          motion->prepare(w,h);
          if(motion->error().empty()) {
            nr->prepare(w,h,motion->width(),motion->height());
            auto builds=nr->get_status().builds;
            if(builds!=motion_model_build){motion->reset();motion_model_build=builds;}
            fresh=motion_capture!=frames || !motion->cached();
          }else nr->motion_error(motion->error());
        }else {
        // ReShade's public API exposes the leaf name within the effect file.
        auto texture = effects->find_texture_variable("lumenite_Kernel.fx","tFlow");
        reshade::api::resource_view view{};
        if (texture.handle) effects->get_texture_binding(texture,&view,nullptr);
        if (view.handle) guide = reinterpret_cast<ID3D12Resource*>(effects->get_device()->get_resource_from_view(view).handle);
        if (guide) nr->prepare(w,h,unsigned(guide->GetDesc().Width),guide->GetDesc().Height);
        }
      }else {if(motion)motion->reset();if(nr)nr->history_reset();}
      auto list = begin(1);
      stamp(list, 4);
      if(custom && motion->error().empty()) {
        if(fresh) {
          guide=motion->render(list,back[n].Get(),motion_options.motion_protection != 0);
          stamp(list, 5);
          if(guide && replay_motion_hook) guide=replay_motion_hook(list,guide);
          stamp(list, 6);
          if(guide){
            nr->render(list,back[n].Get(),guide);
            stamp(list, 7);
            stamp(list, 8);
            motion->compose(list,back[n].Get());
            stamp(list, 9);
            motion_capture=frames;
          }
        }else {
          stamp(list, 5);
          stamp(list, 6);
          stamp(list, 7);
          stamp(list, 8);
          motion->replay(list,back[n].Get());
          stamp(list, 9);
        }
      }else if(guide && nr) {
        stamp(list, 5);
        if(replay_motion_hook) guide=replay_motion_hook(list,guide);
        stamp(list, 6);
        nr->render(list,back[n].Get(),guide);
        stamp(list, 7);
        stamp(list, 8);
        stamp(list, 9);
      }
      transition(list, back[n].Get(), D3D12_RESOURCE_STATE_PRESENT,
                 D3D12_RESOURCE_STATE_RENDER_TARGET);
      setup(list, n);
      // ReShade's startup messages can have opaque alpha even while effects are
      // off. A stopped/paused canvas must remain transparent, with only our
      // menu on top.
      if ((options.paused || !has_frame) && !native_overlay)
        list->ClearRenderTargetView(rtv(n), clear, 0, nullptr);
      // Some color-only ReShade shaders write alpha zero. Their RGB must still
      // cover the captured window; retain transparent idle/menu behavior. The
      // successful neural path already produces opaque output.
      if (has_frame && !options.paused && options.reshade &&
          (!options.neural || !nr || !nr->get_status().active)) {
        list->SetPipelineState(opaque_alpha_pso.Get());
        list->DrawInstanced(3, 1, 0, 0);
      }
      if (nv && ni) {
        uint64_t vb = uint64_t(nv) * sizeof(SpatpitVertex),
                 ib = uint64_t(ni) * 4;
        if (vb > vertex_capacity) {
          vertices = upload(vb);
          vertex_capacity = vb;
        }
        if (ib > index_capacity) {
          indices = upload(ib);
          index_capacity = ib;
        }
        void *ptr;
        vertices->Map(0, nullptr, &ptr);
        memcpy(ptr, v, vb);
        vertices->Unmap(0, nullptr);
        indices->Map(0, nullptr, &ptr);
        memcpy(ptr, ix, ib);
        indices->Unmap(0, nullptr);
        D3D12_VERTEX_BUFFER_VIEW vv{vertices->GetGPUVirtualAddress(),
                                    static_cast<UINT>(vb),
                                    sizeof(SpatpitVertex)};
        D3D12_INDEX_BUFFER_VIEW iv{indices->GetGPUVirtualAddress(),
                                   static_cast<UINT>(ib), DXGI_FORMAT_R32_UINT};
        list->IASetVertexBuffers(0, 1, &vv);
        list->IASetIndexBuffer(&iv);
        list->SetPipelineState(ui_pso.Get());
        params[0] = w / scale;
        params[1] = h / scale;
        float clip_x = scale * float(width) / w;
        float clip_y = scale * float(height) / h;
        list->SetGraphicsRoot32BitConstants(1, 20, params, 0);
        for (uint32_t i = 0; i < nd; i++) {
          auto found = textures.find(draws[i].texture);
          if (found == textures.end())
            continue;
          D3D12_RECT clip{
              static_cast<LONG>(std::max(0.f, draws[i].clip[0] * clip_x)),
              static_cast<LONG>(std::max(0.f, draws[i].clip[1] * clip_y)),
              static_cast<LONG>(std::min(float(width), draws[i].clip[2] * clip_x)),
              static_cast<LONG>(std::min(float(height), draws[i].clip[3] * clip_y))};
          list->RSSetScissorRects(1, &clip);
          list->SetGraphicsRootDescriptorTable(0, gpu(found->second.slot));
          list->DrawIndexedInstanced(draws[i].index_count, 1,
                                     draws[i].first_index, draws[i].base_vertex,
                                     0);
        }
      }
      transition(list, back[n].Get(), D3D12_RESOURCE_STATE_RENDER_TARGET,
                 D3D12_RESOURCE_STATE_PRESENT);
      execute(1);
    };
    if (!effects)
      paint_ui();
    if (latency) latency->current.submit = LatencyTrace::now();
    auto result = swap->Present(1, 0);
    if (latency) latency->current.returned = LatencyTrace::now();
    paint_ui = nullptr;
    check(result, "Present canvas");
    sync();
    if (latency) {
      latency->current.done = LatencyTrace::now();
      latency->finish(swap.Get());
    }
  }
  void screenshot(uint8_t *out, uint32_t w, uint32_t h) {
    if (w != width || h != height)
      throw std::runtime_error("Screenshot dimensions changed");
    sync();
    uint32_t n = (swap->GetCurrentBackBufferIndex() + 1) % 2;
    auto d = back[n]->GetDesc();
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT fp{};
    uint64_t bytes;
    device->GetCopyableFootprints(&d, 0, 1, 0, &fp, nullptr, nullptr, &bytes);
    auto hp = heap(D3D12_HEAP_TYPE_READBACK);
    auto bd = buffer_desc(bytes);
    ComPtr<ID3D12Resource> readback;
    check(device->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &bd,
                                          D3D12_RESOURCE_STATE_COPY_DEST,
                                          nullptr, IID_PPV_ARGS(&readback)),
          "Create screenshot buffer");
    auto list = begin(0);
    transition(list, back[n].Get(), D3D12_RESOURCE_STATE_PRESENT,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
    D3D12_TEXTURE_COPY_LOCATION src{};
    src.pResource = back[n].Get();
    src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    D3D12_TEXTURE_COPY_LOCATION dst{};
    dst.pResource = readback.Get();
    dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    dst.PlacedFootprint = fp;
    list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
    transition(list, back[n].Get(), D3D12_RESOURCE_STATE_COPY_SOURCE,
               D3D12_RESOURCE_STATE_PRESENT);
    execute(0);
    sync();
    uint8_t *ptr;
    readback->Map(0, nullptr, reinterpret_cast<void **>(&ptr));
    for (uint32_t row = 0; row < h; row++)
      memcpy(out + row * w * 4, ptr + row * fp.Footprint.RowPitch, w * 4);
    readback->Unmap(0, nullptr);
  }
};
static std::string exception_message() {
  try {
    throw;
  } catch (const winrt::hresult_error &e) {
    return winrt::to_string(e.message());
  } catch (const std::exception &e) {
    return e.what();
  } catch (...) {
    return "Unknown graphics error";
  }
}
static void runtime_init(reshade::api::effect_runtime *runtime) {
  if (active_engine) {
    active_engine->effects = runtime;
  }
}
static void runtime_destroy(reshade::api::effect_runtime *runtime) {
  if (active_engine && active_engine->effects == runtime)
    active_engine->effects = nullptr;
}
static bool runtime_overlay(reshade::api::effect_runtime *runtime, bool open,
                            reshade::api::input_source) {
  if (active_engine && active_engine->effects == runtime)
    active_engine->native_overlay = open;
  return false;
}
static void runtime_present(reshade::api::effect_runtime *runtime) {
  if (active_engine && active_engine->effects == runtime &&
      active_engine->paint_ui) {
    try {
      runtime->get_command_queue()->flush_immediate_command_list();
      active_engine->paint_ui();
    } catch (...) {
      active_engine->error = exception_message();
    }
  }
}
static bool runtime_technique(reshade::api::effect_runtime *runtime, reshade::api::effect_technique h, bool enabled) {
  if (!enabled || !active_engine || active_engine->effects != runtime) return false;
  char effect[256]{}, name[256]{};
  runtime->get_technique_effect_name(h, effect);
  if (!managed_color(effect)) return false;
  runtime->get_technique_name(h, name);
  const auto &allowed = active_engine->color_allowed;
  return std::find(allowed.begin(), allowed.end(), std::make_pair(std::string(effect), std::string(name))) == allowed.end();
}
static void runtime_reloaded(reshade::api::effect_runtime *runtime) {
  if (active_engine && active_engine->effects == runtime) {
    ++active_engine->effect_generation;
    active_engine->color_allowed.clear();
    // A preset reload can restore both packs from an older saved file. Bypass
    // both until the app reapplies the selected group and its uniform values.
    runtime->enumerate_techniques(nullptr, [](auto *r, auto h) {
      char effect[256]{}; r->get_technique_effect_name(h, effect);
      if (managed_color(effect) && r->get_technique_state(h)) r->set_technique_state(h, false);
    });
  }
}
extern "C" uint64_t spatpit_effect_generation(void *p) {
  return static_cast<Engine *>(p)->effect_generation;
}
extern "C" void spatpit_clear_effect_loading_error(void *p) {
  auto *e = static_cast<Engine *>(p);
  if (e->reloading) e->error.clear();
  e->reloading = false;
}
extern "C" int spatpit_effect_reloading(void *p) {
  return static_cast<Engine *>(p)->reloading ? 1 : 0;
}
extern "C" int spatpit_reload_effect(void *p, const char *effect) {
  auto *e = static_cast<Engine *>(p);
  if (!e->effects) return 0;
  e->effects->reload_effect_next_frame(effect);
  return 1;
}
extern "C" void spatpit_effects_changed(void *p) {
  auto *e = static_cast<Engine *>(p);
  if (e->motion) e->motion->reset();
  if (e->nr) e->nr->history_reset();
}
extern "C" void spatpit_color_preset(void *p, const char *list) {
  auto *e = static_cast<Engine *>(p);
  e->color_allowed.clear();
  std::string text = list ? list : "";
  for (size_t start = 0; start < text.size();) {
    auto end = text.find(',', start);
    auto item = text.substr(start, end == std::string::npos ? end : end - start);
    auto at = item.find('@');
    if (at != std::string::npos && managed_color(item.substr(at + 1)))
      e->color_allowed.emplace_back(item.substr(at + 1), item.substr(0, at));
    if (end == std::string::npos) break;
    start = end + 1;
  }
  if (!e->effects || e->color_allowed.empty()) return;
  std::vector<reshade::api::effect_technique> selected, ordered;
  for (const auto &[effect, name] : e->color_allowed) {
    auto h = e->effects->find_technique(effect.c_str(), name.c_str());
    if (h.handle) selected.push_back(h);
  }
  e->effects->enumerate_techniques(nullptr, [&](auto *, auto h) {
    if (std::none_of(selected.begin(), selected.end(), [h](auto v) { return h.handle == v.handle; })) ordered.push_back(h);
  });
  ordered.insert(ordered.end(), selected.begin(), selected.end());
  e->effects->reorder_techniques(ordered.size(), ordered.data());
}
extern "C" void *spatpit_create(void *hwnd, const wchar_t *folder) {
  Engine *e = new Engine;
  try {
    HRESULT apartment = CoInitializeEx(nullptr, COINIT_MULTITHREADED);
    if (apartment != RPC_E_CHANGED_MODE)
      check(apartment, "Initialize COM");
    e->init(static_cast<HWND>(hwnd), folder);
    return e;
  } catch (...) {
    last_creation_error = exception_message();
    delete e;
    return nullptr;
  }
}
extern "C" void spatpit_destroy(void *p) { delete static_cast<Engine *>(p); }
extern "C" void spatpit_nr_configure(void *p,const SpatpitNrOptions *requested) {
  // The user release only ships SpatpitNeuralFx motion, without protection.
  auto normalized = *requested;
  normalized.motion_backend = 1;
  normalized.motion_protection = 0;
  const auto *o = &normalized;
  auto *e=static_cast<Engine*>(p);
  if (!e->nr) e->nr=std::make_unique<CaptureNr>(e->device.Get(),e->runtime_folder);
  if(o->motion_backend!=e->motion_options.motion_backend || o->enabled!=e->motion_options.enabled) {
    if(e->motion)e->motion->reset();
    e->nr->history_reset();
  }else if(e->motion && nr_processing_changed(*o,e->motion_options))e->motion->invalidate();
  e->motion_options=*o;
  e->nr->configure(*o);
}
extern "C" void spatpit_nr_status(void *p,SpatpitNrStatus *s) {
  auto *e=static_cast<Engine*>(p);*s=e->nr?e->nr->get_status():SpatpitNrStatus{};
  if(e->motion)s->motion_frames=e->motion->dispatches();
}
extern "C" void spatpit_nr_reset(void *p) { auto *e=static_cast<Engine*>(p);if(e->nr)e->nr->retry(); }
extern "C" const char *spatpit_error(void *p) {
  return p ? static_cast<Engine *>(p)->error.c_str()
           : last_creation_error.c_str();
}
#define TRY_ENGINE                                                             \
  auto *e = static_cast<Engine *>(p);                                          \
  try
#define CATCH_ENGINE                                                           \
  catch (...) {                                                                \
    e->error = exception_message();                                            \
    return 0;                                                                  \
  }                                                                            \
  return 1
extern "C" int spatpit_capture(void *p, void *target, int client) {
  TRY_ENGINE { e->capture(static_cast<HWND>(target), client != 0); }
  CATCH_ENGINE;
}
extern "C" void spatpit_stop(void *p) { static_cast<Engine *>(p)->stop(); }
extern "C" int spatpit_texture(void *p, uint32_t id, uint32_t w, uint32_t h,
                              const uint8_t *rgba) {
  TRY_ENGINE { e->texture(id, w, h, rgba); }
  CATCH_ENGINE;
}
extern "C" void spatpit_free_texture(void *p, uint32_t id) {
  auto *e = static_cast<Engine *>(p);
  auto it = e->textures.find(id);
  if (it != e->textures.end()) {
    e->free_slots.push_back(it->second.slot);
    e->textures.erase(it);
  }
}
extern "C" int spatpit_render(void *p, uint32_t w, uint32_t h, float scale,
                             const SpatpitVertex *v, uint32_t nv,
                             const uint32_t *i, uint32_t ni,
                             const SpatpitDraw *d, uint32_t nd,
                             SpatpitOptions o) {
  TRY_ENGINE { e->render(w, h, scale, v, nv, i, ni, d, nd, o); }
  CATCH_ENGINE;
}
extern "C" int spatpit_screenshot(void *p, uint8_t *rgba, uint32_t w,
                                 uint32_t h) {
  TRY_ENGINE { e->screenshot(rgba, w, h); }
  CATCH_ENGINE;
}
extern "C" void spatpit_status(void *p, SpatpitStatus *out) {
  auto *e = static_cast<Engine *>(p);
  *out = {e->session ? 1u : 0u,
          e->has_frame ? 1u : 0u,
          e->source_width,
          e->source_height,
          e->effects ? 1u : 0u,
          0,
          e->frames,
          e->has_frame ? GetTickCount64() - e->last_frame_tick : 0};
  if (e->effects)
    e->effects->enumerate_techniques(
        nullptr,
        [](auto *, auto, void *ptr) { (*static_cast<uint32_t *>(ptr))++; },
        &out->techniques);
}
extern "C" int spatpit_load_reshade(void *p, const wchar_t *, const char *) {
  TRY_ENGINE {
    if (!e->effects)
      throw std::runtime_error("ReShade is unavailable. Import the runtime "
                               "files, then restart NeuralLayer.");
  }
  CATCH_ENGINE;
}
extern "C" void spatpit_reshade_preset(void *p, const char *path) {
  auto *e = static_cast<Engine *>(p);
  if (e->effects) {
    e->effects->set_current_preset_path(path);
    runtime_reloaded(e->effects);
  }
}

extern "C" void
spatpit_techniques(void *p, void (*callback)(const SpatpitTechnique *, void *),
                  void *data) {
  auto *e = static_cast<Engine *>(p);
  if (!e->effects)
    return;
  e->effects->enumerate_techniques(nullptr, [&](auto *runtime, auto handle) {
    SpatpitTechnique t{};
    runtime->get_technique_name(handle, t.name);
    runtime->get_technique_effect_name(handle, t.effect);
    t.enabled = runtime->get_technique_state(handle);
    callback(&t, data);
  });
}
extern "C" void spatpit_uniforms(void *p, const char *effect,
                                void (*callback)(const SpatpitUniform *, void *),
                                void *data) {
  auto *e = static_cast<Engine *>(p);
  if (!e->effects)
    return;
  e->effects->enumerate_uniform_variables(effect, [&](auto *runtime,
                                                      auto handle) {
    char source[128]{};
    bool hidden = false;
    runtime->get_annotation_string_from_uniform_variable(handle, "source",
                                                         source);
    runtime->get_annotation_bool_from_uniform_variable(handle, "ui_hidden",
                                                       &hidden, 1);
    if (source[0] || hidden)
      return;
    reshade::api::format format;
    uint32_t rows, columns, length;
    runtime->get_uniform_variable_type(handle, &format, &rows, &columns,
                                       &length);
    if (rows > 4 || columns != 1 || length > 1)
      return;
    SpatpitUniform u{};
    u.components = rows;
    u.kind = format == reshade::api::format::r32_typeless ? 0
             : format == reshade::api::format::r32_sint   ? 1
             : format == reshade::api::format::r32_uint   ? 2
                                                          : 3;
    runtime->get_uniform_variable_name(handle, u.name);
    runtime->get_annotation_string_from_uniform_variable(handle, "ui_label",
                                                         u.label);
    runtime->get_annotation_string_from_uniform_variable(handle, "ui_category",
                                                         u.category);
    runtime->get_annotation_string_from_uniform_variable(handle, "ui_tooltip",
                                                         u.tooltip);
    runtime->get_annotation_string_from_uniform_variable(handle, "ui_items",
                                                         u.items);
    u.bounded = runtime->get_annotation_float_from_uniform_variable(
                    handle, "ui_min", u.minimum, rows) &&
                runtime->get_annotation_float_from_uniform_variable(
                    handle, "ui_max", u.maximum, rows);
    if (u.kind == 0) {
      bool values[4]{};
      runtime->get_uniform_value_bool(handle, values, rows);
      for (uint32_t i = 0; i < rows; i++)
        u.values[i] = values[i] ? 1.f : 0.f;
    } else if (u.kind == 1) {
      int32_t values[4]{};
      runtime->get_uniform_value_int(handle, values, rows);
      for (uint32_t i = 0; i < rows; i++)
        u.values[i] = float(values[i]);
    } else if (u.kind == 2) {
      uint32_t values[4]{};
      runtime->get_uniform_value_uint(handle, values, rows);
      for (uint32_t i = 0; i < rows; i++)
        u.values[i] = float(values[i]);
    } else
      runtime->get_uniform_value_float(handle, u.values, rows);
    callback(&u, data);
  });
}
extern "C" int spatpit_set_technique(void *p, const char *effect,
                                    const char *name, int enabled) {
  TRY_ENGINE {
    e->reloading = false;
    if (!e->effects)
      throw std::runtime_error("ReShade is unavailable");
    auto h = e->effects->find_technique(effect, name);
    if (h == 0) {
      e->reloading = true;
      throw std::runtime_error(std::string("Effect is reloading; try again: ") + effect + " / " + name);
    }
    if (runtime_technique(e->effects, h, enabled != 0))
      throw std::runtime_error("Choose a single visual preset in the ReShade tab.");
    e->effects->set_technique_state(h, enabled != 0);
  }
  CATCH_ENGINE;
}
extern "C" int spatpit_set_uniform(void *p, const char *effect, const char *name,
                                  const float *values, int reset) {
  TRY_ENGINE {
    e->reloading = false;
    if (!e->effects)
      throw std::runtime_error("ReShade is unavailable");
    auto h = e->effects->find_uniform_variable(effect, name);
    if (h == 0) {
      e->reloading = true;
      throw std::runtime_error("Shader is reloading; try again");
    }
    if (reset) {
      e->effects->reset_uniform_value(h);
      return 1;
    }
    reshade::api::format format;
    uint32_t rows, columns, length;
    e->effects->get_uniform_variable_type(h, &format, &rows, &columns, &length);
    if (rows > 4 || columns != 1 || length > 1)
      throw std::runtime_error("Unsupported shader setting shape");
    if (format == reshade::api::format::r32_typeless) {
      bool v[4]{};
      for (uint32_t i = 0; i < rows; i++)
        v[i] = values[i] != 0;
      e->effects->set_uniform_value_bool(h, v, rows);
    } else if (format == reshade::api::format::r32_sint) {
      int32_t v[4]{};
      for (uint32_t i = 0; i < rows; i++)
        v[i] = static_cast<int32_t>(
            std::clamp(double(values[i]), -2147483648., 2147483647.));
      e->effects->set_uniform_value_int(h, v, rows);
    } else if (format == reshade::api::format::r32_uint) {
      uint32_t v[4]{};
      for (uint32_t i = 0; i < rows; i++)
        v[i] = static_cast<uint32_t>(
            std::clamp(double(values[i]), 0., 4294967295.));
      e->effects->set_uniform_value_uint(h, v, rows);
    } else
      e->effects->set_uniform_value_float(h, values, rows);
  }
  CATCH_ENGINE;
}
extern "C" int spatpit_save_preset(void *p) {
  TRY_ENGINE {
    if (!e->effects)
      throw std::runtime_error("ReShade is unavailable");
    e->effects->save_current_preset();
  }
  CATCH_ENGINE;
}

extern "C" int spatpit_native_overlay(void *p, int open) {
  TRY_ENGINE {
    if (!e->effects ||
        !e->effects->open_overlay(open != 0, reshade::api::input_source::mouse))
      throw std::runtime_error("ReShade's live panel is unavailable");
    e->native_overlay = open != 0;
  }
  CATCH_ENGINE;
}
extern "C" int spatpit_native_overlay_active(void *p) {
  return static_cast<Engine *>(p)->native_overlay;
}

#include "neural_app_replay.inl"
#include "latency_bench.inl"
#include "idle_resize_test.inl"
#include "reshade_replay.inl"
