// Read a guide as an SRV at the same point the neural model reads it. This
// avoids guessing or changing the state of a texture owned by a backend.
struct ReplayMotionTap {
  Engine &engine;
  ComPtr<ID3D12RootSignature> root;
  ComPtr<ID3D12PipelineState> pso;
  ComPtr<ID3D12DescriptorHeap> descriptors;
  ComPtr<ID3D12Resource> output, readback;
  D3D12_PLACED_SUBRESOURCE_FOOTPRINT footprint{};
  unsigned width = 0, height = 0;
  std::ofstream file;
  ReplayMotionTap(Engine &e, const std::filesystem::path &path)
      : engine(e), file(path, std::ios::binary) {
    if (!file)
      throw std::runtime_error("Open replay motion capture");
    const char shader[] =
        "Texture2D<float2> source:register(t0); RWTexture2D<float2> "
        "dest:register(u0);"
        "[numthreads(8,8,1)] void main(uint3 p:SV_DispatchThreadID) { uint "
        "w,h; dest.GetDimensions(w,h);"
        "if(p.x<w && p.y<h) dest[p.xy]=source.Load(int3(p.xy,0)); }";
    ComPtr<ID3DBlob> code, errors, signature;
    check(D3DCompile(shader, sizeof(shader) - 1, nullptr, nullptr, nullptr,
                     "main", "cs_5_0", D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &code,
                     &errors),
          "Compile replay motion capture");
    D3D12_DESCRIPTOR_RANGE ranges[] = {
        {D3D12_DESCRIPTOR_RANGE_TYPE_SRV, 1, 0, 0, 0},
        {D3D12_DESCRIPTOR_RANGE_TYPE_UAV, 1, 0, 0, 1}};
    D3D12_ROOT_PARAMETER parameter{};
    parameter.ParameterType = D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE;
    parameter.DescriptorTable = {2, ranges};
    D3D12_ROOT_SIGNATURE_DESC rs{};
    rs.NumParameters = 1;
    rs.pParameters = &parameter;
    check(D3D12SerializeRootSignature(&rs, D3D_ROOT_SIGNATURE_VERSION_1,
                                      &signature, &errors),
          "Serialize replay motion root");
    check(e.device->CreateRootSignature(0, signature->GetBufferPointer(),
                                        signature->GetBufferSize(),
                                        IID_PPV_ARGS(&root)),
          "Create replay motion root");
    D3D12_COMPUTE_PIPELINE_STATE_DESC pd{};
    pd.pRootSignature = root.Get();
    pd.CS = {code->GetBufferPointer(), code->GetBufferSize()};
    check(e.device->CreateComputePipelineState(&pd, IID_PPV_ARGS(&pso)),
          "Create replay motion pipeline");
    D3D12_DESCRIPTOR_HEAP_DESC hd{};
    hd.Type = D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV;
    hd.NumDescriptors = 2;
    hd.Flags = D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE;
    check(e.device->CreateDescriptorHeap(&hd, IID_PPV_ARGS(&descriptors)),
          "Create replay motion descriptors");
  }
  void record(ID3D12GraphicsCommandList *list, ID3D12Resource *guide) {
    auto desc = guide->GetDesc();
    if (!output) {
      width = unsigned(desc.Width);
      height = desc.Height;
      desc.Format = DXGI_FORMAT_R32G32_FLOAT;
      desc.Flags = D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS;
      desc.MipLevels = 1;
      auto hp = heap(D3D12_HEAP_TYPE_DEFAULT);
      check(engine.device->CreateCommittedResource(
                &hp, D3D12_HEAP_FLAG_NONE, &desc, D3D12_RESOURCE_STATE_COMMON,
                nullptr, IID_PPV_ARGS(&output)),
            "Create replay motion output");
      uint64_t bytes;
      engine.device->GetCopyableFootprints(&desc, 0, 1, 0, &footprint, nullptr,
                                           nullptr, &bytes);
      hp = heap(D3D12_HEAP_TYPE_READBACK);
      auto bd = buffer_desc(bytes);
      check(engine.device->CreateCommittedResource(
                &hp, D3D12_HEAP_FLAG_NONE, &bd, D3D12_RESOURCE_STATE_COPY_DEST,
                nullptr, IID_PPV_ARGS(&readback)),
            "Create replay motion readback");
    }
    if (desc.Width != width || desc.Height != height)
      throw std::runtime_error("Replay guide dimensions changed");
    auto cpu = descriptors->GetCPUDescriptorHandleForHeapStart();
    D3D12_SHADER_RESOURCE_VIEW_DESC srv{};
    srv.Format = guide->GetDesc().Format;
    srv.ViewDimension = D3D12_SRV_DIMENSION_TEXTURE2D;
    srv.Shader4ComponentMapping = D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING;
    srv.Texture2D.MipLevels = 1;
    engine.device->CreateShaderResourceView(guide, &srv, cpu);
    cpu.ptr += engine.device->GetDescriptorHandleIncrementSize(
        D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV);
    D3D12_UNORDERED_ACCESS_VIEW_DESC uav{};
    uav.Format = DXGI_FORMAT_R32G32_FLOAT;
    uav.ViewDimension = D3D12_UAV_DIMENSION_TEXTURE2D;
    engine.device->CreateUnorderedAccessView(output.Get(), nullptr, &uav, cpu);
    transition(list, output.Get(), D3D12_RESOURCE_STATE_COMMON,
               D3D12_RESOURCE_STATE_UNORDERED_ACCESS);
    ID3D12DescriptorHeap *heaps[] = {descriptors.Get()};
    list->SetDescriptorHeaps(1, heaps);
    list->SetComputeRootSignature(root.Get());
    list->SetPipelineState(pso.Get());
    list->SetComputeRootDescriptorTable(
        0, descriptors->GetGPUDescriptorHandleForHeapStart());
    list->Dispatch((width + 7) / 8, (height + 7) / 8, 1);
    transition(list, output.Get(), D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
               D3D12_RESOURCE_STATE_COPY_SOURCE);
    D3D12_TEXTURE_COPY_LOCATION src{}, dst{};
    src.pResource = output.Get();
    src.Type = D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX;
    dst.pResource = readback.Get();
    dst.Type = D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT;
    dst.PlacedFootprint = footprint;
    list->CopyTextureRegion(&dst, 0, 0, 0, &src, nullptr);
    transition(list, output.Get(), D3D12_RESOURCE_STATE_COPY_SOURCE,
               D3D12_RESOURCE_STATE_COMMON);
  }
  void write() {
    if (!readback)
      throw std::runtime_error("Missing captured replay motion");
    char *data;
    check(readback->Map(0, nullptr, reinterpret_cast<void **>(&data)),
          "Map replay motion capture");
    for (unsigned y = 0; y < height; ++y)
      file.write(data + y * footprint.Footprint.RowPitch, width * 8);
    readback->Unmap(0, nullptr);
    if (!file)
      throw std::runtime_error("Write replay motion capture");
  }
};
