#pragma once
// Compare the untouched capture against the completed pass chain. Never feed
// a divider, letterbox or comparison layout into another neural evaluation.
static const char *neural_compare_shader = R"HLSL(
cbuffer Params : register(b0) {
  uint mode; float whitePoint; uint width; uint height;
  float4 unused0; float4 unused1;
  uint guideHeight; uint compareMode; float split; float zoom;
  uint swapSides;
};
Texture2D<float4> processed : register(t0);
Texture2D<float4> original : register(t2);
RWTexture2D<float4> target : register(u0);
SamplerState linearSampler : register(s0);
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID) {
  if (id.x >= width || id.y >= height) return;
  float2 uv = (float2(id.xy) + 0.5) / float2(width, height);
  float2 sampleUv = uv;
  float divider = compareMode == 1 ? 0.5 : split;
  bool showOriginal = (uv.x < divider) != (swapSides != 0);
  if (compareMode == 1) {
    float2 halfUv = float2(uv.x < 0.5 ? uv.x * 2 : (uv.x - 0.5) * 2, uv.y) - 0.5;
    sampleUv = 0.5 + halfUv * float2(1,2) / zoom;
  }
  float3 color;
  if (compareMode == 1)
    color = showOriginal ? original.SampleLevel(linearSampler, sampleUv, 0).rgb
                         : processed.SampleLevel(linearSampler, sampleUv, 0).rgb;
  else
    color = showOriginal ? original[id.xy].rgb : processed[id.xy].rgb;
  if (any(sampleUv < 0) || any(sampleUv > 1)) color = 0;
  if (abs(uv.x - divider) < 1.0 / width) color = 1;
  target[id.xy] = float4(color, 1);
}
)HLSL";
