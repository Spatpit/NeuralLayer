// NeuralLayer's own color-only effect. MIT License.
texture2D SpatpitSource : COLOR;
sampler2D SpatpitSampler { Texture = SpatpitSource; };
void PostProcessVS(uint vertex : SV_VertexID, out float4 position : SV_Position, out float2 uv : TEXCOORD) {
    uv = float2((vertex << 1) & 2, vertex & 2);
    position = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
}
uniform float Sharpness < ui_label = "Sharpness"; ui_type = "slider"; ui_min = 0.0; ui_max = 2.0; ui_step = 0.01; > = 0.3;
uniform float Contrast < ui_label = "Contrast"; ui_type = "slider"; ui_min = 0.5; ui_max = 1.5; ui_step = 0.01; > = 1.08;
float4 SpatpitClarityPS(float4 position : SV_Position, float2 uv : TEXCOORD) : SV_Target {
    float3 c = tex2D(SpatpitSampler, uv).rgb;
    float3 average = (tex2Doffset(SpatpitSampler, uv, int2(1,0)).rgb
        + tex2Doffset(SpatpitSampler, uv, int2(-1,0)).rgb
        + tex2Doffset(SpatpitSampler, uv, int2(0,1)).rgb
        + tex2Doffset(SpatpitSampler, uv, int2(0,-1)).rgb) * 0.25;
    return float4(saturate((c + (c-average)*Sharpness - 0.5)*Contrast + 0.5), 1);
}
technique SpatpitClarity {
    pass { VertexShader = PostProcessVS; PixelShader = SpatpitClarityPS; }
}
