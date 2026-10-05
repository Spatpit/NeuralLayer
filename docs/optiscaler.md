# Neural backend

NeuralLayer embeds the neural forwarder and composition shader from
[OptiScaler_DLSSNR](https://github.com/Dagherbou/OptiScaler_DLSSNR), pinned in
`vendor/optiscaler/PROVENANCE.md`. It adapts that neural module to captured
video; it is not the complete game upscaler middleware.

The NVIDIA neural runtime (`nvngx_dlssnr.dll`) is **not included**. Import your
own copy on the **Neural** page (**Import neural runtime…**). Only that file is
copied into `runtime-optiscaler`; the importer checks that it is a 64-bit Windows
DLL, not that it is authentic or compatible.

## Controls

- **Style:** Default, Natural, Cinematic and Balanced (SpatpitNeuralFx's tuning
  between Default and Natural, with adaptive source-detail protection).
- **Model resolution** (25–100%, default 50%): the share of the canvas size sent
  to the model. 50% evaluates a quarter of the pixels; the output stays full size.
- **Passes:** Normal 1×, 2× or 3×. Extra passes reprocess the previous result and
  can use their own model resolution, style, intensity and blend.
- **Enlargement:** classic or matched residual.
- **Image balance:** intensity, neural blend, colour transfer; under Advanced,
  preset, structure, local tone, skin structure, automatic character mask and
  maximum brightening.
- **Compare:** wipe or side-by-side against the original capture.
- **Diagnostics:** live rates, model state, motion multipliers, rebuild/retry and
  a reset of the neural settings.

Settings persist in `SpatpitOptiScaler.ini` beside the executable. Enabling or
bypassing (F6) keeps the model loaded; composition changes apply on the next
frame; model changes rebuild the feature after a 400 ms pause, which can briefly
hitch.

## What capture cannot provide

Capture supplies SDR color, SpatpitNeuralFx's estimated motion and a flat depth
fallback. It has no game depth, jitter, pre-exposure or separate UI layer, so
OptiScaler's frame generation, Reflex and UI correction options are not exposed,
and the game's HUD can be changed by the model. No frame-rate improvement is
claimed.

The runtime is loaded only into NeuralLayer's own process; nothing is copied to
or injected into the captured application. On failure, the original image is
shown with a diagnostic on the Neural page; details are written to
`runtime-optiscaler/SpatpitOptiScaler.log`. Initialization failures may require
restarting the app. Without a compatible GPU or runtime, capture, looks and
image adjustments still work.

## Building

Install Rust (MSVC) and Visual Studio C++ tools with the Windows SDK, then run
`cargo build --locked --release`. `build.rs` builds the upstream forwarder as
`nvngx.dll_dlssnr.dll` beside the executable, compiles the native capture host
and embeds the precompiled composition shader. See the README for the pinned
ReShade build and packaging.

## Licensing

The combined build is GPL-3.0-only because it includes OptiScaler code. Original
Spatpit code retains its MIT notice in `LICENSE-MIT.txt`; the vendored shader's
RenoDX attribution is preserved. Portable packages include `Source/` with the
corresponding source and build scripts.
