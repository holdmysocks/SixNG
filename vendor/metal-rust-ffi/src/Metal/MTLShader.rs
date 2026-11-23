//! Audited shader library and pipeline creation boundary.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::generated_value_types::{
    CompileSymbolVisibility, DataType, FloatingPointConversionRoundingMode, FunctionOptions,
    FunctionType, LanguageVersion, LibraryOptimizationLevel, LibraryType,
    MathFloatingPointFunctions, MathMode, PatchType, PrimitiveTopologyClass, ShaderValidation,
    TessellationControlPointIndexType, TessellationFactorFormat, TessellationFactorStepFunction,
    TessellationPartitionMode, Winding,
};
use crate::metal::{Device, PixelFormat, Size};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSArray, NSDictionary, NSError, NSRange, NSString};
use objc2_metal::{
    MTL4BinaryFunction, MTL4RenderPipelineBinaryFunctionsDescriptor, MTLCompileOptions,
    MTLCompileSymbolVisibility, MTLComputePipelineState, MTLDynamicLibrary, MTLFunction,
    MTLFunctionConstantValues, MTLFunctionDescriptor, MTLIntersectionFunctionDescriptor,
    MTLLanguageVersion, MTLLibrary, MTLLibraryOptimizationLevel, MTLLibraryType,
    MTLMathFloatingPointFunctions, MTLMathMode, MTLPrimitiveTopologyClass,
    MTLRenderPipelineColorAttachmentDescriptorArray, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLShaderValidation, MTLTessellationControlPointIndexType,
    MTLTessellationFactorFormat, MTLTessellationFactorStepFunction, MTLTessellationPartitionMode,
    MTLWinding,
};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;
use std::sync::{Arc, Mutex};

fn function_constant_abi_size(data_type: DataType) -> Option<usize> {
    match data_type.as_raw() {
        3 | 29 | 33 => Some(4),
        4 | 30 | 34 => Some(8),
        5 | 31 | 35 => Some(16),
        6 | 32 | 36 => Some(16),
        7 => Some(16),
        8 => Some(32),
        9 => Some(32),
        10 => Some(24),
        11 => Some(48),
        12 => Some(48),
        13 => Some(32),
        14 => Some(64),
        15 => Some(64),
        16 | 37 | 41 | 121 => Some(2),
        17 | 38 | 42 | 122 => Some(4),
        18 | 39 | 43 | 123 => Some(8),
        19 | 40 | 44 | 124 => Some(8),
        20 => Some(8),
        21 => Some(16),
        22 => Some(16),
        23 => Some(12),
        24 => Some(24),
        25 => Some(24),
        26 => Some(16),
        27 => Some(32),
        28 => Some(32),
        45 | 49 | 53 => Some(1),
        46 | 50 | 54 => Some(2),
        47 | 51 | 55 => Some(4),
        48 | 52 | 56 => Some(4),
        81 | 85 => Some(8),
        82 | 86 => Some(16),
        83 | 87 => Some(32),
        84 | 88 => Some(32),
        _ => None,
    }
}

fn aligned_constant_bytes(bytes: &[u8]) -> Result<Vec<u128>, Error> {
    if bytes.is_empty() {
        return Err(Error::invalid_argument("function constant bytes are empty"));
    }
    let words = bytes
        .len()
        .checked_add(15)
        .ok_or_else(|| Error::invalid_argument("function constant byte length overflow"))?
        / 16;
    let mut storage = vec![0_u128; words];
    // SAFETY: storage owns words * 16 initialized bytes and that capacity was
    // rounded up from bytes.len(); the non-overlapping source is valid for the
    // slice length. u128 storage also provides the maximum 16-byte SIMD ABI alignment.
    unsafe {
        std::ptr::copy_nonoverlapping(
            bytes.as_ptr(),
            storage.as_mut_ptr().cast::<u8>(),
            bytes.len(),
        );
    }
    Ok(storage)
}

fn validate_function_constant_bytes(
    data_type: DataType,
    bytes: &[u8],
    count: usize,
) -> Result<(), Error> {
    let components = match data_type.as_raw() {
        53 => 1,
        54 => 2,
        55 => 3,
        56 => 4,
        _ => return Ok(()),
    };
    let stride = function_constant_abi_size(data_type)
        .ok_or_else(|| Error::invalid_argument("invalid function constant type"))?;
    for item in 0..count {
        let start = item
            .checked_mul(stride)
            .ok_or_else(|| Error::invalid_argument("function constant byte offset overflow"))?;
        if bytes[start..start + components]
            .iter()
            .any(|value| *value > 1)
        {
            return Err(Error::invalid_argument(
                "boolean function constants must contain only 0 or 1",
            ));
        }
    }
    Ok(())
}

fn callback_function(value: *mut AnyObject, error: *mut NSError) -> Result<Function, Error> {
    // SAFETY: Objective-C completion blocks pass either null or a valid NSError
    // object borrowed for the duration of this callback.
    if let Some(error) = unsafe { error.as_ref() } {
        return Err(crate::foundation::metal_error(error));
    }
    // SAFETY: a non-null completion value is an Objective-C object kept alive
    // for the callback; retaining transfers an owned reference to Rust.
    let value = unsafe { Retained::retain(value) }
        .ok_or_else(|| Error::unsupported("Metal completed without a function or NSError"))?;
    Function::from_any_object(value)
}

/// An owned Metal shader library.
#[derive(Clone)]
pub struct Library {
    pub(super) inner: Retained<ProtocolObject<dyn MTLLibrary>>,
    _thread_bound: ThreadBound,
}

impl Library {
    pub(super) const fn new(inner: Retained<ProtocolObject<dyn MTLLibrary>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    pub(crate) fn from_any_object(inner: Retained<AnyObject>) -> Result<Self, Error> {
        // SAFETY: callers only bridge selectors declared to return MTLLibrary.
        Ok(Self {
            inner: unsafe { Retained::cast_unchecked(inner) },
            _thread_bound: ThreadBound::new(),
        })
    }

    pub(crate) fn as_any_object(&self) -> &AnyObject {
        // SAFETY: MTLLibrary is backed by this same Objective-C object pointer.
        unsafe { &*(std::ptr::from_ref(&*self.inner).cast::<AnyObject>()) }
    }

    /// Looks up a named shader function.
    pub fn function(&self, name: &str) -> Result<Function, Error> {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("shader function name is invalid"));
        }
        let name = NSString::from_str(name);
        self.inner
            .newFunctionWithName(&name)
            .map(Function::new)
            .ok_or_else(|| Error::unsupported("the shader function was not found"))
    }

    /// Returns the names of all functions in this library as owned Rust strings.
    #[must_use]
    pub fn function_names(&self) -> Vec<String> {
        self.inner
            .functionNames()
            .iter()
            .map(|value| value.to_string())
            .collect()
    }

    /// Creates a specialized function from named constant values.
    pub fn specialized_function(
        &self,
        name: &str,
        constants: &crate::metal::generated_object_types::metal::FunctionConstantValues,
    ) -> Result<Function, Error> {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("shader function name is invalid"));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newFunctionWithName:constantValues:error:))
        {
            return Err(Error::unsupported("function specialization is unavailable"));
        }
        let name = NSString::from_str(name);
        // SAFETY: the generated wrapper is created only for the
        // MTLFunctionConstantValues Objective-C class, so the reference cast
        // preserves object identity and the declared class ABI.
        let constants = unsafe {
            &*(std::ptr::from_ref(constants.as_inner()).cast::<MTLFunctionConstantValues>())
        };
        self.inner
            .newFunctionWithName_constantValues_error(&name, constants)
            .map(Function::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Starts one-shot asynchronous specialization from named constants.
    pub fn specialized_function_async(
        &self,
        name: &str,
        constants: &crate::metal::generated_object_types::metal::FunctionConstantValues,
        handler: impl FnOnce(Result<Function, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("shader function name is invalid"));
        }
        if !self.inner.respondsToSelector(sel!(
            newFunctionWithName:constantValues:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous function specialization is unavailable",
            ));
        }
        let name = NSString::from_str(name);
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else { return };
            let result = callback_function(value, error);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; name/constants remain
        // alive for the call and RcBlock has the declared two-object ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newFunctionWithName: &*name,
                constantValues: constants.as_inner(),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Creates a function from a validated function descriptor.
    pub fn function_with_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::FunctionDescriptor,
    ) -> Result<Function, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newFunctionWithDescriptor:error:))
        {
            return Err(Error::unsupported(
                "descriptor-based function creation is unavailable",
            ));
        }
        // SAFETY: the generated wrapper is created only for the
        // MTLFunctionDescriptor Objective-C class.
        let descriptor = unsafe {
            &*(std::ptr::from_ref(descriptor.as_inner()).cast::<MTLFunctionDescriptor>())
        };
        self.inner
            .newFunctionWithDescriptor_error(descriptor)
            .map(Function::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Starts one-shot asynchronous descriptor-based function creation.
    pub fn function_with_descriptor_async(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::FunctionDescriptor,
        handler: impl FnOnce(Result<Function, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newFunctionWithDescriptor:completionHandler:))
        {
            return Err(Error::unsupported(
                "asynchronous descriptor-based function creation is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else { return };
            let result = callback_function(value, error);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; descriptor remains alive
        // for the call and RcBlock has the declared two-object ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newFunctionWithDescriptor: descriptor.as_inner(),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Creates an intersection function from a validated descriptor.
    pub fn intersection_function_with_descriptor(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::IntersectionFunctionDescriptor,
    ) -> Result<Function, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newIntersectionFunctionWithDescriptor:error:))
        {
            return Err(Error::unsupported(
                "intersection-function creation is unavailable",
            ));
        }
        // SAFETY: the generated wrapper is created only for the
        // MTLIntersectionFunctionDescriptor Objective-C class.
        let descriptor = unsafe {
            &*(std::ptr::from_ref(descriptor.as_inner())
                .cast::<MTLIntersectionFunctionDescriptor>())
        };
        self.inner
            .newIntersectionFunctionWithDescriptor_error(descriptor)
            .map(Function::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Starts one-shot asynchronous intersection-function creation.
    pub fn intersection_function_with_descriptor_async(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::IntersectionFunctionDescriptor,
        handler: impl FnOnce(Result<Function, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(
            newIntersectionFunctionWithDescriptor:completionHandler:
        )) {
            return Err(Error::unsupported(
                "asynchronous intersection-function creation is unavailable",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else { return };
            let result = callback_function(value, error);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; descriptor remains alive
        // for the call and RcBlock has the declared two-object ABI.
        unsafe {
            let _: () = msg_send![&*self.inner,
                newIntersectionFunctionWithDescriptor: descriptor.as_inner(),
                completionHandler: &*block
            ];
        }
        Ok(())
    }

    /// Returns reflection metadata for a named library function.
    pub fn reflection_for_function(
        &self,
        name: &str,
    ) -> Result<Option<crate::metal::generated_object_types::metal::FunctionReflection>, Error>
    {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("shader function name is invalid"));
        }
        if !self
            .inner
            .respondsToSelector(sel!(reflectionForFunctionWithName:))
        {
            return Err(Error::unsupported("function reflection is unavailable"));
        }
        let name = NSString::from_str(name);
        Ok(self
            .inner
            .reflectionForFunctionWithName(&name)
            .map(|value| {
                // SAFETY: the typed API returned an owned MTLFunctionReflection.
                let value = unsafe { Retained::cast_unchecked(value) };
                crate::metal::generated_object_types::metal::FunctionReflection::from_inner(value)
            }))
    }

    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }
    /// Returns the optional install name.
    #[must_use]
    pub fn install_name(&self) -> Option<String> {
        self.inner.installName().map(|value| value.to_string())
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|value| value.to_string())
    }
    /// Sets or clears the label.
    pub fn set_label(&self, value: Option<&str>) {
        let value = value.map(NSString::from_str);
        self.inner.setLabel(value.as_deref());
    }
    /// Returns the library type.
    #[must_use]
    pub fn library_type(&self) -> LibraryType {
        LibraryType::from_system_raw(self.inner.r#type().0)
    }
}

/// A named function inside a Metal library.
#[derive(Clone)]
pub struct Function {
    pub(super) inner: Retained<ProtocolObject<dyn MTLFunction>>,
    _thread_bound: ThreadBound,
}

impl Function {
    pub(super) const fn new(inner: Retained<ProtocolObject<dyn MTLFunction>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    pub(crate) fn from_any_object(inner: Retained<AnyObject>) -> Result<Self, Error> {
        // SAFETY: callers only bridge selectors declared to return MTLFunction.
        Ok(Self {
            inner: unsafe { Retained::cast_unchecked(inner) },
            _thread_bound: ThreadBound::new(),
        })
    }

    pub(crate) fn as_any_object(&self) -> &AnyObject {
        // SAFETY: MTLFunction is backed by this same Objective-C object pointer.
        unsafe { &*(std::ptr::from_ref(&*self.inner).cast::<AnyObject>()) }
    }

    /// Returns the function's Metal name.
    #[must_use]
    pub fn name(&self) -> String {
        self.inner.name().to_string()
    }

    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|value| value.to_string())
    }
    /// Sets or clears the label.
    pub fn set_label(&self, value: Option<&str>) {
        let value = value.map(NSString::from_str);
        self.inner.setLabel(value.as_deref());
    }
    /// Returns the function type.
    #[must_use]
    pub fn function_type(&self) -> FunctionType {
        FunctionType::from_system_raw(self.inner.functionType().0)
    }
    /// Returns function option flags.
    #[must_use]
    pub fn options(&self) -> FunctionOptions {
        FunctionOptions::from_system_raw(self.inner.options().0)
    }
    /// Returns the patch control-point count.
    #[must_use]
    pub fn patch_control_point_count(&self) -> isize {
        self.inner.patchControlPointCount()
    }
    /// Returns the patch type.
    #[must_use]
    pub fn patch_type(&self) -> PatchType {
        PatchType::from_system_raw(self.inner.patchType().0)
    }

    /// Returns the declared function-constant names as owned Rust strings.
    #[must_use]
    pub fn function_constant_names(&self) -> Vec<String> {
        self.inner
            .functionConstantsDictionary()
            .keys()
            .map(|value| value.to_string())
            .collect()
    }

    /// Returns the names of declared vertex attributes.
    #[must_use]
    pub fn vertex_attribute_names(&self) -> Vec<String> {
        self.inner
            .vertexAttributes()
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.name().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Returns the names of declared stage-input attributes.
    #[must_use]
    pub fn stage_input_attribute_names(&self) -> Vec<String> {
        self.inner
            .stageInputAttributes()
            .map(|values| {
                values
                    .iter()
                    .map(|value| value.name().to_string())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A compiled render pipeline state.
#[derive(Clone)]
pub struct RenderPipelineState {
    pub(super) inner: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    _thread_bound: ThreadBound,
}

impl RenderPipelineState {
    pub(super) const fn new(inner: Retained<ProtocolObject<dyn MTLRenderPipelineState>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the maximum number of threads per threadgroup.
    #[must_use]
    pub fn max_total_threads_per_threadgroup(&self) -> usize {
        self.inner.maxTotalThreadsPerThreadgroup()
    }
    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|value| value.to_string())
    }
    /// Returns indirect-command-buffer support.
    #[must_use]
    pub fn supports_indirect_command_buffers(&self) -> bool {
        self.inner.supportIndirectCommandBuffers()
    }
    /// Returns whether tile threadgroup size matches tile size.
    #[must_use]
    pub fn threadgroup_size_matches_tile_size(&self) -> bool {
        self.inner.threadgroupSizeMatchesTileSize()
    }
    /// Returns imageblock sample length.
    #[must_use]
    pub fn imageblock_sample_length(&self) -> usize {
        self.inner.imageblockSampleLength()
    }
    /// Returns shader-validation mode.
    pub fn shader_validation(&self) -> Result<ShaderValidation, Error> {
        if !self.inner.respondsToSelector(sel!(shaderValidation)) {
            return Err(Error::unsupported(
                "pipeline shader validation is unavailable",
            ));
        }
        // SAFETY: selector availability and the MTLShaderValidation integer
        // return ABI were checked against the SDK declaration.
        Ok(ShaderValidation::from_system_raw(
            unsafe { self.inner.shaderValidation() }.0,
        ))
    }
    /// Returns tile, mesh, and object required threadgroup sizes.
    #[must_use]
    pub fn required_threadgroups(&self) -> (Size, Size, Size) {
        let tile = self.inner.requiredThreadsPerTileThreadgroup();
        let mesh = self.inner.requiredThreadsPerMeshThreadgroup();
        let object = self.inner.requiredThreadsPerObjectThreadgroup();
        (
            Size::new(tile.width, tile.height, tile.depth),
            Size::new(mesh.width, mesh.height, mesh.depth),
            Size::new(object.width, object.height, object.depth),
        )
    }

    /// Returns the imageblock memory length for non-zero dimensions.
    pub fn imageblock_memory_length(&self, dimensions: Size) -> Result<usize, Error> {
        if dimensions.width == 0 || dimensions.height == 0 || dimensions.depth == 0 {
            return Err(Error::invalid_argument(
                "imageblock dimensions must be non-zero",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(imageblockMemoryLengthForDimensions:))
        {
            return Err(Error::unsupported(
                "pipeline imageblock memory queries are unavailable",
            ));
        }
        Ok(self
            .inner
            .imageblockMemoryLengthForDimensions(dimensions.into()))
    }

    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<u64, Error> {
        if !self.inner.respondsToSelector(sel!(gpuResourceID)) {
            return Err(Error::unsupported(
                "pipeline GPU resource identifiers are unavailable",
            ));
        }
        let value = self.inner.gpuResourceID();
        // SAFETY: MTLResourceID is declared repr(C) with exactly one u64
        // field. Reading that field preserves the opaque identifier bits.
        Ok(unsafe { std::ptr::read_unaligned(std::ptr::from_ref(&value).cast::<u64>()) })
    }

    /// Returns the maximum object-shader threadgroup size when supported.
    pub fn max_total_threads_per_object_threadgroup(&self) -> Result<usize, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(maxTotalThreadsPerObjectThreadgroup))
        {
            return Err(Error::unsupported("object shaders are unavailable"));
        }
        Ok(self.inner.maxTotalThreadsPerObjectThreadgroup())
    }

    /// Returns the maximum mesh-shader threadgroup size when supported.
    pub fn max_total_threads_per_mesh_threadgroup(&self) -> Result<usize, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(maxTotalThreadsPerMeshThreadgroup))
        {
            return Err(Error::unsupported("mesh shaders are unavailable"));
        }
        Ok(self.inner.maxTotalThreadsPerMeshThreadgroup())
    }

    /// Returns the object-shader SIMD width when supported.
    pub fn object_thread_execution_width(&self) -> Result<usize, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(objectThreadExecutionWidth))
        {
            return Err(Error::unsupported("object shaders are unavailable"));
        }
        Ok(self.inner.objectThreadExecutionWidth())
    }

    /// Returns the mesh-shader SIMD width when supported.
    pub fn mesh_thread_execution_width(&self) -> Result<usize, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(meshThreadExecutionWidth))
        {
            return Err(Error::unsupported("mesh shaders are unavailable"));
        }
        Ok(self.inner.meshThreadExecutionWidth())
    }

    /// Returns the maximum number of threadgroups in a mesh grid.
    pub fn max_total_threadgroups_per_mesh_grid(&self) -> Result<usize, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(maxTotalThreadgroupsPerMeshGrid))
        {
            return Err(Error::unsupported("mesh shaders are unavailable"));
        }
        Ok(self.inner.maxTotalThreadgroupsPerMeshGrid())
    }

    /// Returns reflection data when the pipeline retained it.
    pub fn reflection(
        &self,
    ) -> Result<Option<crate::metal::generated_object_types::metal::RenderPipelineReflection>, Error>
    {
        if !self.inner.respondsToSelector(sel!(reflection)) {
            return Err(Error::unsupported("pipeline reflection is unavailable"));
        }
        Ok(self.inner.reflection().map(|value| {
            // SAFETY: objc2's typed return guarantees that this retained object
            // implements the declared MTLRenderPipelineReflection class.
            let value = unsafe { Retained::cast_unchecked(value) };
            crate::metal::generated_object_types::metal::RenderPipelineReflection::from_inner(value)
        }))
    }

    /// Looks up a linked function handle by name and render stage.
    pub fn function_handle_by_name(
        &self,
        name: &str,
        stage: crate::metal::generated_value_types::RenderStages,
    ) -> Result<Option<crate::metal::generated_object_types::metal::FunctionHandle>, Error> {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("function handle name is invalid"));
        }
        if !stage.is_valid() {
            return Err(Error::invalid_argument("render stage flags are invalid"));
        }
        if !self
            .inner
            .respondsToSelector(sel!(functionHandleWithName:stage:))
        {
            return Err(Error::unsupported("function handles are unavailable"));
        }
        let name = NSString::from_str(name);
        let raw = stage.as_raw();
        // SAFETY: selector availability was checked, NSString is the declared
        // object argument, RenderStages has validated option bits, and the
        // returned object is retained before entering the safe wrapper.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*self.inner, functionHandleWithName: &*name, stage: raw] };
        Ok(value.map(crate::metal::generated_object_types::metal::FunctionHandle::from_inner))
    }

    /// Looks up a linked handle using a canonical Metal function's name.
    pub fn function_handle_for_function(
        &self,
        function: &Function,
        stage: crate::metal::generated_value_types::RenderStages,
    ) -> Result<Option<crate::metal::generated_object_types::metal::FunctionHandle>, Error> {
        self.function_handle_by_name(&function.name(), stage)
    }

    /// Looks up a linked Metal 4 binary function by its declared name.
    pub fn function_handle_for_binary_function(
        &self,
        function: &crate::metal::generated_object_types::metal4::BinaryFunction,
        stage: crate::metal::generated_value_types::RenderStages,
    ) -> Result<Option<crate::metal::generated_object_types::metal::FunctionHandle>, Error> {
        let name = function
            .name()?
            .ok_or_else(|| Error::invalid_argument("binary function has no name"))?;
        self.function_handle_by_name(&name, stage)
    }

    /// Creates a visible function table for a validated render stage.
    pub fn new_visible_function_table(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::VisibleFunctionTableDescriptor,
        stage: crate::metal::generated_value_types::RenderStages,
    ) -> Result<Option<crate::metal::generated_object_types::metal::VisibleFunctionTable>, Error>
    {
        if !stage.is_valid() {
            return Err(Error::invalid_argument("render stage flags are invalid"));
        }
        if !self.inner.respondsToSelector(sel!(
            newVisibleFunctionTableWithDescriptor:stage:
        )) {
            return Err(Error::unsupported(
                "visible function tables are unavailable",
            ));
        }
        // SAFETY: selector availability, validated option bits, and the exact
        // object/integer/object return ABI are established above.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![&*self.inner,
                newVisibleFunctionTableWithDescriptor: descriptor.as_inner(),
                stage: stage.as_raw()
            ]
        };
        Ok(
            value
                .map(crate::metal::generated_object_types::metal::VisibleFunctionTable::from_inner),
        )
    }

    /// Creates an intersection function table for a validated render stage.
    pub fn new_intersection_function_table(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::IntersectionFunctionTableDescriptor,
        stage: crate::metal::generated_value_types::RenderStages,
    ) -> Result<Option<crate::metal::generated_object_types::metal::IntersectionFunctionTable>, Error>
    {
        if !stage.is_valid() {
            return Err(Error::invalid_argument("render stage flags are invalid"));
        }
        if !self.inner.respondsToSelector(sel!(
            newIntersectionFunctionTableWithDescriptor:stage:
        )) {
            return Err(Error::unsupported(
                "intersection function tables are unavailable",
            ));
        }
        // SAFETY: selector availability, validated option bits, and the exact
        // object/integer/object return ABI are established above.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![&*self.inner,
                newIntersectionFunctionTableWithDescriptor: descriptor.as_inner(),
                stage: stage.as_raw()
            ]
        };
        Ok(value.map(
            crate::metal::generated_object_types::metal::IntersectionFunctionTable::from_inner,
        ))
    }

    /// Returns a Metal 4 specialization descriptor when supported.
    pub fn new_render_pipeline_descriptor(
        &self,
    ) -> Result<crate::metal::generated_object_types::metal4::PipelineDescriptor, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newRenderPipelineDescriptorForSpecialization))
        {
            return Err(Error::unsupported(
                "render pipeline specialization descriptors are unavailable",
            ));
        }
        // SAFETY: selector availability was checked; new-family semantics
        // return a retained MTL4PipelineDescriptor object.
        let value: Retained<AnyObject> =
            unsafe { msg_send![&*self.inner, newRenderPipelineDescriptorForSpecialization] };
        Ok(crate::metal::generated_object_types::metal4::PipelineDescriptor::from_inner(value))
    }

    /// Creates a derived pipeline from Metal 4 binary functions.
    pub fn with_metal4_binary_functions(
        &self,
        descriptor: &crate::metal::generated_object_types::metal4::RenderPipelineBinaryFunctionsDescriptor,
    ) -> Result<Self, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(newRenderPipelineStateWithBinaryFunctions:error:))
        {
            return Err(Error::unsupported(
                "Metal 4 render pipeline binary linking is unavailable",
            ));
        }
        // SAFETY: the generated wrapper preserves the exact declared Metal 4 descriptor class.
        let descriptor = unsafe {
            &*(std::ptr::from_ref(descriptor.as_inner())
                .cast::<MTL4RenderPipelineBinaryFunctionsDescriptor>())
        };
        self.inner
            .newRenderPipelineStateWithBinaryFunctions_error(descriptor)
            .map(Self::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Creates a derived render pipeline with additional binary functions.
    pub fn with_additional_binary_functions(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::RenderPipelineFunctionsDescriptor,
    ) -> Result<Self, Error> {
        if !self.inner.respondsToSelector(sel!(
            newRenderPipelineStateWithAdditionalBinaryFunctions:error:
        )) {
            return Err(Error::unsupported(
                "render pipeline binary-function linking is unavailable",
            ));
        }
        // SAFETY: the generated wrapper is created only for the
        // MTLRenderPipelineFunctionsDescriptor Objective-C class.
        let descriptor = unsafe {
            &*(std::ptr::from_ref(descriptor.as_inner())
                .cast::<objc2_metal::MTLRenderPipelineFunctionsDescriptor>())
        };
        self.inner
            .newRenderPipelineStateWithAdditionalBinaryFunctions_error(descriptor)
            .map(Self::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }
}

/// A compiled compute pipeline state.
#[derive(Clone)]
pub struct ComputePipelineState {
    pub(super) inner: Retained<ProtocolObject<dyn MTLComputePipelineState>>,
    _thread_bound: ThreadBound,
}

impl ComputePipelineState {
    pub(super) const fn new(inner: Retained<ProtocolObject<dyn MTLComputePipelineState>>) -> Self {
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the execution width of the compute pipeline.
    #[must_use]
    pub fn execution_width(&self) -> usize {
        self.inner.threadExecutionWidth()
    }

    /// Returns the maximum number of threads per threadgroup.
    #[must_use]
    pub fn max_total_threads_per_threadgroup(&self) -> usize {
        self.inner.maxTotalThreadsPerThreadgroup()
    }
    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> Device {
        Device::from_inner(self.inner.device())
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|value| value.to_string())
    }
    /// Returns indirect-command-buffer support.
    #[must_use]
    pub fn supports_indirect_command_buffers(&self) -> bool {
        self.inner.supportIndirectCommandBuffers()
    }
    /// Returns static threadgroup memory length.
    #[must_use]
    pub fn static_threadgroup_memory_length(&self) -> usize {
        self.inner.staticThreadgroupMemoryLength()
    }
    /// Returns required threads per threadgroup.
    #[must_use]
    pub fn required_threads_per_threadgroup(&self) -> Size {
        let value = self.inner.requiredThreadsPerThreadgroup();
        Size::new(value.width, value.height, value.depth)
    }
    /// Returns shader-validation mode.
    pub fn shader_validation(&self) -> Result<ShaderValidation, Error> {
        if !self.inner.respondsToSelector(sel!(shaderValidation)) {
            return Err(Error::unsupported(
                "pipeline shader validation is unavailable",
            ));
        }
        // SAFETY: selector availability and the MTLShaderValidation integer
        // return ABI were checked against the SDK declaration.
        Ok(ShaderValidation::from_system_raw(
            unsafe { self.inner.shaderValidation() }.0,
        ))
    }

    /// Returns the imageblock memory length for non-zero dimensions.
    pub fn imageblock_memory_length(&self, dimensions: Size) -> Result<usize, Error> {
        if dimensions.width == 0 || dimensions.height == 0 || dimensions.depth == 0 {
            return Err(Error::invalid_argument(
                "imageblock dimensions must be non-zero",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(imageblockMemoryLengthForDimensions:))
        {
            return Err(Error::unsupported(
                "pipeline imageblock memory queries are unavailable",
            ));
        }
        // SAFETY: the selector was checked and non-zero dimensions eliminate
        // the documented invalid empty imageblock case. Metal validates any
        // device-specific size limits without exposing memory to Rust.
        Ok(unsafe {
            self.inner
                .imageblockMemoryLengthForDimensions(dimensions.into())
        })
    }

    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<u64, Error> {
        if !self.inner.respondsToSelector(sel!(gpuResourceID)) {
            return Err(Error::unsupported(
                "pipeline GPU resource identifiers are unavailable",
            ));
        }
        let value = self.inner.gpuResourceID();
        // SAFETY: MTLResourceID is declared repr(C) with exactly one u64
        // field. Reading that field preserves the opaque identifier bits.
        Ok(unsafe { std::ptr::read_unaligned(std::ptr::from_ref(&value).cast::<u64>()) })
    }

    /// Returns reflection data when the pipeline retained it.
    pub fn reflection(
        &self,
    ) -> Result<Option<crate::metal::generated_object_types::metal::ComputePipelineReflection>, Error>
    {
        if !self.inner.respondsToSelector(sel!(reflection)) {
            return Err(Error::unsupported("pipeline reflection is unavailable"));
        }
        Ok(self.inner.reflection().map(|value| {
            // SAFETY: objc2's typed return guarantees that this retained object
            // implements the declared MTLComputePipelineReflection class.
            let value = unsafe { Retained::cast_unchecked(value) };
            crate::metal::generated_object_types::metal::ComputePipelineReflection::from_inner(
                value,
            )
        }))
    }

    /// Looks up a linked function handle by name.
    pub fn function_handle_by_name(
        &self,
        name: &str,
    ) -> Result<Option<crate::metal::generated_object_types::metal::FunctionHandle>, Error> {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("function handle name is invalid"));
        }
        if !self.inner.respondsToSelector(sel!(functionHandleWithName:)) {
            return Err(Error::unsupported("function handles are unavailable"));
        }
        let name = NSString::from_str(name);
        // SAFETY: selector availability was checked, NSString is the declared
        // argument, and the returned object is retained before wrapping.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![&*self.inner, functionHandleWithName: &*name] };
        Ok(value.map(crate::metal::generated_object_types::metal::FunctionHandle::from_inner))
    }

    /// Looks up a linked handle using a canonical Metal function's name.
    pub fn function_handle_for_function(
        &self,
        function: &Function,
    ) -> Result<Option<crate::metal::generated_object_types::metal::FunctionHandle>, Error> {
        self.function_handle_by_name(&function.name())
    }

    /// Looks up a linked Metal 4 binary function by its declared name.
    pub fn function_handle_for_binary_function(
        &self,
        function: &crate::metal::generated_object_types::metal4::BinaryFunction,
    ) -> Result<Option<crate::metal::generated_object_types::metal::FunctionHandle>, Error> {
        let name = function
            .name()?
            .ok_or_else(|| Error::invalid_argument("binary function has no name"))?;
        self.function_handle_by_name(&name)
    }

    /// Creates a visible function table.
    pub fn new_visible_function_table(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::VisibleFunctionTableDescriptor,
    ) -> Result<Option<crate::metal::generated_object_types::metal::VisibleFunctionTable>, Error>
    {
        if !self
            .inner
            .respondsToSelector(sel!(newVisibleFunctionTableWithDescriptor:))
        {
            return Err(Error::unsupported(
                "visible function tables are unavailable",
            ));
        }
        // SAFETY: selector availability and the exact object argument/return ABI were checked.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![&*self.inner, newVisibleFunctionTableWithDescriptor: descriptor.as_inner()]
        };
        Ok(
            value
                .map(crate::metal::generated_object_types::metal::VisibleFunctionTable::from_inner),
        )
    }

    /// Creates an intersection function table.
    pub fn new_intersection_function_table(
        &self,
        descriptor: &crate::metal::generated_object_types::metal::IntersectionFunctionTableDescriptor,
    ) -> Result<Option<crate::metal::generated_object_types::metal::IntersectionFunctionTable>, Error>
    {
        if !self
            .inner
            .respondsToSelector(sel!(newIntersectionFunctionTableWithDescriptor:))
        {
            return Err(Error::unsupported(
                "intersection function tables are unavailable",
            ));
        }
        // SAFETY: selector availability and the exact object argument/return ABI were checked.
        let value: Option<Retained<AnyObject>> = unsafe {
            msg_send![&*self.inner, newIntersectionFunctionTableWithDescriptor: descriptor.as_inner()]
        };
        Ok(value.map(
            crate::metal::generated_object_types::metal::IntersectionFunctionTable::from_inner,
        ))
    }

    /// Creates a derived compute pipeline from Metal 4 binary functions.
    pub fn with_metal4_binary_functions(
        &self,
        functions: &[crate::metal::generated_object_types::metal4::BinaryFunction],
    ) -> Result<Self, Error> {
        if functions.is_empty() {
            return Err(Error::invalid_argument(
                "at least one binary function is required",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(newComputePipelineStateWithBinaryFunctions:error:))
        {
            return Err(Error::unsupported(
                "Metal 4 compute pipeline binary linking is unavailable",
            ));
        }
        let functions: Vec<Retained<ProtocolObject<dyn MTL4BinaryFunction>>> = functions
            .iter()
            .map(|function| {
                // SAFETY: the wrapper owns a live MTL4BinaryFunction; retain
                // extends its lifetime and the cast restores the declared protocol type.
                let retained =
                    unsafe { Retained::retain(std::ptr::from_ref(function.as_inner()).cast_mut()) }
                        .expect("a borrowed Objective-C wrapper cannot be null");
                // SAFETY: generated wrapper identity is MTL4BinaryFunction.
                unsafe { Retained::cast_unchecked(retained) }
            })
            .collect();
        let functions = NSArray::from_retained_slice(&functions);
        self.inner
            .newComputePipelineStateWithBinaryFunctions_error(&functions)
            .map(Self::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }

    /// Creates a derived compute pipeline with additional Metal functions.
    pub fn with_additional_functions(&self, functions: &[Function]) -> Result<Self, Error> {
        if functions.is_empty() {
            return Err(Error::invalid_argument(
                "at least one additional function is required",
            ));
        }
        if !self.inner.respondsToSelector(sel!(
            newComputePipelineStateWithAdditionalBinaryFunctions:error:
        )) {
            return Err(Error::unsupported(
                "compute pipeline binary-function linking is unavailable",
            ));
        }
        let functions: Vec<Retained<ProtocolObject<dyn MTLFunction>>> =
            functions.iter().map(|value| value.inner.clone()).collect();
        let functions = NSArray::from_retained_slice(&functions);
        self.inner
            .newComputePipelineStateWithAdditionalBinaryFunctions_error(&functions)
            .map(Self::new)
            .map_err(|error| crate::foundation::metal_error(&error))
    }
}

/// A validated render-pipeline descriptor.
pub struct RenderPipelineDescriptor {
    pub(super) inner: Retained<MTLRenderPipelineDescriptor>,
    _thread_bound: ThreadBound,
}

/// Copyable scalar options for a render pipeline descriptor.
#[derive(Clone, Copy, Debug)]
pub struct RenderPipelineOptions {
    /// Enables multisample alpha-to-coverage conversion.
    pub alpha_to_coverage: bool,
    /// Enables multisample alpha-to-one conversion.
    pub alpha_to_one: bool,
    /// Enables rasterization after vertex processing.
    pub rasterization_enabled: bool,
    /// Enables per-draw tessellation-factor scaling.
    pub tessellation_factor_scale_enabled: bool,
    /// Allows adding fragment binary functions after pipeline creation.
    pub support_adding_fragment_binary_functions: bool,
    /// Allows adding vertex binary functions after pipeline creation.
    pub support_adding_vertex_binary_functions: bool,
    /// Allows the pipeline to be referenced from indirect command buffers.
    pub support_indirect_command_buffers: bool,
    /// Maximum fragment-function call-stack depth.
    pub max_fragment_call_stack_depth: usize,
    /// Maximum tessellation factor accepted by the pipeline.
    pub max_tessellation_factor: usize,
    /// Maximum vertex amplification count accepted by the pipeline.
    pub max_vertex_amplification_count: usize,
    /// Maximum vertex-function call-stack depth.
    pub max_vertex_call_stack_depth: usize,
    /// Raster sample count.
    pub raster_sample_count: usize,
    /// Input primitive-topology class.
    pub input_primitive_topology: PrimitiveTopologyClass,
    /// Depth attachment pixel format.
    pub depth_attachment_pixel_format: PixelFormat,
    /// Stencil attachment pixel format.
    pub stencil_attachment_pixel_format: PixelFormat,
    /// Runtime shader-validation policy.
    pub shader_validation: ShaderValidation,
    /// Tessellation control-point index representation.
    pub tessellation_control_point_index_type: TessellationControlPointIndexType,
    /// Tessellation factor representation.
    pub tessellation_factor_format: TessellationFactorFormat,
    /// Tessellation factor stepping rule.
    pub tessellation_factor_step_function: TessellationFactorStepFunction,
    /// Tessellation output winding order.
    pub tessellation_output_winding_order: Winding,
    /// Tessellation partitioning rule.
    pub tessellation_partition_mode: TessellationPartitionMode,
}

impl RenderPipelineDescriptor {
    /// Creates a descriptor with Metal defaults.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            inner: MTLRenderPipelineDescriptor::new(),
            _thread_bound: ThreadBound::new(),
        }
    }
    /// Creates a descriptor from a vertex and fragment function.
    #[must_use]
    pub fn new(vertex: &Function, fragment: Option<&Function>, color_format: PixelFormat) -> Self {
        let inner = MTLRenderPipelineDescriptor::new();
        inner.setVertexFunction(Some(&vertex.inner));
        inner.setFragmentFunction(fragment.map(|function| &*function.inner));
        inner.setRasterSampleCount(1);
        let attachments: Retained<MTLRenderPipelineColorAttachmentDescriptorArray> =
            inner.colorAttachments();
        // SAFETY: the Metal render pipeline color attachment array has eight
        // entries; index zero is always valid.
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setPixelFormat(color_format.as_objc());
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label().map(|value| value.to_string())
    }
    /// Sets or clears the label.
    pub fn set_label(&self, value: Option<&str>) {
        let value = value.map(NSString::from_str);
        self.inner.setLabel(value.as_deref());
    }
    /// Returns the assigned vertex function.
    #[must_use]
    pub fn vertex_function(&self) -> Option<Function> {
        self.inner.vertexFunction().map(Function::new)
    }
    /// Sets or clears the vertex function.
    pub fn set_vertex_function(&self, value: Option<&Function>) {
        self.inner
            .setVertexFunction(value.map(|value| &*value.inner));
    }
    /// Returns the assigned fragment function.
    #[must_use]
    pub fn fragment_function(&self) -> Option<Function> {
        self.inner.fragmentFunction().map(Function::new)
    }
    /// Sets or clears the fragment function.
    pub fn set_fragment_function(&self, value: Option<&Function>) {
        self.inner
            .setFragmentFunction(value.map(|value| &*value.inner));
    }
    /// Resets the descriptor to Metal defaults.
    pub fn reset(&self) {
        self.inner.reset();
    }

    /// Returns all scalar descriptor options when shader validation is available.
    pub fn options(&self) -> Result<RenderPipelineOptions, Error> {
        if !self.inner.respondsToSelector(sel!(shaderValidation)) {
            return Err(Error::unsupported(
                "render pipeline shader validation is unavailable",
            ));
        }
        let shader_validation = self.inner.shaderValidation();
        Ok(RenderPipelineOptions {
            alpha_to_coverage: self.inner.isAlphaToCoverageEnabled(),
            alpha_to_one: self.inner.isAlphaToOneEnabled(),
            rasterization_enabled: self.inner.isRasterizationEnabled(),
            tessellation_factor_scale_enabled: self.inner.isTessellationFactorScaleEnabled(),
            support_adding_fragment_binary_functions: self
                .inner
                .supportAddingFragmentBinaryFunctions(),
            support_adding_vertex_binary_functions: self.inner.supportAddingVertexBinaryFunctions(),
            support_indirect_command_buffers: self.inner.supportIndirectCommandBuffers(),
            max_fragment_call_stack_depth: self.inner.maxFragmentCallStackDepth(),
            max_tessellation_factor: self.inner.maxTessellationFactor(),
            max_vertex_amplification_count: self.inner.maxVertexAmplificationCount(),
            max_vertex_call_stack_depth: self.inner.maxVertexCallStackDepth(),
            raster_sample_count: self.inner.rasterSampleCount(),
            input_primitive_topology: PrimitiveTopologyClass::from_system_raw(
                self.inner.inputPrimitiveTopology().0,
            ),
            depth_attachment_pixel_format: PixelFormat::from_system_raw(
                self.inner.depthAttachmentPixelFormat().0,
            ),
            stencil_attachment_pixel_format: PixelFormat::from_system_raw(
                self.inner.stencilAttachmentPixelFormat().0,
            ),
            shader_validation: ShaderValidation::from_system_raw(shader_validation.0),
            tessellation_control_point_index_type:
                TessellationControlPointIndexType::from_system_raw(
                    self.inner.tessellationControlPointIndexType().0,
                ),
            tessellation_factor_format: TessellationFactorFormat::from_system_raw(
                self.inner.tessellationFactorFormat().0,
            ),
            tessellation_factor_step_function: TessellationFactorStepFunction::from_system_raw(
                self.inner.tessellationFactorStepFunction().0,
            ),
            tessellation_output_winding_order: Winding::from_system_raw(
                self.inner.tessellationOutputWindingOrder().0,
            ),
            tessellation_partition_mode: TessellationPartitionMode::from_system_raw(
                self.inner.tessellationPartitionMode().0,
            ),
        })
    }

    /// Applies all scalar options after validating non-zero count constraints.
    pub fn set_options(&self, value: &RenderPipelineOptions) -> Result<(), Error> {
        if value.raster_sample_count == 0 || value.max_tessellation_factor == 0 {
            return Err(Error::invalid_argument(
                "sample and tessellation counts must be non-zero",
            ));
        }
        if value.max_tessellation_factor > 64 {
            return Err(Error::invalid_argument(
                "maximum tessellation factor cannot exceed 64",
            ));
        }
        for (selector, property) in [
            (
                sel!(setMaxVertexAmplificationCount:),
                "maximum vertex amplification count",
            ),
            (sel!(setInputPrimitiveTopology:), "input primitive topology"),
            (
                sel!(setTessellationPartitionMode:),
                "tessellation partition mode",
            ),
            (
                sel!(setMaxTessellationFactor:),
                "maximum tessellation factor",
            ),
            (
                sel!(setTessellationControlPointIndexType:),
                "tessellation control-point index type",
            ),
            (sel!(setShaderValidation:), "shader validation"),
        ] {
            if !self.inner.respondsToSelector(selector) {
                return Err(Error::unsupported(format!(
                    "render pipeline {property} is unavailable"
                )));
            }
        }
        self.inner
            .setAlphaToCoverageEnabled(value.alpha_to_coverage);
        self.inner.setAlphaToOneEnabled(value.alpha_to_one);
        self.inner
            .setRasterizationEnabled(value.rasterization_enabled);
        self.inner
            .setTessellationFactorScaleEnabled(value.tessellation_factor_scale_enabled);
        self.inner.setSupportAddingFragmentBinaryFunctions(
            value.support_adding_fragment_binary_functions,
        );
        self.inner
            .setSupportAddingVertexBinaryFunctions(value.support_adding_vertex_binary_functions);
        self.inner
            .setSupportIndirectCommandBuffers(value.support_indirect_command_buffers);
        self.inner
            .setMaxFragmentCallStackDepth(value.max_fragment_call_stack_depth);
        // SAFETY: selector availability was checked above, and Metal documents
        // the supported tessellation-factor range as 1 through 64.
        unsafe {
            self.inner
                .setMaxTessellationFactor(value.max_tessellation_factor)
        };
        // SAFETY: selector availability was checked above. The value remains a
        // normal Metal validation constraint (reported during compilation), not
        // an Objective-C ABI or memory-safety precondition.
        unsafe {
            self.inner
                .setMaxVertexAmplificationCount(value.max_vertex_amplification_count)
        };
        self.inner
            .setMaxVertexCallStackDepth(value.max_vertex_call_stack_depth);
        self.inner.setRasterSampleCount(value.raster_sample_count);
        // SAFETY: selector availability was checked above, and the private raw
        // representation can only originate from a checked public enum value or
        // a value returned by Metal.
        unsafe {
            self.inner
                .setInputPrimitiveTopology(MTLPrimitiveTopologyClass(
                    value.input_primitive_topology.as_raw(),
                ))
        };
        self.inner
            .setDepthAttachmentPixelFormat(value.depth_attachment_pixel_format.as_objc());
        self.inner
            .setStencilAttachmentPixelFormat(value.stencil_attachment_pixel_format.as_objc());
        // SAFETY: selector availability was checked above and the enum cannot be
        // constructed from an unchecked public raw value.
        unsafe {
            self.inner
                .setTessellationControlPointIndexType(MTLTessellationControlPointIndexType(
                    value.tessellation_control_point_index_type.as_raw(),
                ))
        };
        self.inner
            .setTessellationFactorFormat(MTLTessellationFactorFormat(
                value.tessellation_factor_format.as_raw(),
            ));
        self.inner
            .setTessellationFactorStepFunction(MTLTessellationFactorStepFunction(
                value.tessellation_factor_step_function.as_raw(),
            ));
        self.inner.setTessellationOutputWindingOrder(MTLWinding(
            value.tessellation_output_winding_order.as_raw(),
        ));
        // SAFETY: selector availability was checked above and the enum cannot be
        // constructed from an unchecked public raw value.
        unsafe {
            self.inner
                .setTessellationPartitionMode(MTLTessellationPartitionMode(
                    value.tessellation_partition_mode.as_raw(),
                ))
        };
        self.inner
            .setShaderValidation(MTLShaderValidation(value.shader_validation.as_raw()));
        Ok(())
    }
}

/// A validated compile-options object.
pub struct CompileOptions {
    pub(super) inner: Retained<MTLCompileOptions>,
    _thread_bound: ThreadBound,
}

impl CompileOptions {
    pub(crate) fn from_any_object(inner: Retained<AnyObject>) -> Result<Self, Error> {
        // SAFETY: callers only bridge selectors declared to return MTLCompileOptions.
        Ok(Self {
            inner: unsafe { Retained::cast_unchecked(inner) },
            _thread_bound: ThreadBound::new(),
        })
    }

    pub(crate) fn as_any_object(&self) -> &AnyObject {
        &self.inner
    }
    /// Creates options with Metal defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: MTLCompileOptions::new(),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Creates options with fast-math explicitly disabled.
    #[must_use]
    pub fn strict() -> Self {
        let inner = MTLCompileOptions::new();
        inner.setMathMode(objc2_metal::MTLMathMode::Safe);
        Self {
            inner,
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns and sets fast-math behavior.
    #[must_use]
    #[allow(deprecated)]
    pub fn fast_math_enabled(&self) -> bool {
        self.inner.fastMathEnabled()
    }

    /// Sets fast-math behavior.
    #[allow(deprecated)]
    pub fn set_fast_math_enabled(&self, value: bool) {
        self.inner.setFastMathEnabled(value);
    }

    /// Returns and sets undefined-symbol reference behavior.
    #[must_use]
    pub fn allows_referencing_undefined_symbols(&self) -> bool {
        self.inner.allowReferencingUndefinedSymbols()
    }

    /// Sets undefined-symbol reference behavior.
    pub fn set_allows_referencing_undefined_symbols(&self, value: bool) {
        self.inner.setAllowReferencingUndefinedSymbols(value);
    }

    /// Returns and sets compiler logging.
    #[must_use]
    pub fn logging_enabled(&self) -> bool {
        self.inner.enableLogging()
    }

    /// Sets compiler logging.
    pub fn set_logging_enabled(&self, value: bool) {
        self.inner.setEnableLogging(value);
    }

    /// Returns and sets invariance preservation.
    #[must_use]
    pub fn preserves_invariance(&self) -> bool {
        self.inner.preserveInvariance()
    }

    /// Sets invariance preservation.
    pub fn set_preserves_invariance(&self, value: bool) {
        self.inner.setPreserveInvariance(value);
    }

    /// Returns the optional install name.
    #[must_use]
    pub fn install_name(&self) -> Option<String> {
        self.inner.installName().map(|value| value.to_string())
    }

    /// Sets or clears the install name.
    pub fn set_install_name(&self, value: Option<&str>) {
        let value = value.map(NSString::from_str);
        self.inner.setInstallName(value.as_deref());
    }

    /// Returns and sets the maximum total thread count.
    #[must_use]
    pub fn max_total_threads_per_threadgroup(&self) -> usize {
        self.inner.maxTotalThreadsPerThreadgroup()
    }

    /// Sets a non-zero maximum total thread count.
    pub fn set_max_total_threads_per_threadgroup(&self, value: usize) -> Result<(), Error> {
        if value == 0 {
            return Err(Error::invalid_argument(
                "maximum total threads must be non-zero",
            ));
        }
        self.inner.setMaxTotalThreadsPerThreadgroup(value);
        Ok(())
    }

    /// Returns and sets required threadgroup dimensions.
    #[must_use]
    pub fn required_threads_per_threadgroup(&self) -> Size {
        let value = self.inner.requiredThreadsPerThreadgroup();
        Size::new(value.width, value.height, value.depth)
    }

    /// Sets non-zero required threadgroup dimensions.
    pub fn set_required_threads_per_threadgroup(&self, value: Size) -> Result<(), Error> {
        if value.width == 0 || value.height == 0 || value.depth == 0 {
            return Err(Error::invalid_argument(
                "required threadgroup dimensions must be non-zero",
            ));
        }
        self.inner.setRequiredThreadsPerThreadgroup(value.into());
        Ok(())
    }

    /// Returns and sets the math mode.
    #[must_use]
    pub fn math_mode(&self) -> MathMode {
        MathMode::from_system_raw(self.inner.mathMode().0)
    }

    /// Sets a validated math mode.
    pub fn set_math_mode(&self, value: MathMode) {
        self.inner.setMathMode(MTLMathMode(value.as_raw()));
    }

    /// Returns and sets floating-point function options.
    #[must_use]
    pub fn math_floating_point_functions(&self) -> MathFloatingPointFunctions {
        MathFloatingPointFunctions::from_system_raw(self.inner.mathFloatingPointFunctions().0)
    }

    /// Sets validated floating-point function options.
    pub fn set_math_floating_point_functions(&self, value: MathFloatingPointFunctions) {
        self.inner
            .setMathFloatingPointFunctions(MTLMathFloatingPointFunctions(value.as_raw()));
    }

    /// Returns and sets the language version.
    #[must_use]
    pub fn language_version(&self) -> LanguageVersion {
        LanguageVersion::from_system_raw(self.inner.languageVersion().0)
    }

    /// Sets a validated language version.
    pub fn set_language_version(&self, value: LanguageVersion) {
        self.inner
            .setLanguageVersion(MTLLanguageVersion(value.as_raw()));
    }

    /// Returns and sets the library type.
    #[must_use]
    pub fn library_type(&self) -> LibraryType {
        LibraryType::from_system_raw(self.inner.libraryType().0)
    }

    /// Sets a validated library type.
    pub fn set_library_type(&self, value: LibraryType) {
        self.inner.setLibraryType(MTLLibraryType(value.as_raw()));
    }

    /// Returns and sets the optimization level.
    #[must_use]
    pub fn optimization_level(&self) -> LibraryOptimizationLevel {
        LibraryOptimizationLevel::from_system_raw(self.inner.optimizationLevel().0)
    }

    /// Sets a validated optimization level.
    pub fn set_optimization_level(&self, value: LibraryOptimizationLevel) {
        self.inner
            .setOptimizationLevel(MTLLibraryOptimizationLevel(value.as_raw()));
    }

    /// Returns and sets compile-symbol visibility.
    #[must_use]
    pub fn compile_symbol_visibility(&self) -> CompileSymbolVisibility {
        CompileSymbolVisibility::from_system_raw(self.inner.compileSymbolVisibility().0)
    }

    /// Sets validated compile-symbol visibility.
    pub fn set_compile_symbol_visibility(&self, value: CompileSymbolVisibility) {
        self.inner
            .setCompileSymbolVisibility(MTLCompileSymbolVisibility(value.as_raw()));
    }

    /// Returns dependent dynamic libraries as owned safe wrappers.
    pub fn libraries(
        &self,
    ) -> Result<Vec<crate::metal::generated_object_types::metal::DynamicLibrary>, Error> {
        if !self.inner.respondsToSelector(sel!(libraries)) {
            return Err(Error::unsupported(
                "compile-option libraries are unavailable",
            ));
        }
        Ok(self
            .inner
            .libraries()
            .map(|values| {
                values
                    .into_iter()
                    .map(|value| {
                        // SAFETY: the typed array owns MTLDynamicLibrary objects.
                        let value = unsafe { Retained::cast_unchecked(value) };
                        crate::metal::generated_object_types::metal::DynamicLibrary::from_inner(
                            value,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// Sets or clears dependent dynamic libraries from a Rust slice.
    pub fn set_libraries(
        &self,
        libraries: Option<&[crate::metal::generated_object_types::metal::DynamicLibrary]>,
    ) -> Result<(), Error> {
        if !self.inner.respondsToSelector(sel!(setLibraries:)) {
            return Err(Error::unsupported(
                "compile-option libraries are unavailable",
            ));
        }
        let libraries = libraries.map(|values| {
            let retained: Vec<Retained<ProtocolObject<dyn MTLDynamicLibrary>>> = values
                .iter()
                .map(|value| {
                    // SAFETY: the wrapper owns a live MTLDynamicLibrary;
                    // retaining and restoring the declared protocol preserves identity.
                    let value = unsafe {
                        Retained::retain(std::ptr::from_ref(value.as_inner()).cast_mut())
                    }
                    .expect("a borrowed Objective-C wrapper cannot be null");
                    // SAFETY: generated wrapper identity is MTLDynamicLibrary.
                    unsafe { Retained::cast_unchecked(value) }
                })
                .collect();
            NSArray::from_retained_slice(&retained)
        });
        self.inner.setLibraries(libraries.as_deref());
        Ok(())
    }

    /// Returns preprocessor macros as owned Rust strings.
    pub fn preprocessor_macros(&self) -> Result<HashMap<String, String>, Error> {
        if !self.inner.respondsToSelector(sel!(preprocessorMacros)) {
            return Err(Error::unsupported(
                "compile-option preprocessor macros are unavailable",
            ));
        }
        let mut result = HashMap::new();
        if let Some(values) = self.inner.preprocessorMacros() {
            for key in values.keys() {
                if let Some(value) = values.objectForKey(&key) {
                    // SAFETY: NSObject implements description with an NSString return.
                    let description: Retained<NSString> =
                        unsafe { msg_send![&*value, description] };
                    result.insert(key.to_string(), description.to_string());
                }
            }
        }
        Ok(result)
    }

    /// Sets or clears preprocessor macros using NSString values.
    pub fn set_preprocessor_macros(
        &self,
        macros: Option<&HashMap<String, String>>,
    ) -> Result<(), Error> {
        if let Some(macros) = macros
            && macros
                .iter()
                .any(|(key, value)| key.is_empty() || key.contains('\0') || value.contains('\0'))
        {
            return Err(Error::invalid_argument("preprocessor macro is invalid"));
        }
        if !self.inner.respondsToSelector(sel!(setPreprocessorMacros:)) {
            return Err(Error::unsupported(
                "compile-option preprocessor macros are unavailable",
            ));
        }
        let dictionary = macros.map(|values| {
            let keys: Vec<Retained<NSString>> = values
                .keys()
                .map(|value| NSString::from_str(value))
                .collect();
            let objects: Vec<Retained<NSString>> = values
                .values()
                .map(|value| NSString::from_str(value))
                .collect();
            let key_refs: Vec<&NSString> = keys.iter().map(|value| &**value).collect();
            NSDictionary::<NSString, NSString>::from_retained_objects(&key_refs, &objects)
        });
        // SAFETY: selector availability was checked and NSString values are a
        // documented valid NSObject subtype for macro dictionaries.
        unsafe {
            let _: () = msg_send![&*self.inner, setPreprocessorMacros: dictionary.as_deref()];
        }
        Ok(())
    }

    /// Returns floating-point conversion rounding when the selector exists.
    pub fn floating_point_conversion_rounding_mode(
        &self,
    ) -> Result<FloatingPointConversionRoundingMode, Error> {
        if !self
            .inner
            .respondsToSelector(sel!(floatingPointConversionRoundingMode))
        {
            return Err(Error::unsupported(
                "floating-point conversion rounding mode is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the inventory declares
        // an NSInteger-compatible enum return ABI.
        let raw: isize = unsafe { msg_send![&*self.inner, floatingPointConversionRoundingMode] };
        Ok(FloatingPointConversionRoundingMode::from_system_raw(raw))
    }

    /// Sets floating-point conversion rounding when the selector exists.
    pub fn set_floating_point_conversion_rounding_mode(
        &self,
        value: FloatingPointConversionRoundingMode,
    ) -> Result<(), Error> {
        if !value.is_valid() {
            return Err(Error::invalid_argument(
                "floating-point conversion rounding mode is invalid",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(setFloatingPointConversionRoundingMode:))
        {
            return Err(Error::unsupported(
                "floating-point conversion rounding mode is unavailable",
            ));
        }
        let raw = value.as_raw();
        // SAFETY: selector availability and the declared NSInteger argument
        // ABI were checked; the public value rejects undeclared variants.
        unsafe {
            let _: () = msg_send![&*self.inner, setFloatingPointConversionRoundingMode: raw];
        }
        Ok(())
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::metal::generated_object_types::metal::ComputePipelineDescriptor {
    /// Returns the required compute threadgroup dimensions when available.
    pub fn required_threads_per_threadgroup(&self) -> Result<Size, Error> {
        let object = self.as_inner();
        // SAFETY: every Objective-C object supports respondsToSelector: and
        // the selector argument and boolean return use their declared ABIs.
        let available: bool =
            unsafe { msg_send![object, respondsToSelector: sel!(requiredThreadsPerThreadgroup)] };
        if !available {
            return Err(Error::unsupported(
                "required compute threadgroup dimensions are unavailable",
            ));
        }
        // SAFETY: selector availability was checked and the inventory declares
        // the exact MTLSize return ABI.
        let value: objc2_metal::MTLSize =
            unsafe { msg_send![object, requiredThreadsPerThreadgroup] };
        Ok(Size::new(value.width, value.height, value.depth))
    }

    /// Sets required compute threadgroup dimensions, or all zeros to disable.
    pub fn set_required_threads_per_threadgroup(&self, value: Size) -> Result<(), Error> {
        let all_zero = value.width == 0 && value.height == 0 && value.depth == 0;
        let all_non_zero = value.width != 0 && value.height != 0 && value.depth != 0;
        if !all_zero && !all_non_zero {
            return Err(Error::invalid_argument(
                "required threadgroup dimensions must be all zero or all non-zero",
            ));
        }
        let object = self.as_inner();
        // SAFETY: every Objective-C object supports respondsToSelector:.
        let available: bool = unsafe {
            msg_send![object, respondsToSelector: sel!(setRequiredThreadsPerThreadgroup:)]
        };
        if !available {
            return Err(Error::unsupported(
                "required compute threadgroup dimensions are unavailable",
            ));
        }
        let value: objc2_metal::MTLSize = value.into();
        // SAFETY: selector availability was checked, the argument has the
        // declared MTLSize ABI, and partial-zero dimensions were rejected.
        unsafe {
            let _: () = msg_send![object, setRequiredThreadsPerThreadgroup: value];
        }
        Ok(())
    }

    /// Restores all compute-pipeline descriptor properties to Metal defaults.
    pub fn reset(&self) -> Result<(), Error> {
        let object = self.as_inner();
        // SAFETY: every Objective-C object supports respondsToSelector:.
        let available: bool = unsafe { msg_send![object, respondsToSelector: sel!(reset)] };
        if !available {
            return Err(Error::unsupported(
                "compute pipeline descriptor reset is unavailable",
            ));
        }
        // SAFETY: selector availability was checked and reset has no arguments
        // and a void return in the inventory.
        unsafe {
            let _: () = msg_send![object, reset];
        }
        Ok(())
    }
}

impl crate::metal::generated_object_types::metal::FunctionConstantValues {
    /// Removes all previously assigned function constants.
    pub fn reset(&self) -> Result<(), Error> {
        let object = self.as_inner();
        // SAFETY: every Objective-C object implements respondsToSelector:.
        let available: bool = unsafe { msg_send![object, respondsToSelector: sel!(reset)] };
        if !available {
            return Err(Error::unsupported("function constants are unavailable"));
        }
        // SAFETY: reset has the checked zero-argument void ABI.
        unsafe {
            let _: () = msg_send![object, reset];
        }
        Ok(())
    }

    /// Assigns one function constant by index from its documented ABI bytes.
    pub fn set_constant_at_index(
        &self,
        data_type: DataType,
        index: usize,
        bytes: &[u8],
    ) -> Result<(), Error> {
        let size = function_constant_abi_size(data_type).ok_or_else(|| {
            Error::invalid_argument("data type is not a value-compatible function constant")
        })?;
        if bytes.len() != size {
            return Err(Error::invalid_argument(format!(
                "function constant requires {size} ABI bytes"
            )));
        }
        validate_function_constant_bytes(data_type, bytes, 1)?;
        let mut storage = aligned_constant_bytes(bytes)?;
        let pointer: NonNull<std::ffi::c_void> = NonNull::new(storage.as_mut_ptr().cast())
            .ok_or_else(|| Error::invalid_argument("function constant storage is empty"))?;
        let object = self.as_inner();
        // SAFETY: every Objective-C object implements respondsToSelector:.
        let available: bool =
            unsafe { msg_send![object, respondsToSelector: sel!(setConstantValue:type:atIndex:)] };
        if !available {
            return Err(Error::unsupported("function constants are unavailable"));
        }
        // SAFETY: the aligned initialized storage matches the validated ABI
        // size and remains alive for this copying Objective-C call.
        unsafe {
            let _: () = msg_send![object,
                setConstantValue: pointer,
                r#type: data_type.as_raw(),
                atIndex: index
            ];
        }
        Ok(())
    }

    /// Assigns one named function constant from its documented ABI bytes.
    pub fn set_constant_named(
        &self,
        data_type: DataType,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        if name.is_empty() || name.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("function constant name is invalid"));
        }
        let size = function_constant_abi_size(data_type).ok_or_else(|| {
            Error::invalid_argument("data type is not a value-compatible function constant")
        })?;
        if bytes.len() != size {
            return Err(Error::invalid_argument(format!(
                "function constant requires {size} ABI bytes"
            )));
        }
        validate_function_constant_bytes(data_type, bytes, 1)?;
        let mut storage = aligned_constant_bytes(bytes)?;
        let pointer: NonNull<std::ffi::c_void> = NonNull::new(storage.as_mut_ptr().cast())
            .ok_or_else(|| Error::invalid_argument("function constant storage is empty"))?;
        let name = NSString::from_str(name);
        let object = self.as_inner();
        // SAFETY: every Objective-C object implements respondsToSelector:.
        let available: bool =
            unsafe { msg_send![object, respondsToSelector: sel!(setConstantValue:type:withName:)] };
        if !available {
            return Err(Error::unsupported("function constants are unavailable"));
        }
        // SAFETY: the aligned initialized storage matches the validated ABI
        // size and remains alive for this copying call; name is a valid NSString.
        unsafe {
            let _: () = msg_send![object,
                setConstantValue: pointer,
                r#type: data_type.as_raw(),
                withName: &*name
            ];
        }
        Ok(())
    }

    /// Assigns a contiguous range of constants from packed ABI-sized values.
    pub fn set_constants(
        &self,
        data_type: DataType,
        range: std::ops::Range<usize>,
        bytes: &[u8],
    ) -> Result<(), Error> {
        if range.start > range.end {
            return Err(Error::invalid_argument(
                "function constant range is invalid",
            ));
        }
        let count = range.end - range.start;
        if count == 0 {
            return Err(Error::invalid_argument("function constant range is empty"));
        }
        let size = function_constant_abi_size(data_type).ok_or_else(|| {
            Error::invalid_argument("data type is not a value-compatible function constant")
        })?;
        let expected = size
            .checked_mul(count)
            .ok_or_else(|| Error::invalid_argument("function constant range overflow"))?;
        if bytes.len() != expected {
            return Err(Error::invalid_argument(format!(
                "function constant range requires {expected} packed ABI bytes"
            )));
        }
        validate_function_constant_bytes(data_type, bytes, count)?;
        let mut storage = aligned_constant_bytes(bytes)?;
        let pointer: NonNull<std::ffi::c_void> = NonNull::new(storage.as_mut_ptr().cast())
            .ok_or_else(|| Error::invalid_argument("function constant storage is empty"))?;
        let range = NSRange::new(range.start, count);
        let object = self.as_inner();
        // SAFETY: every Objective-C object implements respondsToSelector:.
        let available: bool = unsafe {
            msg_send![object, respondsToSelector: sel!(setConstantValues:type:withRange:)]
        };
        if !available {
            return Err(Error::unsupported("function constants are unavailable"));
        }
        // SAFETY: storage contains count consecutive initialized values of the
        // validated ABI size and remains alive for this copying call.
        unsafe {
            let _: () = msg_send![object,
                setConstantValues: pointer,
                r#type: data_type.as_raw(),
                withRange: range
            ];
        }
        Ok(())
    }
}
