//! Safe public Metal 4 compiler factories and completion handlers.

use crate::Error;
use crate::metal::{
    BinaryArchive, ComputePipelineState, DynamicLibrary, Library, RenderPipelineState,
    SamplePosition, Size, Texture,
};
use crate::metal4::{
    BinaryFunction, BinaryFunctionDescriptor, CommandAllocator, Compiler, CompilerTask,
    CompilerTaskOptions, ComputePipelineDescriptor, FunctionDescriptor, LibraryDescriptor,
    LibraryFunctionDescriptor, MachineLearningPipelineDescriptor, MachineLearningPipelineState,
    MeshRenderPipelineDescriptor, PipelineDataSetSerializer, PipelineStageDynamicLinkingDescriptor,
    RenderPassDescriptor, RenderPipelineBinaryFunctionsDescriptor,
    RenderPipelineColorAttachmentDescriptor, RenderPipelineColorAttachmentDescriptorArray,
    RenderPipelineDescriptor, RenderPipelineDynamicLinkingDescriptor, StaticLinkingDescriptor,
    StitchedFunctionDescriptor, TileRenderPipelineDescriptor,
};
use std::collections::HashMap;
use std::path::Path;

/// A borrowed descriptor for any render, mesh, or tile pipeline build.
#[derive(Clone, Copy)]
pub enum RenderPipelineBuildDescriptor<'a> {
    /// A conventional vertex/fragment render pipeline.
    Render(&'a RenderPipelineDescriptor),
    /// A mesh/object render pipeline.
    Mesh(&'a MeshRenderPipelineDescriptor),
    /// A tile render pipeline.
    Tile(&'a TileRenderPipelineDescriptor),
}

impl<'a> RenderPipelineBuildDescriptor<'a> {
    fn to_ffi(self) -> metal_rust_ffi::RenderPipelineBuildDescriptor<'a> {
        match self {
            Self::Render(value) => {
                metal_rust_ffi::RenderPipelineBuildDescriptor::Render(&value.inner)
            }
            Self::Mesh(value) => metal_rust_ffi::RenderPipelineBuildDescriptor::Mesh(&value.inner),
            Self::Tile(value) => metal_rust_ffi::RenderPipelineBuildDescriptor::Tile(&value.inner),
        }
    }
}

impl Compiler {
    /// Creates a Metal library synchronously.
    pub fn new_library(&self, descriptor: &LibraryDescriptor) -> Result<Library, Error> {
        self.inner
            .new_library(&descriptor.inner)
            .map(Library::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a compute pipeline with optional dynamic-linking and task options.
    pub fn new_compute_pipeline(
        &self,
        descriptor: &ComputePipelineDescriptor,
        linking: Option<&PipelineStageDynamicLinkingDescriptor>,
        options: Option<&CompilerTaskOptions>,
    ) -> Result<ComputePipelineState, Error> {
        self.inner
            .new_compute_pipeline(
                &descriptor.inner,
                linking.map(|value| &value.inner),
                options.map(|value| &value.inner),
            )
            .map(|inner| ComputePipelineState { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates a render pipeline with optional dynamic-linking and task options.
    pub fn new_render_pipeline(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        linking: Option<&RenderPipelineDynamicLinkingDescriptor>,
        options: Option<&CompilerTaskOptions>,
    ) -> Result<RenderPipelineState, Error> {
        self.inner
            .new_render_pipeline(
                descriptor.to_ffi(),
                linking.map(|value| &value.inner),
                options.map(|value| &value.inner),
            )
            .map(|inner| RenderPipelineState { inner })
            .map_err(Error::from_ffi)
    }

    /// Specializes a previously unspecialized render pipeline.
    pub fn specialize_render_pipeline(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        pipeline: &RenderPipelineState,
    ) -> Result<RenderPipelineState, Error> {
        self.inner
            .specialize_render_pipeline(descriptor.to_ffi(), &pipeline.inner)
            .map(|inner| RenderPipelineState { inner })
            .map_err(Error::from_ffi)
    }

    /// Creates a binary shader function synchronously.
    pub fn new_binary_function(
        &self,
        descriptor: &BinaryFunctionDescriptor,
        options: Option<&CompilerTaskOptions>,
    ) -> Result<BinaryFunction, Error> {
        self.inner
            .new_binary_function(&descriptor.inner, options.map(|value| &value.inner))
            .map(BinaryFunction::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a machine-learning pipeline synchronously.
    pub fn new_machine_learning_pipeline(
        &self,
        descriptor: &MachineLearningPipelineDescriptor,
    ) -> Result<MachineLearningPipelineState, Error> {
        self.inner
            .new_machine_learning_pipeline(&descriptor.inner)
            .map(MachineLearningPipelineState::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a dynamic library from a compiled Metal library.
    pub fn new_dynamic_library(&self, library: &Library) -> Result<DynamicLibrary, Error> {
        self.inner
            .new_dynamic_library(&library.inner)
            .map(DynamicLibrary::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates a dynamic library from a file path.
    pub fn new_dynamic_library_from_path(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<DynamicLibrary, Error> {
        self.inner
            .new_dynamic_library_from_path(path)
            .map(DynamicLibrary::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous Metal library compilation.
    pub fn new_library_async(
        &self,
        descriptor: &LibraryDescriptor,
        handler: impl FnOnce(Result<Library, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_library_async(&descriptor.inner, move |result| {
                handler(result.map(Library::from_ffi).map_err(Error::from_ffi));
            })
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous compute-pipeline compilation.
    pub fn new_compute_pipeline_async(
        &self,
        descriptor: &ComputePipelineDescriptor,
        options: Option<&CompilerTaskOptions>,
        handler: impl FnOnce(Result<ComputePipelineState, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_compute_pipeline_async(
                &descriptor.inner,
                options.map(|value| &value.inner),
                move |result| {
                    handler(
                        result
                            .map(|inner| ComputePipelineState { inner })
                            .map_err(Error::from_ffi),
                    );
                },
            )
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous compute compilation with dynamic linking.
    pub fn new_compute_pipeline_linked_async(
        &self,
        descriptor: &ComputePipelineDescriptor,
        linking: Option<&PipelineStageDynamicLinkingDescriptor>,
        options: Option<&CompilerTaskOptions>,
        handler: impl FnOnce(Result<ComputePipelineState, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_compute_pipeline_linked_async(
                &descriptor.inner,
                linking.map(|value| &value.inner),
                options.map(|value| &value.inner),
                move |result| {
                    handler(
                        result
                            .map(|inner| ComputePipelineState { inner })
                            .map_err(Error::from_ffi),
                    );
                },
            )
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous render-pipeline compilation.
    pub fn new_render_pipeline_async(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        options: Option<&CompilerTaskOptions>,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_render_pipeline_async(
                descriptor.to_ffi(),
                options.map(|value| &value.inner),
                move |result| {
                    handler(
                        result
                            .map(|inner| RenderPipelineState { inner })
                            .map_err(Error::from_ffi),
                    );
                },
            )
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous render compilation with dynamic linking.
    pub fn new_render_pipeline_linked_async(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        linking: Option<&RenderPipelineDynamicLinkingDescriptor>,
        options: Option<&CompilerTaskOptions>,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_render_pipeline_linked_async(
                descriptor.to_ffi(),
                linking.map(|value| &value.inner),
                options.map(|value| &value.inner),
                move |result| {
                    handler(
                        result
                            .map(|inner| RenderPipelineState { inner })
                            .map_err(Error::from_ffi),
                    );
                },
            )
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous specialization of a render pipeline.
    pub fn specialize_render_pipeline_async(
        &self,
        descriptor: RenderPipelineBuildDescriptor<'_>,
        pipeline: &RenderPipelineState,
        handler: impl FnOnce(Result<RenderPipelineState, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .specialize_render_pipeline_async(descriptor.to_ffi(), &pipeline.inner, move |result| {
                handler(
                    result
                        .map(|inner| RenderPipelineState { inner })
                        .map_err(Error::from_ffi),
                );
            })
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous binary-function compilation.
    pub fn new_binary_function_async(
        &self,
        descriptor: &BinaryFunctionDescriptor,
        options: Option<&CompilerTaskOptions>,
        handler: impl FnOnce(Result<BinaryFunction, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_binary_function_async(
                &descriptor.inner,
                options.map(|value| &value.inner),
                move |result| {
                    handler(
                        result
                            .map(BinaryFunction::from_ffi)
                            .map_err(Error::from_ffi),
                    );
                },
            )
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous machine-learning pipeline compilation.
    pub fn new_machine_learning_pipeline_async(
        &self,
        descriptor: &MachineLearningPipelineDescriptor,
        handler: impl FnOnce(Result<MachineLearningPipelineState, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_machine_learning_pipeline_async(&descriptor.inner, move |result| {
                handler(
                    result
                        .map(MachineLearningPipelineState::from_ffi)
                        .map_err(Error::from_ffi),
                );
            })
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous dynamic-library compilation from a library.
    pub fn new_dynamic_library_async(
        &self,
        library: &Library,
        handler: impl FnOnce(Result<DynamicLibrary, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_dynamic_library_async(&library.inner, move |result| {
                handler(
                    result
                        .map(DynamicLibrary::from_ffi)
                        .map_err(Error::from_ffi),
                );
            })
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Starts one-shot asynchronous dynamic-library compilation from a file path.
    pub fn new_dynamic_library_from_path_async(
        &self,
        path: impl AsRef<Path>,
        handler: impl FnOnce(Result<DynamicLibrary, Error>) + Send + 'static,
    ) -> Result<CompilerTask, Error> {
        self.inner
            .new_dynamic_library_from_path_async(path, move |result| {
                handler(
                    result
                        .map(DynamicLibrary::from_ffi)
                        .map_err(Error::from_ffi),
                );
            })
            .map(CompilerTask::from_ffi)
            .map_err(Error::from_ffi)
    }
}

impl CompilerTaskOptions {
    /// Returns binary archives searched by this compilation task.
    pub fn lookup_archives_vec(&self) -> Result<Vec<BinaryArchive>, Error> {
        self.inner
            .lookup_archives_vec()
            .map(|values| values.into_iter().map(BinaryArchive::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Replaces binary archives searched by this compilation task.
    pub fn set_lookup_archives_slice(&self, values: &[BinaryArchive]) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_lookup_archives_slice(&values)
            .map_err(Error::from_ffi)
    }
}

impl StaticLinkingDescriptor {
    /// Returns public statically linked functions as owned safe wrappers.
    pub fn function_descriptors_vec(&self) -> Result<Vec<FunctionDescriptor>, Error> {
        self.inner
            .function_descriptors_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionDescriptor::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces public statically linked functions from a borrowed slice.
    pub fn set_function_descriptors_slice(
        &self,
        values: &[FunctionDescriptor],
    ) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_function_descriptors_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns private statically linked functions as owned safe wrappers.
    pub fn internal_functions(&self) -> Result<Vec<FunctionDescriptor>, Error> {
        self.inner
            .internal_functions()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionDescriptor::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces private statically linked functions from a borrowed slice.
    pub fn set_internal_functions(&self, values: &[FunctionDescriptor]) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_internal_functions(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns named static-link groups as Rust-native collections.
    pub fn groups_map(&self) -> Result<HashMap<String, Vec<FunctionDescriptor>>, Error> {
        self.inner
            .groups_map()
            .map(|groups| {
                groups
                    .into_iter()
                    .map(|(name, values)| {
                        (
                            name,
                            values
                                .into_iter()
                                .map(FunctionDescriptor::from_ffi)
                                .collect(),
                        )
                    })
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces named static-link groups from Rust-native collections.
    pub fn set_groups_map(
        &self,
        groups: &HashMap<String, Vec<FunctionDescriptor>>,
    ) -> Result<(), Error> {
        let groups = groups
            .iter()
            .map(|(name, values)| {
                (
                    name.clone(),
                    values.iter().map(|value| value.inner.clone()).collect(),
                )
            })
            .collect();
        self.inner.set_groups_map(&groups).map_err(Error::from_ffi)
    }
}

impl PipelineStageDynamicLinkingDescriptor {
    /// Returns binary functions selected for dynamic linking.
    pub fn binary_linked_functions_vec(&self) -> Result<Vec<BinaryFunction>, Error> {
        self.inner
            .binary_linked_functions_vec()
            .map(|values| values.into_iter().map(BinaryFunction::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Replaces binary functions selected for dynamic linking.
    pub fn set_binary_linked_functions_slice(
        &self,
        values: &[BinaryFunction],
    ) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_binary_linked_functions_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns dynamic libraries preloaded during pipeline linking.
    pub fn preloaded_libraries_vec(&self) -> Result<Vec<DynamicLibrary>, Error> {
        self.inner
            .preloaded_libraries_vec()
            .map(|values| values.into_iter().map(DynamicLibrary::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Replaces dynamic libraries preloaded during pipeline linking.
    pub fn set_preloaded_libraries_slice(&self, values: &[DynamicLibrary]) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_preloaded_libraries_slice(&values)
            .map_err(Error::from_ffi)
    }
}

macro_rules! facade_binary_function_list {
    ($getter:ident, $setter:ident) => {
        /// Returns this pipeline-stage binary-function list.
        pub fn $getter(&self) -> Result<Vec<BinaryFunction>, Error> {
            self.inner
                .$getter()
                .map(|values| values.into_iter().map(BinaryFunction::from_ffi).collect())
                .map_err(Error::from_ffi)
        }

        /// Replaces this pipeline-stage binary-function list.
        pub fn $setter(&self, values: &[BinaryFunction]) -> Result<(), Error> {
            let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
            self.inner.$setter(&values).map_err(Error::from_ffi)
        }
    };
}

impl RenderPipelineBinaryFunctionsDescriptor {
    facade_binary_function_list!(
        fragment_additional_binary_functions_vec,
        set_fragment_additional_binary_functions_slice
    );
    facade_binary_function_list!(
        mesh_additional_binary_functions_vec,
        set_mesh_additional_binary_functions_slice
    );
    facade_binary_function_list!(
        object_additional_binary_functions_vec,
        set_object_additional_binary_functions_slice
    );
    facade_binary_function_list!(
        tile_additional_binary_functions_vec,
        set_tile_additional_binary_functions_slice
    );
    facade_binary_function_list!(
        vertex_additional_binary_functions_vec,
        set_vertex_additional_binary_functions_slice
    );

    /// Restores every binary-function list to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl RenderPipelineColorAttachmentDescriptor {
    /// Restores the attachment to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl RenderPipelineColorAttachmentDescriptorArray {
    /// Returns the optional attachment in one of Metal's eight color slots.
    pub fn attachment(
        &self,
        index: usize,
    ) -> Result<Option<RenderPipelineColorAttachmentDescriptor>, Error> {
        self.inner
            .attachment(index)
            .map(|value| value.map(RenderPipelineColorAttachmentDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Replaces the attachment in one of Metal's eight color slots.
    pub fn set_attachment(
        &self,
        index: usize,
        value: &RenderPipelineColorAttachmentDescriptor,
    ) -> Result<(), Error> {
        self.inner
            .set_attachment(index, &value.inner)
            .map_err(Error::from_ffi)
    }

    /// Restores the attachment array to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl RenderPipelineDescriptor {
    /// Restores all render-pipeline properties to Metal defaults.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

macro_rules! facade_required_threads_descriptor {
    ($type:ty) => {
        impl $type {
            /// Returns the required threads-per-threadgroup size.
            pub fn required_threads_per_threadgroup_safe(&self) -> Result<Size, Error> {
                self.inner
                    .required_threads_per_threadgroup_safe()
                    .map_err(Error::from_ffi)
            }

            /// Sets or disables the required threads-per-threadgroup size.
            pub fn set_required_threads_per_threadgroup_safe(
                &self,
                size: Size,
            ) -> Result<(), Error> {
                self.inner
                    .set_required_threads_per_threadgroup_safe(size)
                    .map_err(Error::from_ffi)
            }

            /// Restores this pipeline descriptor to Metal defaults.
            pub fn reset_pipeline_descriptor(&self) -> Result<(), Error> {
                self.inner
                    .reset_pipeline_descriptor()
                    .map_err(Error::from_ffi)
            }
        }
    };
}

facade_required_threads_descriptor!(ComputePipelineDescriptor);
facade_required_threads_descriptor!(TileRenderPipelineDescriptor);

impl StitchedFunctionDescriptor {
    /// Returns the stitched function's component descriptors.
    pub fn function_descriptors_vec(&self) -> Result<Vec<FunctionDescriptor>, Error> {
        self.inner
            .function_descriptors_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionDescriptor::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces the stitched function's component descriptors.
    pub fn set_function_descriptors_slice(
        &self,
        values: &[FunctionDescriptor],
    ) -> Result<(), Error> {
        let values: Vec<_> = values.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .set_function_descriptors_slice(&values)
            .map_err(Error::from_ffi)
    }
}

impl RenderPassDescriptor {
    /// Sets a checked color attachment with a clear load action and stored result.
    pub fn set_color_attachment(
        &self,
        index: usize,
        texture: &Texture,
        clear_color: crate::metal::ClearColor,
    ) -> Result<(), Error> {
        self.inner
            .set_color_attachment(index, &texture.inner, clear_color)
            .map_err(Error::from_ffi)
    }

    /// Returns up to `count` programmable sample positions.
    pub fn sample_positions_vec(&self, count: usize) -> Result<Vec<SamplePosition>, Error> {
        self.inner
            .sample_positions_vec(count)
            .map_err(Error::from_ffi)
    }

    /// Replaces programmable sample positions from a borrowed slice.
    pub fn set_sample_positions_slice(&self, values: &[SamplePosition]) -> Result<(), Error> {
        self.inner
            .set_sample_positions_slice(values)
            .map_err(Error::from_ffi)
    }
}

impl LibraryFunctionDescriptor {
    /// Exposes the retained base function descriptor required by render pipelines.
    pub fn as_function_descriptor(&self) -> FunctionDescriptor {
        FunctionDescriptor::from_ffi(self.inner.as_function_descriptor())
    }
}

impl PipelineDataSetSerializer {
    /// Serializes captured binaries into an archive at a file path.
    pub fn serialize_archive_to_path(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.inner
            .serialize_archive_to_path(path)
            .map_err(Error::from_ffi)
    }

    /// Serializes captured descriptors into owned pipeline-script bytes.
    pub fn serialize_pipelines_script(&self) -> Result<Vec<u8>, Error> {
        self.inner
            .serialize_pipelines_script()
            .map_err(Error::from_ffi)
    }
}

impl CommandAllocator {
    /// Returns the allocator's currently allocated byte count.
    pub fn allocated_size_safe(&self) -> Result<u64, Error> {
        self.inner.allocated_size_safe().map_err(Error::from_ffi)
    }

    /// Reclaims allocator storage no longer referenced by GPU work.
    pub fn reset_safe(&self) -> Result<(), Error> {
        self.inner.reset_safe().map_err(Error::from_ffi)
    }
}

impl CompilerTask {
    /// Blocks the calling thread until this compilation task finishes.
    pub fn wait_until_completed_safe(&self) -> Result<(), Error> {
        self.inner
            .wait_until_completed_safe()
            .map_err(Error::from_ffi)
    }
}
