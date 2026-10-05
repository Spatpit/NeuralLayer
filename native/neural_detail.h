#pragma once
// Original detail-preserving composition: retain broad neural edits while
// reducing changes to source structure. Extra passes use the game capture and
// a wider detail band so earlier shape changes are not treated as ground truth.
// Additional passes relax this anchor on low-contrast source surfaces to retain
// neural texture, while keeping strong protection around defined source features.
// No temporal history or face mask.
static const char *neural_detail_shader = R"HLSL(
cbuffer Params : register(b0) {
  uint mode; float whitePoint; uint width; uint height;
  // Shares the host's TransferStrength slot, after vendor resolve is complete.
  float detailRadius; float3 unused0; float4 unused1;
  uint guideHeight; uint compareMode; float split; float zoom;
  uint swapSides;
};
Texture2D<float4> processed : register(t0);
Texture2D<float4> original : register(t2);
RWTexture2D<float4> target : register(u0);
SamplerState linearSampler : register(s0);
float3 sampleEdited(float2 uv) {
  // Resolve into the existing float proxy, but preserve the SDR clipping that
  // its former R8 target performed before neighborhood interpolation.
  float2 p=uv*float2(width,height)-.5;
  int2 base=int2(floor(p)), hi=int2(width,height)-1;
  float2 t=frac(p);
  float3 a=saturate(processed.Load(int3(clamp(base,0,hi),0)).rgb);
  float3 b=saturate(processed.Load(int3(clamp(base+int2(1,0),0,hi),0)).rgb);
  float3 c=saturate(processed.Load(int3(clamp(base+int2(0,1),0,hi),0)).rgb);
  float3 d=saturate(processed.Load(int3(clamp(base+1,0,hi),0)).rgb);
  return lerp(lerp(a,b,t.x),lerp(c,d,t.x),t.y);
}
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID) {
  if (id.x >= width || id.y >= height) return;
  float2 uv = (float2(id.xy)+.5)/float2(width,height), sampleUv=uv;
  float divider=compareMode==1 ? .5 : split;
  bool showOriginal=compareMode!=0 && ((uv.x<divider)!=(swapSides!=0));
  if(compareMode==1) {
    float2 halfUv=float2(uv.x<.5 ? uv.x*2 : (uv.x-.5)*2,uv.y)-.5;
    sampleUv=.5+halfUv*float2(1,2)/zoom;
  }
  float3 src=original.SampleLevel(linearSampler,sampleUv,0).rgb;
  float3 color=src;
  if(!showOriginal) {
    float3 edited=sampleEdited(sampleUv);
    float3 low=0, lowSource=0;
    float sourceSquare=0;
    [unroll] for(int y=-1;y<=1;++y)
      [unroll] for(int x=-1;x<=1;++x) {
        float2 q=sampleUv+float2(x,y)*max(detailRadius,2.5)/float2(width,height);
        float weight=(x==0 ? 2 : 1)*(y==0 ? 2 : 1)/16.0;
        low+=weight*sampleEdited(q);
        float3 sourceSample=original.SampleLevel(linearSampler,q,0).rgb;
        lowSource+=weight*sourceSample;
        float luminance=dot(sourceSample,float3(.2126,.7152,.0722));
        sourceSquare+=weight*luminance*luminance;
      }
    float3 gain=clamp((low+.01)/(lowSource+.01),.25,4);
    float anchor=.75;
    if(detailRadius>2.5) {
      // Reuse the same spatial samples. Relative source contrast distinguishes
      // defined structure from flatter surfaces where new texture can survive.
      // The floor stabilizes dark regions; the transition avoids a binary mask.
      float mean=dot(lowSource,float3(.2126,.7152,.0722));
      float contrast=sqrt(max(0,sourceSquare-mean*mean))/(mean+.04);
      anchor=lerp(.15,.75,smoothstep(.025,.08,contrast));
    }
    if(mode==3) {
      // Balanced: retain compatible surface texture, but anchor unsupported
      // changes more strongly where the source contains defined structure.
      float mean=dot(lowSource,float3(.2126,.7152,.0722));
      float structure=smoothstep(.015,.07,sqrt(max(0,sourceSquare-mean*mean)));
      float disagreement=dot(abs((edited-low)-(src-lowSource)*gain),float3(.2126,.7152,.0722));
      float risk=structure*smoothstep(.012,.06,disagreement);
      anchor=lerp(anchor,.95,risk);
    }
    color=lerp(edited,low+(src-lowSource)*gain,anchor);
  }
  if(any(sampleUv<0)||any(sampleUv>1))color=0;
  if(compareMode!=0 && abs(uv.x-divider)<1.0/width)color=1;
  target[id.xy]=float4(saturate(color),1);
}
)HLSL";
