// Offline causal diagnosis only: substitute recorded, normalized RG16 motion
// at the model boundary without changing the estimator or neural history.
struct ReplayMotionInput {
  Engine &engine;
  std::ifstream file;
  unsigned width, height;
  ComPtr<ID3D12Resource> texture, upload;
  D3D12_PLACED_SUBRESOURCE_FOOTPRINT footprint{};
  std::vector<uint16_t> pixels;
  bool initialized = false;
  ReplayMotionInput(Engine &e, const std::filesystem::path &path, unsigned w,
                    unsigned h, uint64_t frames)
      : engine(e), file(path, std::ios::binary), width((w + 7) / 8),
        height((h + 7) / 8), pixels(size_t(width) * height * 2) {
    if (!file || std::filesystem::file_size(path) != frames * pixels.size() * 2)
      throw std::runtime_error(
          "Motion input must contain one RG16 guide per source frame");
    // Reject invalid data before recording GPU commands. Render errors are
    // contained by Engine, which would otherwise obscure this input error.
    for (uint64_t frame = 0; frame < frames; ++frame) {
      file.read(reinterpret_cast<char *>(pixels.data()), pixels.size() * 2);
      if (!file)
        throw std::runtime_error("Truncated motion input");
      for (auto bits : pixels)
        if ((bits & 0x7c00) == 0x7c00)
          throw std::runtime_error("Non-finite motion input");
    }
    file.seekg(0);
  }
  ID3D12Resource *record(ID3D12GraphicsCommandList *list,
                         ID3D12Resource *guide) {
    auto desc = guide->GetDesc();
    if (desc.Width != width || desc.Height != height ||
        desc.Format != DXGI_FORMAT_R16G16_FLOAT)
      throw std::runtime_error("Motion input guide dimensions/format mismatch");
    file.read(reinterpret_cast<char *>(pixels.data()), pixels.size() * 2);
    if (!file)
      throw std::runtime_error("Truncated motion input");
    for (auto bits : pixels)
      if ((bits & 0x7c00) == 0x7c00)
        throw std::runtime_error("Non-finite motion input");
    if (!texture) {
      desc.Flags = D3D12_RESOURCE_FLAG_NONE;
      auto hp = heap(D3D12_HEAP_TYPE_DEFAULT);
      check(engine.device->CreateCommittedResource(
                &hp, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COMMON,
                nullptr, IID_PPV_ARGS(&texture)),
            "Create motion input texture");
      uint64_t bytes;
      engine.device->GetCopyableFootprints(&desc, 0, 1, 0, &footprint, nullptr,
                                           nullptr, &bytes);
      hp = heap(D3D12_HEAP_TYPE_UPLOAD);
      auto bd = buffer_desc(bytes);
      check(engine.device->CreateCommittedResource(
                &hp, D3D12_HEAP_FLAG_NONE, &bd,
                D3D12_RESOURCE_STATE_GENERIC_READ, nullptr,
                IID_PPV_ARGS(&upload)),
            "Create motion input upload");
    }
    char *data;
    D3D12_RANGE readRange{0, 0};
    check(upload->Map(0, &readRange, reinterpret_cast<void **>(&data)),
          "Map motion input upload");
    for (unsigned y = 0; y < height; ++y)
      memcpy(data + footprint.Offset + y * footprint.Footprint.RowPitch,
             pixels.data() + size_t(y) * width * 2, width * 4);
    upload->Unmap(0, nullptr);
    transition(list, texture.Get(),
               initialized ? D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE
                           : D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_COPY_DEST);
    D3D12_TEXTURE_COPY_LOCATION src{}, dst{};
    src.pResource = upload.Get();
    src.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    src.PlacedFootprint = footprint;
    dst.pResource = texture.Get();
    dst.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
    transition(list, texture.Get(), D3D12_RESOURCE_STATE_COPY_DEST,
               D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    initialized = true;
    return texture.Get();
  }
};
