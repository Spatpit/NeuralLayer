#define WIN32_LEAN_AND_MEAN
#define NOMINMAX
#include <windows.h>
#include <commctrl.h>
#include <shellapi.h>
#include <shobjidl.h>
#include <propkey.h>
#include <propvarutil.h>
#include <wrl/client.h>
#include <string>

static LRESULT CALLBACK menu_recovery(HWND hwnd, UINT message, WPARAM wparam,
                                      LPARAM lparam, UINT_PTR id,
                                      DWORD_PTR receiver) {
  const auto command = wparam & 0xfff0;
  const bool taskbar_command = message == WM_SYSCOMMAND &&
      (command == SC_RESTORE || command == SC_MINIMIZE);
  // A click on the interactive comparison panel must not open the full menu.
  const bool activation = message == WM_ACTIVATE && LOWORD(wparam) != WA_INACTIVE &&
      (GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TRANSPARENT);
  if ((taskbar_command || activation) &&
      PostMessageW(reinterpret_cast<HWND>(receiver), WM_APP + 2, 0, 0)) {
    // Clicking an already-active taskbar button normally minimizes it. While
    // controls are hidden, the first click should instead recover the menu.
    if (taskbar_command && command == SC_MINIMIZE)
      return 0;
  }
  if (message == WM_NCDESTROY)
    RemoveWindowSubclass(hwnd, menu_recovery, id);
  return DefSubclassProc(hwnd, message, wparam, lparam);
}

// Called only on the window's UI thread. The subclass owns no Rust pointers.
extern "C" BOOL spatpit_taskbar_menu_recovery(HWND hwnd, HWND receiver, BOOL hidden) {
  if (hidden && receiver)
    return SetWindowSubclass(hwnd, menu_recovery, 1,
                             reinterpret_cast<DWORD_PTR>(receiver));
  RemoveWindowSubclass(hwnd, menu_recovery, 1);
  return TRUE;
}

// Window icons (WM_SETICON) are not the shell's taskbar group icon. Portable
// copies have no installed shortcut supplying this metadata, so publish it on
// the window before it is first shown. All paths follow the running executable.
static HRESULT set_string(IPropertyStore *store, REFPROPERTYKEY key,
                          const std::wstring &text) {
  PROPVARIANT value{};
  HRESULT hr = InitPropVariantFromString(text.c_str(), &value);
  if (SUCCEEDED(hr))
    hr = store->SetValue(key, value);
  PropVariantClear(&value);
  return hr;
}

extern "C" HRESULT spatpit_taskbar_identity(HWND hwnd, const wchar_t *exe) {
  try {
    Microsoft::WRL::ComPtr<IPropertyStore> store;
    HRESULT hr = SHGetPropertyStoreForWindow(hwnd, IID_PPV_ARGS(&store));
    if (FAILED(hr))
      return hr;
    const std::wstring path(exe);
    // Set the ID last: setting it notifies Explorer to refresh this window.
    const std::pair<PROPERTYKEY, std::wstring> values[] = {
        {PKEY_AppUserModel_RelaunchCommand, L"\"" + path + L"\""},
        {PKEY_AppUserModel_RelaunchDisplayNameResource, L"@" + path + L",-101"},
        {PKEY_AppUserModel_RelaunchIconResource, path + L",-1"},
        {PKEY_AppUserModel_ID, L"Spatpit.NeuralLayer"},
    };
    for (const auto &[key, text] : values) {
      hr = set_string(store.Get(), key, text);
      if (FAILED(hr))
        return hr;
    }
    return store->Commit();
  } catch (...) {
    return E_FAIL;
  }
}

extern "C" HRESULT spatpit_taskbar_verify(HWND hwnd, const wchar_t *exe) {
  Microsoft::WRL::ComPtr<IPropertyStore> store;
  HRESULT hr = SHGetPropertyStoreForWindow(hwnd, IID_PPV_ARGS(&store));
  if (FAILED(hr))
    return hr;
  PROPVARIANT value{};
  hr = store->GetValue(PKEY_AppUserModel_RelaunchIconResource, &value);
  const bool matches = SUCCEEDED(hr) && value.vt == VT_LPWSTR &&
                       value.pwszVal &&
                       std::wstring(value.pwszVal) == std::wstring(exe) + L",-1";
  PropVariantClear(&value);
  // The runner already loaded and checked both sizes from resource 1 using
  // LoadImageW. Here verify the shell reference, not a second extraction API.
  return matches ? S_OK : HRESULT_FROM_WIN32(ERROR_INVALID_DATA);
}
