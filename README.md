# NeuralLayer

NeuralLayer is a portable Windows x64 app that captures a window, such as a game
or video, enhances its image on the GPU, and shows the result in an overlay on
top of it. It combines neural rendering with SpatpitNeuralFx motion estimation,
ReShade looks you import yourself, and simple image adjustments.

NeuralLayer is a community project. It is not affiliated with or endorsed by
NVIDIA, and it does not integrate with games: it works on the captured image.

**Current version: 0.7.1** (first public release).

## Requirements

- Windows 10 version 2004 or newer, or Windows 11, 64-bit.
- A DirectX 12 GPU. Neural rendering additionally needs a compatible NVIDIA GPU
  and driver, plus the NVIDIA neural runtime DLL described below.
- Games in windowed or borderless mode. Exclusive fullscreen and protected
  content may not be capturable.

## The NVIDIA neural runtime (required for neural rendering)

Neural rendering uses NVIDIA's neural runtime file, `nvngx_dlssnr.dll`. This file
is **not included** with NeuralLayer and is **not distributed** in this repository
or its releases. You need to **download it separately from an external source**
and import it into the app. NeuralLayer does not download it for you and does not
link to any download.

- Obtain the DLL from a source you trust, and use it under the terms that apply
  to it. NeuralLayer's license grants no rights to that file.
- When importing, NeuralLayer only checks that the file is a 64-bit Windows DLL.
  It cannot verify that it is genuine or compatible with your GPU and driver.
- Without the DLL, everything else still works: window capture, looks, ReShade
  effects and image adjustments.

## Getting started

1. Download `NeuralLayer-0.7.1-portable-win64.zip` from the
   [Releases](../../releases) page and extract the whole ZIP into a writable
   folder. Keep its files together.
2. Run **NeuralLayer.exe** and pick the window to enhance on the **Source** page.
   The overlay is placed over that window.
3. To use neural rendering, open **Neural → Import neural runtime…** and select the
   `nvngx_dlssnr.dll` you downloaded. Only that one file is copied into the app's
   `runtime-optiscaler` folder.
4. Turn on **Enable neural rendering**. Start with **Normal 1×** and model
   resolution **50%**, then try the Default, Natural, Cinematic and Balanced styles.
5. Press **F8** to hide the controls. The overlay then lets mouse and keyboard
   input through to your game.

If something prevents startup, open a command prompt in the app folder and run
`NeuralLayer.exe --safe-graphics` to start without ReShade or the neural runtime.

## Neural rendering

SpatpitNeuralFx estimates motion from the captured frames, protects fine source
detail, and stabilizes generated lighting on the final output. The neural model
itself comes from the NVIDIA runtime you imported.

- **Styles:** Default, Natural, Cinematic and Balanced (a middle ground between
  Default and Natural with adaptive detail protection).
- **Model resolution:** how much of the canvas size the model processes. 50% means
  a quarter of the pixels; the output stays full size.
- **Passes:** 1×, 2× or 3×. Extra passes reprocess the previous result, use more
  GPU time and memory, and can amplify artifacts; they don't raise frame rates.
  Under **Extra passes**, passes 2 and 3 can use their own model resolution,
  style, intensity and blend.
- **Compare (F9):** wipe or side-by-side against the original capture. Drag the
  divider directly on the image.

Generated detail can still change shapes or flicker, and results depend on the
scene and the runtime. The captured image has no game depth buffer, motion
vectors or separate HUD layer, so the game's HUD is processed as part of the
image. NeuralLayer does not increase the captured game's frame rate.

## Looks: your own ReShade presets

The **Look** page runs one ReShade preset at a time on the captured image, before
neural rendering. NeuralLayer doesn't ship third-party presets; you import your own:

1. Download a ReShade preset. It usually comes as a preset `.ini` plus a
   `reshade-shaders` folder (with `Shaders` and `Textures`), often inside a `.zip`.
2. On **Look**, click **Import preset…** and pick either the `.zip` itself, or the
   preset `.ini` with its `reshade-shaders` folder next to it (or one level up).
3. The look is selected right away. Switch looks or choose **Off** at any time;
   the **×** button removes a look and its copied files.

Only the effects the preset uses are copied, with their include files and
textures, and the preset's saved settings are kept. Each look is stored
separately, so looks never interfere with each other. If a download contains
several presets, each becomes its own look. Effects the preset lists but the
download doesn't contain are skipped and reported.

Effects that need the game's depth buffer (ambient occlusion, depth of field, some
fog or lighting effects) can't work on captured video. Presets keep their own
licenses; check a preset's terms before sharing it.

The same page has **Image adjustments** (sharpness, saturation, contrast) and
**ReShade effects** with per-effect toggles and settings, including NeuralLayer's
own SpatpitClarity shader.

## Menu and controls

The menu has six pages: **Source**, **Neural**, **Look**, **Compare**, **Settings**
and **Diagnostics**. It uses a dark theme; a light theme is under
**Settings → Appearance**. The collapse button in the menu header switches to the
**command bar**, a compact strip over the image with the neural switch, passes,
style, compare, Always on top, live rates and quick settings.

| Action | Default shortcut |
| --- | --- |
| Neural rendering on / off | F6 |
| Hide / show the controls you were last using (menu or command bar) | F8 |
| Comparison controls | F9 |
| Clear / resume the overlay | Ctrl+Shift+O |

Change any shortcut under **Settings → Shortcuts**: click it, then press the new
keys. The tray icon also offers these actions.

- **Always on top** keeps the overlay above the game, including games that are
  themselves topmost, without taking keyboard focus.
- **Output FPS limit** caps processing at 30, 60, 90 or 120 FPS. Capture, neural
  evaluation and redraw rates are shown separately.
- **Streaming:** keep the full menu open while picking NeuralLayer in a streaming
  app such as Discord or OBS, then hide it with F8.
- Preferences are saved beside the app in `SpatpitOverlay.ini`,
  `SpatpitOptiScaler.ini` and `SpatpitEffects.ini`.

ReShade and the neural runtime run inside NeuralLayer's own process. Nothing is
installed into or injected into the captured game. Capture is SDR; HDR is not
supported.

## What's in the download

- `NeuralLayer.exe` and the OptiScaler neural forwarder `nvngx.dll_dlssnr.dll`.
- `runtime-optiscaler/` with a ReShade 6.8.0 build made from its official source
  and NeuralLayer's SpatpitClarity shader.
- `docs/` with this README, the changelog, the licenses, third-party notices
  (ReShade's under `docs/licenses/reshade`) and `SHA256SUMS.txt` for every file.
- `Source/` with the complete source code for this build.
- The NVIDIA neural runtime is **not** included; see above.

## Building from source

Install Rust (MSVC toolchain) 1.88+, Visual Studio C++ build tools and a Windows
SDK with C++/WinRT headers. On a new machine, restore Cargo dependencies with
`cargo fetch --locked`. Build the pinned ReShade source once, then check and
package:

```powershell
.\scripts\build-reshade.ps1
cargo fmt --all --check
cargo test --locked --offline
cargo clippy --locked --offline --all-targets -- -D warnings
.\scripts\package.ps1
```

`package.ps1` verifies the ReShade build against its pinned hash, stages only the
app, its runtime files and the source, rejects files that must not be published
(such as the NVIDIA runtime or personal settings), and writes the ZIP and its
SHA-256 file to `dist/`. The packaged executable is built with local build paths
removed.

The app also has live self-tests that drive a controlled test window, the mouse
and the keyboard, for example `NeuralLayer.exe --self-test --test-looks` or
`--self-test --test-neural --test-menu` (the latter needs the runtime imported).
Results are written to `artifacts/`.

More detail: [architecture](docs/architecture.md),
[neural backend](docs/optiscaler.md), [validation summary](docs/validation.md)
and [0.7.1 release checks](docs/release-0.7.1.md).

## License

NeuralLayer is distributed under **GPL-3.0-only** because it includes OptiScaler
code; NeuralLayer's own original code is also available under the MIT license
(`LICENSE-MIT.txt`).

- OptiScaler provenance and license: `vendor/optiscaler`.
- ReShade (BSD-3-Clause) provenance and dependency notices: `vendor/reshade`, and
  `docs/licenses/reshade` in the download.
- Rust crate and font notices: `THIRD-PARTY-NOTICES.txt`.

The NVIDIA neural runtime and any presets you import are third-party files under
their own terms and are not covered by NeuralLayer's licenses.
