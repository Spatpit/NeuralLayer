// Original Spatpit implementation of coarse-to-fine patch matching and local
// gradient refinement. Coordinates are backward displacement in normalized UV.
Texture2D<float4> imageA : register(t0);
Texture2D<float4> imageB : register(t1);
Texture2D<float4> coarse : register(t2);
Texture2D<float4> forwardFlow : register(t3);
Texture2D<float4> backwardFlow : register(t4);
Texture2D<float4> history : register(t5);
RWTexture2D<float4> result : register(u0);
RWTexture2D<float2> motionGuide : register(u1);
SamplerState linearClamp : register(s0);
cbuffer Dimensions : register(b0) {
    uint fullWidth, fullHeight, outWidth, outHeight;
    uint mode, level, resetHistory, direction;
};
float luma(float3 rgb) { return dot(rgb, float3(.2126, .7152, .0722)); }
float sampleA(float2 uv) { return imageA.SampleLevel(linearClamp, uv, 0).r; }
float sampleB(float2 uv) { return imageB.SampleLevel(linearClamp, uv, 0).r; }
float2 imageSize() { uint w, h; imageA.GetDimensions(w,h); return float2(w,h); }

float cost(float2 uv, float2 pixels) {
    float2 size = imageSize(), target = uv + pixels / size;
    // The patch may straddle the image edge: both samplers clamp it equally.
    // Reject only an out-of-image center. Requiring the entire patch to fit
    // excludes zero motion in the outer guide cells and invents inward flow.
    if (any(target < .5 / size) || any(target > 1 - .5 / size)) return 100;
    float sum = 0, squared = 0;
    if (level >= 3) {
        // Dense support only at the small scales that choose large movement.
        [loop] for (int y = -2; y <= 2; ++y)
            [loop] for (int x = -2; x <= 2; ++x) {
                float2 offset = float2(x,y) * 2 / size;
                float d = clamp(sampleA(uv+offset)-sampleB(target+offset),-.25,.25);
                sum += d; squared += d*d;
            }
        float mean = sum/25;
        return max(0,squared/25-mean*mean);
    }
    // Sparse support retains the patch footprint at the expensive scales.
    [unroll] for (int y = -1; y <= 1; ++y)
        [unroll] for (int x = -1; x <= 1; ++x) {
            float2 offset = float2(x,y) * 4 / size;
            float d = clamp(sampleA(uv+offset)-sampleB(target+offset),-.25,.25);
            sum += d; squared += d*d;
        }
    [unroll] for (int j = 0; j < 4; ++j) {
        float2 offset = (j < 2 ? float2(j ? 2 : -2,0) : float2(0,j == 2 ? -2 : 2))/size;
        float d = clamp(sampleA(uv+offset)-sampleB(target+offset),-.25,.25);
        sum += d; squared += d*d;
    }
    float mean = sum/13;
    return max(0,squared/13-mean*mean);
}
void candidate(float2 uv, float2 v, inout float2 best, inout float bestCost, float prior) {
    // Resolve near-equivalent periodic matches in favor of a shorter vector.
    // The prior is tiny relative to a resolved image mismatch, and applies
    // only at full resolution so the wide coarse search is preserved.
    float c = cost(uv,v) + prior*dot(v,v);
    if (c + .0000001 < bestCost) { bestCost = c; best = v; }
}
float2 refine(float2 uv, float2 v) {
    float2 size = imageSize();
    float xx = .00001, xy = 0, yy = .00001;
    float2 b = 0;
    float mean = 0;
    [unroll] for (int y = -1; y <= 1; ++y)
        [unroll] for (int x = -1; x <= 1; ++x) {
            float2 p = uv + float2(x,y)*4/size;
            mean += sampleA(p) - sampleB(p + v/size);
        }
    mean /= 9;
    [unroll] for (int y = -1; y <= 1; ++y)
        [unroll] for (int x = -1; x <= 1; ++x) {
            float2 p = uv + float2(x,y)*4/size, q = p + v/size;
            float residual = sampleA(p) - sampleB(q) - mean;
            float2 g = float2(sampleB(q + float2(1,0)/size) - sampleB(q - float2(1,0)/size),
                              sampleB(q + float2(0,1)/size) - sampleB(q - float2(0,1)/size)) * .5;
            float weight = min(1, .04 / max(abs(residual), .00001));
            xx += weight*g.x*g.x; xy += weight*g.x*g.y; yy += weight*g.y*g.y;
            b += weight*g*residual;
        }
    float det = xx*yy - xy*xy;
    if (det < .00000001) return v;
    float2 delta = float2(yy*b.x-xy*b.y, xx*b.y-xy*b.x)/det;
    return v + clamp(delta, -1, 1);
}
float stationaryNoiseAllowance(float2 uv, float2 displacement, out float observable, out bool nearlyStill) {
    float2 size = imageSize();
    float xx = 0, xy = 0, yy = 0, residual = 0;
    float2 gradientSum = 0;
    [loop] for (int y = -2; y <= 2; ++y)
        [loop] for (int x = -2; x <= 2; ++x) {
            float2 p = uv + float2(x,y)*2/size;
            float2 g = .5 * float2(sampleA(p+float2(1,0)/size)-sampleA(p-float2(1,0)/size),
                                   sampleA(p+float2(0,1)/size)-sampleA(p-float2(0,1)/size));
            xx += g.x*g.x; xy += g.x*g.y; yy += g.y*g.y;
            gradientSum += g;
            float difference = sampleA(p)-sampleB(p);
            residual += difference*difference;
        }
    // The weaker eigenvalue measures texture that resolves the second motion
    // direction. A plain diagonal stripe cannot resolve motion along itself.
    float discriminant = sqrt((xx-yy)*(xx-yy)+4*xy*xy);
    float weak = max(0, .5*(xx+yy-discriminant) / 25);
    float strong = .5*(xx+yy+discriminant) / 25;
    // Smooth shading also has one dominant gradient direction. Unlike a
    // repeating stripe, its gradients keep a consistent sign. Penalizing that
    // ramp would splice original and relit skin into blotchy shadows.
    float coherence = saturate(dot(gradientSum,gradientSum)/max(25*(xx+yy),.00000001));
    float alternating = 1-smoothstep(.02,.08,coherence);
    observable = lerp(1, smoothstep(.05,.2,weak/max(strong,.00000001)),
                      alternating*smoothstep(.000001,.000008,strong));
    float noise = 4.0 / (255.0 * 255.0);
    // Keep this tighter than coarse matching: a coherent subpixel pan can
    // produce less than one quantization step of full-resolution difference.
    nearlyStill = residual/25 <= .05/(255.0*255.0);
    if (nearlyStill) return noise;
    // Do not discard observable movement just because its brightness shift
    // disappears from the illumination-invariant matching cost.
    return length(displacement) > 2 && residual/25 <= 2*noise ? noise * saturate(1-weak/noise) : 0;
}

[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= outWidth || id.y >= outHeight) return;
    float2 uv = (float2(id.xy)+.5)/float2(outWidth,outHeight);
    if (mode == 0) {
        // Pyramid images share normalized coordinates, including odd sizes.
        uint w,h; imageA.GetDimensions(w,h);
        float2 d = .5/float2(w,h);
        result[id.xy] = level == 0 ? luma(imageA.Load(int3(id.xy,0)).rgb) :
            .25*(imageA.SampleLevel(linearClamp, uv + float2(-d.x,-d.y),0) +
                 imageA.SampleLevel(linearClamp, uv + float2(d.x,-d.y),0) +
                 imageA.SampleLevel(linearClamp, uv + float2(-d.x,d.y),0) +
                 imageA.SampleLevel(linearClamp, uv + d,0));
        return;
    }
    if (mode == 1) {
        if (resetHistory) { result[id.xy] = 0; return; }
        float2 size = imageSize();
        float2 best = 0;
        float bestCost = cost(uv,best);
        // Resolve tiny real movement at the finest scale before the output
        // consensus check separates it from isolated capture noise.
        float stationaryThreshold = (level == 0 ? .05 : .5)/(255.0*255.0);
        if (bestCost <= stationaryThreshold) {
            float residual = 0;
            [unroll] for (int y = -1; y <= 1; ++y)
                [unroll] for (int x = -1; x <= 1; ++x) {
                    float2 p = uv + float2(x,y)*4/size;
                    float d = sampleA(p)-sampleB(p);
                    residual += d*d;
                }
            if (bestCost < .0000001 || residual/9 <= stationaryThreshold) {
                // Certainty that a patch is stationary does not require
                // resolving motion in both gradient directions.
                result[id.xy] = float4(0,0,bestCost,1);
                return;
            }
        }
        // Exact/quantization-level stationary matches need no wide search.
        float observable = 1;
        bool nearlyStill = false;
        float noiseFloor = level == 0 ? stationaryNoiseAllowance(uv,float2(3,0),observable,nearlyStill) : 0;
        if (bestCost < .0000001 || (level == 0 && bestCost <= stationaryThreshold && noiseFloor > 0)) {
            result[id.xy] = float4(0,0,bestCost,observable);
            return;
        }
        float2 seed = level == 4 ? 0 : coarse.SampleLevel(linearClamp,uv,0).xy * size;
        float prior = 0;
        if (level == 0) {
            uint cw,ch; coarse.GetDimensions(cw,ch);
            int2 base = int2(floor(uv*float2(cw,ch)-.5));
            float spread = 0;
            [unroll] for(int cy=0;cy<2;++cy)
                [unroll] for(int cx=0;cx<2;++cx) {
                    int2 cell=clamp(base+int2(cx,cy),0,int2(cw,ch)-1);
                    spread=max(spread,length(coarse.Load(int3(cell,0)).xy*size-seed));
                }
            // Coherent movement, including low-contrast moving surfaces, does
            // not need a stationary prior. Apply it to conflicting proposals.
            prior = .000001 * smoothstep(1,4,spread);
        }
        if (level != 4) {
            candidate(uv,seed,best,bestCost,prior);
            uint cw,ch; coarse.GetDimensions(cw,ch);
            int2 base = int2(floor(uv*float2(cw,ch)-.5));
            [unroll] for(int cy=0;cy<2;++cy)
                [unroll] for(int cx=0;cx<2;++cx) {
                    int2 cell=clamp(base+int2(cx,cy),0,int2(cw,ch)-1);
                    candidate(uv,coarse.Load(int3(cell,0)).xy*size,best,bestCost,prior);
                }
            seed=best;
        }
        // Wide search at low resolution; local search and refinement below it.
        int radius = level == 4 ? 4 : 1;
        [loop] for (int y = -radius; y <= radius; ++y)
            [loop] for (int x = -radius; x <= radius; ++x)
                candidate(uv,seed + float2(x,y),best,bestCost,prior);
        [loop] for (int iteration = 0; iteration < 2; ++iteration)
            candidate(uv,refine(uv,best),best,bestCost,prior);
        if (level == 0) {
            float2 local = refine(uv,0);
            candidate(uv,local,best,bestCost,prior);
            candidate(uv,refine(uv,local),best,bestCost,prior);
        }
        // A repeating, low-contrast pattern can match at many displacements.
        // Reject nearly unchanged patches and large ambiguous pattern jumps.
        // Keep subpixel estimates supported by measurable frame differences.
        float stillCost=cost(uv,0);
        if (!nearlyStill && length(best) <= 2) noiseFloor = 0;
        if(stillCost <= bestCost*1.35 + noiseFloor + .0000001) { best=0; bestCost=stillCost; }
        // A stripe can give a perfect match while leaving one motion direction
        // unknown. Keep that ambiguity even on frames that happen to choose
        // zero, so the diagnostic confidence remains stable.
        result[id.xy] = float4(best/size,cost(uv,best),observable);
        return;
    }
    if (mode == 2) {
        float4 f = forwardFlow.Load(int3(id.xy,0));
        // Isolated subpixel proposals in a stationary patch make the model
        // re-decide fine detail. Use neighborhood consensus, but retain the
        // local vector where that consensus crosses a different moving surface.
        float2 neighbors[9];
        [unroll] for (int y=-1;y<=1;++y)
            [unroll] for (int x=-1;x<=1;++x)
                neighbors[(y+1)*3+x+1] = forwardFlow.Load(int3(clamp(int2(id.xy)+int2(x,y),0,int2(outWidth,outHeight)-1),0)).xy;
        [unroll] for (int a=0;a<8;++a)
            [unroll] for (int b=a+1;b<9;++b) {
                float2 low = min(neighbors[a],neighbors[b]);
                neighbors[b] = max(neighbors[a],neighbors[b]);
                neighbors[a] = low;
            }
        float2 consensus = neighbors[4];
        float consensusCost = cost(uv,consensus*float2(fullWidth,fullHeight));
        if (consensusCost <= f.z*1.25 + 1.0/(255.0*255.0)) {
            f.xy = consensus;
            f.z = consensusCost;
        }
        float2 pixels = f.xy*float2(fullWidth,fullHeight);
        float magnitude = length(pixels);
        if (magnitude > .005) {
            // Isolated jumps can churn neural detail at any magnitude. Do not
            // exempt larger estimates: a narrow strip of agreeing neighbors
            // still preserves coherent movement along an edge.
            uint support = 0;
            // Relax support gradually: a hard switch at .35 pixels lets tiny
            // estimate changes abruptly admit a much less coherent field.
            float transition = smoothstep(.35,1,magnitude);
            float tolerance = min(lerp(.08,.35,transition),magnitude*.5);
            [loop] for (int y=-2;y<=2;++y)
                [loop] for (int x=-2;x<=2;++x) {
                    float2 v = forwardFlow.Load(int3(clamp(int2(id.xy)+int2(x,y),0,int2(outWidth,outHeight)-1),0)).xy*float2(fullWidth,fullHeight);
                    support += length(v-pixels) < tolerance;
                }
            if (support < lerp(16,5,transition)) {
                // A small moving feature may have only a narrow strip of
                // agreeing neighbors. Recover that supported motion gradually
                // above the tiny-motion band instead of snapping it to zero.
                // Unsupported estimates and sub-quarter-pixel noise stay out.
                float recovery = support >= 5 ? smoothstep(.25,.45,magnitude) : 0;
                f.xy *= recovery;
                f.z = cost(uv,f.xy*float2(fullWidth,fullHeight));
            }
        }
        float2 q = uv + f.xy;
        float2 inverse = backwardFlow.SampleLevel(linearClamp,q,0).xy;
        float cycle = length((f.xy + inverse)*float2(fullWidth,fullHeight));
        float delta = abs(sampleA(uv)-sampleB(q));
        float matchTrust = exp(-f.z*100) * exp(-max(cycle-2,0)*.5) * exp(-max(delta-.06,0)*10);
        float trust = f.w * matchTrust;
        if (any(q < 0) || any(q > 1) || resetHistory) trust = 0;
        // Coarse reverse flow is conservative at boundaries. A single edge's
        // directional ambiguity alone is not evidence that its motion is wrong.
        bool invalid = any(q < 0) || any(q > 1) || resetHistory;
        motionGuide[id.xy] = invalid || (direction && matchTrust < .05) ? 0 : f.xy;
        // Reject quickly, recover gradually, and reproject confidence rather
        // than averaging vectors belonging to different surfaces.
        float prior = resetHistory ? 0 : history.SampleLevel(linearClamp,q,0).x;
        trust = min(trust,prior+.12);
        result[id.xy] = float4(trust,cycle,delta,1);
        return;
    }
}
