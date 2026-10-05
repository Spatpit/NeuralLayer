# Validation summary

How NeuralLayer has been checked during development. These are functional and
regression checks on one Windows 11 desktop with an NVIDIA RTX GPU at
2560×1440 and 120 Hz, not benchmarks; results on other systems can differ. The
detailed working notes behind this summary are kept privately because they
refer to personal test recordings. See [release-0.7.1.md](release-0.7.1.md) for
the checks run on this release.

## Automated checks

- **Unit tests** cover settings parsing and persistence, shortcut parsing and
  conflicts, settings-file migration, the neural-runtime importer, and
  preset import (INI with `reshade-shaders`, ZIP, missing shaders, removal).
- **GPU tests** (`--test-motion` and replay modes) check the motion estimator and
  detail/lighting shaders with synthetic patterns: stationary and moving content,
  frame borders, thin lines, repeating stripes versus smooth gradients, temporal
  rejection, reset, resize and cached output.
- **Live self-tests** (`--self-test …`) run the real app against a controlled
  source window and verify results on the output pixels. They cover capture and
  fitting, image adjustments, ReShade effects and looks, neural activation and
  bypass, 1×/2×/3× switching, menu and command bar input, click-through, comparison
  controls, shortcuts and recording new ones, tray and taskbar recovery,
  Always on top with topmost games and separate metrics windows, pausing, and
  recovery when the source window closes.

## Image-quality work

Quality changes to SpatpitNeuralFx were evaluated by replaying captured game
footage through the production pipeline and comparing neural-off, previous and
updated output on matched frames, with crops and measurements of change over
time. Issues addressed this way include:

- false motion at frame borders and blocky shading near the top of the frame;
- bright specks on thin moving lines;
- block bursts from unsupported large or isolated motion estimates;
- patchy skin shading from smooth gradients treated as stripes;
- small reshaping of faces, especially with repeated passes, reduced by
  source-detail preservation while keeping texture on clothing and skin;
- low-frequency lighting flicker on smooth surfaces, reduced by automatic
  lighting stabilization on the final pass.

Remaining limits: generated detail can still change shapes or flicker, extra
passes can amplify artifacts, and results depend on the scene and runtime.

## Performance and cadence

- Capture delivers fresh frames above 60 FPS on a 120 Hz desktop once the Windows
  capture interval is removed; the menu reports capture, neural and redraw rates
  separately.
- The newest queued capture frame is used to avoid processing an older image.
- Cached redraws repeat the last result without another neural evaluation.
- Repeated model-resolution changes with three passes no longer retain extra GPU
  memory after downsizing.
- Intermediate copies overwritten before presentation were removed to reduce GPU
  work.

No frame-rate improvement for the captured game is claimed.

## Streaming

With the full menu open, the window is discoverable by streaming application
pickers such as Discord's; hiding the menu restores click-through. External
Windows Graphics Capture of the app's output was verified; an end-to-end
broadcast was not.
