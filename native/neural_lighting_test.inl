// Analytic GPU checks for lighting history rejection and texture preservation.
{
  constexpr unsigned size = 64, pitch = size * 16, bytes = pitch * size;
  ComPtr<ID3D12Resource> images[5];
  D3D12_RESOURCE_DESC desc{};
  desc.Dimension = D3D12_RESOURCE_DIMENSION_TEXTURE2D;
  desc.Width = desc.Height = size;
  desc.DepthOrArraySize = desc.MipLevels = desc.SampleDesc.Count = 1;
  desc.Format = DXGI_FORMAT_R32G32B32A32_FLOAT;
  desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
  D3D12_HEAP_PROPERTIES hp{};
  hp.Type = D3D12_HEAP_TYPE_DEFAULT;
  hp.CreationNodeMask = hp.VisibleNodeMask = 1;
  for (auto &image : images)
    ok(device->CreateCommittedResource(&hp, D3D12_HEAP_FLAG_NONE, &desc,
                                       D3D12_RESOURCE_STATE_COMMON, nullptr,
                                       IID_PPV_ARGS(&image)));
  auto input = buffer(bytes * 4, D3D12_HEAP_TYPE_UPLOAD,
                      D3D12_RESOURCE_STATE_GENERIC_READ);
  auto output =
      buffer(bytes, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST);
  auto params =
      buffer(256, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ);
  D3D12_DESCRIPTOR_RANGE ranges[] = {
      {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 4, 0, 0, 0},
      {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 0, 0, 4}};
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
  ComPtr<ID3DBlob> rootCode, code, errors;
  ok(D3D12SerializeRootSignature(&rd, D3D_ROOT_SIGNATURE_VERSION_1, &rootCode,
                                 &errors));
  ComPtr<ID3D12RootSignature> root;
  ok(device->CreateRootSignature(0, rootCode->GetBufferPointer(),
                                 rootCode->GetBufferSize(),
                                 IID_PPV_ARGS(&root)));
  ok(D3DCompile(neural_lighting_shader, strlen(neural_lighting_shader),
                "lighting_test", nullptr, nullptr, "main", "cs_5_0",
                D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &code, &errors));
  D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
  pd.pRootSignature = root.Get();
  pd.CS = {code->GetBufferPointer(), code->GetBufferSize()};
  ComPtr<ID3D12PipelineState> pipeline;
  ok(device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pipeline)));
  D3D12_DESCRIPTOR_HEAP_DESC hd{D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, 5,
                                D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, 0};
  ComPtr<ID3D12DescriptorHeap> descriptors;
  ok(device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&descriptors)));
  auto cpu = descriptors->GetCPUDescriptorHandleForHeapStart();
  auto step = device->GetDescriptorHandleIncrementSize(hd.Type);
  for (unsigned i = 0; i < 4; ++i) {
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
  device->CreateUnorderedAccessView(images[4].Get(), nullptr, &uav, cpu);
  for (unsigned sample = 0; sample < 16; ++sample) {
    unsigned test = sample % 8;
    DlssNrConstants c{};
    c.TransferStrength = sample >= 8 ? 1.f : 0.f;
    c.Width = c.Height = size;
    c.WhitePoint = test ? 1.f : 0.f;
    c.Mode = test == 5 ? 1 : 0;
    void *mapped;
    ok(params->Map(0, nullptr, &mapped));
    memcpy(mapped, &c, sizeof(c));
    params->Unmap(0, nullptr);
    float *data;
    ok(input->Map(0, nullptr, reinterpret_cast<void **>(&data)));
    for (unsigned y = 0; y < size; ++y)
      for (unsigned x = 0; x < size; ++x) {
        auto off = (y * size + x) * 4;
        for (unsigned channel = 0; channel < 4; ++channel) {
          data[off + channel] =
              channel == 3 ? 1.f : .6f + (test == 5 && x % 2 ? .1f : 0.f);
          data[bytes / 4 + off + channel] = channel == 0   ? .09f
                                            : channel == 1 ? .1f
                                            : channel == 2 ? .5f
                                                           : 0.f;
          data[bytes / 2 + off + channel] = channel == 3 ? 1.f : .5f;
          data[3 * bytes / 4 + off + channel] = 0;
        }
        if (test == 6 || test == 7) {
          data[bytes / 4 + off] = .08f;
          if (test == 7) {
            // Keep low-pass source/history identical, but add real local
            // source contrast. The conservative edge clamp must remain.
            float shade = (int(x) % 6 < 3 ? -.1f : .1f);
            for (unsigned channel = 0; channel < 3; ++channel) {
              data[off + channel] += shade;
              data[bytes / 2 + off + channel] += shade;
            }
          }
        }
        if (test == 0)
          data[bytes / 4 + off] = NAN;
        if (test == 2)
          data[bytes / 4 + off + 2] = .2f;
        if (test == 3)
          data[bytes / 4 + off + 3] = .2f;
        if (test == 4)
          data[3 * bytes / 4 + off] = .5f;
      }
    input->Unmap(0, nullptr);
    ok(alloc->Reset());
    ok(list->Reset(alloc.Get(), nullptr));
    for (unsigned i = 0; i < 4; ++i) {
      transition(list.Get(), images[i].Get(), D3D12_RESOURCE_STATE_COMMON,
                 D3D12_RESOURCE_STATE_COPY_DEST);
      D3D12_TEXTURE_COPY_LOCATION from{}, to{};
      from.pResource = input.Get();
      from.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
      from.PlacedFootprint = {uint64_t(i) * bytes,
                              {desc.Format, size, size, 1, pitch}};
      to.pResource = images[i].Get();
      to.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
      list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
      transition(list.Get(), images[i].Get(), D3D12_RESOURCE_STATE_COPY_DEST,
                 D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    }
    transition(list.Get(), images[4].Get(), D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    ID3D12DescriptorHeap *heaps[] = {descriptors.Get()};
    list->SetDescriptorHeaps(1, heaps);
    list->SetComputeRootSignature(root.Get());
    list->SetPipelineState(pipeline.Get());
    list->SetComputeRootDescriptorTable(
        0, descriptors->GetGPUDescriptorHandleForHeapStart());
    list->SetComputeRootConstantBufferView(1, params->GetGPUVirtualAddress());
    list->Dispatch(size / 8, size / 8, 1);
    transition(list.Get(), images[4].Get(),
               D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
    D3D12_TEXTURE_COPY_LOCATION from{}, to{};
    from.pResource = images[4].Get();
    from.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    to.pResource = output.Get();
    to.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    to.PlacedFootprint = {0, {desc.Format, size, size, 1, pitch}};
    list->CopyTextureRegion(&to, 0, 0, 0, &from, nullptr);
    for (unsigned i = 0; i < 5; ++i)
      transition(list.Get(), images[i].Get(),
                 i == 4 ? D3D12_RESOURCE_STATE_COPY_SOURCE
                        : D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
                 D3D12_RESOURCE_STATE_COMMON);
    ok(list->Close());
    ID3D12CommandList *commands[] = {list.Get()};
    queue->ExecuteCommandLists(1, commands);
    wait();
    ok(output->Map(0, nullptr, reinterpret_cast<void **>(&data)));
    float value = data[(32 * size + 32) * 4];
    if (test == 1)
      require(value > .09f && value < .096f,
              "Lighting history fails to damp an unsupported edit change");
    else if (test == 6)
      require(value < .089f && value > .08f,
              "Smooth surfaces do not receive stronger temporal support");
    else if (test == 7)
      require(value >= .0915f,
              "Stronger pillow smoothing weakens protection at source edges");
    else if (test == 5) {
      require(fabsf(value - .59f) < .00001f, "Lighting correction not applied");
      require(fabsf(data[(32 * size + 33) * 4] - value - .1f) < .00001f,
              "Lighting stabilization blurs current-frame texture");
    } else
      require(std::isfinite(value) && fabsf(value - .1f) < .00001f,
              "Invalid lighting history changes current shading");
    output->Unmap(0, nullptr);
  }
  log << "PASS: lighting history reset, brightness/chroma change rejection, "
         "out-of-frame motion rejection, temporal damping and current-frame "
         "texture contrast; adaptive smooth-surface support and conservative "
         "source edges\n";
}
