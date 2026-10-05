#pragma once
// Original motion-reprojected lighting-edit history. No RGB frame averaging.
static const char *neural_lighting_shader = R"HLSL(
cbuffer Params : register(b0) { uint mode; float valid; uint width; uint height; float balanced; };
Texture2D<float4> processed : register(t0);
Texture2D<float4> history : register(t1);
Texture2D<float4> original : register(t2);
Texture2D<float2> motion : register(t3);
RWTexture2D<float4> target : register(u0);
SamplerState linearSampler : register(s0);
float luminance(float3 c) { return dot(c,float3(.2126,.7152,.0722)); }
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID) {
 if(id.x>=width || id.y>=height) return;
 float2 uv=(float2(id.xy)+.5)/float2(width,height);
 if(mode==0) {
  uint fw,fh; original.GetDimensions(fw,fh);
  float3 source=0; float edit=0,lo=1,hi=-1,sourceLo=1,sourceHi=0;
  [unroll] for(int y=-1;y<=1;++y) [unroll] for(int x=-1;x<=1;++x) {
   float2 q=uv+float2(x,y)*3/float2(fw,fh);
   float3 s=original.SampleLevel(linearSampler,q,0).rgb;
   float d=luminance(processed.SampleLevel(linearSampler,q,0).rgb-s);
   float weight=(x==0?2:1)*(y==0?2:1)/16.0;
   source+=s*weight; edit+=d*weight; lo=min(lo,d);hi=max(hi,d);
   float light=luminance(s);sourceLo=min(sourceLo,light);sourceHi=max(sourceHi,light);
  }
  float2 mv=motion.SampleLevel(linearSampler,uv,0);
  float2 previousUv=uv+mv;
  float sy=luminance(source),chroma=source.r-source.b;
  float4 old=float4(edit,edit,sy,chroma);
  if(valid>.5) old=history.SampleLevel(linearSampler,previousUv,0);
  float trust=valid*saturate(1-abs(sy-old.z)/.035)*saturate(1-abs(chroma-old.w)/.06);
  trust*=trust;
  if(any(previousUv<0)||any(previousUv>1)||any(abs(mv)>.05)) trust=0;
  // Flat source surfaces provide little spatial support for a generated
  // shadow. Allow more temporal support there, retaining the existing
  // narrow clamp around source edges and the same history-rejection tests.
  float flat=1-smoothstep(.015,.07,sourceHi-sourceLo);
  float allowance=lerp(.012,balanced>.5 ? .05 : .04,flat);
  float supported=clamp(old.x,lo-allowance,hi+allowance);
  float limit=lerp(.035,balanced>.5 ? .065 : .055,flat);
  float stable=edit+lerp(.70,balanced>.5 ? .88 : .82,flat)*trust*clamp(supported-edit,-limit,limit);
  target[id.xy]=float4(stable,edit,sy,chroma);
 } else {
  float3 edited=processed.Load(int3(id.xy,0)).rgb;
  float4 h=history.SampleLevel(linearSampler,uv,0);
  float sy=luminance(original.Load(int3(id.xy,0)).rgb);
  // Prevent a coarse lighting correction spilling across defined boundaries.
  float edge=saturate(1-abs(sy-h.z)/.12);
  target[id.xy]=float4(saturate(edited+(h.x-h.y)*edge),1);
 }
}
)HLSL";
