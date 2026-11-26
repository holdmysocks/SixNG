//! Still-camera frame accumulation: while nothing in the view changes, each
//! new frame (drawn with another strand dither pattern) is blended into a
//! running mean, so the dithered hair settles into a smooth image the way the
//! game's frames average over time.

/// Frames in the mean; after that each new frame weighs 1 / CAP, so a late
/// change (a texture that finishes streaming) still shows.
pub const CAP: u32 = 32;

const WGSL: &str = r#"
@group(0) @binding(0) var src: texture_2d<f32>;
@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4(p * 2.0 - 1.0, 0.0, 1.0);
}
@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    return textureLoad(src, vec2<i32>(p.xy), 0);
}
"#;

/// The running mean (linear, 16-bit float) and the sRGB image shown from it.
pub struct Targets {
    pub out: wgpu::Texture,
    pub out_view: wgpu::TextureView,
    sum_view: wgpu::TextureView,
    bg_frame: wgpu::BindGroup,
    bg_sum: wgpu::BindGroup,
}

pub struct Accum {
    bgl: wgpu::BindGroupLayout,
    blend: wgpu::RenderPipeline,
    copy: wgpu::RenderPipeline,
    pub out_fmt: wgpu::TextureFormat,
}

const SUM_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

impl Accum {
    pub fn new(device: &wgpu::Device, out_fmt: wgpu::TextureFormat) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("accumulate"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("accumulate"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("accumulate"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipe = |fmt: wgpu::TextureFormat, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("accumulate"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: Default::default(),
                depth_stencil: None,
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState { format: fmt, blend, write_mask: wgpu::ColorWrites::ALL })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        // sum = frame * k + sum * (1 - k), k from the blend constant
        let c = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Constant,
            dst_factor: wgpu::BlendFactor::OneMinusConstant,
            operation: wgpu::BlendOperation::Add,
        };
        let blend = pipe(SUM_FMT, Some(wgpu::BlendState { color: c, alpha: c }));
        let copy = pipe(out_fmt, None);
        Accum { bgl, blend, copy, out_fmt }
    }

    /// The textures for one view size; `frame` is the finished frame (sRGB).
    pub fn targets(&self, device: &wgpu::Device, frame: &wgpu::TextureView, size: [u32; 2]) -> Targets {
        let mk = |fmt: wgpu::TextureFormat, usage: wgpu::TextureUsages| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("accumulate"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage,
                view_formats: &[],
            })
        };
        let rt = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING;
        let sum_view = mk(SUM_FMT, rt).create_view(&Default::default());
        let out = mk(self.out_fmt, rt | wgpu::TextureUsages::COPY_SRC);
        let out_view = out.create_view(&Default::default());
        let bg = |v: &wgpu::TextureView| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("accumulate"),
                layout: &self.bgl,
                entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(v) }],
            })
        };
        let bg_frame = bg(frame);
        let bg_sum = bg(&sum_view);
        Targets { out, out_view, sum_view, bg_frame, bg_sum }
    }

    /// Blend the frame into the mean as frame number `n` (0: the mean starts
    /// over), then write the mean to the shown image.
    pub fn run(&self, enc: &mut wgpu::CommandEncoder, t: &Targets, n: u32) {
        let k = 1.0 / (n.min(CAP - 1) + 1) as f64;
        let pass = |enc: &mut wgpu::CommandEncoder, pipe: &wgpu::RenderPipeline, bg: &wgpu::BindGroup, view: &wgpu::TextureView, k: Option<f64>| {
            let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("accumulate"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    depth_slice: None,
                    ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            rp.set_pipeline(pipe);
            if let Some(k) = k {
                rp.set_blend_constant(wgpu::Color { r: k, g: k, b: k, a: k });
            }
            rp.set_bind_group(0, bg, &[]);
            rp.draw(0..3, 0..1);
        };
        pass(enc, &self.blend, &t.bg_frame, &t.sum_view, Some(k));
        pass(enc, &self.copy, &t.bg_sum, &t.out_view, None);
    }
}
