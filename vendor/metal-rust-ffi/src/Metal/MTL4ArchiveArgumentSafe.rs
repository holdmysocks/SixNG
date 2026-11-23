//! Audited Metal 4 archive queries and argument-table bindings.

use crate::foundation::{Error, metal_error};
use crate::metal::generated_object_types::{metal, metal4};
use crate::metal::{Buffer, ComputePipelineState, Device, RenderPipelineState, Texture};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
use objc2_foundation::{NSError, NSString};
use objc2_metal::MTLResourceID;

const MAX_BUFFER_BIND_COUNT: usize = 31;
const MAX_TEXTURE_BIND_COUNT: usize = 128;
const MAX_SAMPLER_BIND_COUNT: usize = 16;

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    context: &str,
) -> Result<(), Error> {
    // SAFETY: every Objective-C object implements respondsToSelector:, and
    // selector/bool use the stable Objective-C runtime ABI.
    let supported: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if supported {
        Ok(())
    } else {
        Err(Error::unsupported(format!("{context} is unavailable")))
    }
}

fn resource_id(object: &AnyObject, context: &str) -> Result<MTLResourceID, Error> {
    require_selector(object, sel!(gpuResourceID), context)?;
    // SAFETY: selector availability is checked and gpuResourceID returns the
    // SDK-declared one-u64 MTLResourceID value.
    Ok(unsafe { msg_send![object, gpuResourceID] })
}

/// A checked, Rust-owned description of an argument table.
///
/// Keeping the limits in Rust means invalid binding counts are rejected before
/// Objective-C sees them, and the created table can retain those exact bounds.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ArgumentTableDescriptor {
    label: Option<String>,
    max_buffer_bind_count: usize,
    max_texture_bind_count: usize,
    max_sampler_state_bind_count: usize,
    initialize_bindings: bool,
    support_attribute_strides: bool,
}

impl ArgumentTableDescriptor {
    /// Creates a descriptor with no binding slots.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    /// Returns the optional debug label.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Replaces or clears the debug label.
    pub fn set_label(&mut self, label: Option<&str>) {
        self.label = label.map(str::to_owned);
    }

    #[must_use]
    /// Returns the configured buffer binding count.
    pub const fn max_buffer_bind_count(&self) -> usize {
        self.max_buffer_bind_count
    }

    /// Sets the buffer binding count after checking Metal's limit.
    pub fn set_max_buffer_bind_count(&mut self, count: usize) -> Result<(), Error> {
        if count > MAX_BUFFER_BIND_COUNT {
            return Err(Error::invalid_argument(
                "Metal 4 argument tables support at most 31 buffer bindings",
            ));
        }
        self.max_buffer_bind_count = count;
        Ok(())
    }

    #[must_use]
    /// Returns the configured texture binding count.
    pub const fn max_texture_bind_count(&self) -> usize {
        self.max_texture_bind_count
    }

    /// Sets the texture binding count after checking Metal's limit.
    pub fn set_max_texture_bind_count(&mut self, count: usize) -> Result<(), Error> {
        if count > MAX_TEXTURE_BIND_COUNT {
            return Err(Error::invalid_argument(
                "Metal 4 argument tables support at most 128 texture bindings",
            ));
        }
        self.max_texture_bind_count = count;
        Ok(())
    }

    #[must_use]
    /// Returns the configured sampler binding count.
    pub const fn max_sampler_state_bind_count(&self) -> usize {
        self.max_sampler_state_bind_count
    }

    /// Sets the sampler binding count after checking Metal's limit.
    pub fn set_max_sampler_state_bind_count(&mut self, count: usize) -> Result<(), Error> {
        if count > MAX_SAMPLER_BIND_COUNT {
            return Err(Error::invalid_argument(
                "Metal 4 argument tables support at most 16 sampler bindings",
            ));
        }
        self.max_sampler_state_bind_count = count;
        Ok(())
    }

    #[must_use]
    /// Returns whether Metal initializes bindings when creating the table.
    pub const fn initialize_bindings(&self) -> bool {
        self.initialize_bindings
    }

    /// Configures whether Metal initializes bindings when creating the table.
    pub fn set_initialize_bindings(&mut self, initialize: bool) {
        self.initialize_bindings = initialize;
    }

    #[must_use]
    /// Returns whether dynamic attribute strides are enabled.
    pub const fn support_attribute_strides(&self) -> bool {
        self.support_attribute_strides
    }

    /// Configures support for dynamic attribute strides.
    pub fn set_support_attribute_strides(&mut self, support: bool) {
        self.support_attribute_strides = support;
    }

    fn make_native(&self) -> Result<metal4::ArgumentTableDescriptor, Error> {
        let descriptor = metal4::ArgumentTableDescriptor::new()?;
        descriptor.set_max_buffer_bind_count(self.max_buffer_bind_count)?;
        descriptor.set_max_texture_bind_count(self.max_texture_bind_count)?;
        descriptor.set_max_sampler_state_bind_count(self.max_sampler_state_bind_count)?;
        descriptor.set_initialize_bindings(self.initialize_bindings)?;
        descriptor.set_support_attribute_strides(self.support_attribute_strides)?;
        if let Some(label) = &self.label {
            descriptor.set_label(label)?;
        }
        Ok(descriptor)
    }
}

/// An argument table that retains the exact checked bounds used at creation.
#[derive(Clone)]
pub struct ArgumentTable {
    inner: metal4::ArgumentTable,
    device: Device,
    label: Option<String>,
    max_buffer_bind_count: usize,
    max_texture_bind_count: usize,
    max_sampler_state_bind_count: usize,
    support_attribute_strides: bool,
}

/// A typed Metal resource accepted by an argument-table buffer slot.
#[derive(Clone, Copy)]
pub enum ArgumentTableResource<'a> {
    /// A conventional Metal buffer resource.
    Buffer(&'a Buffer),
    /// A Metal tensor resource.
    Tensor(&'a metal::Tensor),
    /// A Metal acceleration-structure resource.
    AccelerationStructure(&'a metal::AccelerationStructure),
}

impl<'a> ArgumentTableResource<'a> {
    fn object(self) -> (&'a AnyObject, &'static str) {
        match self {
            Self::Buffer(value) => (value.as_any_object(), "MTLBuffer.gpuResourceID"),
            Self::Tensor(value) => (value.as_inner(), "MTLTensor.gpuResourceID"),
            Self::AccelerationStructure(value) => {
                (value.as_inner(), "MTLAccelerationStructure.gpuResourceID")
            }
        }
    }
}

impl Device {
    /// Creates a Metal 4 argument table and owns any returned NSError.
    pub fn new_mtl4_argument_table(
        &self,
        descriptor: &ArgumentTableDescriptor,
    ) -> Result<ArgumentTable, Error> {
        require_selector(
            self.as_any_object(),
            sel!(newArgumentTableWithDescriptor:error:),
            "MTLDevice.newArgumentTableWithDescriptor:error:",
        )?;
        let native = descriptor.make_native()?;
        // SAFETY: the selector is present, the descriptor is a live native
        // MTL4ArgumentTableDescriptor, and `_` uses objc2's owned NSError
        // writeback adapter for the declared error parameter.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_any_object(), newArgumentTableWithDescriptor: native.as_inner(), error: _]
        };
        let inner = result
            .map(metal4::ArgumentTable::from_inner)
            .map_err(|error| metal_error(&error))?;
        Ok(ArgumentTable {
            inner,
            device: self.clone(),
            label: descriptor.label.clone(),
            max_buffer_bind_count: descriptor.max_buffer_bind_count,
            max_texture_bind_count: descriptor.max_texture_bind_count,
            max_sampler_state_bind_count: descriptor.max_sampler_state_bind_count,
            support_attribute_strides: descriptor.support_attribute_strides,
        })
    }
}

impl ArgumentTable {
    #[allow(dead_code)]
    pub(crate) const fn as_generated(&self) -> &metal4::ArgumentTable {
        &self.inner
    }

    #[must_use]
    /// Returns the device that created this table.
    pub fn device(&self) -> Device {
        self.device.clone()
    }

    #[must_use]
    /// Returns the label captured when the table was created.
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    fn ensure_same_device(&self, resource: &AnyObject, context: &str) -> Result<(), Error> {
        require_selector(resource, sel!(device), context)?;
        // SAFETY: selector availability is checked and every Metal allocation
        // used here declares a nonnull retained MTLDevice result.
        let resource_device: Retained<AnyObject> = unsafe { msg_send![resource, device] };
        if std::ptr::eq(&*resource_device, self.device.as_any_object()) {
            Ok(())
        } else {
            Err(Error::invalid_argument(
                "argument-table resources must belong to the table's device",
            ))
        }
    }

    fn check_buffer_index(&self, index: usize) -> Result<(), Error> {
        if index >= self.max_buffer_bind_count {
            Err(Error::invalid_argument(
                "buffer binding index exceeds the table descriptor limit",
            ))
        } else {
            Ok(())
        }
    }

    fn check_texture_index(&self, index: usize) -> Result<(), Error> {
        if index >= self.max_texture_bind_count {
            Err(Error::invalid_argument(
                "texture binding index exceeds the table descriptor limit",
            ))
        } else {
            Ok(())
        }
    }

    fn check_sampler_index(&self, index: usize) -> Result<(), Error> {
        if index >= self.max_sampler_state_bind_count {
            Err(Error::invalid_argument(
                "sampler binding index exceeds the table descriptor limit",
            ))
        } else {
            Ok(())
        }
    }

    /// Binds a buffer address after validating its offset and binding index.
    pub fn set_buffer(&self, buffer: &Buffer, offset: usize, index: usize) -> Result<(), Error> {
        self.check_buffer_index(index)?;
        self.ensure_same_device(buffer.as_any_object(), "MTLBuffer.device")?;
        if offset > buffer.length() {
            return Err(Error::invalid_argument(
                "buffer binding offset is out of bounds",
            ));
        }
        let address = buffer
            .gpu_address()
            .checked_add(offset as u64)
            .ok_or_else(|| Error::invalid_argument("buffer GPU address overflow"))?;
        require_selector(
            self.inner.as_inner(),
            sel!(setAddress:atIndex:),
            "MTL4ArgumentTable.setAddress:atIndex:",
        )?;
        // SAFETY: the table retains the descriptor's exact buffer count,
        // index was checked against it, and the address is within the buffer.
        unsafe {
            let _: () = msg_send![self.inner.as_inner(), setAddress: address, atIndex: index];
        }
        Ok(())
    }

    /// Binds a buffer address with a checked dynamic attribute stride.
    pub fn set_buffer_with_stride(
        &self,
        buffer: &Buffer,
        offset: usize,
        stride: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.check_buffer_index(index)?;
        self.ensure_same_device(buffer.as_any_object(), "MTLBuffer.device")?;
        if !self.support_attribute_strides {
            return Err(Error::invalid_argument(
                "the table descriptor did not enable attribute strides",
            ));
        }
        let remaining = buffer
            .length()
            .checked_sub(offset)
            .ok_or_else(|| Error::invalid_argument("buffer binding offset is out of bounds"))?;
        if stride > remaining {
            return Err(Error::invalid_argument(
                "attribute stride exceeds the remaining buffer length",
            ));
        }
        let address = buffer
            .gpu_address()
            .checked_add(offset as u64)
            .ok_or_else(|| Error::invalid_argument("buffer GPU address overflow"))?;
        require_selector(
            self.inner.as_inner(),
            sel!(setAddress:attributeStride:atIndex:),
            "MTL4ArgumentTable.setAddress:attributeStride:atIndex:",
        )?;
        // SAFETY: index, address range, stride, and descriptor stride support
        // were checked before invoking the SDK-declared selector.
        unsafe {
            let _: () = msg_send![self.inner.as_inner(), setAddress: address, attributeStride: stride, atIndex: index];
        }
        Ok(())
    }

    /// Binds consecutive buffers from a Rust slice.
    pub fn set_buffers(
        &self,
        bindings: &[(&Buffer, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(bindings.len())
            .ok_or_else(|| Error::invalid_argument("buffer binding range overflow"))?;
        if end > self.max_buffer_bind_count {
            return Err(Error::invalid_argument(
                "buffer binding slice exceeds the table descriptor limit",
            ));
        }
        for (relative, &(buffer, offset)) in bindings.iter().enumerate() {
            self.set_buffer(buffer, offset, start_index + relative)?;
        }
        Ok(())
    }

    fn set_buffer_resource_object(
        &self,
        object: &AnyObject,
        index: usize,
        context: &str,
    ) -> Result<(), Error> {
        self.check_buffer_index(index)?;
        self.ensure_same_device(object, context)?;
        let id = resource_id(object, context)?;
        require_selector(
            self.inner.as_inner(),
            sel!(setResource:atBufferIndex:),
            "MTL4ArgumentTable.setResource:atBufferIndex:",
        )?;
        // SAFETY: index was checked against the retained descriptor count and
        // the opaque ID came from the live typed Metal resource itself.
        unsafe {
            let _: () = msg_send![self.inner.as_inner(), setResource: id, atBufferIndex: index];
        }
        Ok(())
    }

    /// Binds a typed resource to a checked buffer binding slot.
    pub fn set_resource(
        &self,
        resource: ArgumentTableResource<'_>,
        index: usize,
    ) -> Result<(), Error> {
        let (object, context) = resource.object();
        self.set_buffer_resource_object(object, index, context)
    }

    /// Binds a tensor to a checked buffer binding slot.
    pub fn set_tensor(&self, tensor: &metal::Tensor, index: usize) -> Result<(), Error> {
        self.set_resource(ArgumentTableResource::Tensor(tensor), index)
    }

    /// Binds an acceleration structure to a checked buffer binding slot.
    pub fn set_acceleration_structure(
        &self,
        structure: &metal::AccelerationStructure,
        index: usize,
    ) -> Result<(), Error> {
        self.set_resource(
            ArgumentTableResource::AccelerationStructure(structure),
            index,
        )
    }

    /// Binds a texture using the identifier read from the typed texture.
    pub fn set_texture(&self, texture: &Texture, index: usize) -> Result<(), Error> {
        self.check_texture_index(index)?;
        self.ensure_same_device(texture.as_any_object(), "MTLTexture.device")?;
        require_selector(
            self.inner.as_inner(),
            sel!(setTexture:atIndex:),
            "MTL4ArgumentTable.setTexture:atIndex:",
        )?;
        let raw = texture.gpu_resource_id()._impl;
        // SAFETY: MTLResourceID has exactly one u64 field in the SDK ABI.
        let id = unsafe { std::mem::transmute::<u64, MTLResourceID>(raw) };
        // SAFETY: index was checked against the retained texture count and the
        // resource ID was obtained from a live typed MTLTexture.
        unsafe {
            let _: () = msg_send![self.inner.as_inner(), setTexture: id, atIndex: index];
        }
        Ok(())
    }

    /// Binds consecutive textures from a Rust slice.
    pub fn set_textures(&self, textures: &[&Texture], start_index: usize) -> Result<(), Error> {
        let end = start_index
            .checked_add(textures.len())
            .ok_or_else(|| Error::invalid_argument("texture binding range overflow"))?;
        if end > self.max_texture_bind_count {
            return Err(Error::invalid_argument(
                "texture binding slice exceeds the table descriptor limit",
            ));
        }
        for (relative, texture) in textures.iter().enumerate() {
            self.set_texture(texture, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds a sampler using the identifier read from the typed sampler.
    pub fn set_sampler_state(
        &self,
        sampler: &metal::SamplerState,
        index: usize,
    ) -> Result<(), Error> {
        self.check_sampler_index(index)?;
        self.ensure_same_device(sampler.as_inner(), "MTLSamplerState.device")?;
        let id = resource_id(sampler.as_inner(), "MTLSamplerState.gpuResourceID")?;
        require_selector(
            self.inner.as_inner(),
            sel!(setSamplerState:atIndex:),
            "MTL4ArgumentTable.setSamplerState:atIndex:",
        )?;
        // SAFETY: index was checked against the retained sampler count and the
        // resource ID was obtained from a live typed MTLSamplerState.
        unsafe {
            let _: () = msg_send![self.inner.as_inner(), setSamplerState: id, atIndex: index];
        }
        Ok(())
    }

    /// Binds consecutive samplers from a checked Rust slice.
    pub fn set_sampler_states(
        &self,
        samplers: &[&metal::SamplerState],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(samplers.len())
            .ok_or_else(|| Error::invalid_argument("sampler binding range overflow"))?;
        if end > self.max_sampler_state_bind_count {
            return Err(Error::invalid_argument(
                "sampler binding slice exceeds the table descriptor limit",
            ));
        }
        for (relative, sampler) in samplers.iter().enumerate() {
            self.set_sampler_state(sampler, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds consecutive tensors to checked buffer binding slots.
    pub fn set_tensors(&self, tensors: &[&metal::Tensor], start_index: usize) -> Result<(), Error> {
        let end = start_index
            .checked_add(tensors.len())
            .ok_or_else(|| Error::invalid_argument("tensor binding range overflow"))?;
        if end > self.max_buffer_bind_count {
            return Err(Error::invalid_argument(
                "tensor binding slice exceeds the table descriptor limit",
            ));
        }
        for (relative, tensor) in tensors.iter().enumerate() {
            self.set_tensor(tensor, start_index + relative)?;
        }
        Ok(())
    }

    /// Binds consecutive acceleration structures to checked buffer slots.
    pub fn set_acceleration_structures(
        &self,
        structures: &[&metal::AccelerationStructure],
        start_index: usize,
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(structures.len())
            .ok_or_else(|| Error::invalid_argument("acceleration-structure range overflow"))?;
        if end > self.max_buffer_bind_count {
            return Err(Error::invalid_argument(
                "acceleration-structure slice exceeds the table descriptor limit",
            ));
        }
        for (relative, structure) in structures.iter().enumerate() {
            self.set_acceleration_structure(structure, start_index + relative)?;
        }
        Ok(())
    }
}

/// A checked wrapper around an archive returned by a Metal 4 serializer.
#[derive(Clone)]
pub struct Archive {
    inner: metal4::Archive,
}

impl Archive {
    #[allow(dead_code)]
    pub(crate) const fn from_generated(inner: metal4::Archive) -> Self {
        Self { inner }
    }

    /// Returns the optional archive label.
    pub fn label(&self) -> Result<Option<String>, Error> {
        self.inner.label()
    }

    /// Replaces or clears the archive label.
    pub fn set_label(&self, label: Option<&str>) -> Result<(), Error> {
        require_selector(
            self.inner.as_inner(),
            sel!(setLabel:),
            "MTL4Archive.setLabel:",
        )?;
        let label = label.map(NSString::from_str);
        // SAFETY: selector availability is checked and the optional NSString
        // remains live for the duration of the call.
        unsafe {
            let _: () = msg_send![self.inner.as_inner(), setLabel: label.as_deref()];
        }
        Ok(())
    }

    /// Looks up a binary function from this archive.
    pub fn new_binary_function(
        &self,
        descriptor: &metal4::BinaryFunctionDescriptor,
    ) -> Result<metal4::BinaryFunction, Error> {
        require_selector(
            self.inner.as_inner(),
            sel!(newBinaryFunctionWithDescriptor:error:),
            "MTL4Archive.newBinaryFunctionWithDescriptor:error:",
        )?;
        // SAFETY: selector availability is checked; the descriptor is live;
        // objc2 owns both the `new` result and NSError writeback.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.inner.as_inner(), newBinaryFunctionWithDescriptor: descriptor.as_inner(), error: _]
        };
        result
            .map(metal4::BinaryFunction::from_inner)
            .map_err(|error| metal_error(&error))
    }

    /// Looks up a compute pipeline, optionally applying dynamic linking.
    pub fn new_compute_pipeline_state(
        &self,
        descriptor: &metal4::ComputePipelineDescriptor,
        linking: Option<&metal4::PipelineStageDynamicLinkingDescriptor>,
    ) -> Result<ComputePipelineState, Error> {
        let result: Result<Retained<AnyObject>, Retained<NSError>> = if let Some(linking) = linking
        {
            require_selector(
                self.inner.as_inner(),
                sel!(newComputePipelineStateWithDescriptor:dynamicLinkingDescriptor:error:),
                "MTL4Archive.newComputePipelineStateWithDescriptor:dynamicLinkingDescriptor:error:",
            )?;
            // SAFETY: selector availability and both live descriptor object
            // contracts were checked; objc2 owns result and error writeback.
            unsafe {
                msg_send![self.inner.as_inner(), newComputePipelineStateWithDescriptor: descriptor.as_inner(), dynamicLinkingDescriptor: linking.as_inner(), error: _]
            }
        } else {
            require_selector(
                self.inner.as_inner(),
                sel!(newComputePipelineStateWithDescriptor:error:),
                "MTL4Archive.newComputePipelineStateWithDescriptor:error:",
            )?;
            // SAFETY: selector availability and descriptor liveness are
            // checked; objc2 owns result and NSError writeback.
            unsafe {
                msg_send![self.inner.as_inner(), newComputePipelineStateWithDescriptor: descriptor.as_inner(), error: _]
            }
        };
        let inner = result.map_err(|error| metal_error(&error))?;
        // SAFETY: both selectors are declared to return an object conforming
        // to MTLComputePipelineState on success.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(ComputePipelineState::new(inner))
    }

    /// Looks up a render pipeline, optionally applying dynamic linking.
    pub fn new_render_pipeline_state(
        &self,
        descriptor: &metal4::PipelineDescriptor,
        linking: Option<&metal4::RenderPipelineDynamicLinkingDescriptor>,
    ) -> Result<RenderPipelineState, Error> {
        let result: Result<Retained<AnyObject>, Retained<NSError>> = if let Some(linking) = linking
        {
            require_selector(
                self.inner.as_inner(),
                sel!(newRenderPipelineStateWithDescriptor:dynamicLinkingDescriptor:error:),
                "MTL4Archive.newRenderPipelineStateWithDescriptor:dynamicLinkingDescriptor:error:",
            )?;
            // SAFETY: selector availability and both descriptor object
            // contracts were checked; objc2 owns result and error writeback.
            unsafe {
                msg_send![self.inner.as_inner(), newRenderPipelineStateWithDescriptor: descriptor.as_inner(), dynamicLinkingDescriptor: linking.as_inner(), error: _]
            }
        } else {
            require_selector(
                self.inner.as_inner(),
                sel!(newRenderPipelineStateWithDescriptor:error:),
                "MTL4Archive.newRenderPipelineStateWithDescriptor:error:",
            )?;
            // SAFETY: selector availability and descriptor liveness are
            // checked; objc2 owns result and NSError writeback.
            unsafe {
                msg_send![self.inner.as_inner(), newRenderPipelineStateWithDescriptor: descriptor.as_inner(), error: _]
            }
        };
        let inner = result.map_err(|error| metal_error(&error))?;
        // SAFETY: both selectors are declared to return an object conforming
        // to MTLRenderPipelineState on success.
        let inner = unsafe { Retained::cast_unchecked(inner) };
        Ok(RenderPipelineState::new(inner))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_accepts_documented_binding_limits() {
        let mut descriptor = ArgumentTableDescriptor::new();
        descriptor
            .set_max_buffer_bind_count(MAX_BUFFER_BIND_COUNT)
            .unwrap();
        descriptor
            .set_max_texture_bind_count(MAX_TEXTURE_BIND_COUNT)
            .unwrap();
        descriptor
            .set_max_sampler_state_bind_count(MAX_SAMPLER_BIND_COUNT)
            .unwrap();
        assert_eq!(descriptor.max_buffer_bind_count(), 31);
        assert_eq!(descriptor.max_texture_bind_count(), 128);
        assert_eq!(descriptor.max_sampler_state_bind_count(), 16);
    }

    #[test]
    fn descriptor_rejects_counts_above_documented_limits() {
        let mut descriptor = ArgumentTableDescriptor::new();
        assert!(descriptor.set_max_buffer_bind_count(32).is_err());
        assert!(descriptor.set_max_texture_bind_count(129).is_err());
        assert!(descriptor.set_max_sampler_state_bind_count(17).is_err());
    }

    #[test]
    fn descriptor_owns_label_and_boolean_state() {
        let mut descriptor = ArgumentTableDescriptor::new();
        descriptor.set_label(Some("bindings"));
        descriptor.set_initialize_bindings(true);
        descriptor.set_support_attribute_strides(true);
        assert_eq!(descriptor.label(), Some("bindings"));
        assert!(descriptor.initialize_bindings());
        assert!(descriptor.support_attribute_strides());
        descriptor.set_label(None);
        assert_eq!(descriptor.label(), None);
    }
}
