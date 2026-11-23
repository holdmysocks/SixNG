//! Audited render and compute command encoder operations.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    AccelerationStructure, CounterSampleBuffer, DepthStencilState, Fence, Heap,
    IndirectCommandBuffer, IntersectionFunctionTable, LogicalToPhysicalColorAttachmentMap,
    SamplerState, VisibleFunctionTable,
};
use crate::metal::generated_struct_types::{ScissorRect, VertexAmplificationViewMapping};
use crate::metal::generated_value_types::{
    BarrierScope, CullMode, DepthClipMode, DispatchType, IndexType, RenderStages, ResourceUsage,
    StoreAction, StoreActionOptions, TriangleFillMode, VisibilityResultMode, Winding,
};
use crate::metal::{
    Buffer, CommandBuffer, ComputePipelineState, Device, PrimitiveType, Region,
    RenderPipelineState, Size, Texture, Viewport,
};
use objc2::rc::Retained;
use objc2::runtime::{MessageReceiver, NSObjectProtocol, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSRange, NSString};
use objc2_metal::{
    MTLCommandEncoder, MTLComputeCommandEncoder, MTLComputePipelineState, MTLCullMode,
    MTLDepthClipMode, MTLDevice, MTLRenderCommandEncoder, MTLScissorRect, MTLSize,
    MTLTriangleFillMode, MTLViewport, MTLWinding,
};
use std::cell::RefCell;
use std::marker::PhantomData;

/// A render encoder borrowing its command buffer until `end_encoding`.
pub struct RenderCommandEncoder<'a> {
    inner: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
    _command_buffer: PhantomData<&'a mut CommandBuffer>,
    ended: bool,
    vertex_buffer_lengths: RefCell<[Option<usize>; 31]>,
    fragment_buffer_lengths: RefCell<[Option<usize>; 31]>,
    tile_buffer_lengths: RefCell<[Option<usize>; 31]>,
    object_buffer_lengths: RefCell<[Option<usize>; 31]>,
    mesh_buffer_lengths: RefCell<[Option<usize>; 31]>,
    _thread_bound: ThreadBound,
}

/// A safely borrowed Metal resource accepted by render usage declarations.
pub enum RenderResource<'a> {
    /// A buffer resource.
    Buffer(&'a Buffer),
    /// A texture resource.
    Texture(&'a Texture),
    /// An acceleration-structure resource.
    AccelerationStructure(&'a AccelerationStructure),
}

impl RenderResource<'_> {
    fn as_object(&self) -> &objc2::runtime::AnyObject {
        match self {
            Self::Buffer(buffer) => AsRef::<objc2::runtime::AnyObject>::as_ref(&*buffer.inner),
            Self::Texture(texture) => AsRef::<objc2::runtime::AnyObject>::as_ref(&*texture.inner),
            Self::AccelerationStructure(structure) => structure.as_inner(),
        }
    }
}

impl<'a> RenderCommandEncoder<'a> {
    pub(super) fn new(
        inner: Retained<ProtocolObject<dyn MTLRenderCommandEncoder>>,
        _command_buffer: &'a mut CommandBuffer,
    ) -> Self {
        Self {
            inner,
            _command_buffer: PhantomData,
            ended: false,
            vertex_buffer_lengths: RefCell::new([None; 31]),
            fragment_buffer_lengths: RefCell::new([None; 31]),
            tile_buffer_lengths: RefCell::new([None; 31]),
            object_buffer_lengths: RefCell::new([None; 31]),
            mesh_buffer_lengths: RefCell::new([None; 31]),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Binds a render pipeline.
    pub fn set_pipeline(&self, pipeline: &RenderPipelineState) {
        self.inner.setRenderPipelineState(&pipeline.inner);
    }

    /// Returns the device executing this encoder.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }

    /// Inserts a copied Rust debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insertDebugSignpost(&NSString::from_str(value));
    }

    /// Pushes a copied Rust debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.pushDebugGroup(&NSString::from_str(value));
    }

    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.popDebugGroup();
    }

    /// Copies vertex-stage constant bytes into the command buffer.
    pub fn set_vertex_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        if bytes.is_empty() || index >= 31 {
            return Err(Error::invalid_argument(
                "vertex bytes must be non-empty and binding index below 31",
            ));
        }
        // SAFETY: the non-empty slice yields a non-null pointer valid for its
        // length; Metal copies inline bytes during this call.
        unsafe {
            self.inner.setVertexBytes_length_atIndex(
                std::ptr::NonNull::new_unchecked(bytes.as_ptr().cast_mut().cast()),
                bytes.len(),
                index,
            );
        }
        Ok(())
    }

    /// Copies fragment-stage constant bytes into the command buffer.
    pub fn set_fragment_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        if bytes.is_empty() || index >= 31 {
            return Err(Error::invalid_argument(
                "fragment bytes must be non-empty and binding index below 31",
            ));
        }
        // SAFETY: the non-empty slice yields a non-null pointer valid for its
        // length; Metal copies inline bytes during this call.
        unsafe {
            self.inner.setFragmentBytes_length_atIndex(
                std::ptr::NonNull::new_unchecked(bytes.as_ptr().cast_mut().cast()),
                bytes.len(),
                index,
            );
        }
        Ok(())
    }

    /// Binds a vertex buffer after checking the offset and binding index.
    pub fn set_vertex_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 || offset > buffer.length() {
            return Err(Error::invalid_argument(
                "vertex buffer binding or offset is out of bounds",
            ));
        }
        // SAFETY: the buffer is retained by the caller for the duration of the
        // encoder, and the checked offset is within its allocation.
        unsafe {
            self.inner
                .setVertexBuffer_offset_atIndex(Some(&buffer.inner), offset, index);
        }
        self.vertex_buffer_lengths.borrow_mut()[index] = Some(buffer.length());
        Ok(())
    }

    /// Binds a vertex buffer with a checked attribute stride.
    pub fn set_vertex_buffer_with_stride(
        &self,
        buffer: &Buffer,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 || offset > buffer.length() || stride == 0 {
            return Err(Error::invalid_argument(
                "vertex buffer binding, offset, or stride is invalid",
            ));
        }
        let selector = sel!(setVertexBuffer:offset:attributeStride:atIndex:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "vertex attribute strides are unavailable",
            ));
        }
        // SAFETY: selector availability, binding index, offset, and non-zero
        // stride are checked; the safe wrapper retains the buffer.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (Some(&*buffer.inner), offset, stride, index));
        }
        self.vertex_buffer_lengths.borrow_mut()[index] = Some(buffer.length());
        Ok(())
    }

    /// Copies vertex bytes with a checked attribute stride.
    pub fn set_vertex_bytes_with_stride(
        &self,
        bytes: &[u8],
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        if bytes.is_empty() || stride == 0 || index >= 31 {
            return Err(Error::invalid_argument(
                "vertex bytes, stride, or binding index is invalid",
            ));
        }
        let selector = sel!(setVertexBytes:length:attributeStride:atIndex:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("vertex byte strides are unavailable"));
        }
        // SAFETY: selector availability, non-empty storage, stride, and index
        // are checked; Metal synchronously copies the bytes.
        unsafe {
            let pointer: std::ptr::NonNull<std::ffi::c_void> =
                std::ptr::NonNull::new_unchecked(bytes.as_ptr().cast_mut().cast());
            let _: () = self
                .inner
                .send_message(selector, (pointer, bytes.len(), stride, index));
        }
        Ok(())
    }

    /// Binds a fragment buffer after checking the offset and binding index.
    pub fn set_fragment_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 || offset > buffer.length() {
            return Err(Error::invalid_argument(
                "fragment buffer binding or offset is out of bounds",
            ));
        }
        // SAFETY: the buffer remains retained by the safe wrapper and the
        // offset and binding index were checked above.
        unsafe {
            self.inner
                .setFragmentBuffer_offset_atIndex(Some(&buffer.inner), offset, index)
        };
        self.fragment_buffer_lengths.borrow_mut()[index] = Some(buffer.length());
        Ok(())
    }

    /// Binds or clears a vertex texture at a checked binding index.
    pub fn set_vertex_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "texture binding index must be below 31",
            ));
        }
        // SAFETY: the binding index is checked and any supplied texture remains
        // retained by its safe wrapper through this encoding call.
        unsafe {
            self.inner
                .setVertexTexture_atIndex(texture.map(|texture| &*texture.inner), index)
        };
        Ok(())
    }

    /// Binds or clears a fragment texture at a checked binding index.
    pub fn set_fragment_texture(
        &self,
        texture: Option<&Texture>,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "texture binding index must be below 31",
            ));
        }
        // SAFETY: the binding index is checked and any supplied texture remains
        // retained by its safe wrapper through this encoding call.
        unsafe {
            self.inner
                .setFragmentTexture_atIndex(texture.map(|texture| &*texture.inner), index)
        };
        Ok(())
    }

    fn set_stage_bytes(
        &self,
        bytes: &[u8],
        index: usize,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if bytes.is_empty() || index >= 31 {
            return Err(Error::invalid_argument(
                "stage bytes must be non-empty and binding index below 31",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: selector availability is checked; the non-empty slice is
        // valid for its length and Metal synchronously copies inline bytes.
        unsafe {
            let pointer: std::ptr::NonNull<std::ffi::c_void> =
                std::ptr::NonNull::new_unchecked(bytes.as_ptr().cast_mut().cast());
            let _: () = self
                .inner
                .send_message(selector, (pointer, bytes.len(), index));
        }
        Ok(())
    }

    /// Copies tile-stage constant bytes when tile shading is available.
    pub fn set_tile_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.set_stage_bytes(
            bytes,
            index,
            sel!(setTileBytes:length:atIndex:),
            "tile byte bindings are unavailable",
        )
    }

    /// Copies object-stage constant bytes when mesh shading is available.
    pub fn set_object_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.set_stage_bytes(
            bytes,
            index,
            sel!(setObjectBytes:length:atIndex:),
            "object byte bindings are unavailable",
        )
    }

    /// Copies mesh-stage constant bytes when mesh shading is available.
    pub fn set_mesh_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        self.set_stage_bytes(
            bytes,
            index,
            sel!(setMeshBytes:length:atIndex:),
            "mesh byte bindings are unavailable",
        )
    }

    fn set_stage_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
        selector: objc2::runtime::Sel,
        lengths: &RefCell<[Option<usize>; 31]>,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if index >= 31 || offset > buffer.length() {
            return Err(Error::invalid_argument(
                "stage buffer binding is out of bounds",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: selector availability, binding index, and buffer offset are
        // checked and the wrapper keeps the buffer retained during the call.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (Some(&*buffer.inner), offset, index));
        }
        lengths.borrow_mut()[index] = Some(buffer.length());
        Ok(())
    }

    /// Binds a tile-stage buffer at a checked slot.
    pub fn set_tile_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_buffer(
            buffer,
            offset,
            index,
            sel!(setTileBuffer:offset:atIndex:),
            &self.tile_buffer_lengths,
            "tile buffer bindings are unavailable",
        )
    }

    /// Binds an object-stage buffer at a checked slot.
    pub fn set_object_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_buffer(
            buffer,
            offset,
            index,
            sel!(setObjectBuffer:offset:atIndex:),
            &self.object_buffer_lengths,
            "object buffer bindings are unavailable",
        )
    }

    /// Binds a mesh-stage buffer at a checked slot.
    pub fn set_mesh_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_buffer(
            buffer,
            offset,
            index,
            sel!(setMeshBuffer:offset:atIndex:),
            &self.mesh_buffer_lengths,
            "mesh buffer bindings are unavailable",
        )
    }

    fn set_stage_buffer_offset(
        &self,
        offset: usize,
        index: usize,
        selector: objc2::runtime::Sel,
        lengths: &RefCell<[Option<usize>; 31]>,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        let length = lengths
            .borrow()
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| Error::invalid_argument("no tracked buffer is bound at this index"))?;
        if offset > length {
            return Err(Error::invalid_argument(
                "stage buffer offset is out of bounds",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: selector availability and the offset against the previously
        // tracked safe buffer binding are checked.
        unsafe {
            let _: () = self.inner.send_message(selector, (offset, index));
        }
        Ok(())
    }

    /// Updates a previously bound vertex buffer offset.
    pub fn set_vertex_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.set_stage_buffer_offset(
            offset,
            index,
            sel!(setVertexBufferOffset:atIndex:),
            &self.vertex_buffer_lengths,
            "vertex buffer offsets are unavailable",
        )
    }

    /// Updates a vertex-buffer offset and attribute stride.
    pub fn set_vertex_buffer_offset_with_stride(
        &self,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        let length = self
            .vertex_buffer_lengths
            .borrow()
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| {
                Error::invalid_argument("no tracked vertex buffer is bound at this index")
            })?;
        if offset > length || stride == 0 {
            return Err(Error::invalid_argument(
                "vertex buffer offset or stride is invalid",
            ));
        }
        let selector = sel!(setVertexBufferOffset:attributeStride:atIndex:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "vertex attribute strides are unavailable",
            ));
        }
        // SAFETY: selector availability, tracked buffer range, binding index,
        // and non-zero stride are checked.
        unsafe {
            let _: () = self.inner.send_message(selector, (offset, stride, index));
        }
        Ok(())
    }

    /// Updates a previously bound fragment buffer offset.
    pub fn set_fragment_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.set_stage_buffer_offset(
            offset,
            index,
            sel!(setFragmentBufferOffset:atIndex:),
            &self.fragment_buffer_lengths,
            "fragment buffer offsets are unavailable",
        )
    }

    /// Updates a previously bound tile buffer offset.
    pub fn set_tile_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.set_stage_buffer_offset(
            offset,
            index,
            sel!(setTileBufferOffset:atIndex:),
            &self.tile_buffer_lengths,
            "tile buffer offsets are unavailable",
        )
    }

    /// Updates a previously bound object buffer offset.
    pub fn set_object_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.set_stage_buffer_offset(
            offset,
            index,
            sel!(setObjectBufferOffset:atIndex:),
            &self.object_buffer_lengths,
            "object buffer offsets are unavailable",
        )
    }

    /// Updates a previously bound mesh buffer offset.
    pub fn set_mesh_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        self.set_stage_buffer_offset(
            offset,
            index,
            sel!(setMeshBufferOffset:atIndex:),
            &self.mesh_buffer_lengths,
            "mesh buffer offsets are unavailable",
        )
    }

    /// Binds multiple vertex buffers through individually checked calls.
    pub fn set_vertex_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, bindings.len())?;
        for (slot, (buffer, offset)) in bindings.iter().enumerate() {
            self.set_vertex_buffer(buffer, *offset, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple vertex buffers and attribute strides through checked calls.
    pub fn set_vertex_buffers_with_strides(
        &self,
        bindings: &[(&Buffer, usize, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, bindings.len())?;
        for (slot, (buffer, offset, stride)) in bindings.iter().enumerate() {
            self.set_vertex_buffer_with_stride(buffer, *offset, *stride, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple fragment buffers through individually checked calls.
    pub fn set_fragment_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, bindings.len())?;
        for (slot, (buffer, offset)) in bindings.iter().enumerate() {
            self.set_fragment_buffer(buffer, *offset, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple tile buffers through individually checked calls.
    pub fn set_tile_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, bindings.len())?;
        for (slot, (buffer, offset)) in bindings.iter().enumerate() {
            self.set_tile_buffer(buffer, *offset, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple object buffers through individually checked calls.
    pub fn set_object_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, bindings.len())?;
        for (slot, (buffer, offset)) in bindings.iter().enumerate() {
            self.set_object_buffer(buffer, *offset, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple mesh buffers through individually checked calls.
    pub fn set_mesh_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, bindings.len())?;
        for (slot, (buffer, offset)) in bindings.iter().enumerate() {
            self.set_mesh_buffer(buffer, *offset, start_index + slot)?;
        }
        Ok(())
    }

    fn set_stage_texture(
        &self,
        texture: Option<&Texture>,
        index: usize,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "texture binding index must be below 31",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: selector availability and binding index are checked; the
        // optional safe wrapper retains the texture through this call.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (texture.map(|value| &*value.inner), index));
        }
        Ok(())
    }

    /// Binds or clears a tile texture.
    pub fn set_tile_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.set_stage_texture(
            texture,
            index,
            sel!(setTileTexture:atIndex:),
            "tile textures are unavailable",
        )
    }

    /// Binds or clears an object texture.
    pub fn set_object_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.set_stage_texture(
            texture,
            index,
            sel!(setObjectTexture:atIndex:),
            "object textures are unavailable",
        )
    }

    /// Binds or clears a mesh texture.
    pub fn set_mesh_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        self.set_stage_texture(
            texture,
            index,
            sel!(setMeshTexture:atIndex:),
            "mesh textures are unavailable",
        )
    }

    /// Binds multiple vertex textures through checked calls.
    pub fn set_vertex_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, textures.len())?;
        for (slot, texture) in textures.iter().enumerate() {
            self.set_vertex_texture(*texture, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple fragment textures through checked calls.
    pub fn set_fragment_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, textures.len())?;
        for (slot, texture) in textures.iter().enumerate() {
            self.set_fragment_texture(*texture, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple tile textures through checked calls.
    pub fn set_tile_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, textures.len())?;
        for (slot, texture) in textures.iter().enumerate() {
            self.set_tile_texture(*texture, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple object textures through checked calls.
    pub fn set_object_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, textures.len())?;
        for (slot, texture) in textures.iter().enumerate() {
            self.set_object_texture(*texture, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple mesh textures through checked calls.
    pub fn set_mesh_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, textures.len())?;
        for (slot, texture) in textures.iter().enumerate() {
            self.set_mesh_texture(*texture, start_index + slot)?;
        }
        Ok(())
    }

    fn set_stage_sampler(
        &self,
        sampler: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if index >= 16 {
            return Err(Error::invalid_argument(
                "sampler binding index must be below 16",
            ));
        }
        if let Some((min, max)) = lod_clamp
            && (!min.is_finite() || !max.is_finite() || min < 0.0 || min > max)
        {
            return Err(Error::invalid_argument(
                "sampler LOD clamps must be finite, non-negative, and ordered",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        let object = sampler.map(SamplerState::as_inner);
        // SAFETY: selector availability, sampler index, and optional finite LOD
        // clamp ordering are checked; the wrapper retains the sampler.
        unsafe {
            if let Some((min, max)) = lod_clamp {
                let _: () = self.inner.send_message(selector, (object, min, max, index));
            } else {
                let _: () = self.inner.send_message(selector, (object, index));
            }
        }
        Ok(())
    }

    /// Binds or clears a vertex sampler with optional checked LOD clamps.
    pub fn set_vertex_sampler(
        &self,
        sampler: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        let selector = if lod_clamp.is_some() {
            sel!(setVertexSamplerState:lodMinClamp:lodMaxClamp:atIndex:)
        } else {
            sel!(setVertexSamplerState:atIndex:)
        };
        self.set_stage_sampler(
            sampler,
            lod_clamp,
            index,
            selector,
            "vertex samplers are unavailable",
        )
    }

    /// Binds or clears a fragment sampler with optional checked LOD clamps.
    pub fn set_fragment_sampler(
        &self,
        sampler: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        let selector = if lod_clamp.is_some() {
            sel!(setFragmentSamplerState:lodMinClamp:lodMaxClamp:atIndex:)
        } else {
            sel!(setFragmentSamplerState:atIndex:)
        };
        self.set_stage_sampler(
            sampler,
            lod_clamp,
            index,
            selector,
            "fragment samplers are unavailable",
        )
    }

    /// Binds or clears a tile sampler with optional checked LOD clamps.
    pub fn set_tile_sampler(
        &self,
        sampler: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        let selector = if lod_clamp.is_some() {
            sel!(setTileSamplerState:lodMinClamp:lodMaxClamp:atIndex:)
        } else {
            sel!(setTileSamplerState:atIndex:)
        };
        self.set_stage_sampler(
            sampler,
            lod_clamp,
            index,
            selector,
            "tile samplers are unavailable",
        )
    }

    /// Binds or clears an object sampler with optional checked LOD clamps.
    pub fn set_object_sampler(
        &self,
        sampler: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        let selector = if lod_clamp.is_some() {
            sel!(setObjectSamplerState:lodMinClamp:lodMaxClamp:atIndex:)
        } else {
            sel!(setObjectSamplerState:atIndex:)
        };
        self.set_stage_sampler(
            sampler,
            lod_clamp,
            index,
            selector,
            "object samplers are unavailable",
        )
    }

    /// Binds or clears a mesh sampler with optional checked LOD clamps.
    pub fn set_mesh_sampler(
        &self,
        sampler: Option<&SamplerState>,
        lod_clamp: Option<(f32, f32)>,
        index: usize,
    ) -> Result<(), Error> {
        let selector = if lod_clamp.is_some() {
            sel!(setMeshSamplerState:lodMinClamp:lodMaxClamp:atIndex:)
        } else {
            sel!(setMeshSamplerState:atIndex:)
        };
        self.set_stage_sampler(
            sampler,
            lod_clamp,
            index,
            selector,
            "mesh samplers are unavailable",
        )
    }

    /// Binds multiple vertex samplers through checked individual calls.
    pub fn set_vertex_samplers(
        &self,
        samplers: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, samplers.len())?;
        for (slot, sampler) in samplers.iter().enumerate() {
            self.set_vertex_sampler(*sampler, None, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple fragment samplers through checked individual calls.
    pub fn set_fragment_samplers(
        &self,
        samplers: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, samplers.len())?;
        for (slot, sampler) in samplers.iter().enumerate() {
            self.set_fragment_sampler(*sampler, None, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple tile samplers through checked individual calls.
    pub fn set_tile_samplers(
        &self,
        samplers: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, samplers.len())?;
        for (slot, sampler) in samplers.iter().enumerate() {
            self.set_tile_sampler(*sampler, None, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple object samplers through checked individual calls.
    pub fn set_object_samplers(
        &self,
        samplers: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, samplers.len())?;
        for (slot, sampler) in samplers.iter().enumerate() {
            self.set_object_sampler(*sampler, None, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple mesh samplers through checked individual calls.
    pub fn set_mesh_samplers(
        &self,
        samplers: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, samplers.len())?;
        for (slot, sampler) in samplers.iter().enumerate() {
            self.set_mesh_sampler(*sampler, None, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds vertex samplers and per-slot checked LOD clamps.
    pub fn set_vertex_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, values.len())?;
        for (slot, (sampler, min, max)) in values.iter().enumerate() {
            self.set_vertex_sampler(*sampler, Some((*min, *max)), start_index + slot)?;
        }
        Ok(())
    }
    /// Binds fragment samplers and per-slot checked LOD clamps.
    pub fn set_fragment_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, values.len())?;
        for (slot, (sampler, min, max)) in values.iter().enumerate() {
            self.set_fragment_sampler(*sampler, Some((*min, *max)), start_index + slot)?;
        }
        Ok(())
    }
    /// Binds tile samplers and per-slot checked LOD clamps.
    pub fn set_tile_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, values.len())?;
        for (slot, (sampler, min, max)) in values.iter().enumerate() {
            self.set_tile_sampler(*sampler, Some((*min, *max)), start_index + slot)?;
        }
        Ok(())
    }
    /// Binds object samplers and per-slot checked LOD clamps.
    pub fn set_object_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, values.len())?;
        for (slot, (sampler, min, max)) in values.iter().enumerate() {
            self.set_object_sampler(*sampler, Some((*min, *max)), start_index + slot)?;
        }
        Ok(())
    }
    /// Binds mesh samplers and per-slot checked LOD clamps.
    pub fn set_mesh_samplers_with_lod_clamps(
        &self,
        values: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_sampler_end(start_index, values.len())?;
        for (slot, (sampler, min, max)) in values.iter().enumerate() {
            self.set_mesh_sampler(*sampler, Some((*min, *max)), start_index + slot)?;
        }
        Ok(())
    }

    fn set_stage_generated_object(
        &self,
        object: Option<&objc2::runtime::AnyObject>,
        index: usize,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "buffer binding index must be below 31",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: callers supply audited `(id, NSUInteger) -> void` selectors;
        // selector availability and the common binding index are checked.
        unsafe {
            let _: () = self.inner.send_message(selector, (object, index));
        }
        Ok(())
    }

    /// Binds or clears a vertex acceleration structure.
    pub fn set_vertex_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(AccelerationStructure::as_inner),
            index,
            sel!(setVertexAccelerationStructure:atBufferIndex:),
            "vertex acceleration structures are unavailable",
        )
    }

    /// Binds or clears a fragment acceleration structure.
    pub fn set_fragment_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(AccelerationStructure::as_inner),
            index,
            sel!(setFragmentAccelerationStructure:atBufferIndex:),
            "fragment acceleration structures are unavailable",
        )
    }

    /// Binds or clears a tile acceleration structure.
    pub fn set_tile_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(AccelerationStructure::as_inner),
            index,
            sel!(setTileAccelerationStructure:atBufferIndex:),
            "tile acceleration structures are unavailable",
        )
    }

    /// Binds or clears a vertex visible-function table.
    pub fn set_vertex_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(VisibleFunctionTable::as_inner),
            index,
            sel!(setVertexVisibleFunctionTable:atBufferIndex:),
            "vertex visible function tables are unavailable",
        )
    }

    /// Binds or clears a fragment visible-function table.
    pub fn set_fragment_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(VisibleFunctionTable::as_inner),
            index,
            sel!(setFragmentVisibleFunctionTable:atBufferIndex:),
            "fragment visible function tables are unavailable",
        )
    }

    /// Binds or clears a tile visible-function table.
    pub fn set_tile_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(VisibleFunctionTable::as_inner),
            index,
            sel!(setTileVisibleFunctionTable:atBufferIndex:),
            "tile visible function tables are unavailable",
        )
    }

    /// Binds or clears a vertex intersection-function table.
    pub fn set_vertex_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(IntersectionFunctionTable::as_inner),
            index,
            sel!(setVertexIntersectionFunctionTable:atBufferIndex:),
            "vertex intersection function tables are unavailable",
        )
    }

    /// Binds or clears a fragment intersection-function table.
    pub fn set_fragment_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(IntersectionFunctionTable::as_inner),
            index,
            sel!(setFragmentIntersectionFunctionTable:atBufferIndex:),
            "fragment intersection function tables are unavailable",
        )
    }

    /// Binds or clears a tile intersection-function table.
    pub fn set_tile_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_stage_generated_object(
            value.map(IntersectionFunctionTable::as_inner),
            index,
            sel!(setTileIntersectionFunctionTable:atBufferIndex:),
            "tile intersection function tables are unavailable",
        )
    }

    /// Binds vertex visible-function tables through safe individual calls.
    pub fn set_vertex_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, values.len())?;
        for (slot, value) in values.iter().enumerate() {
            self.set_vertex_visible_function_table(*value, start_index + slot)?;
        }
        Ok(())
    }
    /// Binds fragment visible-function tables through safe individual calls.
    pub fn set_fragment_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, values.len())?;
        for (slot, value) in values.iter().enumerate() {
            self.set_fragment_visible_function_table(*value, start_index + slot)?;
        }
        Ok(())
    }
    /// Binds tile visible-function tables through safe individual calls.
    pub fn set_tile_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, values.len())?;
        for (slot, value) in values.iter().enumerate() {
            self.set_tile_visible_function_table(*value, start_index + slot)?;
        }
        Ok(())
    }
    /// Binds vertex intersection-function tables through safe individual calls.
    pub fn set_vertex_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, values.len())?;
        for (slot, value) in values.iter().enumerate() {
            self.set_vertex_intersection_function_table(*value, start_index + slot)?;
        }
        Ok(())
    }
    /// Binds fragment intersection-function tables through safe individual calls.
    pub fn set_fragment_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, values.len())?;
        for (slot, value) in values.iter().enumerate() {
            self.set_fragment_intersection_function_table(*value, start_index + slot)?;
        }
        Ok(())
    }
    /// Binds tile intersection-function tables through safe individual calls.
    pub fn set_tile_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        checked_binding_end(start_index, values.len())?;
        for (slot, value) in values.iter().enumerate() {
            self.set_tile_intersection_function_table(*value, start_index + slot)?;
        }
        Ok(())
    }

    /// Sets a finite RGBA blend constant.
    pub fn set_blend_color(&self, color: [f32; 4]) -> Result<(), Error> {
        if !color.into_iter().all(f32::is_finite) {
            return Err(Error::invalid_argument("blend color must be finite"));
        }
        self.inner
            .setBlendColorRed_green_blue_alpha(color[0], color[1], color[2], color[3]);
        Ok(())
    }

    /// Binds or clears a depth-stencil state.
    pub fn set_depth_stencil_state(&self, state: Option<&DepthStencilState>) -> Result<(), Error> {
        let selector = sel!(setDepthStencilState:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("depth-stencil states are unavailable"));
        }
        // SAFETY: selector availability is checked and the optional generated
        // wrapper retains the Objective-C object.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (state.map(DepthStencilState::as_inner),));
        }
        Ok(())
    }

    /// Sets a logical-to-physical color attachment map when supported.
    pub fn set_color_attachment_map(
        &self,
        mapping: &LogicalToPhysicalColorAttachmentMap,
    ) -> Result<(), Error> {
        let selector = sel!(setColorAttachmentMap:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("color attachment maps are unavailable"));
        }
        // SAFETY: selector availability is checked and the generated wrapper
        // retains the mapping object.
        unsafe {
            let _: () = self.inner.send_message(selector, (mapping.as_inner(),));
        }
        Ok(())
    }

    /// Sets checked vertex amplification view mappings.
    pub fn set_vertex_amplification(
        &self,
        mappings: &[VertexAmplificationViewMapping],
    ) -> Result<(), Error> {
        if mappings.is_empty() || mappings.len() > 2 {
            return Err(Error::invalid_argument(
                "vertex amplification count must be between 1 and 2",
            ));
        }
        #[repr(C)]
        struct RawMapping {
            viewport_array_index_offset: u32,
            render_target_array_index_offset: u32,
        }
        let raw: Vec<_> = mappings
            .iter()
            .map(|value| RawMapping {
                viewport_array_index_offset: value.viewport_array_index_offset,
                render_target_array_index_offset: value.render_target_array_index_offset,
            })
            .collect();
        let selector = sel!(setVertexAmplificationCount:viewMappings:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("vertex amplification is unavailable"));
        }
        // SAFETY: selector availability and supported count are checked; the
        // repr(C) array matches the two uint32_t fields and lives for the call.
        unsafe {
            let pointer: std::ptr::NonNull<std::ffi::c_void> =
                std::ptr::NonNull::from(&raw[0]).cast();
            let _: () = self.inner.send_message(selector, (raw.len(), pointer));
        }
        Ok(())
    }

    /// Sets a validated triangle cull mode.
    pub fn set_cull_mode(&self, value: CullMode) {
        self.inner.setCullMode(MTLCullMode(value.as_raw()));
    }

    /// Sets a validated front-facing winding order.
    pub fn set_front_facing_winding(&self, value: Winding) {
        self.inner.setFrontFacingWinding(MTLWinding(value.as_raw()));
    }

    /// Sets a validated triangle fill mode.
    pub fn set_triangle_fill_mode(&self, value: TriangleFillMode) {
        self.inner
            .setTriangleFillMode(MTLTriangleFillMode(value.as_raw()));
    }

    /// Sets finite depth-bias parameters.
    pub fn set_depth_bias(&self, bias: f32, slope_scale: f32, clamp: f32) -> Result<(), Error> {
        if ![bias, slope_scale, clamp].into_iter().all(f32::is_finite) {
            return Err(Error::invalid_argument("depth bias values must be finite"));
        }
        self.inner
            .setDepthBias_slopeScale_clamp(bias, slope_scale, clamp);
        Ok(())
    }

    /// Sets a validated depth clipping mode when supported.
    pub fn set_depth_clip_mode(&self, value: DepthClipMode) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(setDepthClipMode:)) {
            return Err(Error::unsupported("depth clip mode is unavailable"));
        }
        self.inner
            .setDepthClipMode(MTLDepthClipMode(value.as_raw()));
        Ok(())
    }

    /// Sets finite ordered depth-test bounds when supported.
    pub fn set_depth_test_bounds(&self, min: f32, max: f32) -> Result<(), Error> {
        if !min.is_finite() || !max.is_finite() || min > max {
            return Err(Error::invalid_argument(
                "depth bounds must be finite and ordered",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(setDepthTestMinBound:maxBound:))
        {
            return Err(Error::unsupported("depth-test bounds are unavailable"));
        }
        self.inner.setDepthTestMinBound_maxBound(min, max);
        Ok(())
    }

    /// Sets one stencil reference value for both faces.
    pub fn set_stencil_reference_value(&self, value: u32) {
        self.inner.setStencilReferenceValue(value);
    }

    /// Sets independent front and back stencil reference values.
    pub fn set_stencil_reference_values(&self, front: u32, back: u32) {
        self.inner
            .setStencilFrontReferenceValue_backReferenceValue(front, back);
    }

    /// Sets a checked scissor rectangle.
    pub fn set_scissor_rect(&self, rect: ScissorRect) -> Result<(), Error> {
        rect.x
            .checked_add(rect.width)
            .and_then(|_| rect.y.checked_add(rect.height))
            .ok_or_else(|| Error::invalid_argument("scissor rectangle overflows"))?;
        self.inner.setScissorRect(MTLScissorRect {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        });
        Ok(())
    }

    /// Sets multiple checked scissor rectangles when supported.
    pub fn set_scissor_rects(&self, rects: &[ScissorRect]) -> Result<(), Error> {
        if rects.is_empty() || rects.len() > 16 {
            return Err(Error::invalid_argument(
                "scissor rectangle count must be between 1 and 16",
            ));
        }
        let mut raw = Vec::with_capacity(rects.len());
        for rect in rects {
            rect.x
                .checked_add(rect.width)
                .and_then(|_| rect.y.checked_add(rect.height))
                .ok_or_else(|| Error::invalid_argument("scissor rectangle overflows"))?;
            raw.push(MTLScissorRect {
                x: rect.x,
                y: rect.y,
                width: rect.width,
                height: rect.height,
            });
        }
        let selector = sel!(setScissorRects:count:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "multiple scissor rectangles are unavailable",
            ));
        }
        // SAFETY: selector availability and every rectangle are validated; the
        // non-empty contiguous array remains alive for the synchronous call.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (std::ptr::NonNull::from(&raw[0]), raw.len()));
        }
        Ok(())
    }

    /// Returns the tile dimensions when tile shaders are available.
    pub fn tile_size(&self) -> Result<Size, Error> {
        if !self.inner.respondsToSelector(sel!(tileWidth))
            || !self.inner.respondsToSelector(sel!(tileHeight))
        {
            return Err(Error::unsupported("tile dimensions are unavailable"));
        }
        Ok(Size::new(
            self.inner.tileWidth(),
            self.inner.tileHeight(),
            1,
        ))
    }

    /// Encodes a tile-texture barrier when supported.
    #[allow(deprecated)]
    pub fn texture_barrier(&self) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(textureBarrier)) {
            return Err(Error::unsupported("texture barriers are unavailable"));
        }
        self.inner.textureBarrier();
        Ok(())
    }

    /// Sets the viewport after rejecting non-finite or negative dimensions.
    pub fn set_viewport(&self, viewport: Viewport) -> Result<(), Error> {
        if ![
            viewport.origin_x,
            viewport.origin_y,
            viewport.width,
            viewport.height,
            viewport.z_near,
            viewport.z_far,
        ]
        .into_iter()
        .all(f64::is_finite)
            || viewport.width < 0.0
            || viewport.height < 0.0
        {
            return Err(Error::invalid_argument("viewport contains invalid values"));
        }
        self.inner.setViewport(viewport.into());
        Ok(())
    }

    /// Sets multiple validated viewports when supported.
    pub fn set_viewports(&self, viewports: &[Viewport]) -> Result<(), Error> {
        if viewports.is_empty() || viewports.len() > 16 {
            return Err(Error::invalid_argument(
                "viewport count must be between 1 and 16",
            ));
        }
        let mut raw: Vec<MTLViewport> = Vec::with_capacity(viewports.len());
        for viewport in viewports {
            self.validate_viewport(*viewport)?;
            raw.push((*viewport).into());
        }
        let selector = sel!(setViewports:count:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("multiple viewports are unavailable"));
        }
        // SAFETY: selector availability and all viewports are validated; the
        // non-empty contiguous array remains alive for the synchronous call.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (std::ptr::NonNull::from(&raw[0]), raw.len()));
        }
        Ok(())
    }

    fn validate_viewport(&self, viewport: Viewport) -> Result<(), Error> {
        if ![
            viewport.origin_x,
            viewport.origin_y,
            viewport.width,
            viewport.height,
            viewport.z_near,
            viewport.z_far,
        ]
        .into_iter()
        .all(f64::is_finite)
            || viewport.width < 0.0
            || viewport.height < 0.0
        {
            return Err(Error::invalid_argument("viewport contains invalid values"));
        }
        Ok(())
    }

    /// Configures visibility-result writes at an aligned checked offset.
    pub fn set_visibility_result_mode(
        &self,
        mode: VisibilityResultMode,
        offset: usize,
    ) -> Result<(), Error> {
        if !mode.is_valid() {
            return Err(Error::invalid_argument("visibility mode is invalid"));
        }
        if !offset.is_multiple_of(8) {
            return Err(Error::invalid_argument(
                "visibility result offset must be 8-byte aligned",
            ));
        }
        let selector = sel!(setVisibilityResultMode:offset:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("visibility results are unavailable"));
        }
        // SAFETY: selector availability, enum value, and required offset
        // alignment are validated.
        unsafe {
            let _: () = self.inner.send_message(selector, (mode.as_raw(), offset));
        }
        Ok(())
    }

    fn set_store_action(
        &self,
        action: StoreAction,
        attachment: Option<usize>,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if !action.is_valid() {
            return Err(Error::invalid_argument("store action is invalid"));
        }
        if attachment.is_some_and(|index| index >= 8) {
            return Err(Error::invalid_argument(
                "color attachment index must be below 8",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: selector availability, action, and optional attachment index
        // are validated for the audited selector signatures.
        unsafe {
            if let Some(index) = attachment {
                let _: () = self.inner.send_message(selector, (action.as_raw(), index));
            } else {
                let _: () = self.inner.send_message(selector, (action.as_raw(),));
            }
        }
        Ok(())
    }

    /// Sets a color attachment store action.
    pub fn set_color_store_action(&self, action: StoreAction, index: usize) -> Result<(), Error> {
        self.set_store_action(
            action,
            Some(index),
            sel!(setColorStoreAction:atIndex:),
            "color store actions are unavailable",
        )
    }
    /// Sets the depth attachment store action.
    pub fn set_depth_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.set_store_action(
            action,
            None,
            sel!(setDepthStoreAction:),
            "depth store actions are unavailable",
        )
    }
    /// Sets the stencil attachment store action.
    pub fn set_stencil_store_action(&self, action: StoreAction) -> Result<(), Error> {
        self.set_store_action(
            action,
            None,
            sel!(setStencilStoreAction:),
            "stencil store actions are unavailable",
        )
    }

    fn set_store_options(
        &self,
        options: StoreActionOptions,
        attachment: Option<usize>,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if !options.is_valid() {
            return Err(Error::invalid_argument(
                "store action options contain unknown bits",
            ));
        }
        if attachment.is_some_and(|index| index >= 8) {
            return Err(Error::invalid_argument(
                "color attachment index must be below 8",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: selector availability, option bits, and optional attachment
        // index are validated for the audited selector signatures.
        unsafe {
            if let Some(index) = attachment {
                let _: () = self.inner.send_message(selector, (options.as_raw(), index));
            } else {
                let _: () = self.inner.send_message(selector, (options.as_raw(),));
            }
        }
        Ok(())
    }
    /// Sets color attachment store-action options.
    pub fn set_color_store_action_options(
        &self,
        options: StoreActionOptions,
        index: usize,
    ) -> Result<(), Error> {
        self.set_store_options(
            options,
            Some(index),
            sel!(setColorStoreActionOptions:atIndex:),
            "color store options are unavailable",
        )
    }
    /// Sets depth attachment store-action options.
    pub fn set_depth_store_action_options(&self, options: StoreActionOptions) -> Result<(), Error> {
        self.set_store_options(
            options,
            None,
            sel!(setDepthStoreActionOptions:),
            "depth store options are unavailable",
        )
    }
    /// Sets stencil attachment store-action options.
    pub fn set_stencil_store_action_options(
        &self,
        options: StoreActionOptions,
    ) -> Result<(), Error> {
        self.set_store_options(
            options,
            None,
            sel!(setStencilStoreActionOptions:),
            "stencil store options are unavailable",
        )
    }

    /// Encodes a non-indexed primitive draw.
    pub fn draw_primitives(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
    ) -> Result<(), Error> {
        if vertex_count == 0 {
            return Err(Error::invalid_argument("vertex count must be non-zero"));
        }
        // SAFETY: the public method only passes non-zero usize counts and the
        // Metal encoder performs the remaining pipeline/resource validation.
        unsafe {
            self.inner.drawPrimitives_vertexStart_vertexCount(
                primitive.as_objc(),
                vertex_start,
                vertex_count,
            );
        }
        Ok(())
    }

    /// Encodes an instanced non-indexed primitive draw.
    pub fn draw_primitives_instanced(
        &self,
        primitive: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        if vertex_count == 0 || instance_count == 0 {
            return Err(Error::invalid_argument(
                "vertex and instance counts must be non-zero",
            ));
        }
        // SAFETY: non-zero counts are checked, the primitive value is closed,
        // and Metal validates the active pipeline and bound resources.
        unsafe {
            self.inner
                .drawPrimitives_vertexStart_vertexCount_instanceCount_baseInstance(
                    primitive.as_objc(),
                    vertex_start,
                    vertex_count,
                    instance_count,
                    base_instance,
                )
        };
        Ok(())
    }

    /// Encodes an indexed draw after validating index-buffer storage.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_primitives(
        &self,
        primitive: PrimitiveType,
        index_count: usize,
        index_type: IndexType,
        index_buffer: &Buffer,
        index_buffer_offset: usize,
        instance_count: usize,
        base_vertex: isize,
        base_instance: usize,
    ) -> Result<(), Error> {
        if index_count == 0 || instance_count == 0 || !index_type.is_valid() {
            return Err(Error::invalid_argument(
                "indexed draw counts and index type are invalid",
            ));
        }
        let element_size = if index_type.as_raw() == 0 { 2 } else { 4 };
        let bytes = index_count
            .checked_mul(element_size)
            .and_then(|value| index_buffer_offset.checked_add(value))
            .ok_or_else(|| Error::invalid_argument("index range overflows"))?;
        if !index_buffer_offset.is_multiple_of(element_size) || bytes > index_buffer.length() {
            return Err(Error::invalid_argument(
                "index buffer range or alignment is invalid",
            ));
        }
        let selector = sel!(drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:instanceCount:baseVertex:baseInstance:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("indexed draws are unavailable"));
        }
        // SAFETY: selector availability, non-zero counts, index enum,
        // alignment, overflow, and complete buffer storage are checked.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    primitive.as_raw(),
                    index_count,
                    index_type.as_raw(),
                    &*index_buffer.inner,
                    index_buffer_offset,
                    instance_count,
                    base_vertex,
                    base_instance,
                ),
            );
        }
        Ok(())
    }

    /// Encodes a non-indexed draw using checked indirect argument storage.
    pub fn draw_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        checked_buffer_struct::<
            crate::metal::generated_struct_types::DrawPrimitivesIndirectArguments,
        >(buffer, offset, "indirect draw")?;
        let selector = sel!(drawPrimitives:indirectBuffer:indirectBufferOffset:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("indirect draws are unavailable"));
        }
        // SAFETY: selector availability and storage for the complete indirect
        // argument structure are checked.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (primitive.as_raw(), &*buffer.inner, offset));
        }
        Ok(())
    }

    /// Encodes an indexed draw using checked indirect argument storage.
    pub fn draw_indexed_primitives_indirect(
        &self,
        primitive: PrimitiveType,
        index_type: IndexType,
        index_buffer: &Buffer,
        index_buffer_offset: usize,
        indirect_buffer: &Buffer,
        indirect_offset: usize,
    ) -> Result<(), Error> {
        if !index_type.is_valid() {
            return Err(Error::invalid_argument("index type is invalid"));
        }
        let alignment = if index_type.as_raw() == 0 { 2 } else { 4 };
        if index_buffer_offset >= index_buffer.length()
            || !index_buffer_offset.is_multiple_of(alignment)
        {
            return Err(Error::invalid_argument("index buffer offset is invalid"));
        }
        checked_buffer_struct::<
            crate::metal::generated_struct_types::DrawIndexedPrimitivesIndirectArguments,
        >(indirect_buffer, indirect_offset, "indirect indexed draw")?;
        let selector = sel!(drawIndexedPrimitives:indexType:indexBuffer:indexBufferOffset:indirectBuffer:indirectBufferOffset:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("indirect indexed draws are unavailable"));
        }
        // SAFETY: selector availability, index type/alignment, and complete
        // indirect argument storage are checked; both buffers are retained.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    primitive.as_raw(),
                    index_type.as_raw(),
                    &*index_buffer.inner,
                    index_buffer_offset,
                    &*indirect_buffer.inner,
                    indirect_offset,
                ),
            );
        }
        Ok(())
    }

    /// Dispatches tile threads with non-zero dimensions.
    pub fn dispatch_threads_per_tile(&self, threads: Size) -> Result<(), Error> {
        validate_nonzero_size(threads, "tile thread dimensions")?;
        let selector = sel!(dispatchThreadsPerTile:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("tile dispatch is unavailable"));
        }
        // SAFETY: selector availability and non-zero dimensions are checked;
        // MTLSize is the declared ABI type.
        unsafe {
            let _: () = self.inner.send_message(selector, (MTLSize::from(threads),));
        }
        Ok(())
    }

    /// Dispatches mesh threadgroups with checked non-zero dimensions.
    pub fn draw_mesh_threadgroups(
        &self,
        groups: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        validate_nonzero_size(groups, "mesh threadgroups")?;
        validate_nonzero_size(object_threads, "object threads")?;
        validate_nonzero_size(mesh_threads, "mesh threads")?;
        let selector =
            sel!(drawMeshThreadgroups:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("mesh threadgroup draws are unavailable"));
        }
        // SAFETY: selector availability and all dimensions are validated.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    MTLSize::from(groups),
                    MTLSize::from(object_threads),
                    MTLSize::from(mesh_threads),
                ),
            );
        }
        Ok(())
    }

    /// Draws mesh threadgroups using checked indirect argument storage.
    pub fn draw_mesh_threadgroups_indirect(
        &self,
        buffer: &Buffer,
        offset: usize,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        checked_buffer_struct::<
            crate::metal::generated_struct_types::DispatchThreadgroupsIndirectArguments,
        >(buffer, offset, "indirect mesh draw")?;
        validate_nonzero_size(object_threads, "object threads")?;
        validate_nonzero_size(mesh_threads, "mesh threads")?;
        let selector = sel!(drawMeshThreadgroupsWithIndirectBuffer:indirectBufferOffset:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("indirect mesh draws are unavailable"));
        }
        // SAFETY: selector availability, complete indirect storage, and both
        // non-zero threadgroup dimensions are checked.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    &*buffer.inner,
                    offset,
                    MTLSize::from(object_threads),
                    MTLSize::from(mesh_threads),
                ),
            );
        }
        Ok(())
    }

    /// Draws tessellation patches with checked optional patch indices.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_patches(
        &self,
        control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        validate_patch_arguments(
            control_points,
            patch_count,
            instance_count,
            patch_indices,
            patch_index_offset,
        )?;
        let selector = sel!(drawPatches:patchStart:patchCount:patchIndexBuffer:patchIndexBufferOffset:instanceCount:baseInstance:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("patch draws are unavailable"));
        }
        // SAFETY: selector availability, counts, overflow, optional index
        // storage, and alignment are checked.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    control_points,
                    patch_start,
                    patch_count,
                    patch_indices.map(|v| &*v.inner),
                    patch_index_offset,
                    instance_count,
                    base_instance,
                ),
            );
        }
        Ok(())
    }

    /// Draws tessellation patches from checked indirect storage.
    pub fn draw_patches_indirect(
        &self,
        control_points: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        indirect_buffer: &Buffer,
        indirect_offset: usize,
    ) -> Result<(), Error> {
        validate_patch_arguments(control_points, 1, 1, patch_indices, patch_index_offset)?;
        checked_buffer_struct::<crate::metal::generated_struct_types::DrawPatchIndirectArguments>(
            indirect_buffer,
            indirect_offset,
            "indirect patch draw",
        )?;
        let selector = sel!(drawPatches:patchIndexBuffer:patchIndexBufferOffset:indirectBuffer:indirectBufferOffset:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("indirect patch draws are unavailable"));
        }
        // SAFETY: selector availability, control-point count, optional patch
        // index alignment, and complete indirect storage are checked.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    control_points,
                    patch_indices.map(|v| &*v.inner),
                    patch_index_offset,
                    &*indirect_buffer.inner,
                    indirect_offset,
                ),
            );
        }
        Ok(())
    }

    /// Draws indexed tessellation patches with checked index storage.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_patches(
        &self,
        control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        control_point_indices: &Buffer,
        control_point_index_offset: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        validate_patch_arguments(
            control_points,
            patch_count,
            instance_count,
            patch_indices,
            patch_index_offset,
        )?;
        let needed = control_points
            .checked_mul(patch_count)
            .and_then(|v| v.checked_mul(4))
            .and_then(|v| control_point_index_offset.checked_add(v))
            .ok_or_else(|| Error::invalid_argument("control-point index range overflows"))?;
        if !control_point_index_offset.is_multiple_of(4) || needed > control_point_indices.length()
        {
            return Err(Error::invalid_argument(
                "control-point index range is invalid",
            ));
        }
        let selector = sel!(drawIndexedPatches:patchStart:patchCount:patchIndexBuffer:patchIndexBufferOffset:controlPointIndexBuffer:controlPointIndexBufferOffset:instanceCount:baseInstance:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("indexed patch draws are unavailable"));
        }
        // SAFETY: selector availability, counts, overflow, index alignment,
        // and complete patch/control-point storage are checked.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    control_points,
                    patch_start,
                    patch_count,
                    patch_indices.map(|v| &*v.inner),
                    patch_index_offset,
                    &*control_point_indices.inner,
                    control_point_index_offset,
                    instance_count,
                    base_instance,
                ),
            );
        }
        Ok(())
    }

    /// Draws indexed tessellation patches from checked indirect storage.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_patches_indirect(
        &self,
        control_points: usize,
        patch_indices: Option<&Buffer>,
        patch_index_offset: usize,
        control_point_indices: &Buffer,
        control_point_index_offset: usize,
        indirect_buffer: &Buffer,
        indirect_offset: usize,
    ) -> Result<(), Error> {
        validate_patch_arguments(control_points, 1, 1, patch_indices, patch_index_offset)?;
        if !control_point_index_offset.is_multiple_of(4)
            || control_point_index_offset >= control_point_indices.length()
        {
            return Err(Error::invalid_argument(
                "control-point index offset is invalid",
            ));
        }
        checked_buffer_struct::<crate::metal::generated_struct_types::DrawPatchIndirectArguments>(
            indirect_buffer,
            indirect_offset,
            "indirect indexed patch draw",
        )?;
        let selector = sel!(drawIndexedPatches:patchIndexBuffer:patchIndexBufferOffset:controlPointIndexBuffer:controlPointIndexBufferOffset:indirectBuffer:indirectBufferOffset:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "indirect indexed patch draws are unavailable",
            ));
        }
        // SAFETY: selector availability, offsets/alignment, control-point
        // count, and complete indirect storage are checked.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    control_points,
                    patch_indices.map(|v| &*v.inner),
                    patch_index_offset,
                    &*control_point_indices.inner,
                    control_point_index_offset,
                    &*indirect_buffer.inner,
                    indirect_offset,
                ),
            );
        }
        Ok(())
    }

    /// Dispatches mesh threads with checked non-zero dimensions.
    pub fn draw_mesh_threads(
        &self,
        grid: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        validate_nonzero_size(grid, "mesh grid")?;
        validate_nonzero_size(object_threads, "object threads")?;
        validate_nonzero_size(mesh_threads, "mesh threads")?;
        let selector = sel!(drawMeshThreads:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("mesh thread draws are unavailable"));
        }
        // SAFETY: selector availability and all dimensions are validated.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    MTLSize::from(grid),
                    MTLSize::from(object_threads),
                    MTLSize::from(mesh_threads),
                ),
            );
        }
        Ok(())
    }

    /// Samples a counter buffer at a checked index.
    pub fn sample_counters(
        &self,
        buffer: &CounterSampleBuffer,
        index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        if index >= buffer.sample_count()? {
            return Err(Error::invalid_argument(
                "counter sample index is out of bounds",
            ));
        }
        let selector = sel!(sampleCountersInBuffer:atSampleIndex:withBarrier:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("render counter sampling is unavailable"));
        }
        // SAFETY: selector availability and sample index are checked; the
        // generated wrapper retains the counter buffer.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (buffer.as_inner(), index, objc2::runtime::Bool::new(barrier)),
            );
        }
        Ok(())
    }

    /// Executes a checked range from an indirect command buffer.
    pub fn execute_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        if range.start > range.end || range.end > buffer.size()? {
            return Err(Error::invalid_argument(
                "indirect command range is out of bounds",
            ));
        }
        let selector = sel!(executeCommandsInBuffer:withRange:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "indirect command execution is unavailable",
            ));
        }
        let raw = NSRange::new(range.start, range.end - range.start);
        // SAFETY: selector availability and execution range against the
        // retained indirect-command buffer size are checked.
        unsafe {
            let _: () = self.inner.send_message(selector, (buffer.as_inner(), raw));
        }
        Ok(())
    }

    /// Executes an indirect-command range read from checked buffer storage.
    pub fn execute_commands_indirect(
        &self,
        commands: &IndirectCommandBuffer,
        range_buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        checked_buffer_struct::<
            crate::metal::generated_struct_types::IndirectCommandBufferExecutionRange,
        >(range_buffer, offset, "indirect command range")?;
        let selector = sel!(executeCommandsInBuffer:indirectBuffer:indirectBufferOffset:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "indirect command ranges are unavailable",
            ));
        }
        // SAFETY: selector availability and complete range structure storage
        // are checked; both wrappers retain their objects.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (commands.as_inner(), &*range_buffer.inner, offset),
            );
        }
        Ok(())
    }

    /// Declares one resource with validated usage and render stages.
    pub fn use_resource(
        &self,
        resource: RenderResource<'_>,
        usage: ResourceUsage,
        stages: Option<RenderStages>,
    ) -> Result<(), Error> {
        if !usage.is_valid() || stages.is_some_and(|value| !value.is_valid()) {
            return Err(Error::invalid_argument(
                "resource usage or render stages contain unknown bits",
            ));
        }
        let selector = if stages.is_some() {
            sel!(useResource:usage:stages:)
        } else {
            sel!(useResource:usage:)
        };
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("render resource usage is unavailable"));
        }
        // SAFETY: selector availability and option-set bits are checked; the
        // safe resource wrapper remains borrowed during the call.
        unsafe {
            if let Some(stages) = stages {
                let _: () = self.inner.send_message(
                    selector,
                    (resource.as_object(), usage.as_raw(), stages.as_raw()),
                );
            } else {
                let _: () = self
                    .inner
                    .send_message(selector, (resource.as_object(), usage.as_raw()));
            }
        }
        Ok(())
    }

    /// Declares multiple resources through individually checked calls.
    pub fn use_resources(
        &self,
        resources: &[RenderResource<'_>],
        usage: ResourceUsage,
        stages: Option<RenderStages>,
    ) -> Result<(), Error> {
        for resource in resources {
            let borrowed = match resource {
                RenderResource::Buffer(v) => RenderResource::Buffer(v),
                RenderResource::Texture(v) => RenderResource::Texture(v),
                RenderResource::AccelerationStructure(v) => {
                    RenderResource::AccelerationStructure(v)
                }
            };
            self.use_resource(borrowed, usage, stages)?;
        }
        Ok(())
    }

    /// Declares one heap for optional validated render stages.
    pub fn use_heap(&self, heap: &Heap, stages: Option<RenderStages>) -> Result<(), Error> {
        if stages.is_some_and(|value| !value.is_valid()) {
            return Err(Error::invalid_argument(
                "render stages contain unknown bits",
            ));
        }
        let selector = if stages.is_some() {
            sel!(useHeap:stages:)
        } else {
            sel!(useHeap:)
        };
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("render heap usage is unavailable"));
        }
        // SAFETY: selector availability and optional stage bits are checked;
        // the generated heap wrapper is retained.
        unsafe {
            if let Some(stages) = stages {
                let _: () = self
                    .inner
                    .send_message(selector, (heap.as_inner(), stages.as_raw()));
            } else {
                let _: () = self.inner.send_message(selector, (heap.as_inner(),));
            }
        }
        Ok(())
    }

    /// Declares multiple heaps through individually checked calls.
    pub fn use_heaps(&self, heaps: &[&Heap], stages: Option<RenderStages>) -> Result<(), Error> {
        for heap in heaps {
            self.use_heap(heap, stages)?;
        }
        Ok(())
    }

    /// Makes later selected render stages wait for a fence.
    pub fn wait_for_fence(&self, fence: &Fence, stages: RenderStages) -> Result<(), Error> {
        self.send_fence(
            fence,
            stages,
            sel!(waitForFence:beforeStages:),
            "render fence waits are unavailable",
        )
    }

    /// Updates a fence after selected render stages.
    pub fn update_fence(&self, fence: &Fence, stages: RenderStages) -> Result<(), Error> {
        self.send_fence(
            fence,
            stages,
            sel!(updateFence:afterStages:),
            "render fence updates are unavailable",
        )
    }

    fn send_fence(
        &self,
        fence: &Fence,
        stages: RenderStages,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if !stages.is_valid() {
            return Err(Error::invalid_argument(
                "render stages contain unknown bits",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: selector availability and stage bits are checked; the fence
        // wrapper remains retained for the synchronous encoding call.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (fence.as_inner(), stages.as_raw()));
        }
        Ok(())
    }

    /// Inserts a validated render-stage memory barrier.
    pub fn memory_barrier(
        &self,
        scope: BarrierScope,
        after: RenderStages,
        before: RenderStages,
    ) -> Result<(), Error> {
        if !scope.is_valid() || !after.is_valid() || !before.is_valid() {
            return Err(Error::invalid_argument(
                "barrier scope or render stages contain unknown bits",
            ));
        }
        let selector = sel!(memoryBarrierWithScope:afterStages:beforeStages:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("render memory barriers are unavailable"));
        }
        // SAFETY: selector availability and all option-set bits are checked.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (scope.as_raw(), after.as_raw(), before.as_raw()));
        }
        Ok(())
    }

    /// Inserts render-stage memory barriers for safely borrowed resources.
    pub fn memory_barriers(
        &self,
        resources: &[RenderResource<'_>],
        after: RenderStages,
        before: RenderStages,
    ) -> Result<(), Error> {
        if resources.is_empty() || !after.is_valid() || !before.is_valid() {
            return Err(Error::invalid_argument(
                "resource barriers require resources and valid render stages",
            ));
        }
        let selector = sel!(memoryBarrierWithResources:count:afterStages:beforeStages:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "render resource barriers are unavailable",
            ));
        }
        for resource in resources {
            let objects = [resource.as_object()];
            // SAFETY: selector availability and stage bits are checked; each
            // one-element pointer array and borrowed resource live for the call.
            unsafe {
                let _: () = self.inner.send_message(
                    selector,
                    (
                        std::ptr::NonNull::from(&objects[0]),
                        objects.len(),
                        after.as_raw(),
                        before.as_raw(),
                    ),
                );
            }
        }
        Ok(())
    }

    /// Sets tile threadgroup memory with checked length, offset, and index.
    pub fn set_threadgroup_memory_length(
        &self,
        length: usize,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "threadgroup memory index must be below 31",
            ));
        }
        let end = offset
            .checked_add(length)
            .ok_or_else(|| Error::invalid_argument("threadgroup memory range overflows"))?;
        if end > self.inner.device().maxThreadgroupMemoryLength() {
            return Err(Error::invalid_argument(
                "threadgroup memory range exceeds the device limit",
            ));
        }
        let selector = sel!(setThreadgroupMemoryLength:offset:atIndex:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("tile threadgroup memory is unavailable"));
        }
        // SAFETY: selector availability, index, overflow, and device memory
        // limit are validated.
        unsafe {
            let _: () = self.inner.send_message(selector, (length, offset, index));
        }
        Ok(())
    }

    /// Sets object threadgroup memory with a checked index and device limit.
    pub fn set_object_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 || length > self.inner.device().maxThreadgroupMemoryLength() {
            return Err(Error::invalid_argument(
                "object threadgroup memory binding exceeds a limit",
            ));
        }
        let selector = sel!(setObjectThreadgroupMemoryLength:atIndex:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "object threadgroup memory is unavailable",
            ));
        }
        // SAFETY: selector availability, index, and device memory limit are checked.
        unsafe {
            let _: () = self.inner.send_message(selector, (length, index));
        }
        Ok(())
    }

    /// Sets a finite non-negative tessellation factor scale.
    pub fn set_tessellation_factor_scale(&self, scale: f32) -> Result<(), Error> {
        if !scale.is_finite() || scale < 0.0 {
            return Err(Error::invalid_argument(
                "tessellation factor scale must be finite and non-negative",
            ));
        }
        let selector = sel!(setTessellationFactorScale:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("tessellation factors are unavailable"));
        }
        // SAFETY: selector availability and finite non-negative scale are checked.
        unsafe {
            let _: () = self.inner.send_message(selector, (scale,));
        }
        Ok(())
    }

    /// Binds a tessellation-factor buffer with checked offset and stride.
    pub fn set_tessellation_factor_buffer(
        &self,
        buffer: &Buffer,
        offset: usize,
        instance_stride: usize,
    ) -> Result<(), Error> {
        if offset >= buffer.length()
            || !offset.is_multiple_of(4)
            || !instance_stride.is_multiple_of(4)
        {
            return Err(Error::invalid_argument(
                "tessellation factor offset and stride must be aligned and in bounds",
            ));
        }
        let selector = sel!(setTessellationFactorBuffer:offset:instanceStride:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "tessellation factor buffers are unavailable",
            ));
        }
        // SAFETY: selector availability and required buffer offset/stride
        // alignment and bounds are checked; the wrapper retains the buffer.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (Some(&*buffer.inner), offset, instance_stride));
        }
        Ok(())
    }

    /// Ends encoding and releases the borrow of the command buffer.
    pub fn end_encoding(mut self) {
        self.inner.endEncoding();
        self.ended = true;
    }
}

impl Drop for RenderCommandEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.inner.endEncoding();
        }
    }
}

/// A compute encoder borrowing its command buffer until `end_encoding`.
pub struct ComputeCommandEncoder<'a> {
    inner: Retained<ProtocolObject<dyn MTLComputeCommandEncoder>>,
    _command_buffer: PhantomData<&'a mut CommandBuffer>,
    ended: bool,
    bound_buffer_lengths: RefCell<[Option<usize>; 31]>,
    current_pipeline: RefCell<Option<ComputePipelineState>>,
    _thread_bound: ThreadBound,
}

/// A safely borrowed Metal resource accepted by compute usage declarations.
pub enum ComputeResource<'a> {
    /// A buffer resource.
    Buffer(&'a Buffer),
    /// A texture resource.
    Texture(&'a Texture),
    /// An acceleration-structure resource.
    AccelerationStructure(&'a AccelerationStructure),
}

impl ComputeResource<'_> {
    fn as_object(&self) -> &objc2::runtime::AnyObject {
        match self {
            Self::Buffer(buffer) => AsRef::<objc2::runtime::AnyObject>::as_ref(&*buffer.inner),
            Self::Texture(texture) => AsRef::<objc2::runtime::AnyObject>::as_ref(&*texture.inner),
            Self::AccelerationStructure(structure) => structure.as_inner(),
        }
    }
}

impl<'a> ComputeCommandEncoder<'a> {
    pub(super) fn new(
        inner: Retained<ProtocolObject<dyn MTLComputeCommandEncoder>>,
        _command_buffer: &'a mut CommandBuffer,
    ) -> Self {
        Self {
            inner,
            _command_buffer: PhantomData,
            ended: false,
            bound_buffer_lengths: RefCell::new([None; 31]),
            current_pipeline: RefCell::new(None),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Binds a compute pipeline.
    pub fn set_pipeline(&self, pipeline: &ComputePipelineState) {
        self.inner.setComputePipelineState(&pipeline.inner);
        *self.current_pipeline.borrow_mut() = Some(pipeline.clone());
    }

    /// Returns the device executing this encoder.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }

    /// Inserts a copied Rust debug signpost.
    pub fn insert_debug_signpost(&self, value: &str) {
        self.inner.insertDebugSignpost(&NSString::from_str(value));
    }

    /// Pushes a copied Rust debug group.
    pub fn push_debug_group(&self, value: &str) {
        self.inner.pushDebugGroup(&NSString::from_str(value));
    }

    /// Pops the current debug group.
    pub fn pop_debug_group(&self) {
        self.inner.popDebugGroup();
    }

    /// Copies compute constant bytes into the command buffer.
    pub fn set_bytes(&self, bytes: &[u8], index: usize) -> Result<(), Error> {
        if bytes.is_empty() || index >= 31 {
            return Err(Error::invalid_argument(
                "compute bytes must be non-empty and binding index below 31",
            ));
        }
        // SAFETY: the non-empty slice yields a non-null pointer valid for its
        // length; Metal copies inline bytes during this call.
        unsafe {
            self.inner.setBytes_length_atIndex(
                std::ptr::NonNull::new_unchecked(bytes.as_ptr().cast_mut().cast()),
                bytes.len(),
                index,
            );
        }
        Ok(())
    }

    /// Binds a compute buffer after checking the offset and binding index.
    pub fn set_buffer(&self, buffer: &Buffer, offset: usize, index: usize) -> Result<(), Error> {
        if index >= 31 || offset > buffer.length() {
            return Err(Error::invalid_argument(
                "compute buffer binding or offset is out of bounds",
            ));
        }
        // SAFETY: the buffer is retained by the caller for the duration of the
        // encoder, and the checked offset is within its allocation.
        unsafe {
            self.inner
                .setBuffer_offset_atIndex(Some(&buffer.inner), offset, index);
        }
        self.bound_buffer_lengths.borrow_mut()[index] = Some(buffer.length());
        Ok(())
    }

    /// Binds a compute buffer with a checked attribute stride.
    pub fn set_buffer_with_stride(
        &self,
        buffer: &Buffer,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        if stride == 0 {
            return Err(Error::invalid_argument(
                "compute buffer stride must be non-zero",
            ));
        }
        if index >= 31 || offset > buffer.length() {
            return Err(Error::invalid_argument(
                "compute buffer binding or offset is out of bounds",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(setBuffer:offset:attributeStride:atIndex:))
        {
            return Err(Error::unsupported("compute buffer strides are unavailable"));
        }
        // SAFETY: selector availability, binding index, buffer range, and the
        // non-zero stride are validated above.
        unsafe {
            self.inner.setBuffer_offset_attributeStride_atIndex(
                &buffer.inner,
                offset,
                stride,
                index,
            )
        };
        self.bound_buffer_lengths.borrow_mut()[index] = Some(buffer.length());
        Ok(())
    }

    /// Updates the offset of a previously bound compute buffer.
    pub fn set_buffer_offset(&self, offset: usize, index: usize) -> Result<(), Error> {
        let length = self
            .bound_buffer_lengths
            .borrow()
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| Error::invalid_argument("no buffer is bound at this index"))?;
        if offset > length {
            return Err(Error::invalid_argument(
                "compute buffer offset is out of bounds",
            ));
        }
        // SAFETY: the index has a tracked live binding and the new offset is
        // within that buffer's allocation.
        unsafe { self.inner.setBufferOffset_atIndex(offset, index) };
        Ok(())
    }

    /// Updates the offset and attribute stride of a previously bound buffer.
    pub fn set_buffer_offset_with_stride(
        &self,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        if stride == 0 {
            return Err(Error::invalid_argument(
                "compute buffer stride must be non-zero",
            ));
        }
        let length = self
            .bound_buffer_lengths
            .borrow()
            .get(index)
            .copied()
            .flatten()
            .ok_or_else(|| Error::invalid_argument("no buffer is bound at this index"))?;
        if offset > length {
            return Err(Error::invalid_argument(
                "compute buffer offset is out of bounds",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(setBufferOffset:attributeStride:atIndex:))
        {
            return Err(Error::unsupported("compute buffer strides are unavailable"));
        }
        // SAFETY: selector availability, a tracked binding, its new offset,
        // and the non-zero stride are validated above.
        unsafe {
            self.inner
                .setBufferOffset_attributeStride_atIndex(offset, stride, index)
        };
        Ok(())
    }

    /// Binds multiple compute buffers using checked individual bindings.
    pub fn set_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(bindings.len())
            .ok_or_else(|| Error::invalid_argument("buffer binding range overflows"))?;
        if end > 31 {
            return Err(Error::invalid_argument("buffer bindings exceed index 30"));
        }
        for (slot, (buffer, offset)) in bindings.iter().enumerate() {
            self.set_buffer(buffer, *offset, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple compute buffers and attribute strides through checked calls.
    pub fn set_buffers_with_strides(
        &self,
        bindings: &[(&Buffer, usize, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(bindings.len())
            .ok_or_else(|| Error::invalid_argument("buffer binding range overflows"))?;
        if end > 31 {
            return Err(Error::invalid_argument("buffer bindings exceed index 30"));
        }
        for (slot, (buffer, offset, stride)) in bindings.iter().enumerate() {
            self.set_buffer_with_stride(buffer, *offset, *stride, start_index + slot)?;
        }
        Ok(())
    }

    /// Copies compute bytes with a checked attribute stride.
    pub fn set_bytes_with_stride(
        &self,
        bytes: &[u8],
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        if bytes.is_empty() || stride == 0 || index >= 31 {
            return Err(Error::invalid_argument(
                "compute bytes and stride must be non-zero and binding index below 31",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(setBytes:length:attributeStride:atIndex:))
        {
            return Err(Error::unsupported("compute byte strides are unavailable"));
        }
        // SAFETY: the selector is present; the non-empty slice remains valid
        // for the call and Metal copies it synchronously.
        unsafe {
            self.inner.setBytes_length_attributeStride_atIndex(
                std::ptr::NonNull::new_unchecked(bytes.as_ptr().cast_mut().cast()),
                bytes.len(),
                stride,
                index,
            )
        };
        Ok(())
    }

    /// Binds or clears a compute texture at a checked index.
    pub fn set_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "texture binding index must be below 31",
            ));
        }
        // SAFETY: the binding index is checked and an input texture remains
        // retained by its safe wrapper during encoding.
        unsafe {
            self.inner
                .setTexture_atIndex(texture.map(|texture| &*texture.inner), index)
        };
        Ok(())
    }

    /// Binds multiple compute textures using checked individual bindings.
    pub fn set_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(textures.len())
            .ok_or_else(|| Error::invalid_argument("texture binding range overflows"))?;
        if end > 31 {
            return Err(Error::invalid_argument("texture bindings exceed index 30"));
        }
        for (slot, texture) in textures.iter().enumerate() {
            self.set_texture(*texture, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds or clears a sampler at a checked index.
    pub fn set_sampler_state(
        &self,
        sampler: Option<&SamplerState>,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "sampler binding index must be below 31",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(setSamplerState:atIndex:))
        {
            return Err(Error::unsupported("compute samplers are unavailable"));
        }
        let sampler = sampler.map(SamplerState::as_inner);
        // SAFETY: selector availability and binding index are checked, and the
        // generated wrapper retains any supplied Metal sampler object.
        unsafe {
            let _: () = msg_send![&*self.inner, setSamplerState: sampler, atIndex: index];
        }
        Ok(())
    }

    /// Binds a sampler with finite ordered LOD clamps.
    pub fn set_sampler_state_with_lod(
        &self,
        sampler: Option<&SamplerState>,
        lod_min: f32,
        lod_max: f32,
        index: usize,
    ) -> Result<(), Error> {
        if index >= 31 || !lod_min.is_finite() || !lod_max.is_finite() || lod_min > lod_max {
            return Err(Error::invalid_argument(
                "sampler index and LOD clamp range are invalid",
            ));
        }
        if !self.inner.respondsToSelector(sel!(
            setSamplerState:lodMinClamp:lodMaxClamp:atIndex:
        )) {
            return Err(Error::unsupported("sampler LOD clamps are unavailable"));
        }
        let sampler = sampler.map(SamplerState::as_inner);
        // SAFETY: selector availability, index, and both finite ordered clamps
        // are checked; the wrapper retains the optional sampler.
        unsafe {
            let _: () = msg_send![&*self.inner,
                setSamplerState: sampler,
                lodMinClamp: lod_min,
                lodMaxClamp: lod_max,
                atIndex: index
            ];
        }
        Ok(())
    }

    /// Binds multiple samplers through checked individual calls.
    pub fn set_sampler_states(
        &self,
        samplers: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(samplers.len())
            .ok_or_else(|| Error::invalid_argument("sampler binding range overflows"))?;
        if end > 31 {
            return Err(Error::invalid_argument("sampler bindings exceed index 30"));
        }
        for (slot, sampler) in samplers.iter().enumerate() {
            self.set_sampler_state(*sampler, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds multiple samplers and LOD ranges through checked individual calls.
    pub fn set_sampler_states_with_lod(
        &self,
        samplers: &[(Option<&SamplerState>, f32, f32)],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(samplers.len())
            .ok_or_else(|| Error::invalid_argument("sampler binding range overflows"))?;
        if end > 31 {
            return Err(Error::invalid_argument("sampler bindings exceed index 30"));
        }
        for (slot, (sampler, lod_min, lod_max)) in samplers.iter().enumerate() {
            self.set_sampler_state_with_lod(*sampler, *lod_min, *lod_max, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds or clears a visible function table at a checked buffer index.
    pub fn set_visible_function_table(
        &self,
        table: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_generated_object(
            table.map(VisibleFunctionTable::as_inner),
            index,
            sel!(setVisibleFunctionTable:atBufferIndex:),
            "visible function tables are unavailable",
        )
    }

    /// Binds visible function tables through checked individual calls.
    pub fn set_visible_function_tables(
        &self,
        tables: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = checked_binding_end(start_index, tables.len())?;
        for (slot, table) in tables.iter().enumerate().take(end - start_index) {
            self.set_visible_function_table(*table, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds or clears an intersection function table at a checked buffer index.
    pub fn set_intersection_function_table(
        &self,
        table: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_generated_object(
            table.map(IntersectionFunctionTable::as_inner),
            index,
            sel!(setIntersectionFunctionTable:atBufferIndex:),
            "intersection function tables are unavailable",
        )
    }

    /// Binds intersection function tables through checked individual calls.
    pub fn set_intersection_function_tables(
        &self,
        tables: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = checked_binding_end(start_index, tables.len())?;
        for (slot, table) in tables.iter().enumerate().take(end - start_index) {
            self.set_intersection_function_table(*table, start_index + slot)?;
        }
        Ok(())
    }

    /// Binds or clears an acceleration structure at a checked buffer index.
    pub fn set_acceleration_structure(
        &self,
        structure: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        self.set_generated_object(
            structure.map(AccelerationStructure::as_inner),
            index,
            sel!(setAccelerationStructure:atBufferIndex:),
            "acceleration structures are unavailable",
        )
    }

    fn set_generated_object(
        &self,
        object: Option<&objc2::runtime::AnyObject>,
        index: usize,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "buffer binding index must be below 31",
            ));
        }
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: each caller supplies a selector with the Objective-C
        // signature `(id, NSUInteger) -> void`; its object wrapper is retained
        // and the common binding index is checked above.
        unsafe {
            let _: () = self.inner.send_message(selector, (object, index));
        }
        Ok(())
    }

    /// Makes subsequent compute work wait for a fence.
    pub fn wait_for_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.send_retained_object(
            fence.as_inner(),
            sel!(waitForFence:),
            "compute fence waits are unavailable",
        )
    }

    /// Updates a fence after prior compute work.
    pub fn update_fence(&self, fence: &Fence) -> Result<(), Error> {
        self.send_retained_object(
            fence.as_inner(),
            sel!(updateFence:),
            "compute fence updates are unavailable",
        )
    }

    /// Declares one heap used by this encoder.
    pub fn use_heap(&self, heap: &Heap) -> Result<(), Error> {
        self.send_retained_object(
            heap.as_inner(),
            sel!(useHeap:),
            "compute heap declarations are unavailable",
        )
    }

    /// Declares multiple heaps through individually checked calls.
    pub fn use_heaps(&self, heaps: &[&Heap]) -> Result<(), Error> {
        for heap in heaps {
            self.use_heap(heap)?;
        }
        Ok(())
    }

    fn send_retained_object(
        &self,
        object: &objc2::runtime::AnyObject,
        selector: objc2::runtime::Sel,
        unavailable: &'static str,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(unavailable));
        }
        // SAFETY: callers supply audited one-object void selectors and all
        // generated wrappers retain framework-created objects.
        unsafe {
            let _: () = self.inner.send_message(selector, (object,));
        }
        Ok(())
    }

    /// Inserts a validated compute memory barrier scope.
    pub fn memory_barrier(&self, scope: BarrierScope) -> Result<(), Error> {
        if !scope.is_valid() {
            return Err(Error::invalid_argument(
                "memory barrier scope contains unknown bits",
            ));
        }
        let selector = sel!(memoryBarrierWithScope:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "compute memory barriers are unavailable",
            ));
        }
        // SAFETY: selector availability and the option-set bits are validated;
        // BarrierScope is ABI-compatible with NSUInteger.
        unsafe {
            let _: () = self.inner.send_message(selector, (scope.as_raw(),));
        }
        Ok(())
    }

    /// Inserts memory barriers for safely borrowed resources.
    pub fn memory_barriers(&self, resources: &[ComputeResource<'_>]) -> Result<(), Error> {
        let selector = sel!(memoryBarrierWithResources:count:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "compute resource barriers are unavailable",
            ));
        }
        for resource in resources {
            let object = resource.as_object();
            // SAFETY: selector availability is checked; a one-element object
            // pointer array remains live for the synchronous encoding call.
            unsafe {
                let objects = [object];
                let _: () = self.inner.send_message(
                    selector,
                    (std::ptr::NonNull::from(&objects[0]), objects.len()),
                );
            }
        }
        Ok(())
    }

    /// Declares one safely borrowed resource and validated usage flags.
    pub fn use_resource(
        &self,
        resource: ComputeResource<'_>,
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        if !usage.is_valid() {
            return Err(Error::invalid_argument(
                "resource usage contains unknown bits",
            ));
        }
        let selector = sel!(useResource:usage:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("compute resource usage is unavailable"));
        }
        // SAFETY: selector availability and option bits are checked; the
        // borrowed safe wrapper keeps the resource alive for this call.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (resource.as_object(), usage.as_raw()));
        }
        Ok(())
    }

    /// Declares multiple resources through individually validated calls.
    pub fn use_resources(
        &self,
        resources: &[ComputeResource<'_>],
        usage: ResourceUsage,
    ) -> Result<(), Error> {
        for resource in resources {
            let borrowed = match resource {
                ComputeResource::Buffer(value) => ComputeResource::Buffer(value),
                ComputeResource::Texture(value) => ComputeResource::Texture(value),
                ComputeResource::AccelerationStructure(value) => {
                    ComputeResource::AccelerationStructure(value)
                }
            };
            self.use_resource(borrowed, usage)?;
        }
        Ok(())
    }

    /// Samples a counter buffer at a checked sample index.
    pub fn sample_counters(
        &self,
        buffer: &CounterSampleBuffer,
        sample_index: usize,
        barrier: bool,
    ) -> Result<(), Error> {
        if sample_index >= buffer.sample_count()? {
            return Err(Error::invalid_argument(
                "counter sample index is out of bounds",
            ));
        }
        let selector = sel!(sampleCountersInBuffer:atSampleIndex:withBarrier:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "compute counter sampling is unavailable",
            ));
        }
        // SAFETY: selector availability and sample bounds are checked; the
        // generated wrapper retains the counter buffer.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (
                    buffer.as_inner(),
                    sample_index,
                    objc2::runtime::Bool::new(barrier),
                ),
            );
        }
        Ok(())
    }

    /// Executes a checked range of commands from an indirect command buffer.
    pub fn execute_commands(
        &self,
        buffer: &IndirectCommandBuffer,
        execution_range: std::ops::Range<usize>,
    ) -> Result<(), Error> {
        if execution_range.start > execution_range.end || execution_range.end > buffer.size()? {
            return Err(Error::invalid_argument(
                "indirect command execution range is out of bounds",
            ));
        }
        let selector = sel!(executeCommandsInBuffer:withRange:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "indirect command execution is unavailable",
            ));
        }
        let range = NSRange::new(
            execution_range.start,
            execution_range.end - execution_range.start,
        );
        // SAFETY: selector availability and the execution range against the
        // retained indirect command buffer's size are checked.
        unsafe {
            let _: () = self
                .inner
                .send_message(selector, (buffer.as_inner(), range));
        }
        Ok(())
    }

    /// Reads an indirect execution range from a checked buffer offset.
    pub fn execute_commands_indirect(
        &self,
        commands: &IndirectCommandBuffer,
        range_buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        let end = offset
            .checked_add(std::mem::size_of::<
                crate::metal::generated_struct_types::IndirectCommandBufferExecutionRange,
            >())
            .ok_or_else(|| Error::invalid_argument("indirect command range overflows"))?;
        if end > range_buffer.length() {
            return Err(Error::invalid_argument(
                "indirect command range is out of bounds",
            ));
        }
        let selector = sel!(executeCommandsInBuffer:indirectBuffer:indirectBufferOffset:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported(
                "indirect command ranges are unavailable",
            ));
        }
        // SAFETY: selector availability and storage for the complete execution
        // range structure are checked; both wrappers retain their objects.
        unsafe {
            let _: () = self.inner.send_message(
                selector,
                (commands.as_inner(), &*range_buffer.inner, offset),
            );
        }
        Ok(())
    }

    /// Returns the dispatch type selected for this encoder.
    #[must_use]
    pub fn dispatch_type(&self) -> DispatchType {
        DispatchType::from_system_raw(self.inner.dispatchType().0)
    }

    /// Sets a checked imageblock size.
    pub fn set_imageblock_size(&self, width: usize, height: usize) -> Result<(), Error> {
        let selector = sel!(setImageblockWidth:height:);
        if !self.inner.respondsToSelector(selector) {
            return Err(Error::unsupported("imageblock sizing is unavailable"));
        }
        let pipeline = self.current_pipeline.borrow();
        let pipeline = pipeline.as_ref().ok_or_else(|| {
            Error::invalid_argument(
                "a compute pipeline must be bound before validating imageblock dimensions",
            )
        })?;
        let device_limit = self.device_threadgroup_limit();
        validate_dispatch_limits(
            Size::new(1, 1, 1),
            Size::new(width, height, 1),
            device_limit,
            pipeline.max_total_threads_per_threadgroup(),
        )?;
        if !pipeline
            .inner
            .respondsToSelector(sel!(imageblockMemoryLengthForDimensions:))
        {
            return Err(Error::unsupported(
                "imageblock memory requirements are unavailable",
            ));
        }
        // SAFETY: selector availability is checked and MTLSize is the exact
        // declared argument and return ABI for the pipeline query.
        let required_memory = unsafe {
            pipeline
                .inner
                .imageblockMemoryLengthForDimensions(Size::new(width, height, 1).into())
        };
        if required_memory > self.inner.device().maxThreadgroupMemoryLength() {
            return Err(Error::invalid_argument(
                "imageblock memory requirement exceeds the device threadgroup-memory limit",
            ));
        }
        // SAFETY: encoder selector availability, pipeline support, dimensions,
        // total threads, and imageblock memory requirements are validated.
        unsafe {
            let _: () = self.inner.send_message(selector, (width, height));
        }
        Ok(())
    }

    fn device_threadgroup_limit(&self) -> Size {
        let limit = self.inner.device().maxThreadsPerThreadgroup();
        Size::new(limit.width, limit.height, limit.depth)
    }

    fn pipeline_thread_limit(&self) -> usize {
        self.current_pipeline.borrow().as_ref().map_or_else(
            || self.device_threadgroup_limit().width,
            ComputePipelineState::max_total_threads_per_threadgroup,
        )
    }

    /// Sets a checked stage-in region.
    pub fn set_stage_in_region(&self, region: Region) -> Result<(), Error> {
        if region.size.width == 0 || region.size.height == 0 || region.size.depth == 0 {
            return Err(Error::invalid_argument("stage-in region must be non-empty"));
        }
        self.inner.setStageInRegion(region.into());
        Ok(())
    }

    /// Reads a stage-in region from a checked indirect-buffer range.
    pub fn set_stage_in_region_indirect(
        &self,
        buffer: &Buffer,
        offset: usize,
    ) -> Result<(), Error> {
        let end = offset
            .checked_add(std::mem::size_of::<
                crate::metal::generated_struct_types::StageInRegionIndirectArguments,
            >())
            .ok_or_else(|| Error::invalid_argument("stage-in argument range overflows"))?;
        if end > buffer.length() {
            return Err(Error::invalid_argument(
                "stage-in argument range is out of bounds",
            ));
        }
        if !self.inner.respondsToSelector(sel!(
            setStageInRegionWithIndirectBuffer:indirectBufferOffset:
        )) {
            return Err(Error::unsupported(
                "indirect stage-in regions are unavailable",
            ));
        }
        // SAFETY: selector availability and storage for the complete indirect
        // argument structure are validated above.
        unsafe {
            self.inner
                .setStageInRegionWithIndirectBuffer_indirectBufferOffset(&buffer.inner, offset)
        };
        Ok(())
    }

    /// Sets threadgroup memory length at a checked binding index.
    pub fn set_threadgroup_memory_length(&self, length: usize, index: usize) -> Result<(), Error> {
        if index >= 31 {
            return Err(Error::invalid_argument(
                "threadgroup memory index must be below 31",
            ));
        }
        if length > self.inner.device().maxThreadgroupMemoryLength() {
            return Err(Error::invalid_argument(
                "threadgroup memory length exceeds the device limit",
            ));
        }
        // SAFETY: the binding index and device memory limit were checked.
        unsafe { self.inner.setThreadgroupMemoryLength_atIndex(length, index) };
        Ok(())
    }

    /// Dispatches threadgroups using checked indirect-buffer storage.
    pub fn dispatch_threadgroups_indirect(
        &self,
        buffer: &Buffer,
        offset: usize,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        validate_dispatch_limits(
            Size::new(1, 1, 1),
            threads_per_group,
            self.device_threadgroup_limit(),
            self.pipeline_thread_limit(),
        )?;
        let end = offset
            .checked_add(std::mem::size_of::<
                crate::metal::generated_struct_types::DispatchThreadgroupsIndirectArguments,
            >())
            .ok_or_else(|| Error::invalid_argument("indirect dispatch range overflows"))?;
        if end > buffer.length() {
            return Err(Error::invalid_argument(
                "indirect dispatch range is out of bounds",
            ));
        }
        // SAFETY: the retained buffer contains enough bytes for Metal's
        // indirect dispatch argument structure and group dimensions are valid.
        unsafe {
            self.inner
                .dispatchThreadgroupsWithIndirectBuffer_indirectBufferOffset_threadsPerThreadgroup(
                    &buffer.inner,
                    offset,
                    threads_per_group.into(),
                )
        };
        Ok(())
    }

    /// Dispatches a one-dimensional grid.
    pub fn dispatch_1d(&self, threads: usize, threads_per_group: usize) -> Result<(), Error> {
        validate_dispatch_limits(
            Size::new(threads, 1, 1),
            Size::new(threads_per_group, 1, 1),
            self.device_threadgroup_limit(),
            self.pipeline_thread_limit(),
        )?;
        self.inner.dispatchThreads_threadsPerThreadgroup(
            Size::new(threads, 1, 1).into(),
            Size::new(threads_per_group, 1, 1).into(),
        );
        Ok(())
    }

    /// Dispatches a checked multidimensional thread grid.
    pub fn dispatch_threads(&self, threads: Size, threads_per_group: Size) -> Result<(), Error> {
        validate_dispatch_limits(
            threads,
            threads_per_group,
            self.device_threadgroup_limit(),
            self.pipeline_thread_limit(),
        )?;
        self.inner
            .dispatchThreads_threadsPerThreadgroup(threads.into(), threads_per_group.into());
        Ok(())
    }

    /// Dispatches a checked multidimensional threadgroup grid.
    pub fn dispatch_threadgroups(
        &self,
        threadgroups: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        validate_dispatch_limits(
            threadgroups,
            threads_per_group,
            self.device_threadgroup_limit(),
            self.pipeline_thread_limit(),
        )?;
        self.inner.dispatchThreadgroups_threadsPerThreadgroup(
            threadgroups.into(),
            threads_per_group.into(),
        );
        Ok(())
    }

    /// Ends encoding and releases the borrow of the command buffer.
    pub fn end_encoding(mut self) {
        self.inner.endEncoding();
        self.ended = true;
    }
}

fn validate_dispatch_limits(
    grid: Size,
    group: Size,
    device_limit: Size,
    max_total_threads: usize,
) -> Result<(), Error> {
    if grid.width == 0
        || grid.height == 0
        || grid.depth == 0
        || group.width == 0
        || group.height == 0
        || group.depth == 0
    {
        return Err(Error::invalid_argument(
            "dispatch grid and threadgroup dimensions must be non-zero",
        ));
    }
    checked_size_product(grid, "dispatch grid")?;
    let group_total = checked_size_product(group, "threadgroup")?;
    if group.width > device_limit.width
        || group.height > device_limit.height
        || group.depth > device_limit.depth
    {
        return Err(Error::invalid_argument(
            "threadgroup dimensions exceed the device per-dimension limits",
        ));
    }
    if max_total_threads == 0 || group_total > max_total_threads {
        return Err(Error::invalid_argument(
            "threadgroup thread count exceeds the active pipeline or device limit",
        ));
    }
    Ok(())
}

fn checked_size_product(size: Size, label: &str) -> Result<usize, Error> {
    size.width
        .checked_mul(size.height)
        .and_then(|value| value.checked_mul(size.depth))
        .ok_or_else(|| Error::invalid_argument(format!("{label} dimensions overflow")))
}

fn checked_binding_end(start_index: usize, count: usize) -> Result<usize, Error> {
    let end = start_index
        .checked_add(count)
        .ok_or_else(|| Error::invalid_argument("binding range overflows"))?;
    if end > 31 {
        return Err(Error::invalid_argument("bindings exceed index 30"));
    }
    Ok(end)
}

fn checked_sampler_end(start_index: usize, count: usize) -> Result<usize, Error> {
    let end = start_index
        .checked_add(count)
        .ok_or_else(|| Error::invalid_argument("sampler binding range overflows"))?;
    if end > 16 {
        return Err(Error::invalid_argument("sampler bindings exceed index 15"));
    }
    Ok(end)
}

fn checked_buffer_struct<T>(buffer: &Buffer, offset: usize, label: &str) -> Result<(), Error> {
    let end = offset
        .checked_add(std::mem::size_of::<T>())
        .ok_or_else(|| Error::invalid_argument(format!("{label} range overflows")))?;
    if end > buffer.length() {
        return Err(Error::invalid_argument(format!(
            "{label} range is out of bounds"
        )));
    }
    Ok(())
}

fn validate_nonzero_size(size: Size, label: &str) -> Result<(), Error> {
    if size.width == 0 || size.height == 0 || size.depth == 0 {
        return Err(Error::invalid_argument(format!("{label} must be non-zero")));
    }
    Ok(())
}

fn validate_patch_arguments(
    control_points: usize,
    patch_count: usize,
    instance_count: usize,
    patch_indices: Option<&Buffer>,
    patch_index_offset: usize,
) -> Result<(), Error> {
    if control_points == 0 || control_points > 32 || patch_count == 0 || instance_count == 0 {
        return Err(Error::invalid_argument(
            "patch control-point, patch, and instance counts are invalid",
        ));
    }
    if let Some(buffer) = patch_indices {
        let end = patch_count
            .checked_mul(4)
            .and_then(|value| patch_index_offset.checked_add(value))
            .ok_or_else(|| Error::invalid_argument("patch index range overflows"))?;
        if !patch_index_offset.is_multiple_of(4) || end > buffer.length() {
            return Err(Error::invalid_argument("patch index range is invalid"));
        }
    } else if patch_index_offset != 0 {
        return Err(Error::invalid_argument(
            "patch index offset requires a patch index buffer",
        ));
    }
    Ok(())
}

impl Drop for ComputeCommandEncoder<'_> {
    fn drop(&mut self) {
        if !self.ended {
            self.inner.endEncoding();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Size, validate_dispatch_limits};

    const DEVICE_LIMIT: Size = Size::new(1024, 1024, 64);

    #[test]
    fn dispatch_validation_accepts_checked_group() {
        assert!(
            validate_dispatch_limits(
                Size::new(4096, 1, 1),
                Size::new(32, 8, 1),
                DEVICE_LIMIT,
                1024,
            )
            .is_ok()
        );
    }

    #[test]
    fn dispatch_validation_rejects_zero_and_dimension_limit() {
        assert!(
            validate_dispatch_limits(Size::new(1, 1, 1), Size::new(0, 1, 1), DEVICE_LIMIT, 1024,)
                .is_err()
        );
        assert!(
            validate_dispatch_limits(
                Size::new(1, 1, 1),
                Size::new(1, 1025, 1),
                DEVICE_LIMIT,
                2048,
            )
            .is_err()
        );
    }

    #[test]
    fn dispatch_validation_rejects_product_limit_and_overflow() {
        assert!(
            validate_dispatch_limits(Size::new(1, 1, 1), Size::new(33, 32, 1), DEVICE_LIMIT, 1024,)
                .is_err()
        );
        assert!(
            validate_dispatch_limits(
                Size::new(usize::MAX, 2, 1),
                Size::new(1, 1, 1),
                DEVICE_LIMIT,
                1024,
            )
            .is_err()
        );
    }
}
