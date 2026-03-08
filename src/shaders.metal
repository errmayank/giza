#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
};

vertex VertexOut vertex_main(uint vertex_id [[vertex_id]]) {
    constexpr float2 positions[3] = {
        float2(0.0, 0.65),
        float2(-0.65, -0.55),
        float2(0.65, -0.55),
    };

    VertexOut out;
    out.position = float4(positions[vertex_id], 0.0, 1.0);
    return out;
}

fragment float4 fragment_main() {
    return float4(207.0 / 255.0, 159.0 / 255.0, 121.0 / 255.0, 1.0);
}

