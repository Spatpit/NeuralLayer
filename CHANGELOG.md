# Changelog

## 0.7.1 — first public release

NeuralLayer captures a window, enhances its image on the GPU and shows the
result in an overlay on top of it.

- Window capture with Windows Graphics Capture, placed over the source once
  (**Fit again** to re-align), with Always on top that also works over topmost
  games without taking focus.
- Neural rendering through the OptiScaler neural module and an NVIDIA neural
  runtime the user downloads separately and imports (**Import neural runtime…**);
  the runtime is not included.
- SpatpitNeuralFx motion estimation with source-detail protection and automatic
  lighting stabilization; Default, Natural, Cinematic and Balanced styles; model
  resolution slider; 1×/2×/3× passes, with optional own settings for passes 2
  and 3.
- Looks: import ReShade presets from a preset `.ini` with its `reshade-shaders`
  folder or from a `.zip`; only referenced effects, includes and textures are
  copied, saved values are kept, and looks can be switched, turned off or
  removed. Image adjustments and per-effect ReShade controls, plus the included
  SpatpitClarity shader.
- Comparison (F9) with wipe or side-by-side and a divider dragged directly on
  the image.
- Dark menu (light theme available) with Source, Neural, Look, Compare,
  Settings and Diagnostics pages, and a compact command bar.
- Configurable global shortcuts (defaults F6, F8, F9, Ctrl+Shift+O); F8 brings
  back whichever controls were last used.
- Click-through when the controls are hidden; discoverable by streaming
  application pickers while the full menu is open.
- Output FPS limit (30/60/90/120) with separate capture, neural and redraw rates.
- Preferences saved automatically beside the app.
- Portable ZIP with a ReShade 6.8.0 build from pinned official source and the
  complete source code.
