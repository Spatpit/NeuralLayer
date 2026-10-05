// Build-time utility: keep shader compilation out of first launch and resizing.
#include <d3dcompiler.h>
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>
#include <wrl/client.h>

int main(int argc, char **argv) {
  if (argc != 3)
    return 1;
  std::ifstream input(argv[1], std::ios::binary);
  if (!input)
    return 1;
  std::string source((std::istreambuf_iterator<char>(input)), {});
  Microsoft::WRL::ComPtr<ID3DBlob> shader, errors;
  HRESULT result = D3DCompile(
      source.data(), source.size(), argv[1], nullptr, nullptr, "main", "cs_5_1",
      D3DCOMPILE_OPTIMIZATION_LEVEL3, 0, &shader, &errors);
  if (FAILED(result)) {
    if (errors)
      std::cerr.write(static_cast<char *>(errors->GetBufferPointer()),
                      errors->GetBufferSize());
    return 1;
  }
  std::ofstream output(argv[2], std::ios::binary);
  output.write(static_cast<char *>(shader->GetBufferPointer()),
               shader->GetBufferSize());
  return output ? 0 : 1;
}
