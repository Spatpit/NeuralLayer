// Functional-test consumer: captures the app from a separate process through
// public Windows Graphics Capture, without touching its swapchain or ReShade.
#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <chrono>
#include <d3d11.h>
#include <dxgi.h>
#include <thread>
#include <windows.graphics.capture.interop.h>
#include <windows.graphics.directx.direct3d11.interop.h>
#include <windows.h>
#include <winrt/Windows.Foundation.h>
#include <winrt/Windows.Graphics.Capture.h>
#include <winrt/Windows.Graphics.DirectX.Direct3D11.h>
#include <wrl/client.h>
using Microsoft::WRL::ComPtr;
using namespace winrt::Windows::Graphics::Capture;
using namespace winrt::Windows::Graphics::DirectX;
using namespace winrt::Windows::Graphics::DirectX::Direct3D11;

extern "C" int spatpit_stream_probe(void *target, unsigned char *pixels) {
  try {
    winrt::init_apartment(winrt::apartment_type::multi_threaded);
    ComPtr<ID3D11Device> device;
    ComPtr<ID3D11DeviceContext> context;
    winrt::check_hresult(
        D3D11CreateDevice(nullptr, D3D_DRIVER_TYPE_HARDWARE, nullptr,
                          D3D11_CREATE_DEVICE_BGRA_SUPPORT, nullptr, 0,
                          D3D11_SDK_VERSION, &device, nullptr, &context));
    ComPtr<IDXGIDevice> dxgi;
    winrt::check_hresult(device.As(&dxgi));
    winrt::com_ptr<IInspectable> inspect;
    winrt::check_hresult(
        CreateDirect3D11DeviceFromDXGIDevice(dxgi.Get(), inspect.put()));
    auto capture_device = inspect.as<IDirect3DDevice>();
    auto interop = winrt::get_activation_factory<GraphicsCaptureItem,
                                                 IGraphicsCaptureItemInterop>();
    GraphicsCaptureItem item{nullptr};
    winrt::check_hresult(interop->CreateForWindow(
        static_cast<HWND>(target), winrt::guid_of<GraphicsCaptureItem>(),
        winrt::put_abi(item)));
    auto pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        capture_device, DirectXPixelFormat::B8G8R8A8UIntNormalized, 2,
        item.Size());
    auto session = pool.CreateCaptureSession(item);
    session.IsCursorCaptureEnabled(false);
    session.StartCapture();
    const auto deadline =
        std::chrono::steady_clock::now() + std::chrono::seconds(8);
    bool received = false;
    while (std::chrono::steady_clock::now() < deadline) {
      auto frame = pool.TryGetNextFrame();
      if (!frame) {
        std::this_thread::sleep_for(std::chrono::milliseconds(20));
        continue;
      }
      auto access = frame.Surface()
                        .as<::Windows::Graphics::DirectX::Direct3D11::
                                IDirect3DDxgiInterfaceAccess>();
      ComPtr<ID3D11Texture2D> texture;
      winrt::check_hresult(access->GetInterface(IID_PPV_ARGS(&texture)));
      D3D11_TEXTURE2D_DESC desc{};
      texture->GetDesc(&desc);
      auto size = frame.ContentSize();
      if (size.Width <= 0 || size.Height <= 0 ||
          static_cast<UINT>(size.Width) > desc.Width ||
          static_cast<UINT>(size.Height) > desc.Height)
        continue;
      desc.Usage = D3D11_USAGE_STAGING;
      desc.BindFlags = desc.MiscFlags = 0;
      desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ;
      ComPtr<ID3D11Texture2D> readback;
      winrt::check_hresult(device->CreateTexture2D(&desc, nullptr, &readback));
      context->CopyResource(readback.Get(), texture.Get());
      D3D11_MAPPED_SUBRESOURCE mapped{};
      winrt::check_hresult(
          context->Map(readback.Get(), 0, D3D11_MAP_READ, 0, &mapped));
      const int xs[] = {size.Width / 6, size.Width / 2};
      const int ys[] = {size.Height / 2, size.Height / 4};
      for (int i = 0; i < 2; ++i) {
        auto *bgra = static_cast<unsigned char *>(mapped.pData) +
                     ys[i] * mapped.RowPitch + xs[i] * 4;
        pixels[i * 4] = bgra[2];
        pixels[i * 4 + 1] = bgra[1];
        pixels[i * 4 + 2] = bgra[0];
        pixels[i * 4 + 3] = bgra[3];
      }
      context->Unmap(readback.Get(), 0);
      received = true;
      break;
    }
    session.Close();
    pool.Close();
    return received ? 1 : 0;
  } catch (...) {
    return 0;
  }
}
