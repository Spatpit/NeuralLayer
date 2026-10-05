Source: https://github.com/Dagherbou/OptiScaler_DLSSNR

Branch: dlss-neural-rendering
Commit: 973761621353b99bee3dc7d4bb27b117fef2644f

These files are copied unchanged from that commit:

- `dlssnr_forwarder.cpp`: OptiScaler/dlssnr/forwarder/dlssnr_forwarder.cpp
- `DlssNr_Common.h`: OptiScaler/shaders/dlssnr/DlssNr_Common.h
- `dlssnr.hlsl`, `DlssNr_Shader.h`: OptiScaler/shaders/dlssnr/precompile/
- `LICENSE`: repository GPL-3.0 license
- `RenoDX_ATTRIBUTION.txt`: Licenses/RenoDX_ATTRIBUTION.txt

Silver's adaptation is in native/optiscaler.h. It hosts the forwarder and SDR
composition shader in its own capture renderer, with generated motion guides
and flat depth. It does not load OptiScaler's game interception layer or offer
engine-dependent frame generation. NVIDIA's neural runtime is supplied locally
by the user and is not part of this source distribution.

To regenerate the checked-in shader bytecode on Windows, run the Windows SDK
`fxc /T cs_5_0 /E CSMain /O3 /Fh DlssNr_Shader.h /Vn DlssNr_cso dlssnr.hlsl`.
The entry point must match the source; see docs/optiscaler.md for build details.
