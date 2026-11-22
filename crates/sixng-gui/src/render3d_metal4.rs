#![allow(dead_code)]

use crate::glb;
use crate::render3d::{Camera, FrameU, Instance, LineV, Look};
use anyhow::{bail, Context};
use eframe::egui;
use glam::{Mat4, Vec3};
use metal_rust::metal::{
    BlendFactor, BlendOperation, ClearColor, CompareFunction, DepthStencilDescriptor, Device,
    IndexType, Library, LoadAction, PixelFormat, PrimitiveTopologyClass, PrimitiveType,
    RenderPassDepthAttachmentDescriptor, RenderStages, ResidencyAllocation, ResidencySet,
    ResidencySetDescriptor, ResourceOptions, SamplerAddressMode, SamplerDescriptor,
    SamplerMinMagFilter, SamplerState, StoreAction, Texture, TextureDescriptor, TextureUsage,
};
use metal_rust::metal4::{
    ArgumentTable, ArgumentTableDescriptor, BlendState, BufferImageLayout, CommandQueue,
    CompilerDescriptor, LibraryDescriptor, LibraryFunctionDescriptor, RecordingRenderEncoder,
    RenderPassDescriptor, RenderPipelineBuildDescriptor, RenderPipelineColorAttachmentDescriptor,
    RenderPipelineDescriptor, TextureOriginSelection, TextureRegionSelection,
};
use std::collections::HashMap;

pub const MAP_TEXTURE_ID: u64 = 0x5349_584e_4701;

const MESH_SHADER: &str = r#"
    #include <metal_stdlib>
    using namespace metal;

    struct Frame {
        float4x4 view_proj;
        float4x4 inv_view_proj;
        float4 cam_pos;
        float4 sun_dir;
        float4 sun_col;
        float4 sky_top;
        float4 sky_hor;
        uint4 mode;
        float4 paint1;
        float4 paint2;
        float4 misc;
        float4 misc2;
        float4 misc3;
    };

    struct Vertex {
        float4 pos;
        float4 nrm;
        float4 tan;
        float2 uv0;
        float2 uv1;
        float4 color;
        uint4 joints;
        float4 weights;
        float4 tint;
    };

    struct Instance {
        float4x4 transform;
        float4 extra;
    };

    struct Material {
        float4 base;
        uint4 flags;
    };

    struct LineVertex {
        packed_float3 pos;
        uchar4 color;
    };

    struct LineVarying {
        float4 position [[position]];
        float4 color;
    };

    struct Varying {
        float4 position [[position]];
        float3 world;
        float3 normal;
        float4 tangent;
        float2 uv;
        float4 color;
        float4 extra;
        float4 tint;
    };

    vertex Varying mesh_vertex(
        uint vertex_id [[vertex_id]],
        uint instance_id [[instance_id]],
        constant Frame& frame [[buffer(0)]],
        device const Vertex* vertices [[buffer(1)]],
        device const Instance* instances [[buffer(2)]],
        device const float4x4* joints [[buffer(4)]]
    ) {
        const Vertex vtx = vertices[vertex_id];
        const Instance instance = instances[instance_id];
        float4x4 skin = float4x4(1.0);
        if (instance.extra.x > 0.5) {
            float4x4 weighted = float4x4(0.0);
            float weight_sum = 0.0;
            for (uint i = 0; i < 4; ++i) {
                const float weight = vtx.weights[i];
                if (weight > 0.0 && vtx.joints[i] < uint(instance.extra.w)) {
                    weighted += joints[vtx.joints[i]] * weight;
                    weight_sum += weight;
                }
            }
            if (weight_sum > 0.001) {
                skin = weighted / weight_sum;
            }
        }
        const float4x4 model = instance.transform * skin;
        const float4 world = model * float4(vtx.pos.xyz, 1.0);
        Varying output;
        output.position = frame.view_proj * world;
        output.world = world.xyz;
        output.normal = (model * float4(vtx.nrm.xyz, 0.0)).xyz;
        output.tangent = float4((model * float4(vtx.tan.xyz, 0.0)).xyz, vtx.tan.w);
        output.uv = vtx.uv0;
        output.color = vtx.color;
        output.extra = instance.extra;
        output.tint = vtx.tint;
        return output;
    }

    fragment float4 mesh_fragment(
        Varying input [[stage_in]],
        constant Frame& frame [[buffer(0)]],
        constant Material& material [[buffer(3)]],
        texture2d<float> base_texture [[texture(0)]],
        texture2d<float> normal_texture [[texture(1)]],
        texture2d<float> packed_texture [[texture(2)]],
        texture2d<float> emissive_texture [[texture(3)]],
        sampler material_sampler [[sampler(0)]]
    ) {
        float4 base = material.base;
        if ((material.flags.x & 1u) != 0u) {
            base *= base_texture.sample(material_sampler, input.uv);
        }
        if (material.flags.z == 1u && base.a < 0.5) {
            discard_fragment();
        }

        float3 normal = normalize(input.normal);
        if ((material.flags.x & 2u) != 0u) {
            float3 tangent = normalize(input.tangent.xyz);
            float3 bitangent = normalize(cross(normal, tangent));
            float3 sampled = normal_texture.sample(material_sampler, input.uv).xyz * 2.0 - 1.0;
            sampled.y = -sampled.y;
            bitangent *= select(1.0, -1.0, input.tangent.w < 0.0);
            normal = normalize(tangent * sampled.x + bitangent * sampled.y + normal * sampled.z);
        }

        float3 albedo = base.rgb;
        if (frame.misc3.x > 0.5) {
            albedo *= input.tint.rgb;
        }
        const uint cls = material.flags.y;
        float alpha = base.a;
        if (cls == 10u) {
            albedo = frame.paint1.rgb;
            if ((material.flags.x & 16u) != 0u) {
                albedo = frame.misc2.yzw;
            }
        } else if (cls == 5u || cls == 6u) {
            alpha = min(alpha, 0.45);
        } else if (cls == 8u) {
            albedo = float3(0.04, 0.12, 0.16);
            alpha = 0.7;
        }

        float roughness = 0.75;
        float metalness = 0.0;
        if ((material.flags.x & 4u) != 0u) {
            const float3 packed = packed_texture.sample(material_sampler, input.uv).rgb;
            if (frame.mode.y == 0u) {
                roughness = packed.r;
                metalness = packed.g;
            } else if (frame.mode.y == 1u) {
                roughness = packed.g;
                metalness = packed.b;
            } else if (frame.mode.y == 2u) {
                roughness = packed.g;
                metalness = packed.r;
            }
            roughness = clamp(roughness, 0.04, 1.0);
        }
        const float3 light_dir = normalize(frame.sun_dir.xyz);
        const float ndl = max(dot(normal, light_dir), 0.0);
        const float ambient = max(frame.misc.w, 0.0);
        float3 color = albedo * (frame.sky_hor.rgb * ambient
            + frame.sun_col.rgb * frame.sun_dir.w * ndl * 2.0);
        if (cls == 7u) {
            float3 emissive = albedo;
            if ((material.flags.x & 8u) != 0u) {
                emissive = emissive_texture.sample(material_sampler, input.uv).rgb;
            }
            color += emissive * frame.misc.y;
        }
        if (frame.mode.x == 1u) {
            color = albedo;
        } else if (frame.mode.x == 2u) {
            color = normal * 0.5 + 0.5;
        } else if (frame.mode.x == 6u) {
            color = input.color.rgb;
        }
        if (input.extra.y > 0.5) {
            color = mix(color, float3(1.0, 0.55, 0.1), 0.45);
        }
        const float distance_to_camera = length(frame.cam_pos.xyz - input.world);
        const float fog = 1.0 - exp(-distance_to_camera * frame.misc2.x);
        color = mix(color, frame.sky_hor.rgb, fog);
        if (material.flags.z != 2u && cls != 5u && cls != 6u && cls != 8u) {
            alpha = 1.0;
        }
        return float4(color, alpha);
    }

    vertex LineVarying line_vertex(
        uint vertex_id [[vertex_id]],
        constant Frame& frame [[buffer(0)]],
        device const LineVertex* lines [[buffer(1)]]
    ) {
        const LineVertex line = lines[vertex_id];
        LineVarying output;
        output.position = frame.view_proj * float4(float3(line.pos), 1.0);
        output.color = float4(line.color) / 255.0;
        return output;
    }

    fragment float4 line_fragment(LineVarying input [[stage_in]]) {
        return input.color;
    }
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex4 {
    pos: [f32; 4],
    nrm: [f32; 4],
    tan: [f32; 4],
    uv0: [f32; 2],
    uv1: [f32; 2],
    color: [f32; 4],
    joints: [u32; 4],
    weights: [f32; 4],
    tint: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Material4 {
    base: [f32; 4],
    flags: [u32; 4],
}

pub struct GpuMesh4 {
    pub index_range: std::ops::Range<usize>,
    pub vertex_offset: usize,
    pub n_idx: u32,
    pub material: usize,
    pub skinned: bool,
    pub centre: Vec3,
    pub visible: bool,
}

pub struct GpuModel4 {
    pub meshes: Vec<GpuMesh4>,
    pub mat_alpha: Vec<u32>,
    pub mat_class: Vec<u32>,
    pub mat_decal: Vec<Option<u8>>,
    pub bbox: [Vec3; 2],
    pub n_instances: u32,
    instance_cap: u32,
    joint_count: usize,
    vertex_buffer: metal_rust::metal::Buffer,
    index_buffer: metal_rust::metal::Buffer,
    instance_buffer: metal_rust::metal::Buffer,
    material_buffer: metal_rust::metal::Buffer,
    joints_buffer: metal_rust::metal::Buffer,
    textures: Vec<Texture>,
    image_texture_indices: Vec<Option<usize>>,
    material_texture_indices: Vec<[Option<usize>; 4]>,
}

impl GpuModel4 {
    pub fn buffer_bytes(&self) -> u64 {
        self.vertex_buffer.length() as u64
            + self.index_buffer.length() as u64
            + self.instance_buffer.length() as u64
            + self.material_buffer.length() as u64
            + self.joints_buffer.length() as u64
    }
}

const MATERIAL_STRIDE: usize = 256;

struct PendingTextureUpload {
    texture: Texture,
    staging: metal_rust::metal::Buffer,
    mips: Vec<(usize, usize, usize, usize)>,
}

fn function_descriptor(
    library: &Library,
    name: &str,
) -> anyhow::Result<metal_rust::metal4::FunctionDescriptor> {
    let descriptor = LibraryFunctionDescriptor::new()?;
    descriptor.set_library(Some(library))?;
    descriptor.set_name(name)?;
    Ok(descriptor.as_function_descriptor())
}

fn create_mesh_pipeline(
    compiler: &metal_rust::metal4::Compiler,
    library: &Library,
) -> anyhow::Result<metal_rust::metal::RenderPipelineState> {
    let vertex = function_descriptor(library, "mesh_vertex")?;
    let fragment = function_descriptor(library, "mesh_fragment")?;
    let descriptor = RenderPipelineDescriptor::new()?;
    descriptor.set_vertex_function_descriptor(Some(&vertex))?;
    descriptor.set_fragment_function_descriptor(Some(&fragment))?;

    let color = RenderPipelineColorAttachmentDescriptor::new()?;
    color.set_pixel_format(PixelFormat::RGBA8_UNORM_SRGB)?;
    color.set_blending_state(BlendState::BlendStateEnabled)?;
    color.set_source_rgb_blend_factor(BlendFactor::BlendFactorOne)?;
    color.set_destination_rgb_blend_factor(BlendFactor::BlendFactorOneMinusSourceAlpha)?;
    color.set_rgb_blend_operation(BlendOperation::BlendOperationAdd)?;
    color.set_source_alpha_blend_factor(BlendFactor::BlendFactorOne)?;
    color.set_destination_alpha_blend_factor(BlendFactor::BlendFactorOneMinusSourceAlpha)?;
    color.set_alpha_blend_operation(BlendOperation::BlendOperationAdd)?;
    descriptor
        .color_attachments()?
        .context("MTL4 mesh pipeline has no color attachment array")?
        .set_attachment(0, &color)?;
    Ok(compiler.new_render_pipeline(
        RenderPipelineBuildDescriptor::Render(&descriptor),
        None,
        None,
    )?)
}

fn create_line_pipeline(
    compiler: &metal_rust::metal4::Compiler,
    library: &Library,
) -> anyhow::Result<metal_rust::metal::RenderPipelineState> {
    let vertex = function_descriptor(library, "line_vertex")?;
    let fragment = function_descriptor(library, "line_fragment")?;
    let descriptor = RenderPipelineDescriptor::new()?;
    descriptor.set_vertex_function_descriptor(Some(&vertex))?;
    descriptor.set_fragment_function_descriptor(Some(&fragment))?;
    descriptor.set_input_primitive_topology(PrimitiveTopologyClass::PrimitiveTopologyClassLine)?;

    let color = RenderPipelineColorAttachmentDescriptor::new()?;
    color.set_pixel_format(PixelFormat::RGBA8_UNORM_SRGB)?;
    color.set_blending_state(BlendState::BlendStateEnabled)?;
    color.set_source_rgb_blend_factor(BlendFactor::BlendFactorOne)?;
    color.set_destination_rgb_blend_factor(BlendFactor::BlendFactorOneMinusSourceAlpha)?;
    color.set_rgb_blend_operation(BlendOperation::BlendOperationAdd)?;
    color.set_source_alpha_blend_factor(BlendFactor::BlendFactorOne)?;
    color.set_destination_alpha_blend_factor(BlendFactor::BlendFactorOneMinusSourceAlpha)?;
    color.set_alpha_blend_operation(BlendOperation::BlendOperationAdd)?;
    descriptor
        .color_attachments()?
        .context("MTL4 line pipeline has no color attachment array")?
        .set_attachment(0, &color)?;
    Ok(compiler.new_render_pipeline(
        RenderPipelineBuildDescriptor::Render(&descriptor),
        None,
        None,
    )?)
}

fn vertex4(vertex: &glb::Vertex) -> Vertex4 {
    let normalized = |v: [u8; 4]| v.map(|c| c as f32 / 255.0);
    Vertex4 {
        pos: [vertex.pos[0], vertex.pos[1], vertex.pos[2], 1.0],
        nrm: [vertex.nrm[0], vertex.nrm[1], vertex.nrm[2], 0.0],
        tan: vertex.tan,
        uv0: vertex.uv0,
        uv1: vertex.uv1,
        color: normalized(vertex.col),
        joints: vertex.joints.map(u32::from),
        weights: normalized(vertex.weights),
        tint: normalized(vertex.tint),
    }
}

fn material_image_indices(material: &glb::Material) -> [Option<usize>; 4] {
    [
        material.base,
        material.normal,
        material.packed,
        material.emissive,
    ]
}

fn frame_uniform(camera: &Camera, look: &Look, aspect: f32) -> FrameU {
    let view_projection = camera.proj(aspect) * camera.view();
    let rgba = |color: [f32; 3]| [color[0], color[1], color[2], 1.0];
    FrameU {
        view_proj: view_projection.to_cols_array_2d(),
        inv_view_proj: view_projection.inverse().to_cols_array_2d(),
        cam_pos: camera.eye().extend(1.0).to_array(),
        sun_dir: look.sun_dir.extend(look.sun_intensity).to_array(),
        sun_col: rgba(look.sun_col),
        sky_top: rgba(look.sky_top),
        sky_hor: [
            look.sky_hor[0],
            look.sky_hor[1],
            look.sky_hor[2],
            look.exposure,
        ],
        mode: [look.mode, look.rma_order, look.finish, look.slot],
        paint1: rgba(look.paint),
        paint2: [
            look.pearl[0],
            look.pearl[1],
            look.pearl[2],
            look.pearl_amount,
        ],
        misc: [look.wet, look.emissive, 0.0, look.ambient],
        misc2: [
            look.fog,
            look.paint2nd[0],
            look.paint2nd[1],
            look.paint2nd[2],
        ],
        misc3: [look.tint as u32 as f32, 0.0, 0.0, 0.0],
    }
}

fn bind_mesh(
    encoder: &RecordingRenderEncoder<'_>,
    table: &ArgumentTable,
    fallback_texture: &Texture,
    sampler: &SamplerState,
    model: &GpuModel4,
    mesh: &GpuMesh4,
) -> anyhow::Result<()> {
    table.set_buffer(&model.vertex_buffer, mesh.vertex_offset, 1)?;
    table.set_buffer(&model.instance_buffer, 0, 2)?;
    table.set_buffer(&model.material_buffer, mesh.material * MATERIAL_STRIDE, 3)?;
    table.set_buffer(&model.joints_buffer, 0, 4)?;
    for texture_slot in 0..4 {
        let texture = model
            .material_texture_indices
            .get(mesh.material)
            .and_then(|textures| textures[texture_slot])
            .and_then(|index| model.textures.get(index))
            .unwrap_or(fallback_texture);
        table.set_texture(texture, texture_slot)?;
    }
    table.set_sampler_state(sampler, 0)?;
    encoder.draw_indexed_primitives_instanced(
        PrimitiveType::Triangle,
        mesh.n_idx as usize,
        IndexType::IndexTypeUInt32,
        &model.index_buffer,
        mesh.index_range.clone(),
        model.n_instances as usize,
    )?;
    Ok(())
}

struct Target {
    size: [u32; 2],
    color: Texture,
    depth: Texture,
}

pub struct Renderer3D4 {
    device: Device,
    queue: CommandQueue,
    residency: ResidencySet,
    pipeline: metal_rust::metal::RenderPipelineState,
    line_pipeline: metal_rust::metal::RenderPipelineState,
    depth_state: metal_rust::metal::DepthStencilState,
    blend_depth_state: metal_rust::metal::DepthStencilState,
    line_top_depth_state: metal_rust::metal::DepthStencilState,
    argument_table: ArgumentTable,
    sampler: SamplerState,
    fallback_texture: Texture,
    frame_buffer: metal_rust::metal::Buffer,
    line_buffer: Option<metal_rust::metal::Buffer>,
    target: Option<Target>,
    readback: Option<metal_rust::metal::Buffer>,
    pub draws: u32,
}

impl Renderer3D4 {
    pub fn new(device: &Device, queue: &CommandQueue) -> anyhow::Result<Self> {
        let compiler = device.new_mtl4_compiler(&CompilerDescriptor::new()?)?;
        let library_descriptor = LibraryDescriptor::new()?;
        library_descriptor.set_source(MESH_SHADER)?;
        let library = compiler.new_library(&library_descriptor)?;
        let pipeline = create_mesh_pipeline(&compiler, &library)?;
        let line_pipeline = create_line_pipeline(&compiler, &library)?;

        let depth_descriptor = DepthStencilDescriptor::new()?;
        depth_descriptor
            .set_depth_compare_function(CompareFunction::CompareFunctionGreaterEqual)?;
        depth_descriptor.set_depth_write_enabled(true)?;
        let depth_state = device.new_depth_stencil_state(&depth_descriptor)?;
        depth_descriptor.set_depth_write_enabled(false)?;
        let blend_depth_state = device.new_depth_stencil_state(&depth_descriptor)?;
        depth_descriptor.set_depth_compare_function(CompareFunction::CompareFunctionAlways)?;
        let line_top_depth_state = device.new_depth_stencil_state(&depth_descriptor)?;

        let descriptor = ResidencySetDescriptor::new()?;
        descriptor.set_label("SixNG MTL4 3D renderer")?;
        descriptor.set_initial_capacity(128)?;
        let residency = device.new_residency_set(&descriptor)?;
        queue.add_residency_set(&residency)?;

        let mut table_descriptor = ArgumentTableDescriptor::new();
        table_descriptor.set_max_buffer_bind_count(5)?;
        table_descriptor.set_max_texture_bind_count(4)?;
        table_descriptor.set_max_sampler_state_bind_count(1)?;
        table_descriptor.set_initialize_bindings(true);
        let argument_table = device.new_mtl4_argument_table(&table_descriptor)?;

        let sampler_descriptor = SamplerDescriptor::new()?;
        sampler_descriptor.set_min_filter(SamplerMinMagFilter::SamplerMinMagFilterLinear)?;
        sampler_descriptor.set_mag_filter(SamplerMinMagFilter::SamplerMinMagFilterLinear)?;
        sampler_descriptor.set_s_address_mode(SamplerAddressMode::SamplerAddressModeRepeat)?;
        sampler_descriptor.set_t_address_mode(SamplerAddressMode::SamplerAddressModeRepeat)?;
        let sampler = device.new_sampler_state(&sampler_descriptor)?;

        let fallback_descriptor = TextureDescriptor::new_2d(
            PixelFormat::RGBA8_UNORM,
            1,
            1,
            ResourceOptions::SHARED,
            TextureUsage::SHADER_READ,
        )?;
        let fallback_texture = device.new_texture(&fallback_descriptor)?;
        fallback_texture.replace_region_2d(
            metal_rust::metal::Region::new_2d(0, 0, 1, 1),
            0,
            &[255, 255, 255, 255],
            4,
        )?;
        let frame_buffer =
            device.new_buffer(std::mem::size_of::<FrameU>(), ResourceOptions::SHARED)?;
        residency.add_allocations(&[
            ResidencyAllocation::Texture(&fallback_texture),
            ResidencyAllocation::Buffer(&frame_buffer),
        ])?;
        residency.commit()?;

        Ok(Self {
            device: device.clone(),
            queue: queue.clone(),
            residency,
            pipeline,
            line_pipeline,
            depth_state,
            blend_depth_state,
            line_top_depth_state,
            argument_table,
            sampler,
            fallback_texture,
            frame_buffer,
            line_buffer: None,
            target: None,
            readback: None,
            draws: 0,
        })
    }

    pub fn upload_shared(
        &mut self,
        model: &glb::Model,
        texture_cache: &mut HashMap<String, Texture>,
    ) -> anyhow::Result<GpuModel4> {
        let mut textures = Vec::new();
        let mut image_texture_indices = vec![None; model.images.len()];
        let mut new_cache_entries = Vec::new();
        let mut pending_textures = Vec::new();
        for (image_index, image) in model.images.iter().enumerate() {
            let existing = texture_cache.get(&image.key).cloned().or_else(|| {
                new_cache_entries
                    .iter()
                    .find(|(key, _): &&(String, Texture)| key == &image.key)
                    .map(|(_, texture)| texture.clone())
            });
            let texture = if let Some(texture) = existing {
                Some(texture)
            } else if image.mips.is_empty() {
                if image.bc.is_some() {
                    bail!(
                        "MTL4 texture {} was block-compressed without an RGBA fallback",
                        image.key
                    );
                }
                None
            } else {
                let pending = self.new_texture_upload(image)?;
                let texture = pending.texture.clone();
                new_cache_entries.push((image.key.clone(), texture.clone()));
                pending_textures.push(pending);
                Some(texture)
            };
            if let Some(texture) = texture {
                let local_index = textures.len();
                textures.push(texture);
                image_texture_indices[image_index] = Some(local_index);
            }
        }

        let default_material = glb::Material {
            name: "default".into(),
            class: "opaque".into(),
            alpha_mode: "OPAQUE".into(),
            base_factor: [0.7, 0.7, 0.7, 1.0],
            base: None,
            normal: None,
            packed: None,
            emissive: None,
            slots: Vec::new(),
            gap: "ok".into(),
            shader_hash: String::new(),
            decal: None,
        };
        let materials: Vec<&glb::Material> = if model.materials.is_empty() {
            vec![&default_material]
        } else {
            model.materials.iter().collect()
        };
        let mut material_data = vec![0u8; MATERIAL_STRIDE * materials.len()];
        let mut mat_alpha = Vec::with_capacity(materials.len());
        let mut mat_class = Vec::with_capacity(materials.len());
        let mut mat_decal = Vec::with_capacity(materials.len());
        let mut material_texture_indices = Vec::with_capacity(materials.len());
        for (material_index, material) in materials.iter().enumerate() {
            let image_indices = material_image_indices(material);
            let local_textures = image_indices.map(|index| {
                index.and_then(|index| image_texture_indices.get(index).copied().flatten())
            });
            let mut has = 0u32;
            for (slot, texture) in local_textures.iter().enumerate() {
                if texture.is_some() {
                    has |= 1 << slot;
                }
            }
            let alpha = match material.alpha_mode.as_str() {
                "MASK" => 1,
                "BLEND" => 2,
                _ => 0,
            };
            let gap = match material.gap.as_str() {
                "flat" => 2,
                "lowmip" => 1,
                _ => 0,
            };
            let class = crate::render3d::class_id(&material.class);
            let value = Material4 {
                base: material.base_factor,
                flags: [has, class, alpha, gap],
            };
            let offset = material_index * MATERIAL_STRIDE;
            material_data[offset..offset + std::mem::size_of::<Material4>()]
                .copy_from_slice(bytemuck::bytes_of(&value));
            mat_alpha.push(alpha);
            mat_class.push(class);
            mat_decal.push(material.decal);
            material_texture_indices.push(local_textures);
        }

        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut meshes = Vec::with_capacity(model.meshes.len());
        for mesh in &model.meshes {
            if mesh.indices.len() % 3 != 0 {
                bail!("MTL4 mesh {} has an incomplete triangle", mesh.name);
            }
            let vertex_offset = vertices.len() * std::mem::size_of::<Vertex4>();
            let index_start = indices.len() * std::mem::size_of::<u32>();
            vertices.extend(mesh.verts.iter().map(vertex4));
            indices.extend_from_slice(&mesh.indices);
            let index_end = indices.len() * std::mem::size_of::<u32>();
            meshes.push(GpuMesh4 {
                index_range: index_start..index_end,
                vertex_offset,
                n_idx: u32::try_from(mesh.indices.len())
                    .context("MTL4 mesh index count exceeds u32")?,
                material: mesh.material.unwrap_or(0).min(materials.len() - 1),
                skinned: mesh.skinned,
                centre: Vec3::from(mesh.centre),
                visible: true,
            });
        }

        let vertex_bytes = bytemuck::cast_slice::<Vertex4, u8>(&vertices);
        let index_bytes = bytemuck::cast_slice::<u32, u8>(&indices);
        let vertex_buffer = self
            .device
            .new_buffer(vertex_bytes.len().max(4), ResourceOptions::PRIVATE)?;
        let index_buffer = self
            .device
            .new_buffer(index_bytes.len().max(4), ResourceOptions::PRIVATE)?;
        let vertex_staging = if vertex_bytes.is_empty() {
            None
        } else {
            Some(
                self.device
                    .new_buffer_with_bytes(vertex_bytes, ResourceOptions::SHARED)?,
            )
        };
        let index_staging = if index_bytes.is_empty() {
            None
        } else {
            Some(
                self.device
                    .new_buffer_with_bytes(index_bytes, ResourceOptions::SHARED)?,
            )
        };

        let joint_count = model.joints.len();
        let joints = vec![Mat4::IDENTITY.to_cols_array_2d(); joint_count.max(1)];
        let joints_buffer = self
            .device
            .new_buffer_with_bytes(bytemuck::cast_slice(&joints), ResourceOptions::SHARED)?;
        let mut initial_instance = Instance::new(Mat4::IDENTITY, joint_count > 0);
        initial_instance.extra[3] = joint_count as f32;
        let instance_buffer = self.device.new_buffer_with_bytes(
            bytemuck::bytes_of(&initial_instance),
            ResourceOptions::SHARED,
        )?;
        let material_buffer = self
            .device
            .new_buffer_with_bytes(&material_data, ResourceOptions::SHARED)?;

        let mut allocations = vec![
            ResidencyAllocation::Buffer(&vertex_buffer),
            ResidencyAllocation::Buffer(&index_buffer),
            ResidencyAllocation::Buffer(&instance_buffer),
            ResidencyAllocation::Buffer(&material_buffer),
            ResidencyAllocation::Buffer(&joints_buffer),
        ];
        if let Some(buffer) = &vertex_staging {
            allocations.push(ResidencyAllocation::Buffer(buffer));
        }
        if let Some(buffer) = &index_staging {
            allocations.push(ResidencyAllocation::Buffer(buffer));
        }
        for pending in &pending_textures {
            allocations.push(ResidencyAllocation::Buffer(&pending.staging));
            allocations.push(ResidencyAllocation::Texture(&pending.texture));
        }
        self.residency.add_allocations(&allocations)?;
        self.residency.commit()?;

        if vertex_staging.is_some() || index_staging.is_some() || !pending_textures.is_empty() {
            let allocator = self.device.new_mtl4_command_allocator()?;
            let mut command_buffer = self
                .device
                .new_mtl4_command_buffer()?
                .begin(&allocator, None)?;
            command_buffer.use_residency_set(&self.residency)?;
            {
                let encoder = command_buffer.compute_encoder()?;
                if let Some(staging) = &vertex_staging {
                    encoder.copy_buffer(staging, 0..vertex_bytes.len(), &vertex_buffer, 0)?;
                }
                if let Some(staging) = &index_staging {
                    encoder.copy_buffer(staging, 0..index_bytes.len(), &index_buffer, 0)?;
                }
                for pending in &pending_textures {
                    for &(offset, width, height, byte_length) in &pending.mips {
                        encoder.copy_buffer_to_texture(
                            &pending.staging,
                            BufferImageLayout {
                                offset,
                                bytes_per_row: width * 4,
                                bytes_per_image: byte_length,
                            },
                            metal_rust::metal::Size::new(width, height, 1),
                            &pending.texture,
                            TextureOriginSelection {
                                slice: 0,
                                level: pending
                                    .mips
                                    .iter()
                                    .position(|entry| entry.0 == offset)
                                    .context("MTL4 mip upload offset was lost")?,
                                origin: metal_rust::metal::Origin::new(0, 0, 0),
                            },
                            None,
                        )?;
                    }
                }
                encoder.end()?;
            }
            self.queue
                .submit(vec![command_buffer.end()?], None)?
                .wait()?;
        }

        let mut staging_allocations = Vec::new();
        if let Some(buffer) = &vertex_staging {
            staging_allocations.push(ResidencyAllocation::Buffer(buffer));
        }
        if let Some(buffer) = &index_staging {
            staging_allocations.push(ResidencyAllocation::Buffer(buffer));
        }
        for pending in &pending_textures {
            staging_allocations.push(ResidencyAllocation::Buffer(&pending.staging));
        }
        for (key, texture) in new_cache_entries {
            texture_cache.insert(key, texture);
        }
        if !staging_allocations.is_empty() {
            self.residency.remove_allocations(&staging_allocations)?;
            self.residency.commit()?;
        }

        Ok(GpuModel4 {
            meshes,
            mat_alpha,
            mat_class,
            mat_decal,
            bbox: [Vec3::from(model.bbox[0]), Vec3::from(model.bbox[1])],
            n_instances: 1,
            instance_cap: 1,
            joint_count,
            vertex_buffer,
            index_buffer,
            instance_buffer,
            material_buffer,
            joints_buffer,
            textures,
            image_texture_indices,
            material_texture_indices,
        })
    }

    fn new_texture_upload(&self, image: &glb::Image) -> anyhow::Result<PendingTextureUpload> {
        let (base_width, base_height, _) = image
            .mips
            .first()
            .context("MTL4 texture has no mip levels")?;
        if *base_width == 0 || *base_height == 0 {
            bail!("MTL4 texture dimensions must be non-zero");
        }
        let format = if image.srgb {
            PixelFormat::RGBA8_UNORM_SRGB
        } else {
            PixelFormat::RGBA8_UNORM
        };
        let descriptor = TextureDescriptor::new_2d(
            format,
            *base_width as usize,
            *base_height as usize,
            ResourceOptions::PRIVATE,
            TextureUsage::SHADER_READ,
        )?;
        descriptor.set_mipmap_level_count(image.mips.len())?;
        let texture = self.device.new_texture(&descriptor)?;
        let mut bytes = Vec::new();
        let mut mips = Vec::with_capacity(image.mips.len());
        for (width, height, pixels) in &image.mips {
            let expected = (*width as usize)
                .checked_mul(*height as usize)
                .and_then(|pixels| pixels.checked_mul(4))
                .context("MTL4 texture mip size overflow")?;
            if pixels.len() != expected {
                bail!(
                    "MTL4 texture mip {}x{} has {} bytes; expected {}",
                    width,
                    height,
                    pixels.len(),
                    expected
                );
            }
            let offset = bytes.len();
            bytes.extend_from_slice(pixels);
            mips.push((offset, *width as usize, *height as usize, expected));
        }
        let staging = self
            .device
            .new_buffer_with_bytes(&bytes, ResourceOptions::SHARED)?;
        Ok(PendingTextureUpload {
            texture,
            staging,
            mips,
        })
    }

    pub fn set_instances(
        &mut self,
        model: &mut GpuModel4,
        instances: &[Instance],
    ) -> anyhow::Result<()> {
        if instances.len() > model.instance_cap as usize {
            let capacity = instances.len().next_power_of_two().max(1);
            let buffer = self.device.new_buffer(
                capacity
                    .checked_mul(std::mem::size_of::<Instance>())
                    .context("MTL4 instance buffer size overflow")?,
                ResourceOptions::SHARED,
            )?;
            self.residency
                .add_allocations(&[ResidencyAllocation::Buffer(&buffer)])?;
            self.residency.commit()?;
            self.residency
                .remove_allocations(&[ResidencyAllocation::Buffer(&model.instance_buffer)])?;
            self.residency.commit()?;
            model.instance_buffer = buffer;
            model.instance_cap = capacity as u32;
        }
        let mut values = instances.to_vec();
        for instance in &mut values {
            instance.extra[3] = model.joint_count as f32;
        }
        if !values.is_empty() {
            model
                .instance_buffer
                .write(0, bytemuck::cast_slice(&values))?;
        }
        model.n_instances =
            u32::try_from(values.len()).context("MTL4 instance count exceeds u32")?;
        Ok(())
    }

    pub fn release_model(&mut self, model: &GpuModel4) -> anyhow::Result<()> {
        self.residency.remove_allocations(&[
            ResidencyAllocation::Buffer(&model.vertex_buffer),
            ResidencyAllocation::Buffer(&model.index_buffer),
            ResidencyAllocation::Buffer(&model.instance_buffer),
            ResidencyAllocation::Buffer(&model.material_buffer),
            ResidencyAllocation::Buffer(&model.joints_buffer),
        ])?;
        self.residency.commit()?;
        Ok(())
    }

    pub fn release_texture(&mut self, texture: &Texture) -> anyhow::Result<()> {
        self.residency
            .remove_allocations(&[ResidencyAllocation::Texture(texture)])?;
        self.residency.commit()?;
        Ok(())
    }

    pub fn output_texture(&self) -> Option<Texture> {
        self.target.as_ref().map(|target| target.color.clone())
    }

    pub fn render(
        &mut self,
        size: [u32; 2],
        camera: &Camera,
        look: &Look,
        models: &[&GpuModel4],
        lines: &[LineV],
        lines_on_top: &[LineV],
    ) -> anyhow::Result<egui::TextureId> {
        self.ensure_target(size)?;
        let aspect = size[0] as f32 / size[1] as f32;
        let frame = frame_uniform(camera, look, aspect);
        self.frame_buffer.write(0, bytemuck::bytes_of(&frame))?;
        let mut line_vertices = Vec::with_capacity(lines.len() + lines_on_top.len());
        line_vertices.extend_from_slice(lines);
        let under_count = line_vertices.len();
        line_vertices.extend_from_slice(lines_on_top);
        let top_count = line_vertices.len() - under_count;
        if !line_vertices.is_empty() {
            self.ensure_line_buffer(bytemuck::cast_slice(&line_vertices))?;
        }
        let target = self
            .target
            .as_ref()
            .context("MTL4 target was not created")?;

        let pass = RenderPassDescriptor::new()?;
        pass.set_color_attachment(
            0,
            &target.color,
            ClearColor::new(
                look.clear[0] as f64,
                look.clear[1] as f64,
                look.clear[2] as f64,
                1.0,
            ),
        )?;
        let depth_attachment = RenderPassDepthAttachmentDescriptor::new()?;
        depth_attachment.set_texture(Some(&target.depth))?;
        depth_attachment.set_load_action(LoadAction::LoadActionClear)?;
        depth_attachment.set_store_action(StoreAction::StoreActionDontCare)?;
        depth_attachment.set_clear_depth(0.0)?;
        pass.set_depth_attachment(Some(&depth_attachment))?;

        let allocator = self.device.new_mtl4_command_allocator()?;
        let mut command_buffer = self
            .device
            .new_mtl4_command_buffer()?
            .begin(&allocator, None)?;
        command_buffer.use_residency_set(&self.residency)?;
        let mut draw_count = 0u32;
        {
            let encoder = command_buffer.render_encoder(&pass, None)?;
            encoder.set_render_pipeline_state(&self.pipeline)?;
            encoder.set_depth_stencil_state(Some(&self.depth_state))?;
            encoder.set_argument_table(
                Some(&self.argument_table),
                RenderStages::RenderStageVertex | RenderStages::RenderStageFragment,
            )?;
            self.argument_table.set_buffer(&self.frame_buffer, 0, 0)?;

            for model in models {
                if model.n_instances == 0 {
                    continue;
                }
                for mesh in model
                    .meshes
                    .iter()
                    .filter(|mesh| mesh.visible && mesh.n_idx > 0)
                {
                    let alpha = model.mat_alpha[mesh.material] == 2
                        || matches!(model.mat_class[mesh.material], 5 | 6 | 8);
                    if alpha && look.mode == 0 {
                        continue;
                    }
                    bind_mesh(
                        &encoder,
                        &self.argument_table,
                        &self.fallback_texture,
                        &self.sampler,
                        model,
                        mesh,
                    )?;
                    draw_count += 1;
                }
            }

            if look.mode == 0 {
                let eye = camera.eye();
                let mut blended = Vec::new();
                for model in models {
                    if model.n_instances == 0 {
                        continue;
                    }
                    for mesh in model
                        .meshes
                        .iter()
                        .filter(|mesh| mesh.visible && mesh.n_idx > 0)
                    {
                        if model.mat_alpha[mesh.material] == 2
                            || matches!(model.mat_class[mesh.material], 5 | 6 | 8)
                        {
                            blended.push(((mesh.centre - eye).length(), *model, mesh));
                        }
                    }
                }
                blended.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
                encoder.set_depth_stencil_state(Some(&self.blend_depth_state))?;
                for (_, model, mesh) in blended {
                    bind_mesh(
                        &encoder,
                        &self.argument_table,
                        &self.fallback_texture,
                        &self.sampler,
                        model,
                        mesh,
                    )?;
                    draw_count += 1;
                }
            }
            if under_count > 0 {
                let buffer = self
                    .line_buffer
                    .as_ref()
                    .context("MTL4 line buffer was not created")?;
                encoder.set_render_pipeline_state(&self.line_pipeline)?;
                encoder.set_depth_stencil_state(Some(&self.blend_depth_state))?;
                self.argument_table.set_buffer(buffer, 0, 1)?;
                encoder.draw_primitives(PrimitiveType::Line, 0, under_count)?;
                draw_count += 1;
            }
            if top_count > 0 {
                let buffer = self
                    .line_buffer
                    .as_ref()
                    .context("MTL4 line buffer was not created")?;
                encoder.set_render_pipeline_state(&self.line_pipeline)?;
                encoder.set_depth_stencil_state(Some(&self.line_top_depth_state))?;
                self.argument_table.set_buffer(
                    buffer,
                    under_count * std::mem::size_of::<LineV>(),
                    1,
                )?;
                encoder.draw_primitives(PrimitiveType::Line, 0, top_count)?;
                draw_count += 1;
            }
            encoder.end()?;
        }
        self.queue
            .submit(vec![command_buffer.end()?], None)?
            .wait()?;
        self.draws = draw_count;
        Ok(egui::TextureId::User(MAP_TEXTURE_ID))
    }

    fn ensure_line_buffer(&mut self, data: &[u8]) -> anyhow::Result<()> {
        if self
            .line_buffer
            .as_ref()
            .is_some_and(|buffer| buffer.length() >= data.len())
        {
            self.line_buffer
                .as_ref()
                .context("MTL4 line buffer disappeared")?
                .write(0, data)?;
            return Ok(());
        }
        let buffer = self
            .device
            .new_buffer(data.len(), ResourceOptions::SHARED)?;
        buffer.write(0, data)?;
        self.residency
            .add_allocations(&[ResidencyAllocation::Buffer(&buffer)])?;
        self.residency.commit()?;
        if let Some(old) = self.line_buffer.replace(buffer) {
            self.residency
                .remove_allocations(&[ResidencyAllocation::Buffer(&old)])?;
            self.residency.commit()?;
        }
        Ok(())
    }

    pub fn read_back(&mut self) -> anyhow::Result<Option<(u32, u32, Vec<u8>)>> {
        let Some(target) = self.target.as_ref() else {
            return Ok(None);
        };
        let size = target.size;
        let byte_length = (size[0] as usize)
            .checked_mul(size[1] as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .context("MTL4 readback buffer size overflow")?;
        self.ensure_readback(byte_length)?;
        let target = self
            .target
            .as_ref()
            .context("MTL4 target was not created")?;
        let buffer = self
            .readback
            .as_ref()
            .context("MTL4 readback buffer was not created")?;
        let region = TextureRegionSelection {
            slice: 0,
            level: 0,
            region: metal_rust::metal::Region::new_2d(0, 0, size[0] as usize, size[1] as usize),
        };
        let allocator = self.device.new_mtl4_command_allocator()?;
        let mut command_buffer = self
            .device
            .new_mtl4_command_buffer()?
            .begin(&allocator, None)?;
        command_buffer.use_residency_set(&self.residency)?;
        {
            let encoder = command_buffer.compute_encoder()?;
            encoder.copy_texture_to_buffer(
                &target.color,
                region,
                buffer,
                BufferImageLayout {
                    offset: 0,
                    bytes_per_row: size[0] as usize * 4,
                    bytes_per_image: byte_length,
                },
                None,
            )?;
            encoder.end()?;
        }
        self.queue
            .submit(vec![command_buffer.end()?], None)?
            .wait()?;
        Ok(Some((size[0], size[1], buffer.read(0, byte_length)?)))
    }

    fn ensure_target(&mut self, size: [u32; 2]) -> anyhow::Result<()> {
        if size.contains(&0) {
            bail!("MTL4 render target dimensions must be non-zero");
        }
        if self
            .target
            .as_ref()
            .is_some_and(|target| target.size == size)
        {
            return Ok(());
        }

        let descriptor = TextureDescriptor::new_2d(
            PixelFormat::RGBA8_UNORM_SRGB,
            size[0] as usize,
            size[1] as usize,
            ResourceOptions::PRIVATE,
            TextureUsage::RENDER_TARGET.union(TextureUsage::SHADER_READ),
        )?;
        let color = self.device.new_texture(&descriptor)?;
        let depth_descriptor = TextureDescriptor::new_2d(
            PixelFormat::DEPTH32_FLOAT,
            size[0] as usize,
            size[1] as usize,
            ResourceOptions::MEMORYLESS,
            TextureUsage::RENDER_TARGET,
        )?;
        let depth = self.device.new_texture(&depth_descriptor)?;
        self.residency
            .add_allocations(&[ResidencyAllocation::Texture(&color)])?;
        self.residency.commit()?;

        if let Some(old) = self.target.take() {
            self.residency
                .remove_allocations(&[ResidencyAllocation::Texture(&old.color)])?;
            self.residency.commit()?;
        }
        self.target = Some(Target { size, color, depth });
        Ok(())
    }

    fn ensure_readback(&mut self, byte_length: usize) -> anyhow::Result<()> {
        if self
            .readback
            .as_ref()
            .is_some_and(|buffer| buffer.length() >= byte_length)
        {
            return Ok(());
        }
        let buffer = self
            .device
            .new_buffer(byte_length, ResourceOptions::SHARED)?;
        self.residency
            .add_allocations(&[ResidencyAllocation::Buffer(&buffer)])?;
        self.residency.commit()?;
        if let Some(old) = self.readback.replace(buffer) {
            self.residency
                .remove_allocations(&[ResidencyAllocation::Buffer(&old)])?;
            self.residency.commit()?;
        }
        Ok(())
    }

    pub fn clear_and_readback(
        &mut self,
        size: [u32; 2],
        clear: [f64; 4],
    ) -> anyhow::Result<Vec<u8>> {
        self.ensure_target(size)?;
        let byte_length = (size[0] as usize)
            .checked_mul(size[1] as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .context("MTL4 readback buffer size overflow")?;
        self.ensure_readback(byte_length)?;
        let target = self
            .target
            .as_ref()
            .context("MTL4 target was not created")?;

        let pass = RenderPassDescriptor::new()?;
        pass.set_color_attachment(
            0,
            &target.color,
            ClearColor::new(clear[0], clear[1], clear[2], clear[3]),
        )?;
        let depth_attachment = RenderPassDepthAttachmentDescriptor::new()?;
        depth_attachment.set_texture(Some(&target.depth))?;
        depth_attachment.set_load_action(LoadAction::LoadActionClear)?;
        depth_attachment.set_store_action(StoreAction::StoreActionDontCare)?;
        depth_attachment.set_clear_depth(0.0)?;
        pass.set_depth_attachment(Some(&depth_attachment))?;
        let render_allocator = self.device.new_mtl4_command_allocator()?;
        let mut command_buffer = self
            .device
            .new_mtl4_command_buffer()?
            .begin(&render_allocator, None)?;
        command_buffer.use_residency_set(&self.residency)?;
        {
            let encoder = command_buffer.render_encoder(&pass, None)?;
            encoder.set_depth_stencil_state(Some(&self.depth_state))?;
            encoder.end()?;
        }
        self.queue
            .submit(vec![command_buffer.end()?], None)?
            .wait()?;

        let region = TextureRegionSelection {
            slice: 0,
            level: 0,
            region: metal_rust::metal::Region::new_2d(0, 0, size[0] as usize, size[1] as usize),
        };
        let buffer = self
            .readback
            .as_ref()
            .context("MTL4 readback buffer was not created")?;
        let readback_allocator = self.device.new_mtl4_command_allocator()?;
        let mut command_buffer = self
            .device
            .new_mtl4_command_buffer()?
            .begin(&readback_allocator, None)?;
        command_buffer.use_residency_set(&self.residency)?;
        {
            let encoder = command_buffer.compute_encoder()?;
            encoder.copy_texture_to_buffer(
                &target.color,
                region,
                buffer,
                BufferImageLayout {
                    offset: 0,
                    bytes_per_row: size[0] as usize * 4,
                    bytes_per_image: byte_length,
                },
                None,
            )?;
            encoder.end()?;
        }
        self.queue
            .submit(vec![command_buffer.end()?], None)?
            .wait()?;
        Ok(buffer.read(0, byte_length)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn triangle_model() -> glb::Model {
        let vertex = |pos| glb::Vertex {
            pos,
            nrm: [0.0, 0.0, 1.0],
            col: [255; 4],
            tint: [255; 4],
            ..glb::Vertex::default()
        };
        glb::Model {
            path: PathBuf::new(),
            meshes: vec![glb::Mesh {
                name: "test triangle".into(),
                verts: vec![
                    vertex([-1.0, -1.0, 0.0]),
                    vertex([1.0, -1.0, 0.0]),
                    vertex([0.0, 1.0, 0.0]),
                ],
                indices: vec![0, 1, 2],
                material: Some(0),
                skinned: false,
                has_tangents: false,
                uv_sets: 1,
                centre: [0.0; 3],
            }],
            materials: vec![glb::Material {
                name: "red".into(),
                class: "opaque".into(),
                alpha_mode: "OPAQUE".into(),
                base_factor: [1.0, 1.0, 1.0, 1.0],
                base: Some(0),
                normal: None,
                packed: None,
                emissive: None,
                slots: Vec::new(),
                gap: "ok".into(),
                shader_hash: String::new(),
                decal: None,
            }],
            joints: Vec::new(),
            images: vec![glb::Image {
                key: "mtl4-test-green".into(),
                mips: vec![(1, 1, vec![0, 255, 0, 255])],
                srgb: true,
                bc: None,
            }],
            bbox: [[-1.0, -1.0, 0.0], [1.0, 1.0, 0.0]],
            triangles: 1,
            vertices: 3,
            extras: serde_json::Value::Null,
        }
    }
    use metal_rust::metal::GPUFamily;

    #[test]
    fn mtl4_clear_and_readback_round_trip_when_available() -> anyhow::Result<()> {
        let Some(device) = Device::system_default() else {
            return Ok(());
        };
        if !device.supports_family(GPUFamily::GPUFamilyMetal4)? {
            return Ok(());
        }
        let queue = device.new_mtl4_command_queue()?;
        let mut renderer = Renderer3D4::new(&device, &queue)?;
        let size = [4, 3];
        let pixels = renderer.clear_and_readback(size, [1.0, 1.0, 1.0, 1.0])?;
        assert_eq!(pixels.len(), size[0] as usize * size[1] as usize * 4);
        assert!(pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 255, 255, 255]));
        Ok(())
    }

    #[test]
    fn mtl4_mesh_upload_and_draw_changes_pixels_when_available() -> anyhow::Result<()> {
        let Some(device) = Device::system_default() else {
            return Ok(());
        };
        if !device.supports_family(GPUFamily::GPUFamilyMetal4)? {
            return Ok(());
        }
        let queue = device.new_mtl4_command_queue()?;
        let mut renderer = Renderer3D4::new(&device, &queue)?;
        let model = triangle_model();
        let mut model = renderer.upload_shared(&model, &mut HashMap::new())?;
        renderer.set_instances(&mut model, &[Instance::new(Mat4::IDENTITY, false)])?;
        let mut camera = Camera::default();
        camera.fly = true;
        camera.eye = Vec3::new(0.0, 0.0, 3.0);
        camera.yaw = 0.0;
        camera.pitch = 0.0;
        let mut look = Look::default();
        look.mode = 1;
        let line = [
            LineV {
                pos: [-0.9, 0.8, 0.0],
                col: [255, 0, 0, 255],
            },
            LineV {
                pos: [0.9, 0.8, 0.0],
                col: [255, 0, 0, 255],
            },
        ];
        renderer.render([32, 32], &camera, &look, &[&model], &[], &line)?;
        let (_, _, pixels) = renderer
            .read_back()?
            .context("MTL4 renderer did not expose a rendered target")?;
        let center = &pixels[(16 * 32 + 16) * 4..][..4];
        assert!(
            center[1] > center[0] + 30,
            "expected sampled green triangle pixel, got {center:?}"
        );
        assert!(
            center[1] > 100,
            "expected rendered geometry at center, got {center:?}"
        );
        assert!(
            pixels
                .chunks_exact(4)
                .any(|pixel| pixel[0] > 180 && pixel[1] < 80 && pixel[2] < 80),
            "expected the MTL4 top-line overlay in the rendered image"
        );
        Ok(())
    }
}
