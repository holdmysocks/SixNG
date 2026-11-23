//! Safe binary-archive, function-stitching, and function-table APIs.

use super::{
    Argument, ArgumentEncoder, Attribute, BinaryArchive, BinaryArchiveDescriptor, Binding, Buffer,
    ComputePipelineDescriptor, Counter, CounterSampleBuffer, CounterSet, DepthStencilDescriptor,
    DepthStencilState, DynamicLibrary, Function, FunctionDescriptor, FunctionHandle,
    FunctionLogDebugLocation, FunctionReflection, FunctionStitchingAttribute,
    FunctionStitchingAttributeAlwaysInline, FunctionStitchingFunctionNode, FunctionStitchingGraph,
    FunctionStitchingInputNode, FunctionStitchingNode, IntersectionFunctionSignature,
    IntersectionFunctionTable, Library, MeshRenderPipelineDescriptor, PipelineBufferDescriptor,
    PipelineBufferDescriptorArray, RenderPipelineDescriptor, ResourceID, SamplerAddressMode,
    SamplerDescriptor, SamplerState, StitchedLibraryDescriptor, TileRenderPipelineDescriptor,
    VertexAttribute, VisibleFunctionTable,
};
use crate::Error;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// A borrowed function-stitching node accepted by slice-based APIs.
#[derive(Clone, Copy)]
pub enum StitchingNodeRef<'a> {
    /// An input node.
    Input(&'a FunctionStitchingInputNode),
    /// A function-call node.
    Function(&'a FunctionStitchingFunctionNode),
    /// An opaque node returned by Metal.
    Node(&'a FunctionStitchingNode),
}

/// A borrowed function-stitching attribute accepted by slice-based APIs.
#[derive(Clone, Copy)]
pub enum StitchingAttributeRef<'a> {
    /// An always-inline attribute.
    AlwaysInline(&'a FunctionStitchingAttributeAlwaysInline),
    /// An opaque attribute returned by Metal.
    Attribute(&'a FunctionStitchingAttribute),
}

impl BinaryArchiveDescriptor {
    /// Sets or clears the archive source using a Rust file path.
    pub fn set_file_path(&self, path: Option<&Path>) -> Result<(), Error> {
        self.inner.set_file_path(path).map_err(Error::from_ffi)
    }

    /// Returns the optional archive source as an owned Rust path.
    pub fn file_path(&self) -> Result<Option<PathBuf>, Error> {
        self.inner.file_path().map_err(Error::from_ffi)
    }
}

impl BinaryArchive {
    /// Adds functions selected by a compute-pipeline descriptor.
    pub fn add_compute_pipeline_functions(
        &self,
        descriptor: &ComputePipelineDescriptor,
    ) -> Result<(), Error> {
        self.inner
            .add_compute_pipeline_functions(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Adds one function selected from a library.
    pub fn add_function(
        &self,
        descriptor: &FunctionDescriptor,
        library: &Library,
    ) -> Result<(), Error> {
        self.inner
            .add_function(&descriptor.inner, &library.inner)
            .map_err(Error::from_ffi)
    }

    /// Adds functions selected by a stitched-library descriptor.
    pub fn add_stitched_library(
        &self,
        descriptor: &StitchedLibraryDescriptor,
    ) -> Result<(), Error> {
        self.inner
            .add_stitched_library(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Adds functions selected by a mesh render-pipeline descriptor.
    pub fn add_mesh_render_pipeline_functions(
        &self,
        descriptor: &MeshRenderPipelineDescriptor,
    ) -> Result<(), Error> {
        self.inner
            .add_mesh_render_pipeline_functions(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Adds functions selected by a render-pipeline descriptor.
    pub fn add_render_pipeline_functions(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Result<(), Error> {
        self.inner
            .add_render_pipeline_functions(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Adds functions selected by a tile render-pipeline descriptor.
    pub fn add_tile_render_pipeline_functions(
        &self,
        descriptor: &TileRenderPipelineDescriptor,
    ) -> Result<(), Error> {
        self.inner
            .add_tile_render_pipeline_functions(&descriptor.inner)
            .map_err(Error::from_ffi)
    }

    /// Serializes this archive to a Rust file path.
    pub fn serialize_to_file(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.inner
            .serialize_to_file(path.as_ref())
            .map_err(Error::from_ffi)
    }

    /// Sets or clears the diagnostic label.
    pub fn set_optional_label(&self, label: Option<&str>) -> Result<(), Error> {
        self.inner
            .set_optional_label(label)
            .map_err(Error::from_ffi)
    }
}

impl FunctionStitchingInputNode {
    /// Creates an input node for one argument index.
    pub fn with_argument_index(argument_index: usize) -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::FunctionStitchingInputNode::with_argument_index(
            argument_index,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }
}

fn ffi_nodes<'a>(values: &[StitchingNodeRef<'a>]) -> Vec<metal_rust_ffi::StitchingNodeRef<'a>> {
    values
        .iter()
        .map(|value| match value {
            StitchingNodeRef::Input(value) => metal_rust_ffi::StitchingNodeRef::Input(&value.inner),
            StitchingNodeRef::Function(value) => {
                metal_rust_ffi::StitchingNodeRef::Function(&value.inner)
            }
            StitchingNodeRef::Node(value) => metal_rust_ffi::StitchingNodeRef::Node(&value.inner),
        })
        .collect()
}

fn ffi_attributes<'a>(
    values: &[StitchingAttributeRef<'a>],
) -> Vec<metal_rust_ffi::StitchingAttributeRef<'a>> {
    values
        .iter()
        .map(|value| match value {
            StitchingAttributeRef::AlwaysInline(value) => {
                metal_rust_ffi::StitchingAttributeRef::AlwaysInline(&value.inner)
            }
            StitchingAttributeRef::Attribute(value) => {
                metal_rust_ffi::StitchingAttributeRef::Attribute(&value.inner)
            }
        })
        .collect()
}

impl FunctionStitchingFunctionNode {
    /// Creates a fully initialized function-call node from Rust slices.
    pub fn with_details(
        name: &str,
        arguments: &[StitchingNodeRef<'_>],
        control_dependencies: &[FunctionStitchingFunctionNode],
    ) -> Result<Self, Error> {
        let arguments = ffi_nodes(arguments);
        let dependencies = control_dependencies
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        metal_rust_ffi::__private::objects::metal::FunctionStitchingFunctionNode::with_details(
            name,
            &arguments,
            &dependencies,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Returns argument nodes as owned safe wrappers.
    pub fn arguments_vec(&self) -> Result<Vec<FunctionStitchingNode>, Error> {
        self.inner
            .arguments_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionStitchingNode::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces argument nodes from a safe Rust slice.
    pub fn set_arguments_slice(&self, values: &[StitchingNodeRef<'_>]) -> Result<(), Error> {
        self.inner
            .set_arguments_slice(&ffi_nodes(values))
            .map_err(Error::from_ffi)
    }

    /// Returns control dependencies as owned safe wrappers.
    pub fn control_dependencies_vec(&self) -> Result<Vec<FunctionStitchingFunctionNode>, Error> {
        self.inner
            .control_dependencies_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionStitchingFunctionNode::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces control dependencies from a safe Rust slice.
    pub fn set_control_dependencies_slice(
        &self,
        values: &[FunctionStitchingFunctionNode],
    ) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        self.inner
            .set_control_dependencies_slice(&values)
            .map_err(Error::from_ffi)
    }
}

impl FunctionStitchingGraph {
    /// Creates a fully initialized function graph from Rust slices.
    pub fn with_details(
        function_name: &str,
        nodes: &[FunctionStitchingFunctionNode],
        output_node: &FunctionStitchingFunctionNode,
        attributes: &[StitchingAttributeRef<'_>],
    ) -> Result<Self, Error> {
        let nodes = nodes
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        let attributes = ffi_attributes(attributes);
        metal_rust_ffi::__private::objects::metal::FunctionStitchingGraph::with_details(
            function_name,
            &nodes,
            &output_node.inner,
            &attributes,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Returns graph nodes as owned safe wrappers.
    pub fn nodes_vec(&self) -> Result<Vec<FunctionStitchingFunctionNode>, Error> {
        self.inner
            .nodes_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionStitchingFunctionNode::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces graph nodes from a safe Rust slice.
    pub fn set_nodes_slice(&self, values: &[FunctionStitchingFunctionNode]) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        self.inner.set_nodes_slice(&values).map_err(Error::from_ffi)
    }

    /// Returns graph attributes as owned safe wrappers.
    pub fn attributes_vec(&self) -> Result<Vec<FunctionStitchingAttribute>, Error> {
        self.inner
            .attributes_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionStitchingAttribute::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces graph attributes from a safe Rust slice.
    pub fn set_attributes_slice(&self, values: &[StitchingAttributeRef<'_>]) -> Result<(), Error> {
        self.inner
            .set_attributes_slice(&ffi_attributes(values))
            .map_err(Error::from_ffi)
    }
}

impl StitchedLibraryDescriptor {
    /// Returns function graphs as owned safe wrappers.
    pub fn function_graphs_vec(&self) -> Result<Vec<FunctionStitchingGraph>, Error> {
        self.inner
            .function_graphs_vec()
            .map(|values| {
                values
                    .into_iter()
                    .map(FunctionStitchingGraph::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }

    /// Replaces function graphs from a safe Rust slice.
    pub fn set_function_graphs_slice(
        &self,
        values: &[FunctionStitchingGraph],
    ) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        self.inner
            .set_function_graphs_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns source functions as owned safe wrappers.
    pub fn functions_vec(&self) -> Result<Vec<Function>, Error> {
        self.inner
            .functions_vec()
            .map(|values| values.into_iter().map(Function::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Replaces source functions from a safe Rust slice.
    pub fn set_functions_slice(&self, values: &[Function]) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        self.inner
            .set_functions_slice(&values)
            .map_err(Error::from_ffi)
    }

    /// Returns binary archives as owned safe wrappers.
    pub fn binary_archives_vec(&self) -> Result<Vec<BinaryArchive>, Error> {
        self.inner
            .binary_archives_vec()
            .map(|values| values.into_iter().map(BinaryArchive::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Replaces binary archives from a safe Rust slice.
    pub fn set_binary_archives_slice(&self, values: &[BinaryArchive]) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        self.inner
            .set_binary_archives_slice(&values)
            .map_err(Error::from_ffi)
    }
}

impl VisibleFunctionTable {
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        self.inner.gpu_resource_id().map_err(Error::from_ffi)
    }

    /// Binds or clears one function-table slot.
    pub fn set_function(
        &self,
        function: Option<&FunctionHandle>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_function(function.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safe slice substitute for Metal's function-handle pointer array.
    pub fn set_functions(
        &self,
        functions: &[Option<&FunctionHandle>],
        start_index: usize,
    ) -> Result<(), Error> {
        let functions = functions
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_functions(&functions, start_index)
            .map_err(Error::from_ffi)
    }
}

impl IntersectionFunctionTable {
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        self.inner.gpu_resource_id().map_err(Error::from_ffi)
    }

    /// Binds or clears a checked buffer region.
    pub fn set_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_buffer(buffer.map(|value| &value.inner), offset, index)
            .map_err(Error::from_ffi)
    }

    /// Safe slice substitute for Metal's parallel buffer/offset arrays.
    pub fn set_buffers(
        &self,
        bindings: &[(Option<&Buffer>, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let bindings = bindings
            .iter()
            .map(|(buffer, offset)| (buffer.map(|value| &value.inner), *offset))
            .collect::<Vec<_>>();
        self.inner
            .set_buffers(&bindings, start_index)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears one function-table slot.
    pub fn set_function(
        &self,
        function: Option<&FunctionHandle>,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_function(function.map(|value| &value.inner), index)
            .map_err(Error::from_ffi)
    }

    /// Safe slice substitute for Metal's function-handle pointer array.
    pub fn set_functions(
        &self,
        functions: &[Option<&FunctionHandle>],
        start_index: usize,
    ) -> Result<(), Error> {
        let functions = functions
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_functions(&functions, start_index)
            .map_err(Error::from_ffi)
    }

    /// Installs an opaque triangle function at one slot.
    pub fn set_opaque_triangle_function(
        &self,
        signature: IntersectionFunctionSignature,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_opaque_triangle_function(signature, index)
            .map_err(Error::from_ffi)
    }

    /// Installs an opaque triangle function across a checked Rust range.
    pub fn set_opaque_triangle_function_range(
        &self,
        signature: IntersectionFunctionSignature,
        range: Range<usize>,
    ) -> Result<(), Error> {
        self.inner
            .set_opaque_triangle_function_range(signature, range)
            .map_err(Error::from_ffi)
    }

    /// Installs an opaque curve function at one slot.
    pub fn set_opaque_curve_function(
        &self,
        signature: IntersectionFunctionSignature,
        index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_opaque_curve_function(signature, index)
            .map_err(Error::from_ffi)
    }

    /// Installs an opaque curve function across a checked Rust range.
    pub fn set_opaque_curve_function_range(
        &self,
        signature: IntersectionFunctionSignature,
        range: Range<usize>,
    ) -> Result<(), Error> {
        self.inner
            .set_opaque_curve_function_range(signature, range)
            .map_err(Error::from_ffi)
    }

    /// Binds or clears one visible-function table at a buffer index.
    pub fn set_visible_function_table(
        &self,
        table: Option<&VisibleFunctionTable>,
        buffer_index: usize,
    ) -> Result<(), Error> {
        self.inner
            .set_visible_function_table(table.map(|value| &value.inner), buffer_index)
            .map_err(Error::from_ffi)
    }

    /// Safe slice substitute for Metal's visible-function-table pointer array.
    pub fn set_visible_function_tables(
        &self,
        tables: &[Option<&VisibleFunctionTable>],
        start_buffer_index: usize,
    ) -> Result<(), Error> {
        let tables = tables
            .iter()
            .map(|value| value.map(|value| &value.inner))
            .collect::<Vec<_>>();
        self.inner
            .set_visible_function_tables(&tables, start_buffer_index)
            .map_err(Error::from_ffi)
    }
}

impl FunctionDescriptor {
    /// Returns associated binary archives as owned safe wrappers.
    pub fn binary_archives_vec(&self) -> Result<Vec<BinaryArchive>, Error> {
        self.inner
            .binary_archives_vec()
            .map(|values| values.into_iter().map(BinaryArchive::from_ffi).collect())
            .map_err(Error::from_ffi)
    }

    /// Replaces associated binary archives from a safe Rust slice.
    pub fn set_binary_archives_slice(&self, values: &[BinaryArchive]) -> Result<(), Error> {
        let values = values
            .iter()
            .map(|value| value.inner.clone())
            .collect::<Vec<_>>();
        self.inner
            .set_binary_archives_slice(&values)
            .map_err(Error::from_ffi)
    }
}

impl PipelineBufferDescriptorArray {
    /// Returns one optional pipeline-buffer descriptor at a checked index.
    pub fn buffer(&self, index: usize) -> Result<Option<PipelineBufferDescriptor>, Error> {
        self.inner
            .buffer(index)
            .map(|value| value.map(PipelineBufferDescriptor::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Replaces or clears one pipeline-buffer descriptor at a checked index.
    pub fn set_buffer(
        &self,
        index: usize,
        value: Option<&PipelineBufferDescriptor>,
    ) -> Result<(), Error> {
        self.inner
            .set_buffer(index, value.map(|value| &value.inner))
            .map_err(Error::from_ffi)
    }
}

impl SamplerDescriptor {
    /// Returns the R-coordinate address mode after availability checks.
    pub fn r_address_mode(&self) -> Result<SamplerAddressMode, Error> {
        self.inner.r_address_mode().map_err(Error::from_ffi)
    }

    /// Sets a checked R-coordinate address mode.
    pub fn set_r_address_mode(&self, value: SamplerAddressMode) -> Result<(), Error> {
        self.inner
            .set_r_address_mode(value)
            .map_err(Error::from_ffi)
    }
}

impl DepthStencilDescriptor {
    /// Safe canonical alias for Metal's deprecated depth-write getter.
    pub fn depth_write_enabled(&self) -> Result<bool, Error> {
        self.inner.depth_write_enabled().map_err(Error::from_ffi)
    }
}

macro_rules! facade_gpu_resource_id {
    ($type:ty) => {
        impl $type {
            /// Returns the opaque GPU resource identifier.
            pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
                self.inner.gpu_resource_id().map_err(Error::from_ffi)
            }
        }
    };
}

facade_gpu_resource_id!(DepthStencilState);
facade_gpu_resource_id!(SamplerState);
facade_gpu_resource_id!(FunctionHandle);

impl CounterSet {
    /// Returns the counters in this set as owned wrappers.
    pub fn counters_vec(&self) -> Result<Vec<Counter>, Error> {
        self.inner
            .counters_vec()
            .map(|values| values.into_iter().map(Counter::from_ffi).collect())
            .map_err(Error::from_ffi)
    }
}

impl CounterSampleBuffer {
    /// Resolves a checked counter sample range into owned bytes when available.
    pub fn resolve_counter_range(&self, range: Range<usize>) -> Result<Option<Vec<u8>>, Error> {
        self.inner
            .resolve_counter_range(range)
            .map_err(Error::from_ffi)
    }
}

impl DynamicLibrary {
    /// Serializes this dynamic library to a Rust file path.
    pub fn serialize_to_file(&self, path: impl AsRef<Path>) -> Result<(), Error> {
        self.inner
            .serialize_to_file(path.as_ref())
            .map_err(Error::from_ffi)
    }
}

impl FunctionLogDebugLocation {
    /// Returns the source URL as an owned absolute string.
    pub fn source_url(&self) -> Result<Option<String>, Error> {
        self.inner.source_url().map_err(Error::from_ffi)
    }
}

impl FunctionReflection {
    /// Returns reflected bindings as owned safe wrappers.
    pub fn bindings_vec(&self) -> Result<Vec<Binding>, Error> {
        self.inner
            .bindings_vec()
            .map(|values| values.into_iter().map(Binding::from_ffi).collect())
            .map_err(Error::from_ffi)
    }
}

macro_rules! facade_deprecated_attribute_getters {
    ($type:ty) => {
        impl $type {
            /// Safe canonical alias for Metal's deprecated `active` getter.
            pub fn active(&self) -> Result<bool, Error> {
                self.inner.active().map_err(Error::from_ffi)
            }

            /// Safe canonical alias for Metal's deprecated patch-control-point getter.
            pub fn patch_control_point_data(&self) -> Result<bool, Error> {
                self.inner
                    .patch_control_point_data()
                    .map_err(Error::from_ffi)
            }

            /// Safe canonical alias for Metal's deprecated patch-data getter.
            pub fn patch_data(&self) -> Result<bool, Error> {
                self.inner.patch_data().map_err(Error::from_ffi)
            }
        }
    };
}

facade_deprecated_attribute_getters!(VertexAttribute);
facade_deprecated_attribute_getters!(Attribute);

impl Function {
    /// Creates an argument encoder for a checked buffer binding index.
    pub fn new_argument_encoder(&self, buffer_index: usize) -> Result<ArgumentEncoder, Error> {
        self.inner
            .new_argument_encoder(buffer_index)
            .map(ArgumentEncoder::from_ffi)
            .map_err(Error::from_ffi)
    }

    /// Creates an argument encoder and returns its optional owned reflection.
    pub fn new_argument_encoder_with_reflection(
        &self,
        buffer_index: usize,
    ) -> Result<(ArgumentEncoder, Option<Argument>), Error> {
        self.inner
            .new_argument_encoder_with_reflection(buffer_index)
            .map(|(encoder, reflection)| {
                (
                    ArgumentEncoder::from_ffi(encoder),
                    reflection.map(Argument::from_ffi),
                )
            })
            .map_err(Error::from_ffi)
    }
}
