# Bundled ReShade

ReShade 6.8.0, official source revision `18deaa52de0c425a78b329e9cb3c497281cd00ec`:
https://github.com/crosire/reshade/tree/18deaa52de0c425a78b329e9cb3c497281cd00ec

Built locally from the unmodified source and pinned submodules, with the upstream
Release / 64-bit configuration (full add-on support). This is an unofficial,
unsigned source build, not the installer binary distributed by reshade.me.
Build script: `scripts/build-reshade.ps1`. The initial build used VS 2026 v145
and Windows SDK 10.0.26100.0. Upstream increments its numeric build field during
compilation; this does not change the app's version.

ReShade and its bundled dependency notices are in `runtime-notices/`.
The generated DLL is excluded from git; the release package includes it.
`BUNDLED.sha256` pins the locally built DLL accepted by packaging. Rebuilding
ReShade updates that pin before packaging.
