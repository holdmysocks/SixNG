//! The game's screen-space hair blur (effect `hair_resolve`, techniques
//! `SingleDrawBlurLow*`), run on the lit colour before tone mapping.
//!
//! What the game does, as read from its programs and the code that drives
//! them:
//! - A pixel is blurred when any pixel of its 3 x 3 neighbourhood is a
//!   resolved strand pixel (the game's strand stencil class). So the strands
//!   and a one-pixel rim around them are blurred. The neighbours are taken
//!   whatever they are: there is no depth or edge stop.
//! - The blur is a 3 x 3 Gaussian with weights 1, exp(-1 / (2 s^2)) and
//!   exp(-1 / s^2) for the centre, edge and corner taps. The result
//!   replaces the pixel.
//! - s = s_far + (s_near - s_far) x sat(0.0002 x H / z). H is the target
//!   height in pixels. z is the view depth (m) of the nearest strand pixel
//!   in the 3 x 3, taken in the order centre, edge, corner.
//! - s_near = clamp(x, 0.1, 2) and s_far = clamp(k x, 0.1, s_near), with
//!   k = 0.3 + 0.2 x sat(H / 720 - 1). x is the strength, 1.0 on the game's
//!   main path. The time-cycle values `hair_blur_strength` and
//!   `hair_blur_depth_fade` are not read by the game's executable.
//! - The game runs it after its temporal upscaler. With an upscaled output
//!   it reads 5 x 5 or 7 x 7 output pixels with s kept in render pixels. The
//!   viewer has no upscaler, so this is the 1 x variant.
//!
//! The strand mask: the strands drawn again over the main pass's depth with
//! an equal test (fragment `fs_hair_mask` in shader.wgsl), so a sample counts
//! where a strand is the surface the main pass kept. A pixel is a strand
//! pixel when at least half its samples are. `SIXNG_HAIR_BLUR=0` turns the
//! blur off; `SIXNG_HAIR_BLUR=mask` shows the mask in red.

const WGSL: &str = r#"
struct U {
    // view depth from the reversed depth d: z = r far / (d + r)
    r: f32,
    far: f32,
    s_far: f32,
    s_near: f32,
    h: f32,
    // 1: show the strand mask instead (SIXNG_HAIR_BLUR=mask)
    show_mask: f32,
    _p1: f32,
    _p2: f32,
};
@group(0) @binding(0) var src: texture_2d<f32>;
// r: strand share of the pixel's samples, g: their depth times that share
@group(0) @binding(1) var mask: texture_2d<f32>;
@group(0) @binding(2) var<uniform> u: U;

@vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let p = vec2(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn fs(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let c = vec2<i32>(p.xy);
    let hi = vec2<i32>(textureDimensions(src)) - 1;
    let centre = textureLoad(src, c, 0);
    if (u.show_mask > 0.5) {
        return select(centre, vec4(1.0, 0.0, 0.0, 1.0), textureLoad(mask, c, 0).r >= 0.5);
    }
    // the nearest strand pixel: centre 0, edges 1, corners 2 (first in row order on a tie)
    var best = 3;
    var m = vec4(0.0);
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let d = abs(x) + abs(y);
            if (d < best) {
                let q = textureLoad(mask, clamp(c + vec2(x, y), vec2(0), hi), 0);
                if (q.r >= 0.5) {
                    best = d;
                    m = q;
                }
            }
        }
    }
    if (best > 2) {
        return centre;
    }
    let z = u.r * u.far / max(m.g / m.r + u.r, 1e-9);
    let s = mix(u.s_far, u.s_near, saturate(0.0002 * u.h / max(z, 1e-6)));
    let k = 1.0 / max(s * s, 1e-6);
    let we = exp(-0.5 * k);
    let wc = exp(-k);
    var sum = centre.rgb;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            if (x == 0 && y == 0) {
                continue;
            }
            let w = select(we, wc, x != 0 && y != 0);
            sum += textureLoad(src, clamp(c + vec2(x, y), vec2(0), hi), 0).rgb * w;
        }
    }
    return vec4(sum / (1.0 + 4.0 * we + 4.0 * wc), centre.a);
}
"#;

/// The mask's format (resolved from the multisampled strand draw).
pub const MASK_FMT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// The blur's constants for a target `h` pixels high, at the game's main
/// path strength (1.0): (s_far, s_near).
pub fn sigmas(h: f32) -> (f32, f32) {
    let x = 1.0f32;
    let near = x.clamp(0.1, 2.0);
    let k = 0.3 + 0.2 * (h / 720.0 - 1.0).clamp(0.0, 1.0);
    ((k * x).clamp(0.1, near), near)
}

/// The per-size textures: the frame copy the blur reads, the multisampled
/// mask and its resolve.
struct Sized {
    size: [u32; 2],
    fmt: Option<wgpu::TextureFormat>,
    src: Option<(wgpu::Texture, wgpu::TextureView)>,
    mask_ms: wgpu::TextureView,
    mask: wgpu::TextureView,
}

pub struct HairBlur {
    bgl: wgpu::BindGroupLayout,
    layout: wgpu::PipelineLayout,
    module: wgpu::ShaderModule,
    pipes: Vec<(wgpu::TextureFormat, wgpu::RenderPipeline)>,
    ub: wgpu::Buffer,
    sized: Option<Sized>,
    pub enabled: bool,
    show_mask: bool,
}

impl HairBlur {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("hair blur"),
            source: wgpu::ShaderSource::Wgsl(WGSL.into()),
        });
        let tex = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("hair blur"),
            entries: &[
                tex(0),
                tex(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None },
                    count: None,
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("hair blur"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let ub = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("hair blur"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let env = std::env::var("SIXNG_HAIR_BLUR").unwrap_or_default();
        HairBlur { bgl, layout, module, pipes: Vec::new(), ub, sized: None, enabled: env != "0", show_mask: env == "mask" }
    }

    /// The mask textures for a view of `size`.
    pub fn ensure(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.sized.as_ref().is_some_and(|s| s.size == size) {
            return;
        }
        let mk = |samples: u32| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some("hair blur mask"),
                    size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format: MASK_FMT,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
                .create_view(&Default::default())
        };
        let mask_ms = mk(crate::render3d::MSAA);
        let mask = mk(1);
        self.sized = Some(Sized { size, fmt: None, src: None, mask_ms, mask });
    }

    /// The multisampled mask to draw into and its resolve (after `ensure`).
    pub fn mask_views(&self) -> (&wgpu::TextureView, &wgpu::TextureView) {
        let s = self.sized.as_ref().expect("hair blur: ensure first");
        (&s.mask_ms, &s.mask)
    }

    fn pipe(&mut self, device: &wgpu::Device, fmt: wgpu::TextureFormat) -> usize {
        if let Some(i) = self.pipes.iter().position(|p| p.0 == fmt) {
            return i;
        }
        let p = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("hair blur"),
            layout: Some(&self.layout),
            vertex: wgpu::VertexState { module: &self.module, entry_point: Some("vs"), compilation_options: Default::default(), buffers: &[] },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState { format: fmt, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview_mask: None,
            cache: None,
        });
        self.pipes.push((fmt, p));
        self.pipes.len() - 1
    }

    /// Blur the strands of `frame` (`frame_view` is its view) in place, after
    /// the mask is drawn. `near` / `far` are the camera's planes (the depth
    /// is reversed: near -> 1, far -> 0).
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        enc: &mut wgpu::CommandEncoder,
        frame: &wgpu::Texture,
        frame_view: &wgpu::TextureView,
        size: [u32; 2],
        near: f32,
        far: f32,
    ) {
        let fmt = frame.format();
        let pi = self.pipe(device, fmt);
        let Some(sz) = self.sized.as_mut() else { return };
        if sz.fmt != Some(fmt) || sz.src.is_none() {
            let t = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("hair blur source"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: fmt,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            let v = t.create_view(&Default::default());
            sz.src = Some((t, v));
            sz.fmt = Some(fmt);
        }
        let (s_far, s_near) = sigmas(size[1] as f32);
        let r = near / (far - near).max(1e-6);
        let u = [r, far, s_far, s_near, size[1] as f32, if self.show_mask { 1.0 } else { 0.0 }, 0.0, 0.0];
        queue.write_buffer(&self.ub, 0, bytemuck::cast_slice(&u));
        let src = sz.src.as_ref().unwrap();
        enc.copy_texture_to_texture(
            frame.as_image_copy(),
            src.0.as_image_copy(),
            wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
        );
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("hair blur"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&src.1) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&sz.mask) },
                wgpu::BindGroupEntry { binding: 2, resource: self.ub.as_entire_binding() },
            ],
        });
        let mut rp = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("hair blur"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: frame_view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Load, store: wgpu::StoreOp::Store },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        rp.set_pipeline(&self.pipes[pi].1);
        rp.set_bind_group(0, &bg, &[]);
        rp.draw(0..3, 0..1);
    }
}
