# v0.7.1 release validation

v0.7.1 is the first public NeuralLayer release. It ships a ReShade 6.8.0 build
made from pinned official source, imports the NVIDIA neural runtime from a file
the user provides, and excludes third-party binaries and presets from the
package. See [validation.md](validation.md) for checks made during development.

## Release scope

- SpatpitNeuralFx is the motion backend. The native configuration boundary
  enforces it, so a hand-edited settings file cannot select another.
- No third-party visual presets are shipped. SpatpitClarity is the only included
  shader, in a self-contained form that needs no ReShade include files.
- **Look → Import preset…** imports user presets from a preset `.ini` with its
  `reshade-shaders` folder, or from a `.zip` (extracted with Windows' built-in
  `tar`). Referenced effects are copied as `SpatpitLook_<id>_<file>.fx` under
  `runtime-optiscaler/shaders/looks/<id>/` with every include/helper file in its
  original layout, textures under `textures/looks/<id>/`, and a rewritten preset
  with the saved values under `looks/<id>.ini`. Unreferenced effects are not
  copied. `ReShade.ini` search paths are made recursive. One look runs at a time
  through the managed allow-list keyed on the `SpatpitLook_` prefix.

## Validation (2026-10-05)

- Release build, `cargo fmt --check`, Clippy with warnings denied, and ten unit
  tests pass. The preset-import tests cover INI + `reshade-shaders` (includes
  kept, unused effects skipped, textures copied, values parsed, search paths
  updated, duplicate imports get new ids, removal deletes files and turns an
  active look off), ZIP import with staging cleanup, and rejection of presets
  whose shaders are missing.
- Package audit: the portable ZIP contains only the allowed binaries
  (`NeuralLayer.exe`, the OptiScaler forwarder and the pinned `ReShade64.dll`,
  SHA-256 `055eee2b…e965`). It contains no NVIDIA runtime, no third-party
  presets or shaders and no personal settings; packaging rejects them by name.
  The executable is built with the builder's home, Cargo, rustup and project
  paths remapped (`--remap-path-prefix`), and packaging scans every staged file,
  binaries included, for those paths before creating the ZIP.
- Live checks ran on a private copy of the package with a locally imported
  NVIDIA runtime, which is never added to the package:
  - `--test-looks`: a fixture preset (an include file and a saved value that
    inverts the image) imported from INI and from ZIP compiles in the bundled
    ReShade, changes the output and keeps it opaque; Off and removing the active
    look restore the exact original pixels.
  - `--test-looks --look-source <preset.ini>` with a publicly available
    multi-effect preset (8 effects with include files and LUT textures): imported
    with nothing missing, compiled, changed the sampled pixel from (80,140,200)
    to (110,146,189), and was removed cleanly.
  - `--test-neural --test-custom --test-multipass --test-menu`,
    `--safe-graphics --test-hotkeys`, `--safe-graphics --test-input-routing`,
    `--test-menu-recovery --safe-graphics`, `--test-source-close --test-neural`,
    `--safe-graphics --test-topmost` and `--test-reshade` all pass, including
    3× with own settings for passes 2 and 3.
- First start of the clean package without the NVIDIA runtime shows **Import
  neural runtime…** on the Neural page, no motion-backend selector, and an empty
  Look page with **Import preset…**.

The interactive file picker itself was not driven by the tests; the import logic
is exercised directly.
