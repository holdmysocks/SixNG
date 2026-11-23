//! Audited Metal 4 compiler factory and callback boundary.

use crate::foundation::{Error, metal_error};
use crate::metal::generated_object_types::{metal, metal4};
use crate::metal::{ClearColor, ComputePipelineState, Library, RenderPipelineState, Texture};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, ProtocolObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSData, NSError, NSString, NSURL};
use objc2_metal::{
    MTL4PipelineDataSetSerializer, MTLClearColor, MTLLoadAction, MTLSamplePosition, MTLSize,
    MTLStoreAction,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::{Arc, Mutex};

trait RespondsToSelector {
    fn responds_to(&self, selector: objc2::runtime::Sel) -> bool;
}

impl RespondsToSelector for AnyObject {
    fn responds_to(&self, selector: objc2::runtime::Sel) -> bool {
        // SAFETY: every Objective-C object implements respondsToSelector: and
        // the selector/bool ABI is stable across Foundation objects.
        unsafe { msg_send![self, respondsToSelector: selector] }
    }
}

fn object_array<'a>(values: impl IntoIterator<Item = &'a AnyObject>) -> Retained<AnyObject> {
    let class = AnyClass::get(c"NSMutableArray")
        .expect("Foundation always provides NSMutableArray when Metal is loaded");
    // SAFETY: NSMutableArray implements `new` and returns an owned empty array.
    let array: Retained<AnyObject> = unsafe { msg_send![class, new] };
    for value in values {
        // SAFETY: both objects remain live and NSMutableArray accepts any
        // non-null Objective-C object through addObject:.
        unsafe {
            let _: () = msg_send![&*array, addObject: value];
        }
    }
    array
}

fn array_objects(array: Option<Retained<AnyObject>>) -> Vec<Retained<AnyObject>> {
    let Some(array) = array else {
        return Vec::new();
    };
    // SAFETY: generated callers obtain this object from properties declared as
    // NSArray; count and objectAtIndex: therefore have their standard ABIs.
    let count: usize = unsafe { msg_send![&*array, count] };
    (0..count)
        .map(|index| {
            // SAFETY: index is strictly below the count read from this same
            // immutable array and objc2 retains the returned object.
            unsafe { msg_send![&*array, objectAtIndex: index] }
        })
        .collect()
}

fn unsupported(selector: &str) -> Error {
    Error::unsupported(format!("MTL4::Compiler::{selector} is unavailable"))
}

fn callback_error(
    value: *mut AnyObject,
    error: *mut NSError,
) -> Result<Retained<AnyObject>, Error> {
    if !error.is_null() {
        // SAFETY: Metal supplies an NSError that remains live for the block
        // invocation. Retaining it makes conversion independent of that scope.
        let error = unsafe { Retained::retain(error) }
            .expect("the pointer was checked as non-null immediately above");
        return Err(metal_error(&error));
    }
    // SAFETY: Metal supplies a result object that remains live for the block
    // invocation. Retaining it transfers an owned reference into Rust.
    unsafe { Retained::retain(value) }.ok_or_else(|| {
        Error::unsupported("Metal completed compilation without a result or NSError")
    })
}

fn compute_pipeline(inner: Retained<AnyObject>) -> ComputePipelineState {
    // SAFETY: these helpers are called only for selectors declared to return
    // an object conforming to MTLComputePipelineState.
    let inner = unsafe { Retained::cast_unchecked(inner) };
    ComputePipelineState::new(inner)
}

fn render_pipeline(inner: Retained<AnyObject>) -> RenderPipelineState {
    // SAFETY: these helpers are called only for selectors declared to return
    // an object conforming to MTLRenderPipelineState.
    let inner = unsafe { Retained::cast_unchecked(inner) };
    RenderPipelineState::new(inner)
}

/// A borrowed Metal 4 descriptor for any render, mesh, or tile pipeline build.
#[derive(Clone, Copy)]
pub enum RenderPipelineBuildDescriptor<'a> {
    /// A conventional vertex/fragment render pipeline.
    Render(&'a metal4::RenderPipelineDescriptor),
    /// A mesh/object render pipeline.
    Mesh(&'a metal4::MeshRenderPipelineDescriptor),
    /// A tile render pipeline.
    Tile(&'a metal4::TileRenderPipelineDescriptor),
}

impl<'a> RenderPipelineBuildDescriptor<'a> {
    fn as_inner(self) -> &'a AnyObject {
        match self {
            Self::Render(value) => value.as_inner(),
            Self::Mesh(value) => value.as_inner(),
            Self::Tile(value) => value.as_inner(),
        }
    }
}

impl metal4::Compiler {
    fn supports(&self, selector: objc2::runtime::Sel) -> bool {
        // SAFETY: every Objective-C object implements respondsToSelector: and
        // the selector/bool ABI is stable across Foundation objects.
        unsafe { msg_send![self.as_inner(), respondsToSelector: selector] }
    }

    /// Creates a Metal library synchronously and owns any returned NSError.
    pub fn new_library(&self, descriptor: &metal4::LibraryDescriptor) -> Result<Library, Error> {
        if !self.supports(sel!(newLibraryWithDescriptor:error:)) {
            return Err(unsupported("newLibraryWithDescriptor:error:"));
        }
        // SAFETY: selector availability and both object class contracts are
        // checked by the generated wrappers; `_` requests objc2's NSError
        // writeback adapter and the returned object follows the `new` family.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), newLibraryWithDescriptor: descriptor.as_inner(), error: _]
        };
        result
            .map(Library::from_any_object)
            .map_err(|error| metal_error(&error))?
    }

    /// Creates a compute pipeline synchronously with optional task and dynamic-linking options.
    pub fn new_compute_pipeline(
        &self,
        descriptor: &metal4::ComputePipelineDescriptor,
        linking: Option<&metal4::PipelineStageDynamicLinkingDescriptor>,
        options: Option<&metal4::CompilerTaskOptions>,
    ) -> Result<ComputePipelineState, Error> {
        if linking.is_some() {
            if !self.supports(sel!(newComputePipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:error:)) {
                return Err(unsupported("newComputePipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:error:"));
            }
            // SAFETY: the selector is present and generated wrappers preserve
            // every descriptor's Objective-C class identity.
            let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
                msg_send![self.as_inner(), newComputePipelineStateWithDescriptor: descriptor.as_inner(), dynamicLinkingDescriptor: linking.map(metal4::PipelineStageDynamicLinkingDescriptor::as_inner), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), error: _]
            };
            return result
                .map(compute_pipeline)
                .map_err(|error| metal_error(&error));
        }
        if !self.supports(sel!(newComputePipelineStateWithDescriptor:compilerTaskOptions:error:)) {
            return Err(unsupported(
                "newComputePipelineStateWithDescriptor:compilerTaskOptions:error:",
            ));
        }
        // SAFETY: the selector is present and generated wrappers preserve the
        // descriptor and task-options Objective-C class identities.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), newComputePipelineStateWithDescriptor: descriptor.as_inner(), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), error: _]
        };
        result
            .map(compute_pipeline)
            .map_err(|error| metal_error(&error))
    }

    /// Creates a render pipeline synchronously with optional dynamic linking.
    pub fn new_render_pipeline(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        linking: Option<&metal4::RenderPipelineDynamicLinkingDescriptor>,
        options: Option<&metal4::CompilerTaskOptions>,
    ) -> Result<RenderPipelineState, Error> {
        if linking.is_some() {
            if !self.supports(sel!(newRenderPipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:error:)) {
                return Err(unsupported("newRenderPipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:error:"));
            }
            // SAFETY: the selector is present and all arguments remain live
            // for the synchronous Objective-C call.
            let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
                msg_send![self.as_inner(), newRenderPipelineStateWithDescriptor: descriptor.as_inner(), dynamicLinkingDescriptor: linking.map(metal4::RenderPipelineDynamicLinkingDescriptor::as_inner), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), error: _]
            };
            return result
                .map(render_pipeline)
                .map_err(|error| metal_error(&error));
        }
        if !self.supports(sel!(newRenderPipelineStateWithDescriptor:compilerTaskOptions:error:)) {
            return Err(unsupported(
                "newRenderPipelineStateWithDescriptor:compilerTaskOptions:error:",
            ));
        }
        // SAFETY: the selector is present and all arguments remain live for
        // the synchronous Objective-C call.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), newRenderPipelineStateWithDescriptor: descriptor.as_inner(), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), error: _]
        };
        result
            .map(render_pipeline)
            .map_err(|error| metal_error(&error))
    }

    /// Specializes a previously unspecialized render pipeline.
    pub fn specialize_render_pipeline(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        pipeline: &RenderPipelineState,
    ) -> Result<RenderPipelineState, Error> {
        if !self
            .supports(sel!(newRenderPipelineStateBySpecializationWithDescriptor:pipeline:error:))
        {
            return Err(unsupported(
                "newRenderPipelineStateBySpecializationWithDescriptor:pipeline:error:",
            ));
        }
        // SAFETY: the selector is present; the descriptor and pipeline are
        // type-checked owned wrappers and remain live for the call.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), newRenderPipelineStateBySpecializationWithDescriptor: descriptor.as_inner(), pipeline: &*pipeline.inner, error: _]
        };
        result
            .map(render_pipeline)
            .map_err(|error| metal_error(&error))
    }

    /// Creates a binary shader function synchronously.
    pub fn new_binary_function(
        &self,
        descriptor: &metal4::BinaryFunctionDescriptor,
        options: Option<&metal4::CompilerTaskOptions>,
    ) -> Result<metal4::BinaryFunction, Error> {
        if !self.supports(sel!(newBinaryFunctionWithDescriptor:compilerTaskOptions:error:)) {
            return Err(unsupported(
                "newBinaryFunctionWithDescriptor:compilerTaskOptions:error:",
            ));
        }
        // SAFETY: the selector is present and generated wrappers preserve the
        // descriptor and task-options Objective-C class identities.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), newBinaryFunctionWithDescriptor: descriptor.as_inner(), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), error: _]
        };
        result
            .map(metal4::BinaryFunction::from_inner)
            .map_err(|error| metal_error(&error))
    }

    /// Creates a machine-learning pipeline synchronously.
    pub fn new_machine_learning_pipeline(
        &self,
        descriptor: &metal4::MachineLearningPipelineDescriptor,
    ) -> Result<metal4::MachineLearningPipelineState, Error> {
        if !self.supports(sel!(newMachineLearningPipelineStateWithDescriptor:error:)) {
            return Err(unsupported(
                "newMachineLearningPipelineStateWithDescriptor:error:",
            ));
        }
        // SAFETY: selector availability and descriptor class identity are
        // checked before the NSError-producing call.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), newMachineLearningPipelineStateWithDescriptor: descriptor.as_inner(), error: _]
        };
        result
            .map(metal4::MachineLearningPipelineState::from_inner)
            .map_err(|error| metal_error(&error))
    }

    /// Creates a dynamic library synchronously from a compiled Metal library.
    pub fn new_dynamic_library(&self, library: &Library) -> Result<metal::DynamicLibrary, Error> {
        if !self.supports(sel!(newDynamicLibrary:error:)) {
            return Err(unsupported("newDynamicLibrary:error:"));
        }
        // SAFETY: selector availability is checked and `library` conforms to
        // MTLLibrary for the duration of this call.
        let result: Result<Retained<AnyObject>, Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), newDynamicLibrary: library.as_any_object(), error: _]
        };
        result
            .map(metal::DynamicLibrary::from_inner)
            .map_err(|error| metal_error(&error))
    }

    /// Creates a dynamic library synchronously from a file path.
    pub fn new_dynamic_library_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<metal::DynamicLibrary, Error> {
        let path = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::invalid_argument("dynamic library path is not valid UTF-8"))?;
        if path.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("dynamic library path contains NUL"));
        }
        if !self.supports(sel!(newDynamicLibraryWithURL:error:)) {
            return Err(unsupported("newDynamicLibraryWithURL:error:"));
        }
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: selector availability is checked and the owned file URL
        // remains live for this synchronous call.
        let result: Result<Retained<AnyObject>, Retained<NSError>> =
            unsafe { msg_send![self.as_inner(), newDynamicLibraryWithURL: &*url, error: _] };
        result
            .map(metal::DynamicLibrary::from_inner)
            .map_err(|error| metal_error(&error))
    }

    /// Starts one-shot asynchronous library compilation.
    pub fn new_library_async(
        &self,
        descriptor: &metal4::LibraryDescriptor,
        handler: impl FnOnce(Result<Library, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(sel!(newLibraryWithDescriptor:completionHandler:)) {
            return Err(unsupported("newLibraryWithDescriptor:completionHandler:"));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).and_then(Library::from_any_object);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; the block has the exact
        // two-object completion ABI and Metal copies it for deferred delivery.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newLibraryWithDescriptor: descriptor.as_inner(), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous compute-pipeline compilation.
    pub fn new_compute_pipeline_async(
        &self,
        descriptor: &metal4::ComputePipelineDescriptor,
        options: Option<&metal4::CompilerTaskOptions>,
        handler: impl FnOnce(Result<ComputePipelineState, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(
            sel!(newComputePipelineStateWithDescriptor:compilerTaskOptions:completionHandler:),
        ) {
            return Err(unsupported(
                "newComputePipelineStateWithDescriptor:compilerTaskOptions:completionHandler:",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(compute_pipeline);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; the block has the exact
        // two-object completion ABI and Metal copies it for deferred delivery.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newComputePipelineStateWithDescriptor: descriptor.as_inner(), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous compute-pipeline compilation with dynamic linking.
    pub fn new_compute_pipeline_linked_async(
        &self,
        descriptor: &metal4::ComputePipelineDescriptor,
        linking: Option<&metal4::PipelineStageDynamicLinkingDescriptor>,
        options: Option<&metal4::CompilerTaskOptions>,
        handler: impl FnOnce(Result<ComputePipelineState, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(sel!(newComputePipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:completionHandler:)) {
            return Err(unsupported("newComputePipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:completionHandler:"));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(compute_pipeline);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; argument wrappers retain
        // their Objective-C identities and Metal copies the exact-ABI block.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newComputePipelineStateWithDescriptor: descriptor.as_inner(), dynamicLinkingDescriptor: linking.map(metal4::PipelineStageDynamicLinkingDescriptor::as_inner), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous render-pipeline compilation.
    pub fn new_render_pipeline_async(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        options: Option<&metal4::CompilerTaskOptions>,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(
            sel!(newRenderPipelineStateWithDescriptor:compilerTaskOptions:completionHandler:),
        ) {
            return Err(unsupported(
                "newRenderPipelineStateWithDescriptor:compilerTaskOptions:completionHandler:",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(render_pipeline);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; the block has the exact
        // two-object completion ABI and Metal copies it for deferred delivery.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newRenderPipelineStateWithDescriptor: descriptor.as_inner(), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous render-pipeline compilation with dynamic linking.
    pub fn new_render_pipeline_linked_async(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        linking: Option<&metal4::RenderPipelineDynamicLinkingDescriptor>,
        options: Option<&metal4::CompilerTaskOptions>,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(sel!(newRenderPipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:completionHandler:)) {
            return Err(unsupported("newRenderPipelineStateWithDescriptor:dynamicLinkingDescriptor:compilerTaskOptions:completionHandler:"));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(render_pipeline);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; argument wrappers retain
        // their Objective-C identities and Metal copies the exact-ABI block.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newRenderPipelineStateWithDescriptor: descriptor.as_inner(), dynamicLinkingDescriptor: linking.map(metal4::RenderPipelineDynamicLinkingDescriptor::as_inner), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous specialization of a render pipeline.
    pub fn specialize_render_pipeline_async(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        pipeline: &RenderPipelineState,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(
            sel!(newRenderPipelineStateBySpecializationWithDescriptor:pipeline:completionHandler:),
        ) {
            return Err(unsupported(
                "newRenderPipelineStateBySpecializationWithDescriptor:pipeline:completionHandler:",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(render_pipeline);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; descriptor and pipeline
        // are live type-checked wrappers and Metal copies the exact-ABI block.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newRenderPipelineStateBySpecializationWithDescriptor: descriptor.as_inner(), pipeline: &*pipeline.inner, completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous binary-function compilation.
    pub fn new_binary_function_async(
        &self,
        descriptor: &metal4::BinaryFunctionDescriptor,
        options: Option<&metal4::CompilerTaskOptions>,
        handler: impl FnOnce(Result<metal4::BinaryFunction, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self
            .supports(sel!(newBinaryFunctionWithDescriptor:compilerTaskOptions:completionHandler:))
        {
            return Err(unsupported(
                "newBinaryFunctionWithDescriptor:compilerTaskOptions:completionHandler:",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(metal4::BinaryFunction::from_inner);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; Metal copies the block,
        // whose arguments match the binary-function completion ABI.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newBinaryFunctionWithDescriptor: descriptor.as_inner(), compilerTaskOptions: options.map(metal4::CompilerTaskOptions::as_inner), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous machine-learning pipeline compilation.
    pub fn new_machine_learning_pipeline_async(
        &self,
        descriptor: &metal4::MachineLearningPipelineDescriptor,
        handler: impl FnOnce(Result<metal4::MachineLearningPipelineState, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(sel!(newMachineLearningPipelineStateWithDescriptor:completionHandler:)) {
            return Err(unsupported(
                "newMachineLearningPipelineStateWithDescriptor:completionHandler:",
            ));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result =
                callback_error(value, error).map(metal4::MachineLearningPipelineState::from_inner);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; Metal copies the block,
        // whose arguments match the machine-learning completion ABI.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newMachineLearningPipelineStateWithDescriptor: descriptor.as_inner(), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous dynamic-library compilation from a library.
    pub fn new_dynamic_library_async(
        &self,
        library: &Library,
        handler: impl FnOnce(Result<metal::DynamicLibrary, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        if !self.supports(sel!(newDynamicLibrary:completionHandler:)) {
            return Err(unsupported("newDynamicLibrary:completionHandler:"));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(metal::DynamicLibrary::from_inner);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        // SAFETY: selector availability was checked; library is live and
        // Metal copies the block with the dynamic-library completion ABI.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newDynamicLibrary: library.as_any_object(), completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }

    /// Starts one-shot asynchronous dynamic-library compilation from a file path.
    pub fn new_dynamic_library_from_path_async(
        &self,
        path: impl AsRef<Path>,
        handler: impl FnOnce(Result<metal::DynamicLibrary, Error>) + Send + 'static,
    ) -> Result<metal4::CompilerTask, Error> {
        let path = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::invalid_argument("dynamic library path is not valid UTF-8"))?;
        if path.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("dynamic library path contains NUL"));
        }
        if !self.supports(sel!(newDynamicLibraryWithURL:completionHandler:)) {
            return Err(unsupported("newDynamicLibraryWithURL:completionHandler:"));
        }
        let state = Arc::new(Mutex::new(Some(handler)));
        let callback_state = Arc::clone(&state);
        let block = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let callback = callback_state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .take();
            let Some(callback) = callback else {
                return;
            };
            let result = callback_error(value, error).map(metal::DynamicLibrary::from_inner);
            let _ = catch_unwind(AssertUnwindSafe(|| callback(result)));
        });
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        // SAFETY: selector availability was checked; URL is live for the call
        // and Metal copies the block with the exact completion ABI.
        let task: Retained<AnyObject> = unsafe {
            msg_send![self.as_inner(), newDynamicLibraryWithURL: &*url, completionHandler: &*block]
        };
        Ok(metal4::CompilerTask::from_inner(task))
    }
}

impl metal4::CompilerTaskOptions {
    /// Returns binary archives searched by this compilation task.
    pub fn lookup_archives_vec(&self) -> Result<Vec<metal::BinaryArchive>, Error> {
        if !self.as_inner().responds_to(sel!(lookupArchives)) {
            return Err(Error::unsupported(
                "MTL4CompilerTaskOptions.lookupArchives is unavailable",
            ));
        }
        // SAFETY: selector presence and its nullable NSArray return are checked.
        let array = unsafe { msg_send![self.as_inner(), lookupArchives] };
        Ok(array_objects(array)
            .into_iter()
            .map(metal::BinaryArchive::from_inner)
            .collect())
    }

    /// Replaces the binary archives searched by this compilation task.
    pub fn set_lookup_archives_slice(&self, values: &[metal::BinaryArchive]) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(setLookupArchives:)) {
            return Err(Error::unsupported(
                "MTL4CompilerTaskOptions.setLookupArchives: is unavailable",
            ));
        }
        let array = object_array(values.iter().map(metal::BinaryArchive::as_inner));
        // SAFETY: selector presence is checked and every element is an owned
        // MTLBinaryArchive wrapper.
        unsafe {
            let _: () = msg_send![self.as_inner(), setLookupArchives: &*array];
        }
        Ok(())
    }
}

impl metal4::StaticLinkingDescriptor {
    /// Returns owned function descriptors, translating a nullable NSArray to an empty Vec.
    pub fn function_descriptors_vec(&self) -> Result<Vec<metal4::FunctionDescriptor>, Error> {
        if !self.as_inner().responds_to(sel!(functionDescriptors)) {
            return Err(Error::unsupported(
                "MTL4StaticLinkingDescriptor.functionDescriptors is unavailable",
            ));
        }
        // SAFETY: selector presence and its nullable NSArray return are checked.
        let array = unsafe { msg_send![self.as_inner(), functionDescriptors] };
        Ok(array_objects(array)
            .into_iter()
            .map(metal4::FunctionDescriptor::from_inner)
            .collect())
    }

    /// Replaces all public function descriptors from a borrowed Rust slice.
    pub fn set_function_descriptors_slice(
        &self,
        values: &[metal4::FunctionDescriptor],
    ) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(setFunctionDescriptors:)) {
            return Err(Error::unsupported(
                "MTL4StaticLinkingDescriptor.setFunctionDescriptors: is unavailable",
            ));
        }
        let array = object_array(values.iter().map(metal4::FunctionDescriptor::as_inner));
        // SAFETY: selector presence is checked and the owned array contains
        // only MTL4FunctionDescriptor objects.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFunctionDescriptors: &*array];
        }
        Ok(())
    }

    /// Returns owned private function descriptors.
    pub fn internal_functions(&self) -> Result<Vec<metal4::FunctionDescriptor>, Error> {
        if !self
            .as_inner()
            .responds_to(sel!(privateFunctionDescriptors))
        {
            return Err(Error::unsupported(
                "MTL4StaticLinkingDescriptor.privateFunctionDescriptors is unavailable",
            ));
        }
        // SAFETY: selector presence and its nullable NSArray return are checked.
        let array = unsafe { msg_send![self.as_inner(), privateFunctionDescriptors] };
        Ok(array_objects(array)
            .into_iter()
            .map(metal4::FunctionDescriptor::from_inner)
            .collect())
    }

    /// Replaces all private function descriptors from a borrowed Rust slice.
    pub fn set_internal_functions(
        &self,
        values: &[metal4::FunctionDescriptor],
    ) -> Result<(), Error> {
        if !self
            .as_inner()
            .responds_to(sel!(setPrivateFunctionDescriptors:))
        {
            return Err(Error::unsupported(
                "MTL4StaticLinkingDescriptor.setPrivateFunctionDescriptors: is unavailable",
            ));
        }
        let array = object_array(values.iter().map(metal4::FunctionDescriptor::as_inner));
        // SAFETY: selector presence is checked and the owned array contains
        // only MTL4FunctionDescriptor objects.
        unsafe {
            let _: () = msg_send![self.as_inner(), setPrivateFunctionDescriptors: &*array];
        }
        Ok(())
    }

    /// Returns named static-link groups as Rust strings and owned descriptors.
    pub fn groups_map(&self) -> Result<HashMap<String, Vec<metal4::FunctionDescriptor>>, Error> {
        if !self.as_inner().responds_to(sel!(groups)) {
            return Err(Error::unsupported(
                "MTL4StaticLinkingDescriptor.groups is unavailable",
            ));
        }
        // SAFETY: selector presence and its nullable NSDictionary return are checked.
        let dictionary: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), groups] };
        let Some(dictionary) = dictionary else {
            return Ok(HashMap::new());
        };
        // SAFETY: the property is declared NSDictionary, whose allKeys result
        // is an NSArray retained here for traversal.
        let keys: Retained<AnyObject> = unsafe { msg_send![&*dictionary, allKeys] };
        let mut result = HashMap::new();
        for key in array_objects(Some(keys)) {
            // SAFETY: the SDK declares every key as NSString.
            let key_string: Retained<NSString> = unsafe { Retained::cast_unchecked(key.clone()) };
            // SAFETY: dictionary and key are live and objectForKey: returns the
            // NSArray<MTL4FunctionDescriptor *> associated with this key.
            let array: Option<Retained<AnyObject>> =
                unsafe { msg_send![&*dictionary, objectForKey: &*key] };
            let values = array_objects(array)
                .into_iter()
                .map(metal4::FunctionDescriptor::from_inner)
                .collect();
            result.insert(key_string.to_string(), values);
        }
        Ok(result)
    }

    /// Replaces named static-link groups from a Rust map.
    pub fn set_groups_map(
        &self,
        groups: &HashMap<String, Vec<metal4::FunctionDescriptor>>,
    ) -> Result<(), Error> {
        if groups.keys().any(|key| key.as_bytes().contains(&0)) {
            return Err(Error::invalid_argument(
                "static-link group name contains NUL",
            ));
        }
        if !self.as_inner().responds_to(sel!(setGroups:)) {
            return Err(Error::unsupported(
                "MTL4StaticLinkingDescriptor.setGroups: is unavailable",
            ));
        }
        let class = AnyClass::get(c"NSMutableDictionary")
            .ok_or_else(|| Error::unsupported("NSMutableDictionary is unavailable"))?;
        // SAFETY: NSMutableDictionary implements new and returns an owned empty dictionary.
        let dictionary: Retained<AnyObject> = unsafe { msg_send![class, new] };
        for (name, values) in groups {
            let key = NSString::from_str(name);
            let array = object_array(values.iter().map(metal4::FunctionDescriptor::as_inner));
            // SAFETY: key and value have the NSDictionary property's declared
            // NSString and NSArray<MTL4FunctionDescriptor *> classes.
            unsafe {
                let _: () = msg_send![&*dictionary, setObject: &*array, forKey: &*key];
            }
        }
        // SAFETY: selector is present and the dictionary has the exact declared
        // generic key/value object classes.
        unsafe {
            let _: () = msg_send![self.as_inner(), setGroups: &*dictionary];
        }
        Ok(())
    }
}

impl metal4::PipelineStageDynamicLinkingDescriptor {
    /// Returns owned binary functions selected for dynamic linking.
    pub fn binary_linked_functions_vec(&self) -> Result<Vec<metal4::BinaryFunction>, Error> {
        if !self.as_inner().responds_to(sel!(binaryLinkedFunctions)) {
            return Err(Error::unsupported(
                "MTL4PipelineStageDynamicLinkingDescriptor.binaryLinkedFunctions is unavailable",
            ));
        }
        // SAFETY: selector presence and its nullable NSArray return are checked.
        let array = unsafe { msg_send![self.as_inner(), binaryLinkedFunctions] };
        Ok(array_objects(array)
            .into_iter()
            .map(metal4::BinaryFunction::from_inner)
            .collect())
    }

    /// Replaces binary functions selected for dynamic linking.
    pub fn set_binary_linked_functions_slice(
        &self,
        values: &[metal4::BinaryFunction],
    ) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(setBinaryLinkedFunctions:)) {
            return Err(Error::unsupported(
                "MTL4PipelineStageDynamicLinkingDescriptor.setBinaryLinkedFunctions: is unavailable",
            ));
        }
        let array = object_array(values.iter().map(metal4::BinaryFunction::as_inner));
        // SAFETY: selector presence is checked and every element conforms to
        // MTL4BinaryFunction by construction.
        unsafe {
            let _: () = msg_send![self.as_inner(), setBinaryLinkedFunctions: &*array];
        }
        Ok(())
    }

    /// Returns owned dynamic libraries preloaded for linking.
    pub fn preloaded_libraries_vec(&self) -> Result<Vec<metal::DynamicLibrary>, Error> {
        if !self.as_inner().responds_to(sel!(preloadedLibraries)) {
            return Err(Error::unsupported(
                "MTL4PipelineStageDynamicLinkingDescriptor.preloadedLibraries is unavailable",
            ));
        }
        // SAFETY: selector presence and its non-null NSArray return are checked.
        let array = unsafe { msg_send![self.as_inner(), preloadedLibraries] };
        Ok(array_objects(array)
            .into_iter()
            .map(metal::DynamicLibrary::from_inner)
            .collect())
    }

    /// Replaces dynamic libraries preloaded for linking.
    pub fn set_preloaded_libraries_slice(
        &self,
        values: &[metal::DynamicLibrary],
    ) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(setPreloadedLibraries:)) {
            return Err(Error::unsupported(
                "MTL4PipelineStageDynamicLinkingDescriptor.setPreloadedLibraries: is unavailable",
            ));
        }
        let array = object_array(values.iter().map(metal::DynamicLibrary::as_inner));
        // SAFETY: selector presence is checked and every element conforms to
        // MTLDynamicLibrary by construction.
        unsafe {
            let _: () = msg_send![self.as_inner(), setPreloadedLibraries: &*array];
        }
        Ok(())
    }
}

macro_rules! binary_function_list {
    ($getter:ident, $setter:ident, $selector:ident, $set_selector:ident, $context:literal) => {
        /// Returns this pipeline-stage binary-function list as owned wrappers.
        pub fn $getter(&self) -> Result<Vec<metal4::BinaryFunction>, Error> {
            if !self.as_inner().responds_to(sel!($selector)) {
                return Err(Error::unsupported(concat!($context, " is unavailable")));
            }
            // SAFETY: selector presence and its nullable NSArray return are checked.
            let array = unsafe { msg_send![self.as_inner(), $selector] };
            Ok(array_objects(array).into_iter().map(metal4::BinaryFunction::from_inner).collect())
        }

        /// Replaces this pipeline-stage binary-function list from a Rust slice.
        pub fn $setter(&self, values: &[metal4::BinaryFunction]) -> Result<(), Error> {
            if !self.as_inner().responds_to(sel!($set_selector:)) {
                return Err(Error::unsupported(concat!($context, " setter is unavailable")));
            }
            let array = object_array(values.iter().map(metal4::BinaryFunction::as_inner));
            // SAFETY: selector presence is checked and every array element is
            // an owned MTL4BinaryFunction wrapper.
            unsafe { let _: () = msg_send![self.as_inner(), $set_selector: &*array]; }
            Ok(())
        }
    };
}

impl metal4::RenderPipelineBinaryFunctionsDescriptor {
    binary_function_list!(
        fragment_additional_binary_functions_vec,
        set_fragment_additional_binary_functions_slice,
        fragmentAdditionalBinaryFunctions,
        setFragmentAdditionalBinaryFunctions,
        "fragmentAdditionalBinaryFunctions"
    );
    binary_function_list!(
        mesh_additional_binary_functions_vec,
        set_mesh_additional_binary_functions_slice,
        meshAdditionalBinaryFunctions,
        setMeshAdditionalBinaryFunctions,
        "meshAdditionalBinaryFunctions"
    );
    binary_function_list!(
        object_additional_binary_functions_vec,
        set_object_additional_binary_functions_slice,
        objectAdditionalBinaryFunctions,
        setObjectAdditionalBinaryFunctions,
        "objectAdditionalBinaryFunctions"
    );
    binary_function_list!(
        tile_additional_binary_functions_vec,
        set_tile_additional_binary_functions_slice,
        tileAdditionalBinaryFunctions,
        setTileAdditionalBinaryFunctions,
        "tileAdditionalBinaryFunctions"
    );
    binary_function_list!(
        vertex_additional_binary_functions_vec,
        set_vertex_additional_binary_functions_slice,
        vertexAdditionalBinaryFunctions,
        setVertexAdditionalBinaryFunctions,
        "vertexAdditionalBinaryFunctions"
    );

    /// Restores every binary-function list to its default value.
    pub fn reset_safe(&self) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(reset)) {
            return Err(Error::unsupported(
                "MTL4RenderPipelineBinaryFunctionsDescriptor.reset is unavailable",
            ));
        }
        // SAFETY: the zero-argument selector is present on this exact class.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

impl metal4::RenderPipelineColorAttachmentDescriptor {
    /// Restores the attachment's default blend and format state.
    pub fn reset_safe(&self) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(reset)) {
            return Err(Error::unsupported(
                "MTL4RenderPipelineColorAttachmentDescriptor.reset is unavailable",
            ));
        }
        // SAFETY: the zero-argument selector is present on this exact class.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

impl metal4::RenderPipelineColorAttachmentDescriptorArray {
    /// Returns the attachment at an index, rejecting unreasonable indices before Objective-C.
    pub fn attachment(
        &self,
        index: usize,
    ) -> Result<Option<metal4::RenderPipelineColorAttachmentDescriptor>, Error> {
        if index >= 8 {
            return Err(Error::invalid_argument(
                "render color attachment index must be below 8",
            ));
        }
        if !self.as_inner().responds_to(sel!(objectAtIndexedSubscript:)) {
            return Err(Error::unsupported(
                "MTL4RenderPipelineColorAttachmentDescriptorArray.object is unavailable",
            ));
        }
        // SAFETY: selector is present and index is restricted to Metal's eight
        // render color attachment slots.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
        Ok(value.map(metal4::RenderPipelineColorAttachmentDescriptor::from_inner))
    }

    /// Replaces the attachment at an index.
    pub fn set_attachment(
        &self,
        index: usize,
        value: &metal4::RenderPipelineColorAttachmentDescriptor,
    ) -> Result<(), Error> {
        if index >= 8 {
            return Err(Error::invalid_argument(
                "render color attachment index must be below 8",
            ));
        }
        if !self
            .as_inner()
            .responds_to(sel!(setObject:atIndexedSubscript:))
        {
            return Err(Error::unsupported(
                "MTL4RenderPipelineColorAttachmentDescriptorArray.setObject is unavailable",
            ));
        }
        // SAFETY: selector is present, index is validated, and the value is an
        // owned attachment descriptor.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setObject: value.as_inner(), atIndexedSubscript: index];
        }
        Ok(())
    }

    /// Restores the array's default attachment state.
    pub fn reset_safe(&self) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(reset)) {
            return Err(Error::unsupported(
                "MTL4RenderPipelineColorAttachmentDescriptorArray.reset is unavailable",
            ));
        }
        // SAFETY: the zero-argument selector is present on this exact class.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

impl metal4::RenderPipelineDescriptor {
    /// Restores all render-pipeline properties to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(reset)) {
            return Err(Error::unsupported(
                "MTL4RenderPipelineDescriptor.reset is unavailable",
            ));
        }
        // SAFETY: the zero-argument selector is present on this exact class.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

fn required_threadgroup_size(size: crate::metal::Size) -> Result<MTLSize, Error> {
    let all_zero = size.width == 0 && size.height == 0 && size.depth == 0;
    let all_nonzero = size.width != 0 && size.height != 0 && size.depth != 0;
    if !all_zero && !all_nonzero {
        return Err(Error::invalid_argument(
            "required threadgroup size must be entirely zero or entirely non-zero",
        ));
    }
    size.width
        .checked_mul(size.height)
        .and_then(|value| value.checked_mul(size.depth))
        .ok_or_else(|| Error::invalid_argument("required threadgroup size overflows usize"))?;
    Ok(size.into())
}

macro_rules! required_threads_descriptor {
    ($type:ty, $context:literal) => {
        impl $type {
            /// Returns the optional required threadgroup size.
            pub fn required_threads_per_threadgroup_safe(
                &self,
            ) -> Result<crate::metal::Size, Error> {
                if !self
                    .as_inner()
                    .responds_to(sel!(requiredThreadsPerThreadgroup))
                {
                    return Err(Error::unsupported(concat!(
                        $context,
                        ".requiredThreadsPerThreadgroup is unavailable"
                    )));
                }
                // SAFETY: selector presence is checked and MTLSize has the
                // exact SDK-declared aggregate return ABI.
                let value: MTLSize =
                    unsafe { msg_send![self.as_inner(), requiredThreadsPerThreadgroup] };
                Ok(crate::metal::Size::new(
                    value.width,
                    value.height,
                    value.depth,
                ))
            }

            /// Sets or disables the required threadgroup size after validation.
            pub fn set_required_threads_per_threadgroup_safe(
                &self,
                size: crate::metal::Size,
            ) -> Result<(), Error> {
                let size = required_threadgroup_size(size)?;
                if !self
                    .as_inner()
                    .responds_to(sel!(setRequiredThreadsPerThreadgroup:))
                {
                    return Err(Error::unsupported(concat!(
                        $context,
                        ".setRequiredThreadsPerThreadgroup: is unavailable"
                    )));
                }
                // SAFETY: selector presence is checked and size was validated
                // as disabled or a complete non-zero three-dimensional size.
                unsafe {
                    let _: () = msg_send![self.as_inner(), setRequiredThreadsPerThreadgroup: size];
                }
                Ok(())
            }

            /// Restores the descriptor to Metal defaults.
            pub fn reset_pipeline_descriptor(&self) -> Result<(), Error> {
                if !self.as_inner().responds_to(sel!(reset)) {
                    return Err(Error::unsupported(concat!(
                        $context,
                        ".reset is unavailable"
                    )));
                }
                // SAFETY: the zero-argument selector is present on this exact class.
                unsafe {
                    let _: () = msg_send![self.as_inner(), reset];
                }
                Ok(())
            }
        }
    };
}

required_threads_descriptor!(
    metal4::ComputePipelineDescriptor,
    "MTL4ComputePipelineDescriptor"
);
required_threads_descriptor!(
    metal4::TileRenderPipelineDescriptor,
    "MTL4TileRenderPipelineDescriptor"
);

impl metal4::StitchedFunctionDescriptor {
    /// Returns the stitched function's component descriptors.
    pub fn function_descriptors_vec(&self) -> Result<Vec<metal4::FunctionDescriptor>, Error> {
        if !self.as_inner().responds_to(sel!(functionDescriptors)) {
            return Err(Error::unsupported(
                "MTL4StitchedFunctionDescriptor.functionDescriptors is unavailable",
            ));
        }
        // SAFETY: selector presence and its nullable NSArray return are checked.
        let array = unsafe { msg_send![self.as_inner(), functionDescriptors] };
        Ok(array_objects(array)
            .into_iter()
            .map(metal4::FunctionDescriptor::from_inner)
            .collect())
    }

    /// Replaces the stitched function's component descriptors.
    pub fn set_function_descriptors_slice(
        &self,
        values: &[metal4::FunctionDescriptor],
    ) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(setFunctionDescriptors:)) {
            return Err(Error::unsupported(
                "MTL4StitchedFunctionDescriptor.setFunctionDescriptors: is unavailable",
            ));
        }
        let array = object_array(values.iter().map(metal4::FunctionDescriptor::as_inner));
        // SAFETY: selector presence is checked and all array elements are
        // type-checked MTL4FunctionDescriptor wrappers.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFunctionDescriptors: &*array];
        }
        Ok(())
    }
}

impl metal4::RenderPassDescriptor {
    /// Sets a checked color attachment using a clear load and store action.
    pub fn set_color_attachment(
        &self,
        index: usize,
        texture: &Texture,
        clear_color: ClearColor,
    ) -> Result<(), Error> {
        validate_mtl4_color_attachment(index, clear_color)?;
        let descriptor = self.as_inner();
        if !descriptor.responds_to(sel!(colorAttachments)) {
            return Err(Error::unsupported(
                "MTL4RenderPassDescriptor color attachments are unavailable",
            ));
        }
        // SAFETY: the selector is checked and the property is an attachment array.
        let attachments: Retained<AnyObject> = unsafe { msg_send![descriptor, colorAttachments] };
        if !attachments.responds_to(sel!(objectAtIndexedSubscript:)) {
            return Err(Error::unsupported(
                "MTL4RenderPassDescriptor color attachment indexing is unavailable",
            ));
        }
        // SAFETY: Metal defines eight color attachment slots and the index was checked.
        let attachment: Retained<AnyObject> =
            unsafe { msg_send![&*attachments, objectAtIndexedSubscript: index] };
        for (selector, operation) in [
            (sel!(setTexture:), "MTL4 color attachment texture"),
            (sel!(setLoadAction:), "MTL4 color attachment load action"),
            (sel!(setStoreAction:), "MTL4 color attachment store action"),
            (sel!(setClearColor:), "MTL4 color attachment clear color"),
        ] {
            if !attachment.responds_to(selector) {
                return Err(Error::unsupported(format!("{operation} is unavailable")));
            }
        }
        // SAFETY: all setters are checked; texture is a live typed Metal texture,
        // and the load/store enums and clear-color struct match their ObjC ABI.
        unsafe {
            let _: () = msg_send![&*attachment, setTexture: texture.as_any_object()];
            let _: () = msg_send![&*attachment, setLoadAction: MTLLoadAction::Clear];
            let _: () = msg_send![&*attachment, setStoreAction: MTLStoreAction::Store];
            let _: () = msg_send![&*attachment, setClearColor: MTLClearColor {
                red: clear_color.red,
                green: clear_color.green,
                blue: clear_color.blue,
                alpha: clear_color.alpha,
            }];
        }
        Ok(())
    }

    /// Returns up to `count` programmable sample positions as owned values.
    pub fn sample_positions_vec(
        &self,
        count: usize,
    ) -> Result<Vec<crate::metal::generated_struct_types::SamplePosition>, Error> {
        if count > 32 {
            return Err(Error::invalid_argument(
                "Metal supports at most 32 programmable sample positions",
            ));
        }
        if !self.as_inner().responds_to(sel!(getSamplePositions:count:)) {
            return Err(Error::unsupported(
                "MTL4RenderPassDescriptor programmable sample positions are unavailable",
            ));
        }
        let mut positions = vec![MTLSamplePosition { x: 0.0, y: 0.0 }; count];
        // SAFETY: the vector owns writable storage for exactly count values;
        // Metal writes synchronously and returns the initialized prefix length.
        let written: usize = unsafe {
            msg_send![self.as_inner(), getSamplePositions: positions.as_mut_ptr(), count: count]
        };
        if written > count {
            return Err(Error::unsupported(
                "Metal returned more sample positions than requested",
            ));
        }
        positions.truncate(written);
        Ok(positions
            .into_iter()
            .map(
                |value| crate::metal::generated_struct_types::SamplePosition {
                    x: value.x,
                    y: value.y,
                },
            )
            .collect())
    }

    /// Sets programmable sample positions from a borrowed Rust slice.
    pub fn set_sample_positions_slice(
        &self,
        positions: &[crate::metal::generated_struct_types::SamplePosition],
    ) -> Result<(), Error> {
        if positions.len() > 32
            || positions
                .iter()
                .any(|position| !position.x.is_finite() || !position.y.is_finite())
        {
            return Err(Error::invalid_argument(
                "sample positions must be finite and contain at most 32 entries",
            ));
        }
        if !self.as_inner().responds_to(sel!(setSamplePositions:count:)) {
            return Err(Error::unsupported(
                "MTL4RenderPassDescriptor programmable sample positions are unavailable",
            ));
        }
        let positions: Vec<_> = positions
            .iter()
            .map(|value| MTLSamplePosition {
                x: value.x,
                y: value.y,
            })
            .collect();
        // SAFETY: the pointer references positions.len() initialized values
        // and Metal copies them synchronously.
        unsafe {
            let _: () = msg_send![self.as_inner(), setSamplePositions: positions.as_ptr(), count: positions.len()];
        }
        Ok(())
    }
}

const MAX_MTL4_COLOR_ATTACHMENTS: usize = 8;

fn validate_mtl4_color_attachment(index: usize, clear_color: ClearColor) -> Result<(), Error> {
    if index >= MAX_MTL4_COLOR_ATTACHMENTS {
        return Err(Error::invalid_argument(format!(
            "color attachment index {index} exceeds Metal's limit of {MAX_MTL4_COLOR_ATTACHMENTS}"
        )));
    }
    if [
        clear_color.red,
        clear_color.green,
        clear_color.blue,
        clear_color.alpha,
    ]
    .iter()
    .any(|component| !component.is_finite())
    {
        return Err(Error::invalid_argument(
            "clear-color components must be finite",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod mtl4_color_attachment_tests {
    use super::*;

    #[test]
    fn validates_attachment_index_and_clear_color() {
        let color = ClearColor::new(0.0, 0.0, 0.0, 1.0);
        assert!(validate_mtl4_color_attachment(7, color).is_ok());
        assert!(validate_mtl4_color_attachment(8, color).is_err());
        assert!(
            validate_mtl4_color_attachment(0, ClearColor::new(f64::NAN, 0.0, 0.0, 1.0)).is_err()
        );
    }
}

impl metal4::PipelineDataSetSerializer {
    fn as_serializer(&self) -> &ProtocolObject<dyn MTL4PipelineDataSetSerializer> {
        // SAFETY: this wrapper is constructed only from selectors declared to
        // return an object conforming to MTL4PipelineDataSetSerializer.
        unsafe {
            &*(std::ptr::from_ref(self.as_inner())
                .cast::<ProtocolObject<dyn MTL4PipelineDataSetSerializer>>())
        }
    }

    /// Serializes captured binaries to an archive at a validated file path.
    pub fn serialize_archive_to_path(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        let path = path
            .as_ref()
            .to_str()
            .ok_or_else(|| Error::invalid_argument("archive path is not valid UTF-8"))?;
        if path.as_bytes().contains(&0) {
            return Err(Error::invalid_argument("archive path contains NUL"));
        }
        if !self
            .as_inner()
            .responds_to(sel!(serializeAsArchiveAndFlushToURL:error:))
        {
            return Err(Error::unsupported(
                "pipeline archive serialization is unavailable",
            ));
        }
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        self.as_serializer()
            .serializeAsArchiveAndFlushToURL_error(&url)
            .map_err(|error| metal_error(&error))
    }

    /// Serializes captured pipeline descriptors into owned script bytes.
    pub fn serialize_pipelines_script(&self) -> Result<Vec<u8>, Error> {
        if !self
            .as_inner()
            .responds_to(sel!(serializeAsPipelinesScriptWithError:))
        {
            return Err(Error::unsupported(
                "pipeline script serialization is unavailable",
            ));
        }
        let data: Retained<NSData> = self
            .as_serializer()
            .serializeAsPipelinesScriptWithError()
            .map_err(|error| metal_error(&error))?;
        let mut bytes = vec![0_u8; data.length()];
        if !bytes.is_empty() {
            let pointer = std::ptr::NonNull::new(bytes.as_mut_ptr().cast::<c_void>())
                .expect("a non-empty Vec has a non-null allocation");
            // SAFETY: pointer references exactly data.length() writable bytes;
            // NSData copies synchronously into the caller-owned Vec.
            unsafe { data.getBytes_length(pointer, bytes.len()) };
        }
        Ok(bytes)
    }
}

impl metal4::CommandAllocator {
    /// Returns the number of bytes currently allocated by this allocator.
    pub fn allocated_size_safe(&self) -> Result<u64, Error> {
        if !self.as_inner().responds_to(sel!(allocatedSize)) {
            return Err(Error::unsupported(
                "MTL4CommandAllocator.allocatedSize is unavailable",
            ));
        }
        // SAFETY: selector presence is checked and uint64_t has the exact u64 ABI.
        Ok(unsafe { msg_send![self.as_inner(), allocatedSize] })
    }

    /// Reclaims allocator storage that is no longer referenced by GPU work.
    pub fn reset_safe(&self) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(reset)) {
            return Err(Error::unsupported(
                "MTL4CommandAllocator.reset is unavailable",
            ));
        }
        // SAFETY: the zero-argument selector is present on this exact class.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

impl metal4::CompilerTask {
    /// Blocks until the compiler task finishes after checking availability.
    pub fn wait_until_completed_safe(&self) -> Result<(), Error> {
        if !self.as_inner().responds_to(sel!(waitUntilCompleted)) {
            return Err(Error::unsupported(
                "MTL4CompilerTask.waitUntilCompleted is unavailable",
            ));
        }
        // SAFETY: the zero-argument selector is present and blocks only the
        // calling thread according to the framework contract.
        unsafe {
            let _: () = msg_send![self.as_inner(), waitUntilCompleted];
        }
        Ok(())
    }
}
