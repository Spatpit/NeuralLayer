cbuffer Params : register(b0) {
    float2 canvas; float2 padding;
    float4 crop;
    float4 adjustment;
    float4 modes;
};
Texture2D inputImage : register(t0);
SamplerState linearSampler : register(s0);
struct VSOut { float4 position : SV_POSITION; float2 uv : TEXCOORD0; float4 color : COLOR0; };
VSOut ui_vs(float2 position : POSITION, float2 uv : TEXCOORD0, float4 color : COLOR0) {
    VSOut o; o.position=float4(position.x/canvas.x*2-1, 1-position.y/canvas.y*2,0,1); o.uv=uv; o.color=color; return o;
}
float4 ui_ps(VSOut i) : SV_TARGET { return inputImage.Sample(linearSampler, i.uv) * i.color; }
float4 opaque_alpha_ps(VSOut i) : SV_TARGET { return float4(0,0,0,1); }
VSOut image_vs(uint id : SV_VertexID) {
    VSOut o; o.uv=float2((id << 1) & 2, id & 2); o.position=float4(o.uv*float2(2,-2)+float2(-1,1),0,1); o.color=1; return o;
}
float4 image_ps(VSOut i) : SV_TARGET {
    float2 uv = crop.xy + i.uv*crop.zw;
    float3 original=inputImage.Sample(linearSampler, uv).rgb;
    if (modes.x<0.5 || (modes.y>0.5 && i.uv.x<adjustment.w)) return float4(original,1);
    uint w,h; inputImage.GetDimensions(w,h);
    float2 px=1.0/float2(w,h);
    float3 neighbors=(inputImage.Sample(linearSampler,uv+float2(px.x,0)).rgb+inputImage.Sample(linearSampler,uv-float2(px.x,0)).rgb+inputImage.Sample(linearSampler,uv+float2(0,px.y)).rgb+inputImage.Sample(linearSampler,uv-float2(0,px.y)).rgb)*0.25;
    float3 color=original+(original-neighbors)*adjustment.x;
    color=lerp(dot(color,float3(0.2126,0.7152,0.0722)).xxx,color,adjustment.y);
    color=(color-0.5)*adjustment.z+0.5;
    if (modes.y>0.5 && abs(i.uv.x-adjustment.w)<1.0/canvas.x) color=float3(0.75,0.82,0.92);
    return float4(saturate(color),1);
}
