//! Audited argument-buffer and indirect-command encoding boundaries.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    AccelerationStructure, ArgumentEncoder, DepthStencilState, IndirectCommandBuffer,
    IndirectComputeCommand, IndirectRenderCommand, IntersectionFunctionTable, SamplerState,
    VisibleFunctionTable,
};
use crate::metal::generated_struct_types::ResourceID;
use crate::metal::generated_value_types::{
    CullMode, DepthClipMode, IndexType, TriangleFillMode, Winding,
};
use crate::metal::{
    Buffer, ComputePipelineState, PrimitiveType, Region, RenderPipelineState, Size, StorageMode,
    Texture,
};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
use objc2_foundation::NSRange;
use objc2_metal::MTLBuffer as _;
use std::marker::PhantomData;

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    name: &str,
) -> Result<(), Error> {
    // SAFETY: every Objective-C object implements `respondsToSelector:` and
    // both the selector argument and Boolean return have stable runtime ABIs.
    let available: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if available {
        Ok(())
    } else {
        Err(Error::unsupported(format!("{name} is unavailable")))
    }
}

fn checked_end(offset: usize, length: usize, limit: usize, what: &str) -> Result<(), Error> {
    let end = offset
        .checked_add(length)
        .ok_or_else(|| Error::invalid_argument(format!("{what} range overflow")))?;
    if end > limit {
        return Err(Error::invalid_argument(format!(
            "{what} range is out of bounds"
        )));
    }
    Ok(())
}

fn check_buffer_offset(buffer: Option<&Buffer>, offset: usize, what: &str) -> Result<(), Error> {
    match buffer {
        Some(buffer) if offset <= buffer.length() => Ok(()),
        Some(_) => Err(Error::invalid_argument(format!(
            "{what} offset is out of bounds"
        ))),
        None if offset == 0 => Ok(()),
        None => Err(Error::invalid_argument(format!(
            "{what} offset must be zero for an unbound buffer"
        ))),
    }
}

fn check_size(size: Size, what: &str) -> Result<(), Error> {
    if size.width == 0 || size.height == 0 || size.depth == 0 {
        Err(Error::invalid_argument(format!(
            "{what} dimensions must be non-zero"
        )))
    } else {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn check_patch_draw_ranges(
    number_of_patch_control_points: usize,
    patch_start: usize,
    patch_count: usize,
    patch_index_buffer: Option<&Buffer>,
    patch_index_buffer_offset: usize,
    control_point_index_buffer: Option<&Buffer>,
    control_point_index_buffer_offset: usize,
    instance_count: usize,
    base_instance: usize,
    tessellation_factor_buffer: &Buffer,
    tessellation_factor_buffer_offset: usize,
    instance_stride: usize,
) -> Result<(), Error> {
    if number_of_patch_control_points == 0 {
        return Err(Error::invalid_argument(
            "patch control-point count must be non-zero",
        ));
    }
    let patch_end = patch_start
        .checked_add(patch_count)
        .ok_or_else(|| Error::invalid_argument("patch range overflow"))?;
    base_instance
        .checked_add(instance_count)
        .ok_or_else(|| Error::invalid_argument("patch instance range overflow"))?;
    if !patch_index_buffer_offset.is_multiple_of(4)
        || !control_point_index_buffer_offset.is_multiple_of(4)
    {
        return Err(Error::invalid_argument(
            "patch index-buffer offsets must be 4-byte aligned",
        ));
    }
    if !tessellation_factor_buffer_offset.is_multiple_of(2) || !instance_stride.is_multiple_of(2) {
        return Err(Error::invalid_argument(
            "tessellation-factor offset and stride must be 2-byte aligned",
        ));
    }
    if let Some(buffer) = patch_index_buffer {
        let bytes = patch_end
            .checked_mul(4)
            .ok_or_else(|| Error::invalid_argument("patch-index byte range overflow"))?;
        checked_end(
            patch_index_buffer_offset,
            bytes,
            buffer.length(),
            "patch-index buffer",
        )?;
    } else if patch_index_buffer_offset != 0 {
        return Err(Error::invalid_argument(
            "patch-index offset must be zero without a patch-index buffer",
        ));
    }
    if let Some(buffer) = control_point_index_buffer {
        let indices = patch_end
            .checked_mul(number_of_patch_control_points)
            .ok_or_else(|| Error::invalid_argument("control-point index count overflow"))?;
        let bytes = indices
            .checked_mul(4)
            .ok_or_else(|| Error::invalid_argument("control-point byte range overflow"))?;
        checked_end(
            control_point_index_buffer_offset,
            bytes,
            buffer.length(),
            "control-point index buffer",
        )?;
    }
    // Quad patches have the largest Metal tessellation factor record: six
    // half-floats (12 bytes). Checking that footprint is conservative for
    // both supported patch topologies and does not trust pipeline metadata.
    let factors_per_instance = patch_end
        .checked_mul(12)
        .ok_or_else(|| Error::invalid_argument("tessellation-factor size overflow"))?;
    if instance_count > 1 && instance_stride != 0 && instance_stride < factors_per_instance {
        return Err(Error::invalid_argument(
            "tessellation-factor instance stride is too small",
        ));
    }
    let preceding_instances = instance_count.saturating_sub(1);
    let instance_bytes = if instance_stride == 0 {
        0
    } else {
        preceding_instances
            .checked_mul(instance_stride)
            .ok_or_else(|| Error::invalid_argument("tessellation instance range overflow"))?
    };
    let required = if instance_count == 0 {
        0
    } else {
        instance_bytes
            .checked_add(factors_per_instance)
            .ok_or_else(|| Error::invalid_argument("tessellation-factor range overflow"))?
    };
    checked_end(
        tessellation_factor_buffer_offset,
        required,
        tessellation_factor_buffer.length(),
        "tessellation-factor buffer",
    )
}

/// A render-command view that cannot outlive the indirect command buffer that owns it.
pub struct IndirectRenderCommandRef<'a> {
    inner: IndirectRenderCommand,
    _indirect_command_buffer: PhantomData<&'a IndirectCommandBuffer>,
}

impl IndirectRenderCommandRef<'_> {
    /// Resets this indirect render command.
    pub fn reset(&self) -> Result<(), Error> {
        self.inner.reset()
    }

    /// Inserts a barrier before later operations in this indirect render command.
    pub fn set_barrier(&self) -> Result<(), Error> {
        self.inner.set_barrier()
    }

    /// Removes the barrier configured on this indirect render command.
    pub fn clear_barrier(&self) -> Result<(), Error> {
        self.inner.clear_barrier()
    }

    /// Encodes a conservatively bounds-checked non-indexed patch draw.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_patches(
        &self,
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&Buffer>,
        patch_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    ) -> Result<(), Error> {
        self.inner.draw_patches(
            number_of_patch_control_points,
            patch_start,
            patch_count,
            patch_index_buffer,
            patch_index_buffer_offset,
            instance_count,
            base_instance,
            tessellation_factor_buffer,
            tessellation_factor_buffer_offset,
            instance_stride,
        )
    }

    /// Encodes a conservatively bounds-checked indexed patch draw.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_patches(
        &self,
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&Buffer>,
        patch_index_buffer_offset: usize,
        control_point_index_buffer: &Buffer,
        control_point_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    ) -> Result<(), Error> {
        self.inner.draw_indexed_patches(
            number_of_patch_control_points,
            patch_start,
            patch_count,
            patch_index_buffer,
            patch_index_buffer_offset,
            control_point_index_buffer,
            control_point_index_buffer_offset,
            instance_count,
            base_instance,
            tessellation_factor_buffer,
            tessellation_factor_buffer_offset,
            instance_stride,
        )
    }
}

/// A compute-command view that cannot outlive the indirect command buffer that owns it.
pub struct IndirectComputeCommandRef<'a> {
    inner: IndirectComputeCommand,
    _indirect_command_buffer: PhantomData<&'a IndirectCommandBuffer>,
}

impl IndirectComputeCommandRef<'_> {
    /// Resets this indirect compute command.
    pub fn reset(&self) -> Result<(), Error> {
        self.inner.reset()
    }

    /// Inserts a barrier before later operations in this indirect compute command.
    pub fn set_barrier(&self) -> Result<(), Error> {
        self.inner.set_barrier()
    }

    /// Removes the barrier configured on this indirect compute command.
    pub fn clear_barrier(&self) -> Result<(), Error> {
        self.inner.clear_barrier()
    }
}

impl IndirectCommandBuffer {
    /// Returns the stable GPU resource identifier after an availability check.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        require_selector(
            self.as_inner(),
            sel!(gpuResourceID),
            "MTL::IndirectCommandBuffer::gpuResourceID",
        )?;
        // SAFETY: selector availability was checked and MTLResourceID is the
        // SDK-declared one-u64 C structure returned by this selector.
        let raw: objc2_metal::MTLResourceID = unsafe { msg_send![self.as_inner(), gpuResourceID] };
        // SAFETY: both structures are repr(C) one-u64 values with identical ABI.
        let value = unsafe { std::mem::transmute::<objc2_metal::MTLResourceID, u64>(raw) };
        Ok(ResourceID { _impl: value })
    }

    /// Runs `body` with the checked render command at `command_index`.
    ///
    /// The higher-ranked closure prevents the command view from escaping the
    /// lifetime of this indirect command buffer.
    pub fn with_indirect_render_command<R>(
        &self,
        command_index: usize,
        body: impl for<'command> FnOnce(&IndirectRenderCommandRef<'command>) -> R,
    ) -> Result<R, Error> {
        let size = self.size()?;
        if command_index >= size {
            return Err(Error::invalid_argument(
                "indirect render command index is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(indirectRenderCommandAtIndex:),
            "MTL::IndirectCommandBuffer::indirectRenderCommand",
        )?;
        // SAFETY: selector availability and command index bounds were checked;
        // the retained result remains privately lifetime-bound to this ICB.
        let inner: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), indirectRenderCommandAtIndex: command_index] };
        let inner = inner
            .map(IndirectRenderCommand::from_inner)
            .ok_or_else(|| Error::unsupported("Metal did not return an indirect render command"))?;
        Ok(body(&IndirectRenderCommandRef {
            inner,
            _indirect_command_buffer: PhantomData,
        }))
    }

    /// Runs `body` with the checked compute command at `command_index`.
    ///
    /// The higher-ranked closure prevents the command view from escaping the
    /// lifetime of this indirect command buffer.
    pub fn with_indirect_compute_command<R>(
        &self,
        command_index: usize,
        body: impl for<'command> FnOnce(&IndirectComputeCommandRef<'command>) -> R,
    ) -> Result<R, Error> {
        let size = self.size()?;
        if command_index >= size {
            return Err(Error::invalid_argument(
                "indirect compute command index is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(indirectComputeCommandAtIndex:),
            "MTL::IndirectCommandBuffer::indirectComputeCommand",
        )?;
        // SAFETY: selector availability and command index bounds were checked;
        // the retained result remains privately lifetime-bound to this ICB.
        let inner: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), indirectComputeCommandAtIndex: command_index] };
        let inner = inner
            .map(IndirectComputeCommand::from_inner)
            .ok_or_else(|| {
                Error::unsupported("Metal did not return an indirect compute command")
            })?;
        Ok(body(&IndirectComputeCommandRef {
            inner,
            _indirect_command_buffer: PhantomData,
        }))
    }

    /// Resets a checked half-open range of commands.
    pub fn reset_commands(&self, range: std::ops::Range<usize>) -> Result<(), Error> {
        if range.start > range.end || range.end > self.size()? {
            return Err(Error::invalid_argument(
                "indirect command reset range is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(resetWithRange:),
            "MTL::IndirectCommandBuffer::reset",
        )?;
        // SAFETY: selector availability and the complete NSRange were checked
        // against the indirect command buffer's command count.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), resetWithRange: NSRange::new(range.start, range.len())];
        }
        Ok(())
    }
}

impl ArgumentEncoder {
    /// Creates a nested encoder for an argument-buffer member.
    pub fn new_argument_encoder(&self, index: usize) -> Result<Self, Error> {
        require_selector(
            self.as_inner(),
            sel!(newArgumentEncoderForBufferAtIndex:),
            "MTL::ArgumentEncoder::newArgumentEncoder",
        )?;
        // SAFETY: selector availability and the retained-return convention of
        // `newArgumentEncoderForBufferAtIndex:` are declared by Metal.
        let inner: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), newArgumentEncoderForBufferAtIndex: index] };
        inner
            .map(Self::from_inner)
            .ok_or_else(|| Error::unsupported("Metal could not create a nested argument encoder"))
    }

    /// Selects a checked buffer region as the argument encoder's destination.
    pub fn set_argument_buffer(&self, buffer: &Buffer, offset: usize) -> Result<(), Error> {
        if offset >= buffer.length() {
            return Err(Error::invalid_argument(
                "argument-buffer offset is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setArgumentBuffer:offset:),
            "MTL::ArgumentEncoder::setArgumentBuffer",
        )?;
        // SAFETY: the selector is available and the retained buffer contains
        // the checked destination offset.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setArgumentBuffer: &*buffer.inner, offset: offset];
        }
        Ok(())
    }

    /// Selects one checked array element in a buffer-backed argument array.
    pub fn set_argument_buffer_element(
        &self,
        buffer: &Buffer,
        start_offset: usize,
        array_element: usize,
    ) -> Result<(), Error> {
        if start_offset >= buffer.length() {
            return Err(Error::invalid_argument(
                "argument-buffer start offset is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setArgumentBuffer:startOffset:arrayElement:),
            "MTL::ArgumentEncoder::setArgumentBuffer(array element)",
        )?;
        // SAFETY: selector availability and the checked buffer start offset
        // satisfy the Objective-C method contract; Metal validates the member layout.
        unsafe {
            let _: () = msg_send![self.as_inner(), setArgumentBuffer: &*buffer.inner, startOffset: start_offset, arrayElement: array_element];
        }
        Ok(())
    }

    /// Builds initialized bytes in a scoped slice and writes them to a verified
    /// constant-data member of the currently selected argument buffer.
    pub fn with_constant_data<R>(
        &self,
        index: usize,
        argument_buffer: &Buffer,
        argument_buffer_offset: usize,
        member_range: std::ops::Range<usize>,
        body: impl for<'data> FnOnce(&'data mut [u8]) -> R,
    ) -> Result<R, Error> {
        if member_range.start > member_range.end {
            return Err(Error::invalid_argument(
                "constant-data member range is reversed",
            ));
        }
        let absolute_start = argument_buffer_offset
            .checked_add(member_range.start)
            .ok_or_else(|| Error::invalid_argument("constant-data offset overflow"))?;
        checked_end(
            absolute_start,
            member_range.len(),
            argument_buffer.length(),
            "constant-data member",
        )?;
        require_selector(
            self.as_inner(),
            sel!(constantDataAtIndex:),
            "MTL::ArgumentEncoder::constantData",
        )?;
        if matches!(
            argument_buffer.storage_mode(),
            StorageMode::Private | StorageMode::Memoryless
        ) {
            return Err(Error::unsupported(
                "constant data requires a CPU-visible argument buffer",
            ));
        }
        let base = argument_buffer.inner.contents().as_ptr().cast::<u8>();
        // SAFETY: selector availability was checked. The pointer is used only
        // as an address identity and is never dereferenced here.
        let native: *mut u8 = unsafe { msg_send![self.as_inner(), constantDataAtIndex: index] };
        // SAFETY: `absolute_start` was checked within the retained allocation;
        // this pointer is used only for address comparison.
        let expected = unsafe { base.add(absolute_start) };
        if native != expected {
            return Err(Error::invalid_argument(
                "constant-data layout does not match the selected argument member",
            ));
        }
        let mut bytes = vec![0_u8; member_range.len()];
        let result = body(&mut bytes);
        argument_buffer.write(absolute_start, &bytes)?;
        Ok(result)
    }

    /// Binds an optional buffer with a checked byte offset.
    pub fn set_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        check_buffer_offset(buffer, offset, "argument buffer binding")?;
        require_selector(
            self.as_inner(),
            sel!(setBuffer:offset:atIndex:),
            "MTL::ArgumentEncoder::setBuffer",
        )?;
        let object = buffer.map(|value| &*value.inner);
        // SAFETY: selector availability was checked and every non-null buffer
        // offset is inside the retained allocation.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setBuffer: object, offset: offset, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's parallel buffer/offset arrays.
    pub fn set_buffers(
        &self,
        bindings: &[(Option<&Buffer>, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index
            .checked_add(bindings.len())
            .ok_or_else(|| Error::invalid_argument("argument buffer index range overflow"))?;
        for (relative, (buffer, offset)) in bindings.iter().enumerate() {
            self.set_buffer(*buffer, *offset, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional texture.
    pub fn set_texture(&self, texture: Option<&Texture>, index: usize) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setTexture:atIndex:),
            "MTL::ArgumentEncoder::setTexture",
        )?;
        let object = texture.map(|value| &*value.inner);
        // SAFETY: selector availability was checked and the optional texture
        // protocol object is retained for the duration of the message.
        unsafe {
            let _: () = msg_send![self.as_inner(), setTexture: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's texture pointer array.
    pub fn set_textures(
        &self,
        textures: &[Option<&Texture>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index
            .checked_add(textures.len())
            .ok_or_else(|| Error::invalid_argument("argument texture index range overflow"))?;
        for (relative, texture) in textures.iter().enumerate() {
            self.set_texture(*texture, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional acceleration structure.
    pub fn set_acceleration_structure(
        &self,
        value: Option<&AccelerationStructure>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setAccelerationStructure:atIndex:),
            "MTL::ArgumentEncoder::setAccelerationStructure",
        )?;
        let object = value.map(AccelerationStructure::as_inner);
        // SAFETY: selector availability was checked and the optional object is retained.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setAccelerationStructure: object, atIndex: index];
        }
        Ok(())
    }

    /// Binds an optional compute pipeline state.
    pub fn set_compute_pipeline_state(
        &self,
        value: Option<&ComputePipelineState>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setComputePipelineState:atIndex:),
            "MTL::ArgumentEncoder::setComputePipelineState",
        )?;
        let object = value.map(|value| &*value.inner);
        // SAFETY: selector availability was checked and the optional protocol object is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setComputePipelineState: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's compute-pipeline pointer array.
    pub fn set_compute_pipeline_states(
        &self,
        values: &[Option<&ComputePipelineState>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index
            .checked_add(values.len())
            .ok_or_else(|| Error::invalid_argument("compute-pipeline index range overflow"))?;
        for (relative, value) in values.iter().enumerate() {
            self.set_compute_pipeline_state(*value, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional depth-stencil state.
    pub fn set_depth_stencil_state(
        &self,
        value: Option<&DepthStencilState>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setDepthStencilState:atIndex:),
            "MTL::ArgumentEncoder::setDepthStencilState",
        )?;
        let object = value.map(DepthStencilState::as_inner);
        // SAFETY: selector availability was checked and the optional object is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setDepthStencilState: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's depth-stencil pointer array.
    pub fn set_depth_stencil_states(
        &self,
        values: &[Option<&DepthStencilState>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index
            .checked_add(values.len())
            .ok_or_else(|| Error::invalid_argument("depth-stencil index range overflow"))?;
        for (relative, value) in values.iter().enumerate() {
            self.set_depth_stencil_state(*value, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional indirect command buffer.
    pub fn set_indirect_command_buffer(
        &self,
        value: Option<&IndirectCommandBuffer>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setIndirectCommandBuffer:atIndex:),
            "MTL::ArgumentEncoder::setIndirectCommandBuffer",
        )?;
        let object = value.map(IndirectCommandBuffer::as_inner);
        // SAFETY: selector availability was checked and the optional object is retained.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setIndirectCommandBuffer: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's indirect-command-buffer array.
    pub fn set_indirect_command_buffers(
        &self,
        values: &[Option<&IndirectCommandBuffer>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index.checked_add(values.len()).ok_or_else(|| {
            Error::invalid_argument("indirect-command-buffer index range overflow")
        })?;
        for (relative, value) in values.iter().enumerate() {
            self.set_indirect_command_buffer(*value, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional intersection function table.
    pub fn set_intersection_function_table(
        &self,
        value: Option<&IntersectionFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setIntersectionFunctionTable:atIndex:),
            "MTL::ArgumentEncoder::setIntersectionFunctionTable",
        )?;
        let object = value.map(IntersectionFunctionTable::as_inner);
        // SAFETY: selector availability was checked and the optional object is retained.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setIntersectionFunctionTable: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's intersection-function-table array.
    pub fn set_intersection_function_tables(
        &self,
        values: &[Option<&IntersectionFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index.checked_add(values.len()).ok_or_else(|| {
            Error::invalid_argument("intersection-function-table index range overflow")
        })?;
        for (relative, value) in values.iter().enumerate() {
            self.set_intersection_function_table(*value, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional render pipeline state.
    pub fn set_render_pipeline_state(
        &self,
        value: Option<&RenderPipelineState>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setRenderPipelineState:atIndex:),
            "MTL::ArgumentEncoder::setRenderPipelineState",
        )?;
        let object = value.map(|value| &*value.inner);
        // SAFETY: selector availability was checked and the optional protocol object is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setRenderPipelineState: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's render-pipeline pointer array.
    pub fn set_render_pipeline_states(
        &self,
        values: &[Option<&RenderPipelineState>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index
            .checked_add(values.len())
            .ok_or_else(|| Error::invalid_argument("render-pipeline index range overflow"))?;
        for (relative, value) in values.iter().enumerate() {
            self.set_render_pipeline_state(*value, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional sampler state.
    pub fn set_sampler_state(
        &self,
        value: Option<&SamplerState>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setSamplerState:atIndex:),
            "MTL::ArgumentEncoder::setSamplerState",
        )?;
        let object = value.map(SamplerState::as_inner);
        // SAFETY: selector availability was checked and the optional object is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setSamplerState: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's sampler pointer array.
    pub fn set_sampler_states(
        &self,
        values: &[Option<&SamplerState>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index
            .checked_add(values.len())
            .ok_or_else(|| Error::invalid_argument("sampler index range overflow"))?;
        for (relative, value) in values.iter().enumerate() {
            self.set_sampler_state(*value, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds an optional visible function table.
    pub fn set_visible_function_table(
        &self,
        value: Option<&VisibleFunctionTable>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setVisibleFunctionTable:atIndex:),
            "MTL::ArgumentEncoder::setVisibleFunctionTable",
        )?;
        let object = value.map(VisibleFunctionTable::as_inner);
        // SAFETY: selector availability was checked and the optional object is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setVisibleFunctionTable: object, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's visible-function-table array.
    pub fn set_visible_function_tables(
        &self,
        values: &[Option<&VisibleFunctionTable>],
        start_index: usize,
    ) -> Result<(), Error> {
        start_index.checked_add(values.len()).ok_or_else(|| {
            Error::invalid_argument("visible-function-table index range overflow")
        })?;
        for (relative, value) in values.iter().enumerate() {
            self.set_visible_function_table(*value, start_index + relative)?;
        }
        Ok(())
    }
}

impl IndirectRenderCommand {
    /// Resets the indirect render command to its initial empty state.
    pub fn reset(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL::IndirectRenderCommand::reset",
        )?;
        // SAFETY: selector availability and its zero-argument void ABI were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }

    /// Inserts a barrier before later operations in this indirect command.
    pub fn set_barrier(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setBarrier),
            "MTL::IndirectRenderCommand::setBarrier",
        )?;
        // SAFETY: selector availability and its zero-argument void ABI were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), setBarrier];
        }
        Ok(())
    }

    /// Removes the barrier previously configured on this indirect command.
    pub fn clear_barrier(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(clearBarrier),
            "MTL::IndirectRenderCommand::clearBarrier",
        )?;
        // SAFETY: selector availability and its zero-argument void ABI were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), clearBarrier];
        }
        Ok(())
    }

    /// Binds an optional vertex buffer after validating its byte offset.
    pub fn set_vertex_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.set_vertex_buffer_with_stride(buffer, offset, None, index)
    }

    /// Binds an optional vertex buffer with a checked dynamic attribute stride.
    pub fn set_vertex_buffer_with_stride(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        stride: Option<usize>,
        index: usize,
    ) -> Result<(), Error> {
        check_buffer_offset(buffer, offset, "indirect vertex buffer")?;
        if stride == Some(0) {
            return Err(Error::invalid_argument(
                "vertex buffer stride must be non-zero",
            ));
        }
        let object = buffer.map(|value| &*value.inner);
        if let Some(stride) = stride {
            require_selector(
                self.as_inner(),
                sel!(setVertexBuffer:offset:attributeStride:atIndex:),
                "MTL::IndirectRenderCommand::setVertexBuffer(stride)",
            )?;
            // SAFETY: selector availability, optional buffer, offset, and stride were validated.
            unsafe {
                let _: () = msg_send![self.as_inner(), setVertexBuffer: object, offset: offset, attributeStride: stride, atIndex: index];
            }
        } else {
            require_selector(
                self.as_inner(),
                sel!(setVertexBuffer:offset:atIndex:),
                "MTL::IndirectRenderCommand::setVertexBuffer",
            )?;
            // SAFETY: selector availability, optional buffer, and offset were validated.
            unsafe {
                let _: () = msg_send![self.as_inner(), setVertexBuffer: object, offset: offset, atIndex: index];
            }
        }
        Ok(())
    }

    /// Binds an optional fragment buffer after validating its byte offset.
    pub fn set_fragment_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        check_buffer_offset(buffer, offset, "indirect fragment buffer")?;
        require_selector(
            self.as_inner(),
            sel!(setFragmentBuffer:offset:atIndex:),
            "MTL::IndirectRenderCommand::setFragmentBuffer",
        )?;
        let object = buffer.map(|value| &*value.inner);
        // SAFETY: selector availability, optional buffer, and offset were validated.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFragmentBuffer: object, offset: offset, atIndex: index];
        }
        Ok(())
    }

    /// Binds an optional mesh-stage buffer after validating its byte offset.
    pub fn set_mesh_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        check_buffer_offset(buffer, offset, "indirect mesh buffer")?;
        require_selector(
            self.as_inner(),
            sel!(setMeshBuffer:offset:atIndex:),
            "MTL::IndirectRenderCommand::setMeshBuffer",
        )?;
        let object = buffer.map(|value| &*value.inner);
        // SAFETY: selector availability, optional buffer, and offset were validated.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setMeshBuffer: object, offset: offset, atIndex: index];
        }
        Ok(())
    }

    /// Binds an optional object-stage buffer after validating its byte offset.
    pub fn set_object_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        check_buffer_offset(buffer, offset, "indirect object buffer")?;
        require_selector(
            self.as_inner(),
            sel!(setObjectBuffer:offset:atIndex:),
            "MTL::IndirectRenderCommand::setObjectBuffer",
        )?;
        let object = buffer.map(|value| &*value.inner);
        // SAFETY: selector availability, optional buffer, and offset were validated.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setObjectBuffer: object, offset: offset, atIndex: index];
        }
        Ok(())
    }

    /// Selects the retained render pipeline state used by this command.
    pub fn set_render_pipeline_state(&self, state: &RenderPipelineState) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setRenderPipelineState:),
            "MTL::IndirectRenderCommand::setRenderPipelineState",
        )?;
        // SAFETY: selector availability was checked and the pipeline state is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setRenderPipelineState: &*state.inner];
        }
        Ok(())
    }

    /// Selects or clears the depth-stencil state used by this command.
    pub fn set_depth_stencil_state(&self, state: Option<&DepthStencilState>) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setDepthStencilState:),
            "MTL::IndirectRenderCommand::setDepthStencilState",
        )?;
        let object = state.map(DepthStencilState::as_inner);
        // SAFETY: selector availability was checked and the optional object is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setDepthStencilState: object];
        }
        Ok(())
    }

    /// Sets a validated triangle culling mode.
    pub fn set_cull_mode(&self, value: CullMode) -> Result<(), Error> {
        if !value.is_valid() {
            return Err(Error::invalid_argument("invalid cull mode"));
        }
        require_selector(
            self.as_inner(),
            sel!(setCullMode:),
            "MTL::IndirectRenderCommand::setCullMode",
        )?;
        // SAFETY: selector availability and the enum representation were validated.
        unsafe {
            let _: () = msg_send![self.as_inner(), setCullMode: value.as_raw()];
        }
        Ok(())
    }

    /// Sets a validated depth clipping mode.
    pub fn set_depth_clip_mode(&self, value: DepthClipMode) -> Result<(), Error> {
        if !value.is_valid() {
            return Err(Error::invalid_argument("invalid depth clip mode"));
        }
        require_selector(
            self.as_inner(),
            sel!(setDepthClipMode:),
            "MTL::IndirectRenderCommand::setDepthClipMode",
        )?;
        // SAFETY: selector availability and the enum representation were validated.
        unsafe {
            let _: () = msg_send![self.as_inner(), setDepthClipMode: value.as_raw()];
        }
        Ok(())
    }

    /// Sets the validated winding used to identify front-facing primitives.
    pub fn set_front_facing_winding(&self, value: Winding) -> Result<(), Error> {
        if !value.is_valid() {
            return Err(Error::invalid_argument("invalid winding"));
        }
        require_selector(
            self.as_inner(),
            sel!(setFrontFacingWinding:),
            "MTL::IndirectRenderCommand::setFrontFacingWinding",
        )?;
        // SAFETY: selector availability and the enum representation were validated.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFrontFacingWinding: value.as_raw()];
        }
        Ok(())
    }

    /// Sets a validated triangle fill mode.
    pub fn set_triangle_fill_mode(&self, value: TriangleFillMode) -> Result<(), Error> {
        if !value.is_valid() {
            return Err(Error::invalid_argument("invalid triangle fill mode"));
        }
        require_selector(
            self.as_inner(),
            sel!(setTriangleFillMode:),
            "MTL::IndirectRenderCommand::setTriangleFillMode",
        )?;
        // SAFETY: selector availability and the enum representation were validated.
        unsafe {
            let _: () = msg_send![self.as_inner(), setTriangleFillMode: value.as_raw()];
        }
        Ok(())
    }

    /// Sets finite depth-bias parameters for subsequent indirect draws.
    pub fn set_depth_bias(
        &self,
        depth_bias: f32,
        slope_scale: f32,
        clamp: f32,
    ) -> Result<(), Error> {
        if !depth_bias.is_finite() || !slope_scale.is_finite() || !clamp.is_finite() {
            return Err(Error::invalid_argument("depth-bias values must be finite"));
        }
        require_selector(
            self.as_inner(),
            sel!(setDepthBias:slopeScale:clamp:),
            "MTL::IndirectRenderCommand::setDepthBias",
        )?;
        // SAFETY: selector availability and finite scalar arguments were validated.
        unsafe {
            let _: () = msg_send![self.as_inner(), setDepthBias: depth_bias, slopeScale: slope_scale, clamp: clamp];
        }
        Ok(())
    }

    /// Sets object-stage threadgroup memory length for one binding index.
    pub fn set_object_threadgroup_memory_length(
        &self,
        length: usize,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setObjectThreadgroupMemoryLength:atIndex:),
            "MTL::IndirectRenderCommand::setObjectThreadgroupMemoryLength",
        )?;
        // SAFETY: selector availability was checked; Metal validates index-specific pipeline limits.
        unsafe {
            let _: () = msg_send![self.as_inner(), setObjectThreadgroupMemoryLength: length, atIndex: index];
        }
        Ok(())
    }

    /// Encodes a non-indexed patch draw after conservatively validating every
    /// buffer range that Metal can read.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_patches(
        &self,
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&Buffer>,
        patch_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    ) -> Result<(), Error> {
        check_patch_draw_ranges(
            number_of_patch_control_points,
            patch_start,
            patch_count,
            patch_index_buffer,
            patch_index_buffer_offset,
            None,
            0,
            instance_count,
            base_instance,
            tessellation_factor_buffer,
            tessellation_factor_buffer_offset,
            instance_stride,
        )?;
        require_selector(
            self.as_inner(),
            sel!(drawPatches:patchStart:patchCount:patchIndexBuffer:patchIndexBufferOffset:instanceCount:baseInstance:tessellationFactorBuffer:tessellationFactorBufferOffset:tessellationFactorBufferInstanceStride:),
            "MTL::IndirectRenderCommand::drawPatches",
        )?;
        let patch_indices = patch_index_buffer.map(|value| &*value.inner);
        // SAFETY: selector availability, all additive and multiplicative
        // ranges, alignments, and retained buffer footprints were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(),
                drawPatches: number_of_patch_control_points,
                patchStart: patch_start,
                patchCount: patch_count,
                patchIndexBuffer: patch_indices,
                patchIndexBufferOffset: patch_index_buffer_offset,
                instanceCount: instance_count,
                baseInstance: base_instance,
                tessellationFactorBuffer: &*tessellation_factor_buffer.inner,
                tessellationFactorBufferOffset: tessellation_factor_buffer_offset,
                tessellationFactorBufferInstanceStride: instance_stride
            ];
        }
        Ok(())
    }

    /// Encodes an indexed patch draw after conservatively validating every
    /// patch, control-point, and tessellation-factor buffer range.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_indexed_patches(
        &self,
        number_of_patch_control_points: usize,
        patch_start: usize,
        patch_count: usize,
        patch_index_buffer: Option<&Buffer>,
        patch_index_buffer_offset: usize,
        control_point_index_buffer: &Buffer,
        control_point_index_buffer_offset: usize,
        instance_count: usize,
        base_instance: usize,
        tessellation_factor_buffer: &Buffer,
        tessellation_factor_buffer_offset: usize,
        instance_stride: usize,
    ) -> Result<(), Error> {
        check_patch_draw_ranges(
            number_of_patch_control_points,
            patch_start,
            patch_count,
            patch_index_buffer,
            patch_index_buffer_offset,
            Some(control_point_index_buffer),
            control_point_index_buffer_offset,
            instance_count,
            base_instance,
            tessellation_factor_buffer,
            tessellation_factor_buffer_offset,
            instance_stride,
        )?;
        require_selector(
            self.as_inner(),
            sel!(drawIndexedPatches:patchStart:patchCount:patchIndexBuffer:patchIndexBufferOffset:controlPointIndexBuffer:controlPointIndexBufferOffset:instanceCount:baseInstance:tessellationFactorBuffer:tessellationFactorBufferOffset:tessellationFactorBufferInstanceStride:),
            "MTL::IndirectRenderCommand::drawIndexedPatches",
        )?;
        let patch_indices = patch_index_buffer.map(|value| &*value.inner);
        // SAFETY: selector availability, all additive and multiplicative
        // ranges, alignments, and retained buffer footprints were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(),
                drawIndexedPatches: number_of_patch_control_points,
                patchStart: patch_start,
                patchCount: patch_count,
                patchIndexBuffer: patch_indices,
                patchIndexBufferOffset: patch_index_buffer_offset,
                controlPointIndexBuffer: &*control_point_index_buffer.inner,
                controlPointIndexBufferOffset: control_point_index_buffer_offset,
                instanceCount: instance_count,
                baseInstance: base_instance,
                tessellationFactorBuffer: &*tessellation_factor_buffer.inner,
                tessellationFactorBufferOffset: tessellation_factor_buffer_offset,
                tessellationFactorBufferInstanceStride: instance_stride
            ];
        }
        Ok(())
    }

    /// Encodes a non-indexed draw after checking vertex and instance ranges.
    pub fn draw_primitives(
        &self,
        primitive_type: PrimitiveType,
        vertex_start: usize,
        vertex_count: usize,
        instance_count: usize,
        base_instance: usize,
    ) -> Result<(), Error> {
        vertex_start
            .checked_add(vertex_count)
            .ok_or_else(|| Error::invalid_argument("vertex range overflow"))?;
        base_instance
            .checked_add(instance_count)
            .ok_or_else(|| Error::invalid_argument("instance range overflow"))?;
        require_selector(
            self.as_inner(),
            sel!(drawPrimitives:vertexStart:vertexCount:instanceCount:baseInstance:),
            "MTL::IndirectRenderCommand::drawPrimitives",
        )?;
        // SAFETY: selector availability and all additive ranges were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), drawPrimitives: primitive_type.as_raw(), vertexStart: vertex_start, vertexCount: vertex_count, instanceCount: instance_count, baseInstance: base_instance];
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    /// Encodes an indexed draw after validating the index-buffer byte range.
    pub fn draw_indexed_primitives(
        &self,
        primitive_type: PrimitiveType,
        index_count: usize,
        index_type: IndexType,
        index_buffer: &Buffer,
        index_buffer_offset: usize,
        instance_count: usize,
        base_vertex: isize,
        base_instance: usize,
    ) -> Result<(), Error> {
        if !index_type.is_valid() {
            return Err(Error::invalid_argument("invalid index type"));
        }
        let index_size = if index_type.as_raw() == 0 { 2 } else { 4 };
        let bytes = index_count
            .checked_mul(index_size)
            .ok_or_else(|| Error::invalid_argument("index byte range overflow"))?;
        checked_end(
            index_buffer_offset,
            bytes,
            index_buffer.length(),
            "index buffer",
        )?;
        base_instance
            .checked_add(instance_count)
            .ok_or_else(|| Error::invalid_argument("instance range overflow"))?;
        require_selector(
            self.as_inner(),
            sel!(drawIndexedPrimitives:indexCount:indexType:indexBuffer:indexBufferOffset:instanceCount:baseVertex:baseInstance:),
            "MTL::IndirectRenderCommand::drawIndexedPrimitives",
        )?;
        // SAFETY: selector availability, enum value, index-buffer range, and instance range were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), drawIndexedPrimitives: primitive_type.as_raw(), indexCount: index_count, indexType: index_type.as_raw(), indexBuffer: &*index_buffer.inner, indexBufferOffset: index_buffer_offset, instanceCount: instance_count, baseVertex: base_vertex, baseInstance: base_instance];
        }
        Ok(())
    }

    /// Encodes a mesh draw using checked non-zero threadgroup dimensions.
    pub fn draw_mesh_threadgroups(
        &self,
        groups: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        check_size(groups, "mesh threadgroup grid")?;
        check_size(object_threads, "object threadgroup")?;
        check_size(mesh_threads, "mesh threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(drawMeshThreadgroups:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:),
            "MTL::IndirectRenderCommand::drawMeshThreadgroups",
        )?;
        // SAFETY: selector availability and non-zero dimensions were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), drawMeshThreadgroups: objc2_metal::MTLSize::from(groups), threadsPerObjectThreadgroup: objc2_metal::MTLSize::from(object_threads), threadsPerMeshThreadgroup: objc2_metal::MTLSize::from(mesh_threads)];
        }
        Ok(())
    }

    /// Encodes a mesh draw using checked non-zero thread-grid dimensions.
    pub fn draw_mesh_threads(
        &self,
        threads: Size,
        object_threads: Size,
        mesh_threads: Size,
    ) -> Result<(), Error> {
        check_size(threads, "mesh thread grid")?;
        check_size(object_threads, "object threadgroup")?;
        check_size(mesh_threads, "mesh threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(drawMeshThreads:threadsPerObjectThreadgroup:threadsPerMeshThreadgroup:),
            "MTL::IndirectRenderCommand::drawMeshThreads",
        )?;
        // SAFETY: selector availability and non-zero dimensions were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), drawMeshThreads: objc2_metal::MTLSize::from(threads), threadsPerObjectThreadgroup: objc2_metal::MTLSize::from(object_threads), threadsPerMeshThreadgroup: objc2_metal::MTLSize::from(mesh_threads)];
        }
        Ok(())
    }
}

impl IndirectComputeCommand {
    /// Resets the indirect compute command to its initial empty state.
    pub fn reset(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL::IndirectComputeCommand::reset",
        )?;
        // SAFETY: selector availability and its zero-argument void ABI were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }

    /// Inserts a barrier before later operations in this indirect command.
    pub fn set_barrier(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setBarrier),
            "MTL::IndirectComputeCommand::setBarrier",
        )?;
        // SAFETY: selector availability and its zero-argument void ABI were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), setBarrier];
        }
        Ok(())
    }

    /// Removes the barrier previously configured on this indirect command.
    pub fn clear_barrier(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(clearBarrier),
            "MTL::IndirectComputeCommand::clearBarrier",
        )?;
        // SAFETY: selector availability and its zero-argument void ABI were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), clearBarrier];
        }
        Ok(())
    }

    /// Selects the retained compute pipeline state used by this command.
    pub fn set_compute_pipeline_state(&self, state: &ComputePipelineState) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setComputePipelineState:),
            "MTL::IndirectComputeCommand::setComputePipelineState",
        )?;
        // SAFETY: selector availability was checked and the pipeline state is retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setComputePipelineState: &*state.inner];
        }
        Ok(())
    }

    /// Binds an optional kernel buffer after validating its byte offset.
    pub fn set_kernel_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.set_kernel_buffer_with_stride(buffer, offset, None, index)
    }

    /// Binds an optional kernel buffer with a checked dynamic attribute stride.
    pub fn set_kernel_buffer_with_stride(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        stride: Option<usize>,
        index: usize,
    ) -> Result<(), Error> {
        check_buffer_offset(buffer, offset, "indirect kernel buffer")?;
        if stride == Some(0) {
            return Err(Error::invalid_argument(
                "kernel buffer stride must be non-zero",
            ));
        }
        let object = buffer.map(|value| &*value.inner);
        if let Some(stride) = stride {
            require_selector(
                self.as_inner(),
                sel!(setKernelBuffer:offset:attributeStride:atIndex:),
                "MTL::IndirectComputeCommand::setKernelBuffer(stride)",
            )?;
            // SAFETY: selector availability, optional buffer, offset, and stride were validated.
            unsafe {
                let _: () = msg_send![self.as_inner(), setKernelBuffer: object, offset: offset, attributeStride: stride, atIndex: index];
            }
        } else {
            require_selector(
                self.as_inner(),
                sel!(setKernelBuffer:offset:atIndex:),
                "MTL::IndirectComputeCommand::setKernelBuffer",
            )?;
            // SAFETY: selector availability, optional buffer, and offset were validated.
            unsafe {
                let _: () = msg_send![self.as_inner(), setKernelBuffer: object, offset: offset, atIndex: index];
            }
        }
        Ok(())
    }

    /// Sets checked non-zero imageblock dimensions.
    pub fn set_imageblock_size(&self, width: usize, height: usize) -> Result<(), Error> {
        if width == 0 || height == 0 {
            return Err(Error::invalid_argument(
                "imageblock dimensions must be non-zero",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setImageblockWidth:height:),
            "MTL::IndirectComputeCommand::setImageblockWidth",
        )?;
        // SAFETY: selector availability and non-zero dimensions were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), setImageblockWidth: width, height: height];
        }
        Ok(())
    }

    /// Sets a non-empty stage-in region after checking coordinate overflow.
    pub fn set_stage_in_region(&self, region: Region) -> Result<(), Error> {
        check_size(region.size, "stage-in region")?;
        region
            .origin
            .x
            .checked_add(region.size.width)
            .ok_or_else(|| Error::invalid_argument("stage-in x range overflow"))?;
        region
            .origin
            .y
            .checked_add(region.size.height)
            .ok_or_else(|| Error::invalid_argument("stage-in y range overflow"))?;
        region
            .origin
            .z
            .checked_add(region.size.depth)
            .ok_or_else(|| Error::invalid_argument("stage-in z range overflow"))?;
        require_selector(
            self.as_inner(),
            sel!(setStageInRegion:),
            "MTL::IndirectComputeCommand::setStageInRegion",
        )?;
        // SAFETY: selector availability, non-zero size, and all region ranges were checked.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setStageInRegion: objc2_metal::MTLRegion::from(region)];
        }
        Ok(())
    }

    /// Sets threadgroup memory length for one binding index.
    pub fn set_threadgroup_memory_length(&self, length: usize, index: usize) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setThreadgroupMemoryLength:atIndex:),
            "MTL::IndirectComputeCommand::setThreadgroupMemoryLength",
        )?;
        // SAFETY: selector availability was checked; Metal validates index-specific pipeline limits.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setThreadgroupMemoryLength: length, atIndex: index];
        }
        Ok(())
    }

    /// Encodes concurrent threadgroup dispatch with non-zero dimensions.
    pub fn concurrent_dispatch_threadgroups(
        &self,
        groups: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        check_size(groups, "concurrent threadgroup grid")?;
        check_size(threads_per_group, "concurrent threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(concurrentDispatchThreadgroups:threadsPerThreadgroup:),
            "MTL::IndirectComputeCommand::concurrentDispatchThreadgroups",
        )?;
        // SAFETY: selector availability and non-zero dimensions were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), concurrentDispatchThreadgroups: objc2_metal::MTLSize::from(groups), threadsPerThreadgroup: objc2_metal::MTLSize::from(threads_per_group)];
        }
        Ok(())
    }

    /// Encodes concurrent thread dispatch with non-zero dimensions.
    pub fn concurrent_dispatch_threads(
        &self,
        threads: Size,
        threads_per_group: Size,
    ) -> Result<(), Error> {
        check_size(threads, "concurrent thread grid")?;
        check_size(threads_per_group, "concurrent threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(concurrentDispatchThreads:threadsPerThreadgroup:),
            "MTL::IndirectComputeCommand::concurrentDispatchThreads",
        )?;
        // SAFETY: selector availability and non-zero dimensions were checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), concurrentDispatchThreads: objc2_metal::MTLSize::from(threads), threadsPerThreadgroup: objc2_metal::MTLSize::from(threads_per_group)];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_ranges_reject_overflow_and_out_of_bounds() {
        assert!(checked_end(8, 4, 12, "buffer").is_ok());
        assert!(checked_end(9, 4, 12, "buffer").is_err());
        assert!(checked_end(usize::MAX, 1, usize::MAX, "buffer").is_err());
    }

    #[test]
    fn dispatch_sizes_require_all_dimensions() {
        assert!(check_size(Size::new(1, 1, 1), "grid").is_ok());
        assert!(check_size(Size::new(1, 0, 1), "grid").is_err());
    }
}
