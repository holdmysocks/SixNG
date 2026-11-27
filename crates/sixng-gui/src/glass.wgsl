// Ped glass's dual-source blend (appended to shader.wgsl in a module of its
// own, built only when the device has dual-source blending; the
// `enable dual_source_blending;` line goes before everything): the game
// blends its lens programs as dst' = src0 + dst x src1, src0 the light the
// glass sends to the eye and src1 how much of what lies behind it gets
// through (glass_t, set by mesh_colour).

struct GlassOut {
    @location(0) @blend_src(0) col: vec4<f32>,
    @location(0) @blend_src(1) trans: vec4<f32>,
};

@fragment
fn fs_mesh_glass(i: VOut, @builtin(front_facing) ff: bool) -> GlassOut {
    var o: GlassOut;
    let c = mesh_colour(i, ff);
    if (F.mode.x == 0u || F.mode.x == 7u) {
        o.col = vec4(c.rgb, 0.0);
        o.trans = vec4(glass_t, max(glass_t.r, max(glass_t.g, glass_t.b)));
    } else {
        // the debug views: plain alpha blending
        o.col = vec4(c.rgb * c.a, c.a);
        o.trans = vec4(1.0 - c.a);
    }
    return o;
}
