#pragma once
#include <d3d12.h>
#include <memory>
#include <string>

// Owned by one fenced D3D12 queue. No AMD or Lumenite implementation
// dependency.
class NeuralMotion {
  struct Impl;
  std::unique_ptr<Impl> impl;

public:
  explicit NeuralMotion(ID3D12Device *device);
  ~NeuralMotion();
  void prepare(unsigned width, unsigned height);
  ID3D12Resource *render(ID3D12GraphicsCommandList *list,
                         ID3D12Resource *color, bool protect = true);
  // Cache final model output without blending different lighting into it.
  void compose(ID3D12GraphicsCommandList *list, ID3D12Resource *color);
  bool cached() const;
  void invalidate();
  bool replay(ID3D12GraphicsCommandList *list, ID3D12Resource *color);
  void reset();
  unsigned width() const;
  unsigned height() const;
  uint64_t dispatches() const;
  std::string error() const;
  ID3D12Resource *output() const;
  ID3D12Resource *confidence() const;
};
