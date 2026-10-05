// Standalone composition regression, using analytic source/relighting targets.
// Included inside run_motion after all previous GPU work has completed.
{
  constexpr unsigned size = 64, pitch = size * 4, bytes = pitch * size;
  ComPtr<ID3D12Resource> images[3];
  D3D12_RESOURCE_DESC desc{};
  desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
  desc.Width = desc.Height = size;
  desc.DepthOrArraySize = desc.MipLevels = desc.SampleDesc.Count = 1;
  desc.Format = DXGI_FORMAT_R8G8B8A8_UNORM;
  desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
  D3D12_HEAP_PROPERTIES hp{};
  hp.Type = D3D12_HEAP_TYPE_DEFAULT;
  hp.CreationNodeMask = hp.VisibleNodeMask = 1;
  for (auto &image : images)
    ok(device->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &desc,
                                       D3D12_RESOURCE_STATE_COMMON, nullptr,
                                       IID_PPV_ARGS(&image)));
  auto input = buffer(bytes * 2, D3D12_HEAP_TYPE_UPLOAD,
                      D3D12_RESOURCE_STATE_GENERIC_READ);
  auto output =
      buffer(bytes, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
  auto params =
      buffer(256, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
  DlssNrConstants constants{};
  constants.Width = constants.Height = size;
  void *mapped;
  ok(params->Map(0, nullptr, &mapped));
  memcpy(mapped, &constants, sizeof(constants));
  params->Unmap(0, nullptr);
  D3D12_DESCRIPTOR_RANGE ranges[] = {
      {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 0, 0, 0},
      {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 2, 0, 1},
      {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 0, 0, 2}};
  D3D12_ROOT_PARAMETER rp[2]{};
  rp[0].ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
  rp[0].DescriptorTable = {3, ranges};
  rp[1].ParameterType = D3D12_ROOT_PARAMETER_TYPE_CBV;
  D3D12_STATIC_SAMPLER_DESC sampler{};
  sampler.Filter = D3D12_FILTER_MIN_MAG_MIP_LINEAR;
  sampler.AddressU = sampler.AddressV = sampler.AddressW =
      D3D12_TEXTURE_ADDRESS_MODE_CLAMP;
  sampler.MaxLOD = D3D12_FLOAT32_MAX;
  D3D12_ROOT_SIGNATURE_DESC rd{2, rp, 1, &sampler};
  ComPtr<ID3DBlob> code, rootCode, errors;
  ok(D3D12SerializeRootSignature(&rd, D3D_ROOT_SIGNATURE_VERSION_1, &rootCode,
                                 &errors));
  ComPtr<ID3D12RootSignature> root;
  ok(device->CreateRootSignature(0, rootCode->GetBufferPointer(),
                                 rootCode->GetBufferSize(),
                                 IID_PPV_ARGS(&root)));
  ok(D3DCompile(neural_detail_shader, strlen(neural_detail_shader),
                "detail_test", nullptr, nullptr, "main", "cs_5_0",
                D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &code, &errors));
  D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
  pd.pRootSignature = root.Get();
  pd.CS = {code->GetBufferPointer(), code->GetBufferSize()};
  ComPtr<ID3D12PipelineState> pipeline;
  ok(device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pipeline)));
  D3D12_DESCRIPTOR_HEAP_DESC hd{D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 3,
                                D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, 0};
  ComPtr<ID3D12DescriptorHeap> descriptors;
  ok(device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&descriptors)));
  auto cpu = descriptors->GetCPUDescriptorHandleForHeapStart();
  auto step = device->GetDescriptorHandleIncrementSize(hd.Type);
  for (unsigned i = 0; i < 2; ++i) {
    D3D12_SHADER_RESOURCE_VIEW_DESC srv{};
    srv.Format = desc.Format;
    srv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    srv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    srv.Texture2D.MipLevels = 1;
    device->CreateShaderResourceView(images[i].Get(), &srv, cpu);
    cpu.ptr += step;
  }
  D3D12_UNORDERED_ACCESS_VIEW_DESC uav{};
  uav.Format = desc.Format;
  uav.ViewDimension = D3D12_UAV_DIMENSION_TEXTURE2D;
  device->CreateUnorderedAccessView(images[2].Get(), nullptr, &uav, cpu);
  auto pattern = [](float x, float y) {
    // Localized rounded features, fine texture and a thin near-border line.
    float a = (x - 30) / 3, b = (y - 26) / 7;
    float c = (x - 31) / 7, d = (y - 42) / 1.5f;
    return .12f + .45f * expf(-a * a - b * b) + .3f * expf(-c * c - d * d) +
           .03f * sinf(x * 1.7f) * sinf(y * 1.3f) + (x < 2 ? .15f : 0);
  };
  auto quantize = [](float v) {
    return uint8_t(std::clamp(std::lround(v * 255), 0l, 255l));
  };
  double legacy_errors[7]{};
  for (unsigned sample = 0; sample < 14; ++sample) {
    unsigned variant = sample % 7;
    constants.Mode = sample >= 7 ? 3 : 0;
    unsigned test = variant == 6 ? 3 : variant % 3;
    constants.TransferStrength = variant < 3 ? 2.5f : 5.f;
    ok(params->Map(0, nullptr, &mapped));
    memcpy(mapped, &constants, sizeof(constants));
    params->Unmap(0, nullptr);
    std::vector<uint8_t> expected(bytes), uncorrected(bytes);
    uint8_t *data;
    ok(input->Map(0, nullptr, reinterpret_cast<void **>(&data)));
    for (unsigned y = 0; y < size; ++y)
      for (unsigned x = 0; x < size; ++x) {
        auto offset = (y * size + x) * 4;
        for (unsigned channel = 0; channel < 3; ++channel) {
          float tint = 1 - channel * .15f, lighting = test == 0 ? 1.f : .6f;
          data[bytes + offset + channel] =
              quantize(pattern(float(x), float(y)) * tint);
          auto value = quantize(
              pattern(x - (test == 2 ? .75f : 0), y + (test == 2 ? .3f : 0)) *
              tint * lighting);
          data[offset + channel] = uncorrected[offset + channel] = value;
          expected[offset + channel] =
              quantize(pattern(float(x), float(y)) * tint * lighting);
          if (test == 3) {
            // A flat source under dimmer lighting gains fine neural texture.
            // Preserve that texture without weakening defined-feature checks.
            data[bytes + offset + channel] = quantize(.3f * tint);
            value = quantize((.18f + .035f * sinf(x * 2.f) * sinf(y * 1.3f)) * tint);
            data[offset + channel] = uncorrected[offset + channel] = value;
            expected[offset + channel] = value;
          }
        }
        data[offset + 3] = data[bytes + offset + 3] = 255;
      }
    input->Unmap(0, nullptr);
    ok(alloc->Reset());
    ok(list->Reset(alloc.Get(), nullptr));
    for (unsigned i = 0; i < 2; ++i) {
      D3D12_TEXTURE_COPY_LOCATION from{}, to{};
      from.pResource = input.Get();
      from.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
      from.PlacedFootprint = {uint64_t(i) * bytes,
                              {desc.Format, size, size, 1, pitch}};
      to.pResource = images[i].Get();
      to.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
      transition(list.Get(), images[i].Get(), D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_COPY_DEST);
      list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
      transition(list.Get(), images[i].Get(), D3D12_RESOURCE_STATE_COPY_DEST,
                 D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    }
    transition(list.Get(), images[2].Get(), D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    ID3D12DescriptorHeap *heaps[] = {descriptors.Get()};
    list->SetDescriptorHeaps(1, heaps);
    list->SetComputeRootSignature(root.Get());
    list->SetPipelineState(pipeline.Get());
    list->SetComputeRootDescriptorTable(
        0, descriptors->GetGPUDescriptorHandleForHeapStart());
    list->SetComputeRootConstantBufferView(1, params->GetGPUVirtualAddress());
    list->Dispatch(size / 8, size / 8, 1);
    transition(list.Get(), images[2].Get(),
               D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
    D3D12_TEXTURE_COPY_LOCATION from{}, to{};
    from.pResource = images[2].Get();
    from.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    to.pResource = output.Get();
    to.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    to.PlacedFootprint = {0, {desc.Format, size, size, 1, pitch}};
    list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    for (unsigned i = 0; i < 3; ++i)
      transition(list.Get(), images[i].Get(),
                 i == 2 ? D3D12_RESOURCE_STATE_COPY_SOURCE
                        : D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
    ok(list->Close());
    ID3D12CommandList *commands[] = {list.Get()};
    queue->ExecuteCommandLists(1, commands);
    wait();
    ok(output->Map(0, nullptr, reinterpret_cast<void **>(&data)));
    double before = 0, after = 0;
    unsigned maximum = 0;
    for (unsigned i = 0; i < bytes; ++i)
      if (i % 4 != 3) {
        before += std::abs(int(uncorrected[i]) - int(expected[i]));
        unsigned delta = unsigned(std::abs(int(data[i]) - int(expected[i])));
        after += delta;
        maximum = std::max(maximum, delta);
      }
    output->Unmap(0, nullptr);
    if (sample < 7) legacy_errors[variant] = after;
    if (sample >= 7 && test == 2)
      require(after <= legacy_errors[variant],
              "Balanced worsens known feature displacement");
    log << "Detail mode=" << constants.Mode << " radius=" << constants.TransferStrength << " test=" << test
        << " original error=" << before / (size * size * 3)
        << " corrected error=" << after / (size * size * 3)
        << " max=" << maximum << '\n';
    if (test == 0)
      require(maximum == 0, "Detail preservation changes an identity image");
    if (test == 1)
      require(maximum <= 2,
              "Detail preservation creates edges during uniform dimming");
    if (test == 2)
      require(after < before * .85,
              "Detail preservation does not reduce known feature displacement");
    if (test == 3)
      require(after / (size * size * 3) < 1.0,
              "Multipass preservation removes fine texture from a flat surface");
  }
}
