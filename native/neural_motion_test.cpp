#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include "neural_motion.h"
#include <algorithm>
#include <cmath>
#include <cstring>
#include <d3d12.h>
#include <d3d12sdklayers.h>
#include <dxgi1_6.h>
#include <filesystem>
#include <fstream>
#include <stdexcept>
#include <vector>
#include <wrl/client.h>
using Microsoft::WRL::ComPtr;
namespace {
void ok(HRESULT h) {
  if (FAILED(h))
    throw std::runtime_error("GPU test HRESULT " + std::to_string(unsigned(h)));
}
void require(bool b, const char *m) {
  if (!b)
    throw std::runtime_error(m);
}
float half(uint16_t v) {
  float m = float(v & 1023);
  int e = (v >> 10) & 31;
  float f = e == 0    ? std::ldexp(m, -24)
            : e == 31 ? INFINITY
                      : std::ldexp(1.f + m / 1024.f, e - 15);
  return v & 32768 ? -f : f;
}
uint32_t random(uint32_t x) {
  x ^= x >> 16;
  x *= 0x7feb352d;
  x ^= x >> 15;
  x *= 0x846ca68b;
  return x ^ (x >> 16);
}
void transition(ID3D12GraphicsCommandList *l, ID3D12Resource *r,
                D3D12_RESOURCE_STATES a, D3D12_RESOURCE_STATES b) {
  D3D12_RESOURCE_BARRIER t{};
  t.Type = D3D12_RESOURCE_BARRIER_TYPE_TRANSITION;
  t.Transition = {r, D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES, a, b};
  l->ResourceBarrier(1, &t);
}
} // namespace
#include "optiscaler.h"
static int run_motion(const char *output, const char *inputPath = nullptr,
                      const char *videoPath = nullptr, unsigned videoWidth = 0,
                      unsigned videoHeight = 0,
                      const char *runtimePath = nullptr, bool protection = true,
                      bool zeroMotion = false, unsigned style = 0) {
  std::ofstream log(output);
  HANDLE event = nullptr;
  try {
    ComPtr<ID3D12Debug> debug;
    if (SUCCEEDED(D3D12GetDebugInterface(IID_PPV_ARGS(&debug)))) {
      debug->EnableDebugLayer();
      log << "D3D12 debug layer enabled\n";
    }
    ComPtr<IDXGIFactory6> factory;
    ok(CreateDXGIFactory2(0, IID_PPV_ARGS(&factory)));
    ComPtr<IDXGIAdapter1> adapter;
    ok(factory->EnumAdapterByGpuPreference(
        0, DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE, IID_PPV_ARGS(&adapter)));
    ComPtr<ID3D12Device> device;
    ok(D3D12CreateDevice(adapter.Get(), D3D_FEATURE_LEVEL_12_0,
                         IID_PPV_ARGS(&device)));
    ComPtr<ID3D12InfoQueue> diagnostics;
    device.As(&diagnostics);
    DXGI_ADAPTER_DESC1 ad{};
    adapter->GetDesc1(&ad);
    log << "Adapter vendor=" << ad.VendorId << " device=" << ad.DeviceId
        << "\n";
    ComPtr<ID3D12CommandQueue> queue;
    D3D12_COMMAND_QUEUE_DESC q{};
    ok(device->CreateCommandQueue(&q, IID_PPV_ARGS(&queue)));
    ComPtr<ID3D12CommandAllocator> alloc;
    ok(device->CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT,
                                      IID_PPV_ARGS(&alloc)));
    ComPtr<ID3D12GraphicsCommandList> list;
    ok(device->CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, alloc.Get(),
                                 nullptr, IID_PPV_ARGS(&list)));
    ok(list->Close());
    ComPtr<ID3D12Fence> fence;
    ok(device->CreateFence(0, D3D12_FENCE_FLAG_NONE, IID_PPV_ARGS(&fence)));
    event = CreateEvent(nullptr, FALSE, FALSE, nullptr);
    require(event, "Create GPU test fence event");
    uint64_t serial = 0;
    auto wait = [&] {
      ok(queue->Signal(fence.Get(), ++serial));
      ok(fence->SetEventOnCompletion(serial, event));
      require(WaitForSingleObject(event, 10000) == WAIT_OBJECT_0,
              "GPU test timed out");
    };
    auto buffer = [&](size_t size, D3D12_HEAP_TYPE type,
                      D3D12_RESOURCE_STATES state) {
      D3D12_HEAP_PROPERTIES p{};
      p.Type = type;
      p.CreationNodeMask = p.VisibleNodeMask = 1;
      D3D12_RESOURCE_DESC d{};
      d.Dimension = D3D12_RESOURCE_DIMENSION_BUFFER;
      d.Width = size;
      d.Height = d.DepthOrArraySize = d.MipLevels = d.SampleDesc.Count = 1;
      d.Layout = D3D12_TEXTURE_LAYOUT_ROW_MAJOR;
      ComPtr<ID3D12Resource> r;
      ok(device->CreateCommittedResource(&p, D3D12_HEAP_FLAG_NONE, &d, state,
                                         nullptr, IID_PPV_ARGS(&r)));
      return r;
    };
    NeuralMotion flow(device.Get());
    std::unique_ptr<CaptureNr> nr;
    std::vector<uint8_t> recorded;
    std::ofstream video;
    ComPtr<ID3D12Resource> videoReadback;
    unsigned w = 512, h = 384;
    ComPtr<ID3D12Resource> color, upload, readback;
    D3D12_PLACED_SUBRESOURCE_FOOTPRINT inputLayout{}, outputLayout{};
    auto resize = [&](unsigned width, unsigned height) {
      w = width;
      h = height;
      flow.prepare(w, h);
      require(flow.error().empty(), flow.error().c_str());
      D3D12_HEAP_PROPERTIES p{};
      p.Type = D3D12_HEAP_TYPE_DEFAULT;
      p.CreationNodeMask = p.VisibleNodeMask = 1;
      D3D12_RESOURCE_DESC d{};
      d.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
      d.Width = w;
      d.Height = h;
      d.DepthOrArraySize = d.MipLevels = d.SampleDesc.Count = 1;
      d.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
      color.Reset();
      ok(device->CreateCommittedResource(&p, D3D12_HEAP_FLAG_NONE, &d,
                                         D3D12_RESOURCE_STATE_COMMON, nullptr,
                                         IID_PPV_ARGS(&color)));
      uint64_t bytes;
      device->GetCopyableFootprints(&d, 0, 1, 0, &inputLayout, nullptr, nullptr,
                                    &bytes);
      upload = buffer(bytes, D3D12_HEAP_TYPE_UPLOAD,
                      D3D12_RESOURCE_STATE_GENERIC_READ);
      d = flow.output()->GetDesc();
      device->GetCopyableFootprints(&d, 0, 1, 0, &outputLayout, nullptr,
                                    nullptr, &bytes);
      readback = buffer(bytes, D3D12_HEAP_TYPE_READBACK,
                        D3D12_RESOURCE_STATE_COPY_DEST);
    };
    ComPtr<ID3D12QueryHeap> queries;
    D3D12_QUERY_HEAP_DESC qd{};
    qd.Type = D3D12_QUERY_HEAP_TYPE_TIMESTAMP;
    qd.Count = 2;
    ok(device->CreateQueryHeap(&qd, IID_PPV_ARGS(&queries)));
    auto ticks =
        buffer(16, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
    uint64_t frequency;
    ok(queue->GetTimestampFrequency(&frequency));
    double lastMs = 0;
    double borderMax = 0;
    std::vector<std::pair<float, float>> borderValues;
    int scene = 0, phase = 0;
    auto frame = [&](float dx, float dy) {
      uint8_t *data;
      ok(upload->Map(0, nullptr, (void **)&data));
      for (unsigned y = 0; y < h; y++)
        for (unsigned x = 0; x < w; x++) {
          if (!recorded.empty()) {
            memcpy(data + y * inputLayout.Footprint.RowPitch + x * 4,
                   recorded.data() + (size_t(y) * w + x) * 4, 4);
            continue;
          }
          unsigned sx = unsigned(int(x) - dx + 4096),
                   sy = unsigned(int(y) - dy + 4096);
          unsigned v = random((sx / 8) + 8191 * (sy / 8));
          auto p = data + y * inputLayout.Footprint.RowPitch + x * 4;
          p[0] = uint8_t(30 + v % 196);
          p[1] = uint8_t(30 + (v >> 8) % 196);
          p[2] = uint8_t(30 + (v >> 16) % 196);
          p[3] = 255;
          if (scene == 6)
            for (int c = 0; c < 3; ++c)
              p[c] = uint8_t(128 + (int(p[c]) - 128) / 4);
          if (scene == 7 || scene == 8)
            for (int c = 0; c < 3; ++c)
              p[c] = scene == 7
                         ? uint8_t(40 + (x % 96) * 2)
                         : uint8_t(220 + 20 * std::cos((float(x) + float(y)) *
                                                       .785398163f));
          if (scene == 1 || scene == 3 || scene == 5 || scene == 11) {
            float px = float(x) - (scene == 3 && x >= w / 2 ? 0 : dx);
            float py = float(y) - (scene == 3 && x >= w / 2 ? 0 : dy);
            for (int c = 0; c < 3; ++c)
              p[c] = uint8_t(128 + 35 * std::sin(px * (.13f + c * .017f)) +
                             35 * std::cos(py * (.17f + c * .013f)) +
                             25 * std::sin(px * .071f + py * .093f + c));
            if (scene == 5)
              for (int c = 0; c < 3; ++c)
                p[c] = uint8_t(128 + (int(p[c]) - 128) / 4);
            if (scene == 11) {
              // A moving narrow decoration has fewer supporting guide cells
              // than a whole-frame pan, despite having resolvable texture.
              float alpha =
                  std::clamp(12.f - std::abs(py - float(h) * .5f), 0.f, 1.f);
              for (int c = 0; c < 3; ++c)
                p[c] = uint8_t(48 + alpha * (float(p[c]) - 48));
            }
          } else if (scene == 10) {
            // Thin antialiased slanted bars with block-correlated changes in
            // quantization. Geometry stays fixed while encoded values vary.
            float line = std::fmod(float(y) + float(x) * .1f, 64.f);
            float coverage = std::max(0.f, 1.f - std::abs(line - 24.f));
            coverage =
                std::max(coverage, std::max(0.f, 1.f - std::abs(line - 36.f)));
            int noise =
                int(random(x / 8 + (y / 8) * 8191 + unsigned(phase) * 7919) %
                    7) -
                3;
            for (int c = 0; c < 3; ++c)
              p[c] = uint8_t(40 + 120 * coverage + noise);
          } else if (scene == 9) {
            float stripe = std::pow(
                .5f + .5f * std::cos((float(x) - dx + float(y) - dy) * .26f),
                12.f);
            for (int c = 0; c < 3; ++c)
              p[c] = uint8_t(230 - 70 * stripe);
          } else if (scene == 2) {
            // Stationary textured surfaces with independently pulsing lights.
            float lx = float(int(x % 96) - 48), ly = float(int(y % 96) - 48);
            float light = std::exp(-(lx * lx + ly * ly) / 180.f) *
                          (phase % 2 ? 220.f : 0.f);
            for (int c = 0; c < 3; ++c)
              p[c] = uint8_t(std::min(255.f, p[c] + light));
          } else if (scene == 4) {
            // A stationary menu's low-contrast repeating diagonal pattern,
            // with small frame-dependent capture/quantization differences.
            float stripe = std::pow(
                .5f + .5f * std::cos((float(x) + float(y)) * .26f), 12.f);
            int noise = int(random(x + y * w + unsigned(phase) * 7919) % 5) - 2;
            for (int c = 0; c < 3; ++c)
              p[c] = uint8_t(238 - 15 * stripe + noise);
          }
        }
      upload->Unmap(0, nullptr);
      ok(alloc->Reset());
      ok(list->Reset(alloc.Get(), nullptr));
      transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_COPY_DEST);
      D3D12_TEXTURE_COPY_LOCATION src{}, dst{};
      src.pResource = upload.Get();
      src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
      src.PlacedFootprint = inputLayout;
      dst.pResource = color.Get();
      dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
      list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
      transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COPY_DEST,
                 D3D12_RESOURCE_STATE_COMMON);
      list->EndQuery(queries.Get(), D3D12_QUERY_TYPE_TIMESTAMP, 0);
      if (zeroMotion)
        flow.reset();
      auto result = flow.render(list.Get(), color.Get(), protection);
      require(result, flow.error().c_str());
      if (nr)
        nr->render(list.Get(), color.Get(), result);
      flow.compose(list.Get(), color.Get());
      transition(list.Get(), result, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
      list->EndQuery(queries.Get(), D3D12_QUERY_TYPE_TIMESTAMP, 1);
      list->ResolveQueryData(queries.Get(), D3D12_QUERY_TYPE_TIMESTAMP, 0, 2,
                             ticks.Get(), 0);
      transition(list.Get(), result,
                 D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                 D3D12_RESOURCE_STATE_COPY_SOURCE);
      src.pResource = result;
      src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
      src.SubresourceIndex = 0;
      dst.pResource = readback.Get();
      dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
      dst.PlacedFootprint = outputLayout;
      list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
      transition(list.Get(), result, D3D12_RESOURCE_STATE_COPY_SOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
      if (videoReadback) {
        transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COMMON,
                   D3D12_RESOURCE_STATE_COPY_SOURCE);
        src.pResource = color.Get();
        src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
        src.SubresourceIndex = 0;
        dst.pResource = videoReadback.Get();
        dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
        dst.PlacedFootprint = inputLayout;
        list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
        transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COPY_SOURCE,
                   D3D12_RESOURCE_STATE_COMMON);
      }
      ok(list->Close());
      ID3D12CommandList *lists[] = {list.Get()};
      queue->ExecuteCommandLists(1, lists);
      wait();
      uint64_t *t;
      ok(ticks->Map(0, nullptr, (void **)&t));
      lastMs = double(t[1] - t[0]) * 1000 / double(frequency);
      ticks->Unmap(0, nullptr);
      ok(readback->Map(0, nullptr, (void **)&data));
      std::vector<std::pair<float, float>> values;
      borderMax = 0;
      borderValues.clear();
      for (unsigned y = 0; y < flow.height(); y++)
        for (unsigned x = 0; x < flow.width(); x++) {
          auto v =
              (uint16_t *)(data + y * outputLayout.Footprint.RowPitch + x * 4);
          float a = half(v[0]) * w, b = half(v[1]) * h;
          require(std::isfinite(a) && std::isfinite(b),
                  "Non-finite optical flow");
          if (x == 0 || y == 0 || x + 1 == flow.width() ||
              y + 1 == flow.height()) {
            borderMax = std::max(borderMax, double(std::hypot(a, b)));
            borderValues.emplace_back(a, b);
          }
          if (x >= 4 && y >= 4 && x + 4 < flow.width() && y + 4 < flow.height())
            values.emplace_back(a, b);
        }
      readback->Unmap(0, nullptr);
      if (videoReadback) {
        ok(videoReadback->Map(0, nullptr, (void **)&data));
        for (unsigned y = 0; y < h; ++y)
          video.write((const char *)data + y * inputLayout.Footprint.RowPitch,
                      w * 4);
        videoReadback->Unmap(0, nullptr);
        require(bool(video), "Write replay frames");
      }
      return values;
    };
    if (inputPath) {
      require(videoWidth >= 64 && videoHeight >= 64 && videoWidth <= 3840 &&
                  videoHeight <= 2160,
              "Invalid replay dimensions");
      std::ifstream input(inputPath, std::ios::binary);
      require(bool(input), "Open replay input");
      require(!std::filesystem::exists(videoPath) ||
                  !std::filesystem::equivalent(inputPath, videoPath),
              "Replay output would overwrite its source");
      auto sourceBytes = std::filesystem::file_size(inputPath);
      require(sourceBytes > 0 &&
                  sourceBytes % (uint64_t(videoWidth) * videoHeight * 4) == 0,
              "Replay requires complete RGBA frames");
      video.open(videoPath, std::ios::binary);
      require(bool(video), "Open replay output");
      resize(videoWidth, videoHeight);
      videoReadback =
          buffer(uint64_t(inputLayout.Footprint.RowPitch) * h,
                 D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
      nr = std::make_unique<CaptureNr>(device.Get(),
                                       std::filesystem::path(runtimePath));
      SpatpitNrOptions replayOptions{};
      require(style <= 2, "Invalid replay style");
      replayOptions.style = style;
      nr->configure(replayOptions);
      nr->prepare(w, h, flow.width(), flow.height());
      require(nr->get_status().active != 0, nr->get_status().message);
      recorded.resize(size_t(w) * h * 4);
      unsigned frames = 0;
      double total = 0;
      while (input.read((char *)recorded.data(), recorded.size())) {
        frame(0, 0);
        ++frames;
        total += lastMs;
        require(nr->get_status().evaluations == frames,
                nr->get_status().message);
        if (frames % 30 == 0) {
          log << "Replay frames=" << frames << " GPU_ms=" << lastMs << "\n";
          log.flush();
        }
      }
      require(input.gcount() == 0 && frames > 0,
              "Truncated or empty raw replay");
      log << "PASS: replay " << frames << " frames at " << w << "x" << h
          << "; model scale=0.5, protection=" << protection
          << "; zero-motion diagnostic=" << zeroMotion << "; style=" << style
          << "; mean GPU pipeline=" << total / frames << " ms\n";
      CloseHandle(event);
      return 1;
    }
    resize(512, 384);
    std::vector<std::pair<float, float>> values;
    for (int i = 0; i < 10; i++)
      values = frame(0, 0);
    auto validate = [&](const char *label, float x, float y, float fraction) {
      size_t good = 0;
      for (auto v : values)
        if (std::abs(v.first - x) < 2 && std::abs(v.second - y) < 2)
          ++good;
      float ratio = float(good) / values.size();
      log << label << ": correct fraction=" << ratio << " expected=" << x << ","
          << y << " GPU_ms=" << lastMs << "\n";
      log.flush();
      require(ratio >= fraction, label);
    };
    validate("stationary", 0, 0, .95f);
    log << "Stationary frame border maximum motion=" << borderMax << " px\n";
    require(borderMax < .01, "Stationary frame border invents motion");
    double total = 0;
    for (int i = 1; i <= 12; i++) {
      values = frame(i * 16, -i * 8);
      total += lastMs;
    }
    validate("translation", -16, 8, .8f);
    log << "Mean flow GPU time at 512x384=" << total / 12 << " ms\n";
    flow.reset();
    values = frame(0, 0);
    validate("history reset", 0, 0, .99f);
    resize(641, 399);
    for (int i = 0; i < 10; i++)
      values = frame(0, 0);
    validate("odd resize and stationary", 0, 0, .95f);
    log << "Odd-size stationary border maximum motion=" << borderMax << " px\n";
    require(borderMax < .01, "Odd-size stationary border invents motion");
    for (int i = 1; i <= 12; i++)
      values = frame(-i * 8, i * 16);
    validate("opposite translation", 8, -16, .8f);
    flow.reset();
    for (int i = 0; i < 10; i++)
      values = frame(0, 0);
    for (int i = 1; i <= 12; i++)
      values = frame(i * 5, -i * 3);
    validate("non-block-aligned translation", -5, 3, .8f);
    resize(1920, 1080);
    for (int i = 0; i < 10; i++)
      values = frame(0, 0);
    total = 0;
    for (int i = 1; i <= 12; i++) {
      values = frame(i * 16, -i * 8);
      total += lastMs;
    }
    validate("1080p translation", -16, 8, .8f);
    log << "Mean flow GPU time at 1920x1080=" << total / 12 << " ms\n";
    resize(512, 384);
    scene = 1;
    flow.reset();
    frame(0, 0);
    double fractionalError = 0;
    size_t fractionalSamples = 0;
    double borderFractionalError = 0;
    size_t borderFractionalSamples = 0;
    for (int i = 1; i <= 16; ++i) {
      values = frame(i * .5f, i * .5f);
      for (auto v : values) {
        fractionalError += std::hypot(v.first + .5f, v.second + .5f);
        ++fractionalSamples;
      }
      for (auto v : borderValues) {
        borderFractionalError += std::hypot(v.first + .5f, v.second + .5f);
        ++borderFractionalSamples;
      }
    }
    fractionalError /= fractionalSamples;
    log << "Half-pixel translation mean endpoint error=" << fractionalError
        << " px\n";
    log << "Half-pixel frame border mean endpoint error="
        << borderFractionalError / borderFractionalSamples << " px\n";
    // An always-zero border would score 0.7071 pixels and must fail.
    require(borderFractionalError / borderFractionalSamples < .4,
            "Frame border suppresses real subpixel motion");
    values = frame(8, 8);
    validate("motion stops without trailing vectors", 0, 0, .99f);
    flow.reset();
    values = frame(0, 0);
    validate("refinement history reset", 0, 0, 1.f);
    flow.reset();
    frame(0, 0);
    double slowError = 0;
    size_t slowSamples = 0;
    for (int i = 1; i <= 16; ++i) {
      values = frame(i * .2f, i * .2f);
      for (auto v : values) {
        slowError += std::hypot(v.first + .2f, v.second + .2f);
        ++slowSamples;
      }
    }
    log << "Slow pan (0.2px/axis) mean endpoint error="
        << slowError / slowSamples << " px\n";
    require(slowError / slowSamples < .18,
            "Subpixel stabilization suppresses a coherent slow pan");
    scene = 11;
    flow.reset();
    frame(0, 0);
    double ribbonError = 0;
    size_t ribbonSamples = 0;
    for (int i = 1; i <= 16; ++i) {
      values = frame(i * .2f, i * .2f);
      size_t index = 0;
      for (unsigned y = 4; y + 4 < flow.height(); ++y)
        for (unsigned x = 4; x + 4 < flow.width(); ++x, ++index) {
          float pixelY = (y + .5f) * h / flow.height();
          if (std::abs(pixelY - (float(h) * .5f + i * .2f)) > 6)
            continue;
          ribbonError +=
              std::hypot(values[index].first + .2f, values[index].second + .2f);
          ++ribbonSamples;
        }
    }
    log << "KNOWN LIMITATION: narrow ribbon (0.2px/axis) mean endpoint error="
        << ribbonError / ribbonSamples << " px\n";
    values = frame(3.2f, 3.2f);
    validate("narrow ribbon stops without trailing vectors", 0, 0, .99f);
    // Localized features need useful intermediate motion even when they do
    // not occupy most of the surrounding guide neighborhood.
    flow.reset();
    frame(0, 0);
    ribbonError = 0;
    ribbonSamples = 0;
    for (int i = 1; i <= 16; ++i) {
      values = frame(i * .4f, i * .4f);
      size_t index = 0;
      for (unsigned y = 4; y + 4 < flow.height(); ++y)
        for (unsigned x = 4; x + 4 < flow.width(); ++x, ++index) {
          float pixelY = (y + .5f) * h / flow.height();
          if (std::abs(pixelY - (float(h) * .5f + i * .4f)) > 6)
            continue;
          ribbonError +=
              std::hypot(values[index].first + .4f, values[index].second + .4f);
          ++ribbonSamples;
        }
    }
    log << "Localized ribbon (0.4px/axis) mean endpoint error="
        << ribbonError / ribbonSamples << " px\n";
    require(ribbonError / ribbonSamples < .2,
            "Localized intermediate motion is suppressed");
    values = frame(6.4f, 6.4f);
    validate("intermediate ribbon stops without trailing vectors", 0, 0, .99f);
    scene = 2;
    flow.reset();
    phase = 0;
    frame(0, 0);
    size_t sparks = 0, lightSamples = 0;
    for (phase = 1; phase <= 16; ++phase) {
      values = frame(0, 0);
      for (auto v : values) {
        sparks += std::hypot(v.first, v.second) > 2;
        ++lightSamples;
      }
    }
    double lightOutliers = double(sparks) / lightSamples;
    log << "Pulsing lights false motion above 2px=" << lightOutliers << "\n";
    scene = 3;
    flow.reset();
    frame(0, 0);
    double edgeError = 0;
    size_t edgeSamples = 0;
    for (int i = 1; i <= 12; ++i) {
      values = frame(i * 2.f, 0);
      size_t index = 0;
      for (unsigned y = 4; y + 4 < flow.height(); ++y)
        for (unsigned x = 4; x + 4 < flow.width(); ++x, ++index) {
          // Measure the four blocks on either side of a motion boundary.
          if (x < flow.width() / 2 - 4 || x >= flow.width() / 2 + 4)
            continue;
          float expected = x < flow.width() / 2 ? -2.f : 0.f;
          edgeError +=
              std::hypot(values[index].first - expected, values[index].second);
          ++edgeSamples;
        }
    }
    edgeError /= edgeSamples;
    log << "Moving edge mean endpoint error=" << edgeError << " px\n";
    log.flush();
    require(fractionalError < .4, "Half-pixel motion precision");
    require(lightOutliers < .05, "Pulsing lights invent motion");
    require(edgeError < 1, "Motion leaks across object edge");
    scene = 4;
    flow.reset();
    phase = 0;
    frame(0, 0);
    size_t menuOutliers = 0, menuSamples = 0;
    for (phase = 1; phase <= 16; ++phase) {
      values = frame(0, 0);
      for (auto v : values) {
        menuOutliers += std::hypot(v.first, v.second) > 2;
        ++menuSamples;
      }
    }
    double menuRatio = double(menuOutliers) / menuSamples;
    log << "Stationary diagonal menu false motion above 2px=" << menuRatio
        << "\n";
    log.flush();
    scene = 10;
    flow.reset();
    phase = 0;
    frame(0, 0);
    size_t barOutliers = 0, barLargeOutliers = 0, barSamples = 0;
    for (phase = 1; phase <= 16; ++phase) {
      values = frame(0, 0);
      for (auto v : values) {
        // A tangential error can also damage reconstructed line detail.
        barOutliers += std::hypot(v.first, v.second) > .35f;
        barLargeOutliers += std::hypot(v.first, v.second) > 2.5f;
        ++barSamples;
      }
    }
    log << "Quantized thin bars false motion above 0.35px="
        << double(barOutliers) / barSamples << "\n";
    log << "Quantized thin bars false motion above 2.5px="
        << double(barLargeOutliers) / barSamples << "\n";
    require(barLargeOutliers == 0,
            "Stationary quantized bars invent large motion jumps");
    require(double(barOutliers) / barSamples < .005,
            "Quantized thin bars invent isolated motion");
    scene = 9;
    flow.reset();
    frame(0, 0);
    double stripeError = 0;
    size_t stripeSamples = 0;
    for (int i = 1; i <= 16; ++i) {
      values = frame(i * .75f, 0);
      for (auto v : values) {
        // Only the sum is observable on x+y stripes; do not demand a
        // particular solution for the unobservable tangential direction.
        stripeError += std::abs(v.first + v.second + .75f);
        ++stripeSamples;
      }
    }
    // Valid matches at the coarse pyramid borders are necessary to keep
    // repeating detail on its short, observable displacement.
    log << "Thin stripe at 0.75px observable displacement error="
        << stripeError / stripeSamples << " px\n";
    require(stripeError / stripeSamples < .15,
            "Thin stripe loses observable subpixel movement");
    values = frame(12, 0);
    validate("animated stripe stops without history trails", 0, 0, .99f);
    scene = 5;
    flow.reset();
    frame(0, 0);
    for (int i = 1; i <= 12; ++i)
      values = frame(i * 4.f, -i * 3.f);
    // The original matcher already fails this smooth, low-contrast fixture.
    // Keep its measurement visible; do not present it as a passing regression.
    size_t smoothGood = 0;
    for (auto v : values)
      smoothGood += std::abs(v.first + 4) < 2 && std::abs(v.second - 3) < 2;
    log << "KNOWN LIMITATION: smooth low-contrast movement correct fraction="
        << double(smoothGood) / values.size() << " (baseline 0.000893)\n";
    scene = 6;
    flow.reset();
    frame(0, 0);
    for (int i = 1; i <= 12; ++i)
      values = frame(i * 4.f, -i * 3.f);
    validate("low-contrast textured movement", -4, 3, .9f);
    require(menuRatio < .01,
            "Stationary menu invents motion under quantization noise");
    auto meanConfidence = [&](bool ramp) {
      auto resource = flow.confidence();
      auto desc = resource->GetDesc();
      D3D12_PLACED_SUBRESOURCE_FOOTPRINT layout{};
      uint64_t bytes;
      device->GetCopyableFootprints(&desc, 0, 1, 0, &layout, nullptr, nullptr,
                                    &bytes);
      auto copy = buffer(bytes, D3D12_HEAP_TYPE_READBACK,
                         D3D12_RESOURCE_STATE_COPY_DEST);
      ok(alloc->Reset());
      ok(list->Reset(alloc.Get(), nullptr));
      transition(list.Get(), resource, D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_COPY_SOURCE);
      D3D12_TEXTURE_COPY_LOCATION src{}, dst{};
      src.pResource = resource;
      src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
      dst.pResource = copy.Get();
      dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
      dst.PlacedFootprint = layout;
      list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
      transition(list.Get(), resource, D3D12_RESOURCE_STATE_COPY_SOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
      ok(list->Close());
      ID3D12CommandList *commands[] = {list.Get()};
      queue->ExecuteCommandLists(1, commands);
      wait();
      uint8_t *data;
      ok(copy->Map(0, nullptr, reinterpret_cast<void **>(&data)));
      double sum = 0;
      size_t samples = 0;
      for (unsigned y = 4; y + 4 < flow.height(); ++y)
        for (unsigned x = 4; x + 4 < flow.width(); ++x) {
          unsigned pixel = unsigned((x + .5f) * w / flow.width()) % 96;
          if (ramp && (pixel < 12 || pixel > 84))
            continue;
          sum += half(*reinterpret_cast<uint16_t *>(
              data + y * layout.Footprint.RowPitch + x * 8));
          ++samples;
        }
      copy->Unmap(0, nullptr);
      return sum / samples;
    };
    scene = 7;
    flow.reset();
    for (int i = 0; i < 16; ++i)
      frame(0, 0);
    double rampTrust = meanConfidence(true);
    scene = 8;
    flow.reset();
    for (int i = 0; i < 16; ++i)
      frame(0, 0);
    double stripeTrust = meanConfidence(false);
    log << "Smooth shading confidence=" << rampTrust
        << "; repeating stripe confidence=" << stripeTrust << "\n";
    require(rampTrust > .95, "Protection suppresses smooth shading");
    require(stripeTrust > .95,
            "Stationary stripes should retain stable zero motion");
    // A striped source must not be mixed back into a smooth
    // relit output. Reproduce the clothing/wheel failure with different colors.
    const uint32_t relit = 0xff604020;
    auto relitUpload =
        buffer(uint64_t(inputLayout.Footprint.RowPitch) * h,
               D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
    auto relitReadback =
        buffer(uint64_t(inputLayout.Footprint.RowPitch) * h,
               D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
    uint8_t *relitData;
    ok(relitUpload->Map(0, nullptr, reinterpret_cast<void **>(&relitData)));
    for (unsigned y = 0; y < h; ++y)
      std::fill_n(reinterpret_cast<uint32_t *>(
                      relitData + y * inputLayout.Footprint.RowPitch),
                  w, relit);
    relitUpload->Unmap(0, nullptr);
    ok(alloc->Reset());
    ok(list->Reset(alloc.Get(), nullptr));
    require(flow.render(list.Get(), color.Get(), true) != nullptr,
            "Prepare protected relighting regression");
    D3D12_TEXTURE_COPY_LOCATION relitSrc{}, relitDst{};
    relitSrc.pResource = relitUpload.Get();
    relitSrc.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    relitSrc.PlacedFootprint = inputLayout;
    relitDst.pResource = color.Get();
    relitDst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_COPY_DEST);
    list->CopyTextureRegion(&relitDst, 0, 0, 0, &relitSrc, nullptr);
    transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COPY_DEST,
               D3D12_RESOURCE_STATE_COMMON);
    flow.compose(list.Get(), color.Get());
    require(flow.replay(list.Get(), color.Get()), "Replay relit output");
    relitSrc = relitDst;
    relitDst.pResource = relitReadback.Get();
    relitDst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    relitDst.PlacedFootprint = inputLayout;
    transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
    list->CopyTextureRegion(&relitDst, 0, 0, 0, &relitSrc, nullptr);
    transition(list.Get(), color.Get(), D3D12_RESOURCE_STATE_COPY_SOURCE,
               D3D12_RESOURCE_STATE_COMMON);
    ok(list->Close());
    ID3D12CommandList *relitCommands[] = {list.Get()};
    queue->ExecuteCommandLists(1, relitCommands);
    wait();
    ok(relitReadback->Map(0, nullptr, reinterpret_cast<void **>(&relitData)));
    bool relightingPreserved = true;
    for (unsigned y = 0; y < h; ++y)
      for (unsigned x = 0; x < w; ++x)
        relightingPreserved &=
            *reinterpret_cast<uint32_t *>(
                relitData + y * inputLayout.Footprint.RowPitch + x * 4) ==
            relit;
    relitReadback->Unmap(0, nullptr);
    require(relightingPreserved,
            "Protection splices source shading into relit output");
    log << "PASS: protected output and cached replay preserve smooth "
           "relighting\n";
    auto beforeReplay = flow.dispatches();
    ok(alloc->Reset());
    ok(list->Reset(alloc.Get(), nullptr));
    require(flow.replay(list.Get(), color.Get()),
            "Completed output must be reusable");
    require(flow.replay(list.Get(), color.Get()),
            "Repeated output must remain reusable");
    ok(list->Close());
    ID3D12CommandList *cachedLists[] = {list.Get()};
    queue->ExecuteCommandLists(1, cachedLists);
    wait();
    require(flow.dispatches() == beforeReplay,
            "Replaying output advanced motion history");
    flow.reset();
    require(!flow.cached(), "Reset kept stale processed output");
    log << "PASS: repeated output preserves motion history; reset invalidates "
           "cache\n";
    #include "neural_detail_test.inl"
    #include "neural_lighting_test.inl"
    if (diagnostics) {
      bool clean = true;
      for (uint64_t i = 0; i < diagnostics->GetNumStoredMessages(); ++i) {
        size_t size = 0;
        diagnostics->GetMessage(i, nullptr, &size);
        std::vector<uint8_t> storage(size);
        auto message = reinterpret_cast<D3D12_MESSAGE *>(storage.data());
        ok(diagnostics->GetMessage(i, message, &size));
        if (message->Severity <= D3D12_MESSAGE_SEVERITY_ERROR) {
          log << "D3D12 error: " << message->pDescription << "\n";
          clean = false;
        }
      }
      require(clean, "D3D12 validation errors");
    }
    log << "PASS: GPU motion direction, normalized scale, stationary input, "
           "reset, resize, finite vectors. Dispatches="
        << flow.dispatches() << "\n";
    CloseHandle(event);
    return 1;
  } catch (const std::exception &e) {
    log << "FAIL: " << e.what() << "\n";
    if (event)
      CloseHandle(event);
    return 0;
  }
}
extern "C" int spatpit_test_motion(const char *output) {
  return run_motion(output);
}
extern "C" int spatpit_replay_motion(const char *output, const char *input,
                                    const char *video, unsigned w, unsigned h,
                                    const char *runtime, int protection,
                                    int zeroMotion, unsigned style) {
  return run_motion(output, input, video, w, h, runtime, protection != 0,
                    zeroMotion != 0, style);
}
