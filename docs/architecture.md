# Architecture

NeuralLayer is a Rust application with a small C++20 native layer. Rust owns the
window (winit), the egui menu, source selection, input, tray, shortcuts,
settings and lifecycle. The native layer, called through a plain C ABI, owns
DirectX 12, DirectComposition, Windows Graphics Capture, ReShade hosting and the
neural pipeline. The menu and the processed image share one window and one
swapchain.

## Frame pipeline

1. Windows Graphics Capture delivers BGRA8 frames from the selected window
   (never from this app's own windows). The newest queued frame is used.
2. A D3D11 device on the renderer's adapter copies each frame into a shared
   D3D12 resource, ordered by a shared GPU fence. Frames never pass through CPU
   memory.
3. The app draws the cropped source and optional image adjustments.
4. The app-local ReShade runtime runs the active look (an imported preset) and
   any individually enabled effects, such as SpatpitClarity.
5. With neural rendering on, SpatpitNeuralFx estimates motion from consecutive
   captured frames, then the OptiScaler neural module evaluates the user-supplied
   NVIDIA runtime and composites the result at full canvas size. Optional 2× and
   3× passes each use their own feature and history; passes 2 and 3 can have
   their own model resolution, style, intensity and blend. Automatic lighting
   stabilization runs once on the final pass.
6. The egui menu is drawn last, and DirectComposition presents the
   premultiplied-alpha result.

Model-setting changes rebuild only the neural feature, after 400 ms without
further edits. Composition and comparison settings apply immediately and never
reset neural or lighting history. When capture produces no new frame, the last
result is redrawn without another neural evaluation.

## Window and input

The output is placed over the source once, on selection or **Fit again**. There
is no continuous window following. A low-level mouse hook routes input: the full
menu takes the whole window (so streaming pickers can find it), while the
command bar and comparison controls accept input only over themselves. With the
controls hidden, everything passes through to the source. Always on top restores
the overlay directly above a focused topmost source without stealing focus.

Global shortcuts are registered on a separate thread that also owns the tray
icon; it waits for messages instead of polling. Shortcuts are user-configurable
and suspended while a new one is being recorded.

## Looks

Importing a preset copies the effects it references into
`runtime-optiscaler/shaders/looks/<id>/` with a `SpatpitLook_<id>_` prefix, keeps
their include files in place, copies textures to `textures/looks/<id>/`, and
stores the preset with its saved values in `looks/<id>.ini`. ReShade searches
these folders recursively. The native layer allows only the active look's
effects to run, so switching looks never stacks them.

## Settings

Preferences are plain INI files beside the executable: `SpatpitOverlay.ini`
(app and window), `SpatpitOptiScaler.ini` (neural options) and
`SpatpitEffects.ini` (active look). They are written once an edit settles.

## Limits

Capture is SDR. There is no game depth buffer, motion vectors, jitter, engine
exposure or separate HUD layer: depth is a flat fallback and the game's HUD is
part of the processed image. Frame generation and HDR output are not
implemented. Exclusive fullscreen and protected content may not be capturable.
