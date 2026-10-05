#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include "neural_motion.h"
#include "neural_motion_shader.h"
#include <algorithm>
#include <array>
#include <cstring>
#include <stdexcept>
#include <vector>
#include <wrl/client.h>
using Microsoft::WRL::ComPtr;
namespace {
void checked(HRESULT h) {
  if (FAILED(h))
    throw std::runtime_error("Custom motion HRESULT " +
                             std::to_string(unsigned(h)));
}
void barrier(ID3D12GraphicsCommandList *l, ID3D12Resource *r,
             D3D12_RESOURCE_STATES a, D3D12_RESOURCE_STATES b) {
  D3D12_RESOURCE_BARRIER v{};
  v.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
  v.Transition = {r, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, a, b};
  l->ResourceBarrier(1, &v);
}
} // namespace
struct NeuralMotion::Impl {
  ID3D12Device *device;
  unsigned w = 0, h = 0, gw = 0, gh = 0, frame = 0, slot = 0, latest = 0;
  uint64_t count = 0;
  bool reset = true, valid = false, failed = false;
  std::string message;
  std::array<std::array<ComPtr<ID3D12Resource>, 5>, 2> pyramid, fields;
  std::array<ComPtr<ID3D12Resource>, 2> trust;
  ComPtr<ID3D12Resource> guide, composed;
  ComPtr<ID3D12DescriptorHeap> heap;
  ComPtr<ID3D12RootSignature> root;
  ComPtr<ID3D12PipelineState> pso;
  explicit Impl(ID3D12Device *d) : device(d) {}
  ComPtr<ID3D12Resource> texture(unsigned x, unsigned y, DXGI_FORMAT format) {
    D3D12_RESOURCE_DESC d{};
    d.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
    d.Width = x;
    d.Height = y;
    d.DepthOrArraySize = d.MipLevels = d.SampleDesc.Count = 1;
    d.Format = format;
    d.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
    D3D12_HEAP_PROPERTIES p{};
    p.Type = D3D12_HEAP_TYPE_DEFAULT;
    p.CreationNodeMask = p.VisibleNodeMask = 1;
    ComPtr<ID3D12Resource> r;
    checked(device->CreateCommittedResource(&p, D3D12_HEAP_FLAG_NONE, &d,
                                            D3D12_RESOURCE_STATE_COMMON,
                                            nullptr, IID_PPV_ARGS(&r)));
    return r;
  }
  void prepare(unsigned x, unsigned y) {
    if (w == x && h == y && (pso || failed))
      return;
    w = x;
    h = y;
    gw = (x + 7) / 8;
    gh = (y + 7) / 8;
    frame = 0;
    reset = true;
    valid = false;
    failed = false;
    try {
      for (unsigned i = 0; i < 5; ++i) {
        unsigned pw = std::max(1u, (w + (1u << i) - 1) >> i),
                 ph = std::max(1u, (h + (1u << i) - 1) >> i);
        for (unsigned k = 0; k < 2; ++k) {
          pyramid[k][i] = texture(pw, ph, DXGI_FORMAT_R16_FLOAT);
          fields[k][i] = texture((pw + 7) / 8, (ph + 7) / 8,
                                 DXGI_FORMAT_R16G16B16A16_FLOAT);
        }
      }
      for (auto &t : trust)
        t = texture(gw, gh, DXGI_FORMAT_R16G16B16A16_FLOAT);
      guide = texture(gw, gh, DXGI_FORMAT_R16G16_FLOAT);
      composed = texture(w, h, DXGI_FORMAT_R8G8B8A8_UNORM);
      D3D12_DESCRIPTOR_HEAP_DESC hd{};
      hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
      hd.NumDescriptors = 32 * 8;
      hd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
      checked(device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&heap)));
      if (!pso) {
        D3D12_DESCRIPTOR_RANGE ranges[] = {
            {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 6, 0, 0, 0},
            {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 2, 0, 0, 6}};
        D3D12_ROOT_PARAMETER params[2]{};
        params[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
        params[0].DescriptorTable = {2, ranges};
        params[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_32BIT_CONSTANTS;
        params[1].Constants = {0, 0, 8};
        D3D12_STATIC_SAMPLER_DESC s{};
        s.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
        s.AddressU = s.AddressV = s.AddressW = D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
        s.ComparisonFunc = D3D12_COMPARISON_FUNC_NEVER;
        s.MaxAnisotropy = 1;
        s.MaxLOD = D3D12_FLOAT32_MAX;
        D3D12_ROOT_SIGNATURE_DESC rd{};
        rd.NumParameters = 2;
        rd.pParameters = params;
        rd.NumStaticSamplers = 1;
        rd.pStaticSamplers = &s;
        ComPtr<ID3DBlob> blob, errors;
        checked(D3D12SerializeRootSignature(&rd, D3D_ROOT_SIGNATURE_VERSION_1,
                                            &blob, &errors));
        checked(device->CreateRootSignature(0, blob->GetBufferPointer(),
                                            blob->GetBufferSize(),
                                            IID_PPV_ARGS(&root)));
        D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
        pd.pRootSignature = root.Get();
        pd.CS = {neural_motion_shader, sizeof(neural_motion_shader)};
        checked(device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pso)));
      }
      message.clear();
    } catch (const std::exception &e) {
      message = e.what();
      failed = true;
    }
  }
  void dispatch(ID3D12GraphicsCommandList *l, unsigned mode, unsigned level,
                unsigned direction,
                const std::array<ID3D12Resource *, 6> &inputs,
                ID3D12Resource *out) {
    if (slot >= 32)
      throw std::runtime_error("Custom motion descriptor overflow");
    auto cpu = heap->GetCPUDescriptorHandleForHeapStart();
    auto gpu = heap->GetGPUDescriptorHandleForHeapStart();
    auto step = device->GetDescriptorHandleIncrementSize(
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    cpu.ptr += slot * 8 * step;
    gpu.ptr += slot * 8 * step;
    ++slot;
    std::vector<ID3D12Resource *> reads;
    for (auto *r : inputs) {
      D3D12_SHADER_RESOURCE_VIEW_DESC d{};
      d.Format = r->GetDesc().Format;
      d.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
      d.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
      d.Texture2D.MipLevels = 1;
      device->CreateShaderResourceView(r, &d, cpu);
      cpu.ptr += step;
      if (std::find(reads.begin(), reads.end(), r) == reads.end())
        reads.push_back(r);
    }
    for (auto *r : {out, guide.Get()}) {
      D3D12_UNORDERED_ACCESS_VIEW_DESC d{};
      d.Format = r->GetDesc().Format;
      d.ViewDimension = D3D12_UAV_DIMENSION_TEXTURE2D;
      device->CreateUnorderedAccessView(r, nullptr, &d, cpu);
      cpu.ptr += step;
    }
    for (auto *r : reads)
      barrier(l, r, D3D12_RESOURCE_STATE_COMMON,
              D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    barrier(l, out, D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    barrier(l, guide.Get(), D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    ID3D12DescriptorHeap *hs[] = {heap.Get()};
    l->SetDescriptorHeaps(1, hs);
    l->SetComputeRootSignature(root.Get());
    l->SetPipelineState(pso.Get());
    l->SetComputeRootDescriptorTable(0, gpu);
    auto d = out->GetDesc();
    unsigned constants[] = {w,    h,     unsigned(d.Width), d.Height,
                            mode, level, unsigned(reset),   direction};
    l->SetComputeRoot32BitConstants(1, 8, constants, 0);
    l->Dispatch((unsigned(d.Width) + 7) / 8, (d.Height + 7) / 8, 1);
    for (auto *r : reads)
      barrier(l, r, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
              D3D12_RESOURCE_STATE_COMMON);
    barrier(l, out, D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_COMMON);
    barrier(l, guide.Get(), D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_COMMON);
  }
  ID3D12Resource *render(ID3D12GraphicsCommandList *l, ID3D12Resource *color, bool protect) {
    if (failed || !pso)
      return nullptr;
    slot = 0;
    latest = frame % 2;
    unsigned prev = 1 - latest;
    for (unsigned i = 0; i < 5; ++i) {
      auto *src = i ? pyramid[latest][i - 1].Get() : color;
      dispatch(l, 0, i, 0, {src, src, src, src, src, src},
               pyramid[latest][i].Get());
    }
    // Initialize both image histories before they can be sampled.
    if (reset)
      for (unsigned i = 0; i < 5; ++i) {
        auto *a = pyramid[latest][i].Get(), *b = pyramid[prev][i].Get();
        barrier(l, a, D3D12_RESOURCE_STATE_COMMON,
                D3D12_RESOURCE_STATE_COPY_SOURCE);
        barrier(l, b, D3D12_RESOURCE_STATE_COMMON,
                D3D12_RESOURCE_STATE_COPY_DEST);
        l->CopyResource(b, a);
        barrier(l, a, D3D12_RESOURCE_STATE_COPY_SOURCE,
                D3D12_RESOURCE_STATE_COMMON);
        barrier(l, b, D3D12_RESOURCE_STATE_COPY_DEST,
                D3D12_RESOURCE_STATE_COMMON);
      }
    // Reverse flow validates consistency at half resolution; only the backward
    // guide consumed by the model needs the full-resolution refinement.
    for (unsigned direction = 0; direction < 2; ++direction)
      for (int i = 4; i >= (direction ? 1 : 0); --i) {
        auto *a = pyramid[direction ? prev : latest][i].Get(),
             *b = pyramid[direction ? latest : prev][i].Get();
        auto *seed = i == 4 ? a : fields[direction][i + 1].Get();
        dispatch(l, 1, i, direction, {a, b, seed, a, a, a},
                 fields[direction][i].Get());
      }
    dispatch(l, 2, 0, unsigned(protect),
             {pyramid[latest][0].Get(), pyramid[prev][0].Get(),
              fields[0][0].Get(), fields[0][0].Get(), fields[1][1].Get(),
              trust[prev].Get()},
             trust[latest].Get());
    ++frame;
    ++count;
    reset = false;
    valid = false;
    barrier(l, guide.Get(), D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    return guide.Get();
  }
  void copy(ID3D12GraphicsCommandList *l, ID3D12Resource *dst,
            ID3D12Resource *src) {
    barrier(l, src, D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_COPY_SOURCE);
    barrier(l, dst, D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_COPY_DEST);
    l->CopyResource(dst, src);
    barrier(l, src, D3D12_RESOURCE_STATE_COPY_SOURCE,
            D3D12_RESOURCE_STATE_COMMON);
    barrier(l, dst, D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_COMMON);
  }
};
NeuralMotion::NeuralMotion(ID3D12Device *d) : impl(std::make_unique<Impl>(d)) {}
NeuralMotion::~NeuralMotion() = default;
void NeuralMotion::prepare(unsigned w, unsigned h) { impl->prepare(w, h); }
ID3D12Resource *NeuralMotion::render(ID3D12GraphicsCommandList *l,
                                     ID3D12Resource *c, bool protect) {
  return impl->render(l, c, protect);
}
void NeuralMotion::compose(ID3D12GraphicsCommandList *l, ID3D12Resource *color) {
  barrier(l, impl->guide.Get(), D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
          D3D12_RESOURCE_STATE_COMMON);
  impl->copy(l, impl->composed.Get(), color);
  impl->valid = true;
}
bool NeuralMotion::replay(ID3D12GraphicsCommandList *l, ID3D12Resource *c) {
  if (!impl->valid || impl->reset || impl->failed)
    return false;
  impl->copy(l, c, impl->composed.Get());
  return true;
}
void NeuralMotion::reset() {
  impl->reset = true;
  impl->valid = false;
}
void NeuralMotion::invalidate() { impl->valid = false; }
bool NeuralMotion::cached() const {
  return impl->valid && !impl->reset && !impl->failed;
}
unsigned NeuralMotion::width() const { return impl->gw; }
unsigned NeuralMotion::height() const { return impl->gh; }
uint64_t NeuralMotion::dispatches() const { return impl->count; }
std::string NeuralMotion::error() const { return impl->message; }
ID3D12Resource *NeuralMotion::output() const { return impl->guide.Get(); }
ID3D12Resource *NeuralMotion::confidence() const { return impl->trust[impl->latest].Get(); }
