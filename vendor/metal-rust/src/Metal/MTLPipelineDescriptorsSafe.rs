//! Rust-native collection APIs for Metal pipeline descriptors and reflection.

use crate::Error;
use std::collections::HashMap;

macro_rules! facade_vec_property {
    ($type:ty, $get:ident, $set:ident, $ffi_get:ident, $ffi_set:ident, $item:ty) => {
        impl $type {
            /// Returns the object collection as an owned Rust vector.
            pub fn $get(&self) -> Result<Vec<$item>, Error> {
                self.inner
                    .$ffi_get()
                    .map(|values| values.into_iter().map(<$item>::from_ffi).collect())
                    .map_err(Error::from_ffi)
            }

            /// Replaces the object collection from a borrowed Rust slice.
            pub fn $set(&self, values: &[$item]) -> Result<(), Error> {
                let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
                self.inner.$ffi_set(&values).map_err(Error::from_ffi)
            }
        }
    };
}

facade_vec_property!(
    super::ComputePipelineDescriptor,
    binary_archives_vec,
    set_binary_archives_slice,
    binary_archives_vec,
    set_binary_archives_slice,
    super::BinaryArchive
);
facade_vec_property!(
    super::ComputePipelineDescriptor,
    insert_libraries_vec,
    set_insert_libraries_slice,
    insert_libraries_vec,
    set_insert_libraries_slice,
    super::DynamicLibrary
);
facade_vec_property!(
    super::ComputePipelineDescriptor,
    preloaded_libraries_vec,
    set_preloaded_libraries_slice,
    preloaded_libraries_vec,
    set_preloaded_libraries_slice,
    super::DynamicLibrary
);
facade_vec_property!(
    super::TileRenderPipelineDescriptor,
    binary_archives_vec,
    set_binary_archives_slice,
    binary_archives_vec,
    set_binary_archives_slice,
    super::BinaryArchive
);
facade_vec_property!(
    super::TileRenderPipelineDescriptor,
    preloaded_libraries_vec,
    set_preloaded_libraries_slice,
    preloaded_libraries_vec,
    set_preloaded_libraries_slice,
    super::DynamicLibrary
);
facade_vec_property!(
    super::MeshRenderPipelineDescriptor,
    binary_archives_vec,
    set_binary_archives_slice,
    binary_archives_vec,
    set_binary_archives_slice,
    super::BinaryArchive
);

impl super::ComputePipelineDescriptor {
    /// Resets the descriptor to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }

    /// Returns required compute threadgroup dimensions.
    pub fn required_threads_per_threadgroup_safe(&self) -> Result<super::Size, Error> {
        self.inner
            .required_threads_per_threadgroup_safe()
            .map_err(Error::from_ffi)
    }

    /// Sets validated non-zero compute threadgroup dimensions.
    pub fn set_required_threads_per_threadgroup_safe(
        &self,
        value: super::Size,
    ) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_threadgroup_safe(value)
            .map_err(Error::from_ffi)
    }
}

impl super::TileRenderPipelineDescriptor {
    /// Resets the descriptor to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }

    /// Returns required tile threadgroup dimensions.
    pub fn required_threads_per_threadgroup_safe(&self) -> Result<super::Size, Error> {
        self.inner
            .required_threads_per_threadgroup_safe()
            .map_err(Error::from_ffi)
    }

    /// Sets validated non-zero tile threadgroup dimensions.
    pub fn set_required_threads_per_threadgroup_safe(
        &self,
        value: super::Size,
    ) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_threadgroup_safe(value)
            .map_err(Error::from_ffi)
    }
}

impl super::MeshRenderPipelineDescriptor {
    /// Resets the descriptor to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }

    /// Returns required mesh threadgroup dimensions.
    pub fn required_threads_per_mesh_threadgroup_safe(&self) -> Result<super::Size, Error> {
        self.inner
            .required_threads_per_mesh_threadgroup_safe()
            .map_err(Error::from_ffi)
    }

    /// Sets validated non-zero mesh threadgroup dimensions.
    pub fn set_required_threads_per_mesh_threadgroup_safe(
        &self,
        value: super::Size,
    ) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_mesh_threadgroup_safe(value)
            .map_err(Error::from_ffi)
    }

    /// Returns required object threadgroup dimensions.
    pub fn required_threads_per_object_threadgroup_safe(&self) -> Result<super::Size, Error> {
        self.inner
            .required_threads_per_object_threadgroup_safe()
            .map_err(Error::from_ffi)
    }

    /// Sets validated non-zero object threadgroup dimensions.
    pub fn set_required_threads_per_object_threadgroup_safe(
        &self,
        value: super::Size,
    ) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_object_threadgroup_safe(value)
            .map_err(Error::from_ffi)
    }
}

macro_rules! reflection_vec {
    ($type:ty, $method:ident, $item:ty) => {
        impl $type {
            /// Returns reflection entries as owned safe wrappers.
            pub fn $method(&self) -> Result<Vec<$item>, Error> {
                self.inner
                    .$method()
                    .map(|values| values.into_iter().map(<$item>::from_ffi).collect())
                    .map_err(Error::from_ffi)
            }
        }
    };
}

reflection_vec!(
    super::ComputePipelineReflection,
    arguments_vec,
    super::Argument
);
reflection_vec!(
    super::ComputePipelineReflection,
    bindings_vec,
    super::Binding
);
reflection_vec!(
    super::RenderPipelineReflection,
    fragment_arguments_vec,
    super::Argument
);
reflection_vec!(
    super::RenderPipelineReflection,
    fragment_bindings_vec,
    super::Binding
);
reflection_vec!(
    super::RenderPipelineReflection,
    mesh_bindings_vec,
    super::Binding
);
reflection_vec!(
    super::RenderPipelineReflection,
    object_bindings_vec,
    super::Binding
);
reflection_vec!(
    super::RenderPipelineReflection,
    tile_arguments_vec,
    super::Argument
);
reflection_vec!(
    super::RenderPipelineReflection,
    tile_bindings_vec,
    super::Binding
);
reflection_vec!(
    super::RenderPipelineReflection,
    vertex_arguments_vec,
    super::Argument
);
reflection_vec!(
    super::RenderPipelineReflection,
    vertex_bindings_vec,
    super::Binding
);

macro_rules! function_vec_property {
    ($get:ident, $set:ident) => {
        impl super::RenderPipelineFunctionsDescriptor {
            /// Returns additional functions as an owned Rust vector.
            pub fn $get(&self) -> Result<Vec<super::Function>, Error> {
                self.inner
                    .$get()
                    .map(|values| values.into_iter().map(super::Function::from_ffi).collect())
                    .map_err(Error::from_ffi)
            }

            /// Replaces additional functions from a borrowed Rust slice.
            pub fn $set(&self, values: &[super::Function]) -> Result<(), Error> {
                let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
                self.inner.$set(&values).map_err(Error::from_ffi)
            }
        }
    };
}

function_vec_property!(
    fragment_additional_binary_functions_vec,
    set_fragment_additional_binary_functions_slice
);
function_vec_property!(
    tile_additional_binary_functions_vec,
    set_tile_additional_binary_functions_slice
);
function_vec_property!(
    vertex_additional_binary_functions_vec,
    set_vertex_additional_binary_functions_slice
);

impl super::RenderPipelineColorAttachmentDescriptorArray {
    /// Returns an attachment from one of the eight supported slots.
    pub fn get(
        &self,
        index: usize,
    ) -> Result<super::RenderPipelineColorAttachmentDescriptor, Error> {
        self.inner
            .get(index)
            .map(super::RenderPipelineColorAttachmentDescriptor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Replaces an attachment in one of the eight supported slots.
    pub fn set(
        &self,
        index: usize,
        value: &super::RenderPipelineColorAttachmentDescriptor,
    ) -> Result<(), Error> {
        self.inner.set(index, &value.inner).map_err(Error::from_ffi)
    }
}

impl super::TileRenderPipelineColorAttachmentDescriptorArray {
    /// Returns an attachment from one of the eight supported slots.
    pub fn get(
        &self,
        index: usize,
    ) -> Result<super::TileRenderPipelineColorAttachmentDescriptor, Error> {
        self.inner
            .get(index)
            .map(super::TileRenderPipelineColorAttachmentDescriptor::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Replaces an attachment in one of the eight supported slots.
    pub fn set(
        &self,
        index: usize,
        value: &super::TileRenderPipelineColorAttachmentDescriptor,
    ) -> Result<(), Error> {
        self.inner.set(index, &value.inner).map_err(Error::from_ffi)
    }
}

impl super::LogicalToPhysicalColorAttachmentMap {
    /// Returns the physical index for a logical attachment slot.
    pub fn physical_index(&self, logical_index: usize) -> Result<usize, Error> {
        self.inner
            .physical_index(logical_index)
            .map_err(Error::from_ffi)
    }

    /// Sets a logical-to-physical attachment mapping.
    pub fn set_physical_index(
        &self,
        logical_index: usize,
        physical_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_physical_index(logical_index, physical_index)
            .map_err(Error::from_ffi)
    }

    /// Resets all attachment mappings.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl super::RenderPipelineDescriptor {
    /// Returns configured binary archives as an owned Rust vector.
    pub fn binary_archives_vec(&self) -> Result<Vec<super::BinaryArchive>, Error> {
        self.inner
            .binary_archives_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(super::BinaryArchive::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Sets configured binary archives from a borrowed Rust slice.
    pub fn set_binary_archives_slice(&self, values: &[super::BinaryArchive]) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_binary_archives_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns fragment preloaded dynamic libraries.
    pub fn fragment_preloaded_libraries_vec(&self) -> Result<Vec<super::DynamicLibrary>, Error> {
        self.inner
            .fragment_preloaded_libraries_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(super::DynamicLibrary::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Sets fragment preloaded dynamic libraries.
    pub fn set_fragment_preloaded_libraries_slice(
        &self,
        values: &[super::DynamicLibrary],
    ) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_fragment_preloaded_libraries_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns vertex preloaded dynamic libraries.
    pub fn vertex_preloaded_libraries_vec(&self) -> Result<Vec<super::DynamicLibrary>, Error> {
        self.inner
            .vertex_preloaded_libraries_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(super::DynamicLibrary::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Sets vertex preloaded dynamic libraries.
    pub fn set_vertex_preloaded_libraries_slice(
        &self,
        values: &[super::DynamicLibrary],
    ) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_vertex_preloaded_libraries_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns fragment linked-functions configuration when present.
    pub fn fragment_linked_functions_safe(&self) -> Result<Option<super::LinkedFunctions>, Error> {
        self.inner
            .fragment_linked_functions_safe()
            .map(|value| value.map(super::LinkedFunctions::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears fragment linked-functions configuration.
    pub fn set_fragment_linked_functions_safe(
        &self,
        value: Option<&super::LinkedFunctions>,
    ) -> Result<(), Error> {
        self.inner
            .set_fragment_linked_functions_safe(value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Returns vertex linked-functions configuration when present.
    pub fn vertex_linked_functions_safe(&self) -> Result<Option<super::LinkedFunctions>, Error> {
        self.inner
            .vertex_linked_functions_safe()
            .map(|value| value.map(super::LinkedFunctions::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears vertex linked-functions configuration.
    pub fn set_vertex_linked_functions_safe(
        &self,
        value: Option<&super::LinkedFunctions>,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_linked_functions_safe(value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }

    /// Returns color attachment descriptors.
    pub fn color_attachments_safe(
        &self,
    ) -> Result<super::RenderPipelineColorAttachmentDescriptorArray, Error> {
        self.inner
            .color_attachments_safe()
            .map(super::RenderPipelineColorAttachmentDescriptorArray::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns vertex buffer layout descriptors.
    pub fn vertex_buffers_safe(&self) -> Result<super::PipelineBufferDescriptorArray, Error> {
        self.inner
            .vertex_buffers_safe()
            .map(super::PipelineBufferDescriptorArray::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns fragment buffer layout descriptors.
    pub fn fragment_buffers_safe(&self) -> Result<super::PipelineBufferDescriptorArray, Error> {
        self.inner
            .fragment_buffers_safe()
            .map(super::PipelineBufferDescriptorArray::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Returns the optional vertex layout descriptor.
    pub fn vertex_descriptor_safe(&self) -> Result<Option<super::VertexDescriptor>, Error> {
        self.inner
            .vertex_descriptor_safe()
            .map(|value| value.map(super::VertexDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Sets or clears the vertex layout descriptor.
    pub fn set_vertex_descriptor_safe(
        &self,
        value: Option<&super::VertexDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_vertex_descriptor_safe(value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }
}

impl super::LinkedFunctions {
    /// Returns public linked functions as an owned Rust vector.
    pub fn functions_vec(&self) -> Result<Vec<super::Function>, Error> {
        self.inner
            .functions_vec()
            .map(|values| values.into_iter().map(super::Function::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Sets public linked functions from a borrowed Rust slice.
    pub fn set_functions_slice(&self, values: &[super::Function]) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_functions_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns internal linked functions as an owned Rust vector.
    pub fn internal_functions(&self) -> Result<Vec<super::Function>, Error> {
        self.inner
            .internal_functions()
            .map(|values| values.into_iter().map(super::Function::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Sets internal linked functions from a borrowed Rust slice.
    pub fn set_internal_functions(&self, values: &[super::Function]) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_internal_functions(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns binary linked functions as an owned Rust vector.
    pub fn binary_functions_vec(&self) -> Result<Vec<super::Function>, Error> {
        self.inner
            .binary_functions_vec()
            .map(|values| values.into_iter().map(super::Function::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Sets binary linked functions from a borrowed Rust slice.
    pub fn set_binary_functions_slice(&self, values: &[super::Function]) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_binary_functions_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns named function groups as a Rust map.
    pub fn groups_map(&self) -> Result<HashMap<String, Vec<super::Function>>, Error> {
        self.inner
            .groups_map()
            .map(|groups| {
                groups
                    .into_iter()
                    .map(|(name, values)| {
                        (
                            name,
                            values.into_iter().map(super::Function::from_ffi).collect(),
                        )
                    })
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Sets named function groups from a Rust map.
    pub fn set_groups_map(
        &self,
        values: &HashMap<String, Vec<super::Function>>,
    ) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|(name, functions)| {
                (
                    name.clone(),
                    functions.iter().map(|value| value.inner.clone()).collect(),
                )
            })
            .collect();
        self.inner.set_groups_map(&values).map_err(Error::from_ffi)
    }
}
