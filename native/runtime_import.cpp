// App-owned file picker; selecting a file never loads or executes it.
#include <windows.h>
#include <shobjidl.h>
#include <wrl/client.h>
#include <string>
// Shows the standard Open dialog with one filter; returns 1 with a path,
// 0 when cancelled and -1 on failure.
extern "C" int spatpit_pick_file(void *owner, const wchar_t *title, const wchar_t *filter_name,
                                 const wchar_t *filter_spec, wchar_t *out, unsigned capacity) {
  const HRESULT apartment = CoInitializeEx(nullptr, COINIT_APARTMENTTHREADED);
  if (FAILED(apartment) && apartment != RPC_E_CHANGED_MODE) return -1;
  int result = -1;
  {
    Microsoft::WRL::ComPtr<IFileOpenDialog> dialog;
    if (SUCCEEDED(CoCreateInstance(CLSID_FileOpenDialog, nullptr, CLSCTX_INPROC_SERVER, IID_PPV_ARGS(&dialog)))) {
      const COMDLG_FILTERSPEC filters[] = {{filter_name, filter_spec}};
      dialog->SetFileTypes(1, filters);
      dialog->SetOptions(FOS_FILEMUSTEXIST | FOS_PATHMUSTEXIST | FOS_FORCEFILESYSTEM | FOS_NOCHANGEDIR | FOS_DONTADDTORECENT);
      dialog->SetTitle(title);
      auto hr = dialog->Show(static_cast<HWND>(owner));
      if (hr == HRESULT_FROM_WIN32(ERROR_CANCELLED)) result = 0;
      else if (SUCCEEDED(hr)) {
        Microsoft::WRL::ComPtr<IShellItem> item;
        PWSTR path = nullptr;
        if (SUCCEEDED(dialog->GetResult(&item)) && SUCCEEDED(item->GetDisplayName(SIGDN_FILESYSPATH, &path))) {
          if (wcslen(path) < capacity) { wcscpy_s(out, capacity, path); result = 1; }
          CoTaskMemFree(path);
        }
      }
    }
  }
  if (SUCCEEDED(apartment)) CoUninitialize();
  return result;
}

extern "C" int spatpit_pick_neural_runtime(void *owner, wchar_t *out, unsigned capacity) {
  return spatpit_pick_file(owner, L"Import your NVIDIA neural runtime",
                           L"NVIDIA neural runtime (nvngx_dlssnr.dll)", L"nvngx_dlssnr.dll", out,
                           capacity);
}
