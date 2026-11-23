//! Safe collection and descriptor bridges for Metal pipeline declarations.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    Argument, BinaryArchive, Binding, ComputePipelineDescriptor, ComputePipelineReflection,
    DynamicLibrary, LinkedFunctions, LogicalToPhysicalColorAttachmentMap,
    MeshRenderPipelineDescriptor, PipelineBufferDescriptorArray,
    RenderPipelineColorAttachmentDescriptor, RenderPipelineColorAttachmentDescriptorArray,
    RenderPipelineFunctionsDescriptor, RenderPipelineReflection,
    TileRenderPipelineColorAttachmentDescriptor, TileRenderPipelineColorAttachmentDescriptorArray,
    TileRenderPipelineDescriptor, VertexDescriptor,
};
use crate::metal::{Function, RenderPipelineDescriptor, Size};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, MessageReceiver};
use objc2::{msg_send, sel};
use objc2_foundation::NSString;
use objc2_metal::MTLSize;
use std::collections::HashMap;

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    name: &str,
) -> Result<(), Error> {
    // SAFETY: every Objective-C object implements respondsToSelector: and the
    // selector/bool ABI is stable.
    let available: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if available {
        Ok(())
    } else {
        Err(Error::unsupported(format!("{name} is unavailable")))
    }
}

fn object_array<'a>(values: impl IntoIterator<Item = &'a AnyObject>) -> Retained<AnyObject> {
    let class = AnyClass::get(c"NSMutableArray")
        .expect("Foundation provides NSMutableArray whenever Metal is loaded");
    // SAFETY: NSMutableArray implements new and returns a retained empty array.
    let array: Retained<AnyObject> = unsafe { msg_send![class, new] };
    for value in values {
        // SAFETY: the array and non-null element remain live for the message.
        unsafe {
            let _: () = msg_send![&*array, addObject: value];
        }
    }
    array
}

fn array_objects(
    array: Option<Retained<AnyObject>>,
    what: &str,
) -> Result<Vec<Retained<AnyObject>>, Error> {
    let Some(array) = array else {
        return Ok(Vec::new());
    };
    require_selector(&array, sel!(count), what)?;
    require_selector(&array, sel!(objectAtIndex:), what)?;
    // SAFETY: selector presence is checked and NSArray count uses NSUInteger.
    let count: usize = unsafe { msg_send![&*array, count] };
    (0..count)
        .map(|index| {
            // SAFETY: index is below the count read from the same immutable array,
            // and objc2 retains the object result.
            Ok(unsafe { msg_send![&*array, objectAtIndex: index] })
        })
        .collect()
}

fn get_object_array(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    name: &str,
) -> Result<Vec<Retained<AnyObject>>, Error> {
    require_selector(object, selector, name)?;
    // SAFETY: selector presence is checked and the selected property is declared
    // as a nullable NSArray in the local metal-cpp inventory.
    let array = unsafe {
        Retained::retain_autoreleased(object.send_message::<_, *mut AnyObject>(selector, ()))
    };
    array_objects(array, name)
}

fn set_object_array<'a>(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    values: impl IntoIterator<Item = &'a AnyObject>,
    name: &str,
) -> Result<(), Error> {
    require_selector(object, selector, name)?;
    let array = object_array(values);
    // SAFETY: selector presence is checked and every selected setter is declared
    // to accept NSArray; the temporary array outlives the message.
    unsafe {
        let _: () = object.send_message(selector, (&*array,));
    }
    Ok(())
}

fn checked_size(value: Size, name: &str) -> Result<Size, Error> {
    if value.width == 0 || value.height == 0 || value.depth == 0 {
        Err(Error::invalid_argument(format!(
            "{name} dimensions must all be non-zero"
        )))
    } else {
        Ok(value)
    }
}

fn from_mtl_size(value: MTLSize) -> Size {
    Size::new(value.width, value.height, value.depth)
}

macro_rules! generated_array_property {
    ($type:ty, $get:ident, $set:ident, $getter:ident, $setter:ident, $item:ty, $context:literal) => {
        impl $type {
            #[doc = concat!("Returns `", $context, "` as an owned Rust vector.")]
            pub fn $get(&self) -> Result<Vec<$item>, Error> {
                get_object_array(self.as_inner(), sel!($getter), $context).map(|values| {
                    values.into_iter().map(<$item>::from_inner).collect()
                })
            }

            #[doc = concat!("Sets `", $context, "` from a borrowed Rust slice.")]
            pub fn $set(&self, values: &[$item]) -> Result<(), Error> {
                set_object_array(
                    self.as_inner(),
                    sel!($setter:),
                    values.iter().map(<$item>::as_inner),
                    $context,
                )
            }
        }
    };
}

generated_array_property!(
    ComputePipelineDescriptor,
    binary_archives_vec,
    set_binary_archives_slice,
    binaryArchives,
    setBinaryArchives,
    BinaryArchive,
    "MTL::ComputePipelineDescriptor::binaryArchives"
);
generated_array_property!(
    ComputePipelineDescriptor,
    insert_libraries_vec,
    set_insert_libraries_slice,
    insertLibraries,
    setInsertLibraries,
    DynamicLibrary,
    "MTL::ComputePipelineDescriptor::insertLibraries"
);
generated_array_property!(
    ComputePipelineDescriptor,
    preloaded_libraries_vec,
    set_preloaded_libraries_slice,
    preloadedLibraries,
    setPreloadedLibraries,
    DynamicLibrary,
    "MTL::ComputePipelineDescriptor::preloadedLibraries"
);
generated_array_property!(
    TileRenderPipelineDescriptor,
    binary_archives_vec,
    set_binary_archives_slice,
    binaryArchives,
    setBinaryArchives,
    BinaryArchive,
    "MTL::TileRenderPipelineDescriptor::binaryArchives"
);
generated_array_property!(
    TileRenderPipelineDescriptor,
    preloaded_libraries_vec,
    set_preloaded_libraries_slice,
    preloadedLibraries,
    setPreloadedLibraries,
    DynamicLibrary,
    "MTL::TileRenderPipelineDescriptor::preloadedLibraries"
);
generated_array_property!(
    MeshRenderPipelineDescriptor,
    binary_archives_vec,
    set_binary_archives_slice,
    binaryArchives,
    setBinaryArchives,
    BinaryArchive,
    "MTL::MeshRenderPipelineDescriptor::binaryArchives"
);
macro_rules! function_array_property {
    ($get:ident, $set:ident, $getter:ident, $setter:ident, $context:literal) => {
        impl RenderPipelineFunctionsDescriptor {
            #[doc = concat!("Returns `", $context, "` as an owned Rust vector.")]
            pub fn $get(&self) -> Result<Vec<Function>, Error> {
                get_object_array(self.as_inner(), sel!($getter), $context)?
                    .into_iter()
                    .map(Function::from_any_object)
                    .collect()
            }

            #[doc = concat!("Sets `", $context, "` from a borrowed Rust slice.")]
            pub fn $set(&self, values: &[Function]) -> Result<(), Error> {
                set_object_array(
                    self.as_inner(),
                    sel!($setter:),
                    values.iter().map(Function::as_any_object),
                    $context,
                )
            }
        }
    };
}

function_array_property!(
    fragment_additional_binary_functions_vec,
    set_fragment_additional_binary_functions_slice,
    fragmentAdditionalBinaryFunctions,
    setFragmentAdditionalBinaryFunctions,
    "MTL::RenderPipelineFunctionsDescriptor::fragmentAdditionalBinaryFunctions"
);
function_array_property!(
    tile_additional_binary_functions_vec,
    set_tile_additional_binary_functions_slice,
    tileAdditionalBinaryFunctions,
    setTileAdditionalBinaryFunctions,
    "MTL::RenderPipelineFunctionsDescriptor::tileAdditionalBinaryFunctions"
);
function_array_property!(
    vertex_additional_binary_functions_vec,
    set_vertex_additional_binary_functions_slice,
    vertexAdditionalBinaryFunctions,
    setVertexAdditionalBinaryFunctions,
    "MTL::RenderPipelineFunctionsDescriptor::vertexAdditionalBinaryFunctions"
);

impl ComputePipelineDescriptor {
    /// Resets the descriptor after checking selector availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL::ComputePipelineDescriptor::reset",
        )?;
        // SAFETY: selector presence is checked and reset has no arguments/result.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }

    /// Returns required threadgroup dimensions.
    pub fn required_threads_per_threadgroup_safe(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(requiredThreadsPerThreadgroup),
            "MTL::ComputePipelineDescriptor::requiredThreadsPerThreadgroup",
        )?;
        // SAFETY: selector presence and the MTLSize ABI are checked by the inventory.
        let value: MTLSize = unsafe { msg_send![self.as_inner(), requiredThreadsPerThreadgroup] };
        Ok(from_mtl_size(value))
    }

    /// Sets validated non-zero required threadgroup dimensions.
    pub fn set_required_threads_per_threadgroup_safe(&self, value: Size) -> Result<(), Error> {
        let value = checked_size(value, "required threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(setRequiredThreadsPerThreadgroup:),
            "MTL::ComputePipelineDescriptor::setRequiredThreadsPerThreadgroup",
        )?;
        // SAFETY: selector presence is checked and value has the declared MTLSize ABI.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setRequiredThreadsPerThreadgroup: MTLSize::from(value)];
        }
        Ok(())
    }
}

impl TileRenderPipelineDescriptor {
    /// Resets the descriptor after checking selector availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL::TileRenderPipelineDescriptor::reset",
        )?;
        // SAFETY: selector presence is checked and reset has no arguments/result.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }

    /// Returns required tile threadgroup dimensions.
    pub fn required_threads_per_threadgroup_safe(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(requiredThreadsPerThreadgroup),
            "MTL::TileRenderPipelineDescriptor::requiredThreadsPerThreadgroup",
        )?;
        // SAFETY: selector presence and the MTLSize ABI are checked by the inventory.
        let value: MTLSize = unsafe { msg_send![self.as_inner(), requiredThreadsPerThreadgroup] };
        Ok(from_mtl_size(value))
    }

    /// Sets validated non-zero tile threadgroup dimensions.
    pub fn set_required_threads_per_threadgroup_safe(&self, value: Size) -> Result<(), Error> {
        let value = checked_size(value, "required tile threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(setRequiredThreadsPerThreadgroup:),
            "MTL::TileRenderPipelineDescriptor::setRequiredThreadsPerThreadgroup",
        )?;
        // SAFETY: selector presence is checked and value has the declared MTLSize ABI.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setRequiredThreadsPerThreadgroup: MTLSize::from(value)];
        }
        Ok(())
    }
}

impl MeshRenderPipelineDescriptor {
    /// Resets the descriptor after checking selector availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL::MeshRenderPipelineDescriptor::reset",
        )?;
        // SAFETY: selector presence is checked and reset has no arguments/result.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }

    /// Returns required mesh threadgroup dimensions.
    pub fn required_threads_per_mesh_threadgroup_safe(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(requiredThreadsPerMeshThreadgroup),
            "MTL::MeshRenderPipelineDescriptor::requiredThreadsPerMeshThreadgroup",
        )?;
        // SAFETY: selector presence and the MTLSize ABI are checked by the inventory.
        let value: MTLSize =
            unsafe { msg_send![self.as_inner(), requiredThreadsPerMeshThreadgroup] };
        Ok(from_mtl_size(value))
    }

    /// Sets validated non-zero mesh threadgroup dimensions.
    pub fn set_required_threads_per_mesh_threadgroup_safe(&self, value: Size) -> Result<(), Error> {
        let value = checked_size(value, "required mesh threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(setRequiredThreadsPerMeshThreadgroup:),
            "MTL::MeshRenderPipelineDescriptor::setRequiredThreadsPerMeshThreadgroup",
        )?;
        // SAFETY: selector presence is checked and value has the declared MTLSize ABI.
        unsafe {
            let _: () = msg_send![self.as_inner(), setRequiredThreadsPerMeshThreadgroup: MTLSize::from(value)];
        }
        Ok(())
    }

    /// Returns required object threadgroup dimensions.
    pub fn required_threads_per_object_threadgroup_safe(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(requiredThreadsPerObjectThreadgroup),
            "MTL::MeshRenderPipelineDescriptor::requiredThreadsPerObjectThreadgroup",
        )?;
        // SAFETY: selector presence and the MTLSize ABI are checked by the inventory.
        let value: MTLSize =
            unsafe { msg_send![self.as_inner(), requiredThreadsPerObjectThreadgroup] };
        Ok(from_mtl_size(value))
    }

    /// Sets validated non-zero object threadgroup dimensions.
    pub fn set_required_threads_per_object_threadgroup_safe(
        &self,
        value: Size,
    ) -> Result<(), Error> {
        let value = checked_size(value, "required object threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(setRequiredThreadsPerObjectThreadgroup:),
            "MTL::MeshRenderPipelineDescriptor::setRequiredThreadsPerObjectThreadgroup",
        )?;
        // SAFETY: selector presence is checked and value has the declared MTLSize ABI.
        unsafe {
            let _: () = msg_send![self.as_inner(), setRequiredThreadsPerObjectThreadgroup: MTLSize::from(value)];
        }
        Ok(())
    }
}

macro_rules! reflection_vec {
    ($type:ty, $method:ident, $selector:ident, $item:ty, $context:literal) => {
        impl $type {
            #[doc = concat!("Returns `", $context, "` as an owned Rust vector.")]
            pub fn $method(&self) -> Result<Vec<$item>, Error> {
                get_object_array(self.as_inner(), sel!($selector), $context)
                    .map(|values| values.into_iter().map(<$item>::from_inner).collect())
            }
        }
    };
}

reflection_vec!(
    ComputePipelineReflection,
    arguments_vec,
    arguments,
    Argument,
    "MTL::ComputePipelineReflection::arguments"
);
reflection_vec!(
    ComputePipelineReflection,
    bindings_vec,
    bindings,
    Binding,
    "MTL::ComputePipelineReflection::bindings"
);
reflection_vec!(
    RenderPipelineReflection,
    fragment_arguments_vec,
    fragmentArguments,
    Argument,
    "MTL::RenderPipelineReflection::fragmentArguments"
);
reflection_vec!(
    RenderPipelineReflection,
    fragment_bindings_vec,
    fragmentBindings,
    Binding,
    "MTL::RenderPipelineReflection::fragmentBindings"
);
reflection_vec!(
    RenderPipelineReflection,
    mesh_bindings_vec,
    meshBindings,
    Binding,
    "MTL::RenderPipelineReflection::meshBindings"
);
reflection_vec!(
    RenderPipelineReflection,
    object_bindings_vec,
    objectBindings,
    Binding,
    "MTL::RenderPipelineReflection::objectBindings"
);
reflection_vec!(
    RenderPipelineReflection,
    tile_arguments_vec,
    tileArguments,
    Argument,
    "MTL::RenderPipelineReflection::tileArguments"
);
reflection_vec!(
    RenderPipelineReflection,
    tile_bindings_vec,
    tileBindings,
    Binding,
    "MTL::RenderPipelineReflection::tileBindings"
);
reflection_vec!(
    RenderPipelineReflection,
    vertex_arguments_vec,
    vertexArguments,
    Argument,
    "MTL::RenderPipelineReflection::vertexArguments"
);
reflection_vec!(
    RenderPipelineReflection,
    vertex_bindings_vec,
    vertexBindings,
    Binding,
    "MTL::RenderPipelineReflection::vertexBindings"
);

impl RenderPipelineColorAttachmentDescriptorArray {
    /// Returns an attachment at one of Metal's eight color attachment slots.
    pub fn get(&self, index: usize) -> Result<RenderPipelineColorAttachmentDescriptor, Error> {
        if index >= 8 {
            return Err(Error::invalid_argument(
                "color attachment index must be below 8",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(objectAtIndexedSubscript:),
            "MTL::RenderPipelineColorAttachmentDescriptorArray::object",
        )?;
        // SAFETY: selector presence and the validated fixed attachment bound establish the call.
        let value = unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
        Ok(RenderPipelineColorAttachmentDescriptor::from_inner(value))
    }

    /// Replaces an attachment at one of Metal's eight color attachment slots.
    pub fn set(
        &self,
        index: usize,
        value: &RenderPipelineColorAttachmentDescriptor,
    ) -> Result<(), Error> {
        if index >= 8 {
            return Err(Error::invalid_argument(
                "color attachment index must be below 8",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setObject:atIndexedSubscript:),
            "MTL::RenderPipelineColorAttachmentDescriptorArray::setObject",
        )?;
        // SAFETY: selector presence, class-preserving wrapper, and fixed bound are checked.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setObject: value.as_inner(), atIndexedSubscript: index];
        }
        Ok(())
    }
}

impl TileRenderPipelineColorAttachmentDescriptorArray {
    /// Returns an attachment at one of Metal's eight color attachment slots.
    pub fn get(&self, index: usize) -> Result<TileRenderPipelineColorAttachmentDescriptor, Error> {
        if index >= 8 {
            return Err(Error::invalid_argument(
                "tile color attachment index must be below 8",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(objectAtIndexedSubscript:),
            "MTL::TileRenderPipelineColorAttachmentDescriptorArray::object",
        )?;
        // SAFETY: selector presence and the validated fixed attachment bound establish the call.
        let value = unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
        Ok(TileRenderPipelineColorAttachmentDescriptor::from_inner(
            value,
        ))
    }

    /// Replaces an attachment at one of Metal's eight color attachment slots.
    pub fn set(
        &self,
        index: usize,
        value: &TileRenderPipelineColorAttachmentDescriptor,
    ) -> Result<(), Error> {
        if index >= 8 {
            return Err(Error::invalid_argument(
                "tile color attachment index must be below 8",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setObject:atIndexedSubscript:),
            "MTL::TileRenderPipelineColorAttachmentDescriptorArray::setObject",
        )?;
        // SAFETY: selector presence, class-preserving wrapper, and fixed bound are checked.
        unsafe {
            let _: () =
                msg_send![self.as_inner(), setObject: value.as_inner(), atIndexedSubscript: index];
        }
        Ok(())
    }
}

impl LogicalToPhysicalColorAttachmentMap {
    /// Returns the physical attachment index for a validated logical slot.
    pub fn physical_index(&self, logical_index: usize) -> Result<usize, Error> {
        if logical_index >= 8 {
            return Err(Error::invalid_argument(
                "logical attachment index must be below 8",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(getPhysicalIndex:),
            "MTL::LogicalToPhysicalColorAttachmentMap::getPhysicalIndex",
        )?;
        // SAFETY: selector presence and the fixed logical attachment bound are checked.
        Ok(unsafe { msg_send![self.as_inner(), getPhysicalIndex: logical_index] })
    }

    /// Sets a validated logical-to-physical attachment mapping.
    pub fn set_physical_index(
        &self,
        logical_index: usize,
        physical_index: usize,
    ) -> Result<(), Error> {
        if logical_index >= 8 || physical_index >= 8 {
            return Err(Error::invalid_argument(
                "attachment indices must be below 8",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setPhysicalIndex:forLogicalIndex:),
            "MTL::LogicalToPhysicalColorAttachmentMap::setPhysicalIndex",
        )?;
        // SAFETY: selector presence and both fixed attachment bounds are checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), setPhysicalIndex: physical_index, forLogicalIndex: logical_index];
        }
        Ok(())
    }

    /// Resets all logical-to-physical mappings.
    pub fn reset_safe(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL::LogicalToPhysicalColorAttachmentMap::reset",
        )?;
        // SAFETY: selector presence is checked and reset has no arguments/result.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

impl RenderPipelineDescriptor {
    fn as_any_object(&self) -> &AnyObject {
        // SAFETY: MTLRenderPipelineDescriptor is backed by the same Objective-C object pointer.
        unsafe { &*(std::ptr::from_ref(&*self.inner).cast::<AnyObject>()) }
    }

    /// Returns configured binary archives as an owned Rust vector.
    pub fn binary_archives_vec(&self) -> Result<Vec<BinaryArchive>, Error> {
        get_object_array(
            self.as_any_object(),
            sel!(binaryArchives),
            "MTL::RenderPipelineDescriptor::binaryArchives",
        )
        .map(|values| values.into_iter().map(BinaryArchive::from_inner).collect())
    }

    /// Sets binary archives from a borrowed Rust slice.
    pub fn set_binary_archives_slice(&self, values: &[BinaryArchive]) -> Result<(), Error> {
        set_object_array(
            self.as_any_object(),
            sel!(setBinaryArchives:),
            values.iter().map(BinaryArchive::as_inner),
            "MTL::RenderPipelineDescriptor::setBinaryArchives",
        )
    }

    /// Returns fragment preloaded dynamic libraries as an owned Rust vector.
    pub fn fragment_preloaded_libraries_vec(&self) -> Result<Vec<DynamicLibrary>, Error> {
        get_object_array(
            self.as_any_object(),
            sel!(fragmentPreloadedLibraries),
            "MTL::RenderPipelineDescriptor::fragmentPreloadedLibraries",
        )
        .map(|values| values.into_iter().map(DynamicLibrary::from_inner).collect())
    }

    /// Sets fragment preloaded dynamic libraries from a borrowed Rust slice.
    pub fn set_fragment_preloaded_libraries_slice(
        &self,
        values: &[DynamicLibrary],
    ) -> Result<(), Error> {
        set_object_array(
            self.as_any_object(),
            sel!(setFragmentPreloadedLibraries:),
            values.iter().map(DynamicLibrary::as_inner),
            "MTL::RenderPipelineDescriptor::setFragmentPreloadedLibraries",
        )
    }

    /// Returns vertex preloaded dynamic libraries as an owned Rust vector.
    pub fn vertex_preloaded_libraries_vec(&self) -> Result<Vec<DynamicLibrary>, Error> {
        get_object_array(
            self.as_any_object(),
            sel!(vertexPreloadedLibraries),
            "MTL::RenderPipelineDescriptor::vertexPreloadedLibraries",
        )
        .map(|values| values.into_iter().map(DynamicLibrary::from_inner).collect())
    }

    /// Sets vertex preloaded dynamic libraries from a borrowed Rust slice.
    pub fn set_vertex_preloaded_libraries_slice(
        &self,
        values: &[DynamicLibrary],
    ) -> Result<(), Error> {
        set_object_array(
            self.as_any_object(),
            sel!(setVertexPreloadedLibraries:),
            values.iter().map(DynamicLibrary::as_inner),
            "MTL::RenderPipelineDescriptor::setVertexPreloadedLibraries",
        )
    }

    /// Returns the fragment linked-functions descriptor when present.
    pub fn fragment_linked_functions_safe(&self) -> Result<Option<LinkedFunctions>, Error> {
        require_selector(
            self.as_any_object(),
            sel!(fragmentLinkedFunctions),
            "MTL::RenderPipelineDescriptor::fragmentLinkedFunctions",
        )?;
        // SAFETY: selector presence is checked and the retained nullable result has the declared class.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_any_object(), fragmentLinkedFunctions] };
        Ok(value.map(LinkedFunctions::from_inner))
    }

    /// Sets or clears the fragment linked-functions descriptor.
    pub fn set_fragment_linked_functions_safe(
        &self,
        value: Option<&LinkedFunctions>,
    ) -> Result<(), Error> {
        require_selector(
            self.as_any_object(),
            sel!(setFragmentLinkedFunctions:),
            "MTL::RenderPipelineDescriptor::setFragmentLinkedFunctions",
        )?;
        // SAFETY: selector presence and the class-preserving wrapper are checked.
        unsafe {
            let _: () = msg_send![self.as_any_object(), setFragmentLinkedFunctions: value.map(LinkedFunctions::as_inner)];
        }
        Ok(())
    }

    /// Returns the vertex linked-functions descriptor when present.
    pub fn vertex_linked_functions_safe(&self) -> Result<Option<LinkedFunctions>, Error> {
        require_selector(
            self.as_any_object(),
            sel!(vertexLinkedFunctions),
            "MTL::RenderPipelineDescriptor::vertexLinkedFunctions",
        )?;
        // SAFETY: selector presence is checked and the retained nullable result has the declared class.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_any_object(), vertexLinkedFunctions] };
        Ok(value.map(LinkedFunctions::from_inner))
    }

    /// Sets or clears the vertex linked-functions descriptor.
    pub fn set_vertex_linked_functions_safe(
        &self,
        value: Option<&LinkedFunctions>,
    ) -> Result<(), Error> {
        require_selector(
            self.as_any_object(),
            sel!(setVertexLinkedFunctions:),
            "MTL::RenderPipelineDescriptor::setVertexLinkedFunctions",
        )?;
        // SAFETY: selector presence and the class-preserving wrapper are checked.
        unsafe {
            let _: () = msg_send![self.as_any_object(), setVertexLinkedFunctions: value.map(LinkedFunctions::as_inner)];
        }
        Ok(())
    }

    /// Returns the color attachment descriptor array.
    pub fn color_attachments_safe(
        &self,
    ) -> Result<RenderPipelineColorAttachmentDescriptorArray, Error> {
        require_selector(
            self.as_any_object(),
            sel!(colorAttachments),
            "MTL::RenderPipelineDescriptor::colorAttachments",
        )?;
        // SAFETY: selector presence is checked and Metal declares a non-null descriptor-array result.
        let value = unsafe { msg_send![self.as_any_object(), colorAttachments] };
        Ok(RenderPipelineColorAttachmentDescriptorArray::from_inner(
            value,
        ))
    }

    /// Returns vertex buffer layout descriptors.
    pub fn vertex_buffers_safe(&self) -> Result<PipelineBufferDescriptorArray, Error> {
        require_selector(
            self.as_any_object(),
            sel!(vertexBuffers),
            "MTL::RenderPipelineDescriptor::vertexBuffers",
        )?;
        // SAFETY: selector presence is checked and Metal declares a non-null descriptor-array result.
        let value = unsafe { msg_send![self.as_any_object(), vertexBuffers] };
        Ok(PipelineBufferDescriptorArray::from_inner(value))
    }

    /// Returns fragment buffer layout descriptors.
    pub fn fragment_buffers_safe(&self) -> Result<PipelineBufferDescriptorArray, Error> {
        require_selector(
            self.as_any_object(),
            sel!(fragmentBuffers),
            "MTL::RenderPipelineDescriptor::fragmentBuffers",
        )?;
        // SAFETY: selector presence is checked and Metal declares a non-null descriptor-array result.
        let value = unsafe { msg_send![self.as_any_object(), fragmentBuffers] };
        Ok(PipelineBufferDescriptorArray::from_inner(value))
    }

    /// Returns the optional vertex layout descriptor.
    pub fn vertex_descriptor_safe(&self) -> Result<Option<VertexDescriptor>, Error> {
        require_selector(
            self.as_any_object(),
            sel!(vertexDescriptor),
            "MTL::RenderPipelineDescriptor::vertexDescriptor",
        )?;
        // SAFETY: selector presence is checked and Metal declares a nullable descriptor result.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_any_object(), vertexDescriptor] };
        Ok(value.map(VertexDescriptor::from_inner))
    }

    /// Sets or clears the vertex layout descriptor.
    pub fn set_vertex_descriptor_safe(
        &self,
        value: Option<&VertexDescriptor>,
    ) -> Result<(), Error> {
        require_selector(
            self.as_any_object(),
            sel!(setVertexDescriptor:),
            "MTL::RenderPipelineDescriptor::setVertexDescriptor",
        )?;
        // SAFETY: selector presence and the class-preserving wrapper are checked.
        unsafe {
            let _: () = msg_send![self.as_any_object(), setVertexDescriptor: value.map(VertexDescriptor::as_inner)];
        }
        Ok(())
    }
}

impl LinkedFunctions {
    /// Returns linked public functions as owned safe wrappers.
    pub fn functions_vec(&self) -> Result<Vec<Function>, Error> {
        get_object_array(
            self.as_inner(),
            sel!(functions),
            "MTL::LinkedFunctions::functions",
        )?
        .into_iter()
        .map(Function::from_any_object)
        .collect()
    }

    /// Sets linked public functions from a borrowed slice.
    pub fn set_functions_slice(&self, values: &[Function]) -> Result<(), Error> {
        set_object_array(
            self.as_inner(),
            sel!(setFunctions:),
            values.iter().map(Function::as_any_object),
            "MTL::LinkedFunctions::setFunctions",
        )
    }

    /// Returns linked internal functions as owned safe wrappers.
    pub fn internal_functions(&self) -> Result<Vec<Function>, Error> {
        get_object_array(
            self.as_inner(),
            sel!(privateFunctions),
            "MTL::LinkedFunctions::privateFunctions",
        )?
        .into_iter()
        .map(Function::from_any_object)
        .collect()
    }

    /// Sets linked internal functions from a borrowed slice.
    pub fn set_internal_functions(&self, values: &[Function]) -> Result<(), Error> {
        set_object_array(
            self.as_inner(),
            sel!(setPrivateFunctions:),
            values.iter().map(Function::as_any_object),
            "MTL::LinkedFunctions::setPrivateFunctions",
        )
    }

    /// Returns linked binary functions as owned safe wrappers.
    pub fn binary_functions_vec(&self) -> Result<Vec<Function>, Error> {
        get_object_array(
            self.as_inner(),
            sel!(binaryFunctions),
            "MTL::LinkedFunctions::binaryFunctions",
        )?
        .into_iter()
        .map(Function::from_any_object)
        .collect()
    }

    /// Sets linked binary functions from a borrowed slice.
    pub fn set_binary_functions_slice(&self, values: &[Function]) -> Result<(), Error> {
        set_object_array(
            self.as_inner(),
            sel!(setBinaryFunctions:),
            values.iter().map(Function::as_any_object),
            "MTL::LinkedFunctions::setBinaryFunctions",
        )
    }

    /// Returns function groups as a Rust string-to-function-vector map.
    pub fn groups_map(&self) -> Result<HashMap<String, Vec<Function>>, Error> {
        require_selector(
            self.as_inner(),
            sel!(groups),
            "MTL::LinkedFunctions::groups",
        )?;
        // SAFETY: selector presence is checked and the property is a nullable NSDictionary.
        let dictionary: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), groups] };
        let Some(dictionary) = dictionary else {
            return Ok(HashMap::new());
        };
        require_selector(&dictionary, sel!(allKeys), "MTL::LinkedFunctions::groups")?;
        // SAFETY: allKeys returns an autoreleased NSArray retained by objc2.
        let keys: Retained<AnyObject> = unsafe { msg_send![&*dictionary, allKeys] };
        let keys = array_objects(Some(keys), "MTL::LinkedFunctions::groups keys")?;
        let mut result = HashMap::with_capacity(keys.len());
        for key in keys {
            // SAFETY: Metal declares group keys as NSString values.
            let key_string: Retained<NSString> = unsafe { Retained::cast_unchecked(key.clone()) };
            // SAFETY: key comes from this dictionary, so objectForKey: accepts it.
            let values: Option<Retained<AnyObject>> =
                unsafe { msg_send![&*dictionary, objectForKey: &*key] };
            let functions = array_objects(values, "MTL::LinkedFunctions::group")?
                .into_iter()
                .map(Function::from_any_object)
                .collect::<Result<Vec<_>, _>>()?;
            result.insert(key_string.to_string(), functions);
        }
        Ok(result)
    }

    /// Sets function groups from a Rust string-to-function-slice map.
    pub fn set_groups_map(&self, values: &HashMap<String, Vec<Function>>) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setGroups:),
            "MTL::LinkedFunctions::setGroups",
        )?;
        let class = AnyClass::get(c"NSMutableDictionary")
            .expect("Foundation provides NSMutableDictionary whenever Metal is loaded");
        // SAFETY: NSMutableDictionary implements new and returns a retained object.
        let dictionary: Retained<AnyObject> = unsafe { msg_send![class, new] };
        for (name, functions) in values {
            if name.is_empty() || name.as_bytes().contains(&0) {
                return Err(Error::invalid_argument(
                    "linked-function group name is invalid",
                ));
            }
            let key = NSString::from_str(name);
            let array = object_array(functions.iter().map(Function::as_any_object));
            // SAFETY: key and array are non-null Objective-C objects and remain live.
            unsafe {
                let _: () = msg_send![&*dictionary, setObject: &*array, forKey: &*key];
            }
        }
        // SAFETY: selector presence is checked and NSDictionary is the declared argument.
        unsafe {
            let _: () = msg_send![self.as_inner(), setGroups: &*dictionary];
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn required_threadgroup_dimensions_must_be_non_zero() {
        assert!(checked_size(Size::new(1, 2, 3), "threads").is_ok());
        assert!(checked_size(Size::new(0, 2, 3), "threads").is_err());
        assert!(checked_size(Size::new(1, 0, 3), "threads").is_err());
        assert!(checked_size(Size::new(1, 2, 0), "threads").is_err());
    }
}
