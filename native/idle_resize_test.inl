// Real swapchain/ReShade idle-resize fixture; no game or desktop capture.
extern "C" int spatpit_test_idle_resize(const wchar_t *runtime, const wchar_t *report) {
  std::ofstream log{std::filesystem::path(report)};
  HWND window = nullptr;
  try {
    check(CoInitializeEx(nullptr, COINIT_MULTITHREADED), "Resize test COM");
    WNDCLASSW wc{};
    wc.lpfnWndProc = DefWindowProcW;
    wc.hInstance = GetModuleHandleW(nullptr);
    wc.lpszClassName = L"SpatpitIdleResizeTest";
    RegisterClassW(&wc);
    window = CreateWindowExW(WS_EX_NOREDIRECTIONBITMAP | WS_EX_TOOLWINDOW,
        wc.lpszClassName, L"Idle resize fixture", WS_POPUP, 0, 0, 680, 400,
        nullptr, nullptr, wc.hInstance, nullptr);
    if (!window) throw std::runtime_error("Create resize fixture");
    {
      Engine e;
      e.init(window, runtime);
      SpatpitOptions options{};
      options.saturation = options.contrast = 1;
      uint8_t white[] = {255,255,255,255};
      e.texture(77,1,1,white);
      auto draw = [&](unsigned w, unsigned h) {
        MSG msg{};
        while (PeekMessageW(&msg,nullptr,0,0,PM_REMOVE)) { TranslateMessage(&msg); DispatchMessageW(&msg); }
        SetWindowPos(window,nullptr,0,0,w,h,SWP_NOMOVE|SWP_NOZORDER|SWP_NOACTIVATE);
        SpatpitVertex v[]={{0,0,0,0,0xff00ff00},{float(w),0,1,0,0xff00ff00},
            {float(w),float(h),1,1,0xff00ff00},{0,float(h),0,1,0xff00ff00}};
        uint32_t indices[]={0,1,2,0,2,3};
        SpatpitDraw d{0,6,0,77,{w*.25f,h*.25f,w*.75f,h*.75f}};
        e.render(w,h,1,v,4,indices,6,&d,1,options);
      };
      auto until=GetTickCount64()+5500;
      while(GetTickCount64()<until) { draw(680,400); Sleep(10); }
      if (!e.effects) throw std::runtime_error("ReShade runtime not loaded in resize test");
      for(unsigned deferred=0;deferred<2;++deferred) {
        options.idle_resize=0;
        draw(680,400);
        options.idle_resize=deferred;
        unsigned resizes=0;
        auto start=std::chrono::steady_clock::now();
        for(unsigned i=1;i<=40;++i) {
          unsigned before=e.width;
          draw(680+i*6,400+i*3);
          if(e.width!=before) ++resizes;
        }
        double ms=std::chrono::duration<double,std::milli>(std::chrono::steady_clock::now()-start).count();
        std::vector<uint8_t> pixels(size_t(e.width)*e.height*4);
        e.screenshot(pixels.data(),e.width,e.height);
        auto pixel=[&](unsigned x,unsigned y){return &pixels[(size_t(y)*e.width+x)*4];};
        if(pixel(e.width/2,e.height/2)[1]<250 || pixel(e.width*4/5,e.height*4/5)[3]!=0)
          throw std::runtime_error("Idle UI projection/scissor mismatch");
        if(resizes!=(deferred?0u:40u)) throw std::runtime_error("Unexpected drag resize count");
        options.idle_resize=0;
        draw(920,520);
        if(e.width!=920 || e.height!=520) throw std::runtime_error("Resize not committed on release");
        log << "deferred=" << deferred << " drag_frames=40 buffer_resizes=" << resizes
            << " average_render_ms=" << ms/40 << '\n';
      }
      // Native ReShade UI cannot use a stretched old buffer.
      options.idle_resize=1;
      e.native_overlay=true;
      draw(700,420);
      if(e.width!=700 || e.height!=420) throw std::runtime_error("Native panel resize was deferred");
      e.native_overlay=false;
      log << "PASS: idle resize coalescing, clipping during stretch, exact release size, native-panel guard\n";
    }
    DestroyWindow(window);
    CoUninitialize();
    return 1;
  } catch(const std::exception &error) {
    log << "FAIL: " << error.what() << '\n';
    if(window) DestroyWindow(window);
    return 0;
  }
}
