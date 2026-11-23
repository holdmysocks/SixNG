//! Audited binary-archive, function-stitching, and function-table boundaries.

use crate::foundation::{Error, metal_error};
use crate::metal::generated_object_types::metal::{
    Argument, ArgumentEncoder, Attribute, BinaryArchive, BinaryArchiveDescriptor, Binding,
    ComputePipelineDescriptor, Counter, CounterSampleBuffer, CounterSet, DepthStencilDescriptor,
    DepthStencilState, DynamicLibrary, FunctionDescriptor, FunctionHandle,
    FunctionLogDebugLocation, FunctionReflection, FunctionStitchingAttribute,
    FunctionStitchingAttributeAlwaysInline, FunctionStitchingFunctionNode, FunctionStitchingGraph,
    FunctionStitchingInputNode, FunctionStitchingNode, IntersectionFunctionTable,
    MeshRenderPipelineDescriptor, PipelineBufferDescriptor, PipelineBufferDescriptorArray,
    SamplerDescriptor, SamplerState, StitchedLibraryDescriptor, TileRenderPipelineDescriptor,
    VertexAttribute, VisibleFunctionTable,
};
use crate::metal::generated_struct_types::ResourceID;
use crate::metal::generated_value_types::{IntersectionFunctionSignature, SamplerAddressMode};
use crate::metal::{Buffer, Function, Library, RenderPipelineDescriptor};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use objc2_foundation::{NSData, NSError, NSRange, NSString, NSURL};
use std::ffi::CStr;
use std::ops::Range;
use std::path::{Path, PathBuf};

trait RespondsToSelector {
    fn responds_to(&self, selector: objc2::runtime::Sel) -> bool;
}

impl RespondsToSelector for AnyObject {
    fn responds_to(&self, selector: objc2::runtime::Sel) -> bool {
        // SAFETY: every Objective-C object implements respondsToSelector: and
        // the selector/bool ABI is stable.
        unsafe { msg_send![self, respondsToSelector: selector] }
    }
}

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    name: &str,
) -> Result<(), Error> {
    if object.responds_to(selector) {
        Ok(())
    } else {
        Err(Error::unsupported(format!("{name} is unavailable")))
    }
}

fn validate_name(value: &str, what: &str) -> Result<(), Error> {
    if value.is_empty() || value.as_bytes().contains(&0) {
        Err(Error::invalid_argument(format!("{what} is invalid")))
    } else {
        Ok(())
    }
}

fn object_array<'a>(values: impl IntoIterator<Item = &'a AnyObject>) -> Retained<AnyObject> {
    let class = AnyClass::get(c"NSMutableArray")
        .expect("Foundation always provides NSMutableArray when Metal is loaded");
    // SAFETY: NSMutableArray implements `new` and returns an owned empty array.
    let array: Retained<AnyObject> = unsafe { msg_send![class, new] };
    for value in values {
        // SAFETY: both objects remain live for the message and NSMutableArray
        // accepts every non-null Objective-C object.
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
    // SAFETY: callers obtain this object from a property declared as NSArray.
    let count: usize = unsafe { msg_send![&*array, count] };
    (0..count)
        .map(|index| {
            // SAFETY: index is below the count read from this immutable array;
            // objc2 retains the returned object.
            unsafe { msg_send![&*array, objectAtIndex: index] }
        })
        .collect()
}

fn file_url(path: &Path) -> Result<Retained<NSURL>, Error> {
    let path = path
        .to_str()
        .ok_or_else(|| Error::invalid_argument("file path is not valid UTF-8"))?;
    if path.as_bytes().contains(&0) {
        return Err(Error::invalid_argument("file path contains a NUL byte"));
    }
    Ok(NSURL::fileURLWithPath(&NSString::from_str(path)))
}

fn file_path(url: &NSURL) -> PathBuf {
    let pointer = url.fileSystemRepresentation();
    // SAFETY: Foundation returns a NUL-terminated representation valid for at
    // least the retained URL lifetime; it is copied before returning.
    let bytes = unsafe { CStr::from_ptr(pointer.as_ptr()) }.to_bytes();
    PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
}

fn checked_indices(range: Range<usize>, what: &str) -> Result<Range<usize>, Error> {
    if range.start > range.end {
        return Err(Error::invalid_argument(format!(
            "{what} range starts after its end"
        )));
    }
    range
        .start
        .checked_add(range.end - range.start)
        .filter(|end| *end == range.end)
        .ok_or_else(|| Error::invalid_argument(format!("{what} range overflows")))?;
    Ok(range)
}

fn checked_start_len(start: usize, len: usize, what: &str) -> Result<Range<usize>, Error> {
    let end = start
        .checked_add(len)
        .ok_or_else(|| Error::invalid_argument(format!("{what} range overflows")))?;
    Ok(start..end)
}

/// A borrowed stitching-node reference accepted by safe slice APIs.
#[derive(Clone, Copy)]
pub enum StitchingNodeRef<'a> {
    /// An input node.
    Input(&'a FunctionStitchingInputNode),
    /// A function-call node.
    Function(&'a FunctionStitchingFunctionNode),
    /// An opaque node obtained from Metal.
    Node(&'a FunctionStitchingNode),
}

impl<'a> StitchingNodeRef<'a> {
    fn as_inner(self) -> &'a AnyObject {
        match self {
            Self::Input(value) => value.as_inner(),
            Self::Function(value) => value.as_inner(),
            Self::Node(value) => value.as_inner(),
        }
    }
}

/// A borrowed stitching attribute accepted by safe slice APIs.
#[derive(Clone, Copy)]
pub enum StitchingAttributeRef<'a> {
    /// An always-inline attribute.
    AlwaysInline(&'a FunctionStitchingAttributeAlwaysInline),
    /// An opaque attribute obtained from Metal.
    Attribute(&'a FunctionStitchingAttribute),
}

impl<'a> StitchingAttributeRef<'a> {
    fn as_inner(self) -> &'a AnyObject {
        match self {
            Self::AlwaysInline(value) => value.as_inner(),
            Self::Attribute(value) => value.as_inner(),
        }
    }
}

impl BinaryArchiveDescriptor {
    /// Sets or clears the archive's source file using a Rust path.
    pub fn set_file_path(&self, path: Option<&Path>) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setUrl:),
            "MTL::BinaryArchiveDescriptor::setUrl",
        )?;
        let url = path.map(file_url).transpose()?;
        // SAFETY: selector availability is checked and NSURL is retained for
        // the complete Objective-C message.
        unsafe {
            let _: () = msg_send![self.as_inner(), setUrl: url.as_deref()];
        }
        Ok(())
    }

    /// Returns the optional source file as an owned Rust path.
    pub fn file_path(&self) -> Result<Option<PathBuf>, Error> {
        require_selector(
            self.as_inner(),
            sel!(url),
            "MTL::BinaryArchiveDescriptor::url",
        )?;
        // SAFETY: the selector is declared to return a nullable NSURL and the
        // retained result is copied into a Rust path.
        let url: Option<Retained<NSURL>> = unsafe { msg_send![self.as_inner(), url] };
        Ok(url.as_deref().map(file_path))
    }
}

macro_rules! archive_add_descriptor {
    ($method:ident, $descriptor:ty, $selector:ident, $context:literal) => {
        /// Adds the functions selected by the descriptor and owns NSError diagnostics.
        pub fn $method(&self, descriptor: &$descriptor) -> Result<(), Error> {
            require_selector(self.as_inner(), sel!($selector:error:), $context)?;
            // SAFETY: selector presence is checked, the descriptor wrapper
            // preserves its Objective-C class, and objc2 owns NSError output.
            let result: Result<(), Retained<NSError>> = unsafe {
                msg_send![self.as_inner(), $selector: descriptor.as_inner(), error: _]
            };
            result.map_err(|error| metal_error(&error))
        }
    };
}

impl BinaryArchive {
    archive_add_descriptor!(
        add_compute_pipeline_functions,
        ComputePipelineDescriptor,
        addComputePipelineFunctionsWithDescriptor,
        "MTL::BinaryArchive::addComputePipelineFunctions"
    );
    archive_add_descriptor!(
        add_tile_render_pipeline_functions,
        TileRenderPipelineDescriptor,
        addTileRenderPipelineFunctionsWithDescriptor,
        "MTL::BinaryArchive::addTileRenderPipelineFunctions"
    );

    /// Adds the functions selected by a render-pipeline descriptor.
    pub fn add_render_pipeline_functions(
        &self,
        descriptor: &RenderPipelineDescriptor,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(addRenderPipelineFunctionsWithDescriptor:error:),
            "MTL::BinaryArchive::addRenderPipelineFunctions",
        )?;
        // SAFETY: selector presence is checked, the canonical descriptor wraps
        // MTLRenderPipelineDescriptor, and objc2 owns NSError output.
        let result: Result<(), Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), addRenderPipelineFunctionsWithDescriptor: &*descriptor.inner, error: _]
        };
        result.map_err(|error| metal_error(&error))
    }
    archive_add_descriptor!(
        add_mesh_render_pipeline_functions,
        MeshRenderPipelineDescriptor,
        addMeshRenderPipelineFunctionsWithDescriptor,
        "MTL::BinaryArchive::addMeshRenderPipelineFunctions"
    );
    archive_add_descriptor!(
        add_stitched_library,
        StitchedLibraryDescriptor,
        addLibraryWithDescriptor,
        "MTL::BinaryArchive::addLibrary"
    );

    /// Adds one function selected from a library and owns NSError diagnostics.
    pub fn add_function(
        &self,
        descriptor: &FunctionDescriptor,
        library: &Library,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(addFunctionWithDescriptor:library:error:),
            "MTL::BinaryArchive::addFunction",
        )?;
        // SAFETY: selector presence is checked, both wrappers preserve their
        // declared Objective-C identities, and objc2 owns NSError output.
        let result: Result<(), Retained<NSError>> = unsafe {
            msg_send![self.as_inner(), addFunctionWithDescriptor: descriptor.as_inner(), library: library.as_any_object(), error: _]
        };
        result.map_err(|error| metal_error(&error))
    }

    /// Serializes this archive to a file path and owns NSError diagnostics.
    pub fn serialize_to_file(&self, path: &Path) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(serializeToURL:error:),
            "MTL::BinaryArchive::serializeToURL",
        )?;
        let url = file_url(path)?;
        // SAFETY: selector presence is checked, NSURL remains live, and objc2
        // converts and owns the NSError out parameter.
        let result: Result<(), Retained<NSError>> =
            unsafe { msg_send![self.as_inner(), serializeToURL: &*url, error: _] };
        result.map_err(|error| metal_error(&error))
    }

    /// Sets or clears the diagnostic label.
    pub fn set_optional_label(&self, label: Option<&str>) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setLabel:),
            "MTL::BinaryArchive::setLabel",
        )?;
        if label.is_some_and(|value| value.as_bytes().contains(&0)) {
            return Err(Error::invalid_argument(
                "binary archive label contains a NUL byte",
            ));
        }
        let label = label.map(NSString::from_str);
        // SAFETY: selector availability is checked and the optional NSString
        // remains live for the message.
        unsafe {
            let _: () = msg_send![self.as_inner(), setLabel: label.as_deref()];
        }
        Ok(())
    }
}

impl FunctionStitchingInputNode {
    /// Creates an input node for one argument index.
    pub fn with_argument_index(argument_index: usize) -> Result<Self, Error> {
        let value = Self::new()?;
        value.set_argument_index(argument_index)?;
        Ok(value)
    }
}

impl FunctionStitchingFunctionNode {
    /// Creates a fully initialized function-call node from Rust slices.
    pub fn with_details(
        name: &str,
        arguments: &[StitchingNodeRef<'_>],
        control_dependencies: &[FunctionStitchingFunctionNode],
    ) -> Result<Self, Error> {
        validate_name(name, "stitched function-node name")?;
        let value = Self::new()?;
        value.set_name(name)?;
        value.set_arguments_slice(arguments)?;
        value.set_control_dependencies_slice(control_dependencies)?;
        Ok(value)
    }

    /// Returns argument nodes as owned wrappers.
    pub fn arguments_vec(&self) -> Result<Vec<FunctionStitchingNode>, Error> {
        require_selector(
            self.as_inner(),
            sel!(arguments),
            "MTL::FunctionStitchingFunctionNode::arguments",
        )?;
        // SAFETY: selector presence and its NSArray return contract are checked.
        let array: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), arguments] };
        Ok(array_objects(array)
            .into_iter()
            .map(FunctionStitchingNode::from_inner)
            .collect())
    }

    /// Replaces argument nodes from a safe Rust slice.
    pub fn set_arguments_slice(&self, values: &[StitchingNodeRef<'_>]) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setArguments:),
            "MTL::FunctionStitchingFunctionNode::setArguments",
        )?;
        let array = object_array(values.iter().copied().map(StitchingNodeRef::as_inner));
        // SAFETY: selector presence is checked and all array entries are node wrappers.
        unsafe {
            let _: () = msg_send![self.as_inner(), setArguments: &*array];
        }
        Ok(())
    }

    /// Returns control dependencies as owned wrappers.
    pub fn control_dependencies_vec(&self) -> Result<Vec<FunctionStitchingFunctionNode>, Error> {
        require_selector(
            self.as_inner(),
            sel!(controlDependencies),
            "MTL::FunctionStitchingFunctionNode::controlDependencies",
        )?;
        // SAFETY: selector presence and its NSArray return contract are checked.
        let array: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), controlDependencies] };
        Ok(array_objects(array)
            .into_iter()
            .map(FunctionStitchingFunctionNode::from_inner)
            .collect())
    }

    /// Replaces control dependencies from a safe Rust slice.
    pub fn set_control_dependencies_slice(
        &self,
        values: &[FunctionStitchingFunctionNode],
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setControlDependencies:),
            "MTL::FunctionStitchingFunctionNode::setControlDependencies",
        )?;
        let array = object_array(values.iter().map(FunctionStitchingFunctionNode::as_inner));
        // SAFETY: selector presence is checked and every entry has the declared class.
        unsafe {
            let _: () = msg_send![self.as_inner(), setControlDependencies: &*array];
        }
        Ok(())
    }
}

impl FunctionStitchingGraph {
    /// Creates a fully initialized function graph from safe slices.
    pub fn with_details(
        function_name: &str,
        nodes: &[FunctionStitchingFunctionNode],
        output_node: &FunctionStitchingFunctionNode,
        attributes: &[StitchingAttributeRef<'_>],
    ) -> Result<Self, Error> {
        validate_name(function_name, "stitched function name")?;
        if nodes.is_empty() {
            return Err(Error::invalid_argument(
                "a stitching graph must contain at least one node",
            ));
        }
        if !nodes
            .iter()
            .any(|node| std::ptr::eq(node.as_inner(), output_node.as_inner()))
        {
            return Err(Error::invalid_argument(
                "the stitching graph output node must be present in nodes",
            ));
        }
        let value = Self::new()?;
        value.set_function_name(function_name)?;
        value.set_nodes_slice(nodes)?;
        value.set_output_node(Some(output_node))?;
        value.set_attributes_slice(attributes)?;
        Ok(value)
    }

    /// Returns graph nodes as owned wrappers.
    pub fn nodes_vec(&self) -> Result<Vec<FunctionStitchingFunctionNode>, Error> {
        require_selector(
            self.as_inner(),
            sel!(nodes),
            "MTL::FunctionStitchingGraph::nodes",
        )?;
        // SAFETY: selector presence and its NSArray return contract are checked.
        let array: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), nodes] };
        Ok(array_objects(array)
            .into_iter()
            .map(FunctionStitchingFunctionNode::from_inner)
            .collect())
    }

    /// Replaces graph nodes from a safe Rust slice.
    pub fn set_nodes_slice(&self, values: &[FunctionStitchingFunctionNode]) -> Result<(), Error> {
        if values.is_empty() {
            return Err(Error::invalid_argument(
                "a stitching graph must contain at least one node",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setNodes:),
            "MTL::FunctionStitchingGraph::setNodes",
        )?;
        let array = object_array(values.iter().map(FunctionStitchingFunctionNode::as_inner));
        // SAFETY: selector presence is checked and every entry has the declared class.
        unsafe {
            let _: () = msg_send![self.as_inner(), setNodes: &*array];
        }
        Ok(())
    }

    /// Returns graph attributes as owned wrappers.
    pub fn attributes_vec(&self) -> Result<Vec<FunctionStitchingAttribute>, Error> {
        require_selector(
            self.as_inner(),
            sel!(attributes),
            "MTL::FunctionStitchingGraph::attributes",
        )?;
        // SAFETY: selector presence and its NSArray return contract are checked.
        let array: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), attributes] };
        Ok(array_objects(array)
            .into_iter()
            .map(FunctionStitchingAttribute::from_inner)
            .collect())
    }

    /// Replaces graph attributes from a safe Rust slice.
    pub fn set_attributes_slice(&self, values: &[StitchingAttributeRef<'_>]) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setAttributes:),
            "MTL::FunctionStitchingGraph::setAttributes",
        )?;
        let array = object_array(values.iter().copied().map(StitchingAttributeRef::as_inner));
        // SAFETY: selector presence is checked and every entry is an attribute wrapper.
        unsafe {
            let _: () = msg_send![self.as_inner(), setAttributes: &*array];
        }
        Ok(())
    }
}

impl StitchedLibraryDescriptor {
    /// Returns function graphs as owned wrappers.
    pub fn function_graphs_vec(&self) -> Result<Vec<FunctionStitchingGraph>, Error> {
        require_selector(
            self.as_inner(),
            sel!(functionGraphs),
            "MTL::StitchedLibraryDescriptor::functionGraphs",
        )?;
        // SAFETY: selector presence and its NSArray return contract are checked.
        let array: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), functionGraphs] };
        Ok(array_objects(array)
            .into_iter()
            .map(FunctionStitchingGraph::from_inner)
            .collect())
    }

    /// Replaces function graphs from a safe Rust slice.
    pub fn set_function_graphs_slice(
        &self,
        values: &[FunctionStitchingGraph],
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setFunctionGraphs:),
            "MTL::StitchedLibraryDescriptor::setFunctionGraphs",
        )?;
        let array = object_array(values.iter().map(FunctionStitchingGraph::as_inner));
        // SAFETY: selector presence is checked and every entry has the declared class.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFunctionGraphs: &*array];
        }
        Ok(())
    }

    /// Returns source functions as owned canonical wrappers.
    pub fn functions_vec(&self) -> Result<Vec<crate::metal::Function>, Error> {
        require_selector(
            self.as_inner(),
            sel!(functions),
            "MTL::StitchedLibraryDescriptor::functions",
        )?;
        // SAFETY: selector presence and its NSArray return contract are checked.
        let array: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), functions] };
        array_objects(array)
            .into_iter()
            .map(crate::metal::Function::from_any_object)
            .collect()
    }

    /// Replaces source functions from a safe Rust slice.
    pub fn set_functions_slice(&self, values: &[crate::metal::Function]) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setFunctions:),
            "MTL::StitchedLibraryDescriptor::setFunctions",
        )?;
        let array = object_array(values.iter().map(crate::metal::Function::as_any_object));
        // SAFETY: selector presence is checked and every entry conforms to MTLFunction.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFunctions: &*array];
        }
        Ok(())
    }

    /// Returns binary archives as owned wrappers.
    pub fn binary_archives_vec(&self) -> Result<Vec<BinaryArchive>, Error> {
        require_selector(
            self.as_inner(),
            sel!(binaryArchives),
            "MTL::StitchedLibraryDescriptor::binaryArchives",
        )?;
        // SAFETY: selector presence and its NSArray return contract are checked.
        let array: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), binaryArchives] };
        Ok(array_objects(array)
            .into_iter()
            .map(BinaryArchive::from_inner)
            .collect())
    }

    /// Replaces binary archives from a safe Rust slice.
    pub fn set_binary_archives_slice(&self, values: &[BinaryArchive]) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setBinaryArchives:),
            "MTL::StitchedLibraryDescriptor::setBinaryArchives",
        )?;
        let array = object_array(values.iter().map(BinaryArchive::as_inner));
        // SAFETY: selector presence is checked and every entry conforms to MTLBinaryArchive.
        unsafe {
            let _: () = msg_send![self.as_inner(), setBinaryArchives: &*array];
        }
        Ok(())
    }
}

fn gpu_resource_id(object: &AnyObject, context: &str) -> Result<ResourceID, Error> {
    require_selector(object, sel!(gpuResourceID), context)?;
    // SAFETY: selector availability is checked. MTLResourceID is a one-u64 C
    // struct, represented identically by the generated safe value below.
    let raw: objc2_metal::MTLResourceID = unsafe { msg_send![object, gpuResourceID] };
    // SAFETY: MTLResourceID is repr(C) with exactly one u64 field.
    let raw = unsafe { std::ptr::read_unaligned(std::ptr::from_ref(&raw).cast::<u64>()) };
    Ok(ResourceID { _impl: raw })
}

fn validate_signature(value: IntersectionFunctionSignature) -> Result<usize, Error> {
    if value.is_valid() {
        Ok(value.as_raw())
    } else {
        Err(Error::invalid_argument(
            "intersection function signature contains undeclared bits",
        ))
    }
}

impl VisibleFunctionTable {
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        gpu_resource_id(self.as_inner(), "MTL::VisibleFunctionTable::gpuResourceID")
    }

    /// Binds or clears one function-table slot.
    pub fn set_function(
        &self,
        function: Option<&FunctionHandle>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setFunction:atIndex:),
            "MTL::VisibleFunctionTable::setFunction",
        )?;
        // SAFETY: selector presence is checked; the optional handle remains
        // retained and index uses Metal's declared NSUInteger ABI.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFunction: function.map(FunctionHandle::as_inner), atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's function-handle pointer array.
    pub fn set_functions(
        &self,
        functions: &[Option<&FunctionHandle>],
        start_index: usize,
    ) -> Result<(), Error> {
        let indices = checked_start_len(start_index, functions.len(), "visible function table")?;
        for (index, function) in indices.zip(functions.iter().copied()) {
            self.set_function(function, index)?;
        }
        Ok(())
    }
}

impl IntersectionFunctionTable {
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        gpu_resource_id(
            self.as_inner(),
            "MTL::IntersectionFunctionTable::gpuResourceID",
        )
    }

    /// Binds or clears a checked buffer region.
    pub fn set_buffer(
        &self,
        buffer: Option<&Buffer>,
        offset: usize,
        index: usize,
    ) -> Result<(), Error> {
        match buffer {
            Some(buffer) if offset <= buffer.length() => {}
            Some(_) => {
                return Err(Error::invalid_argument(
                    "intersection function buffer offset is out of bounds",
                ));
            }
            None if offset == 0 => {}
            None => {
                return Err(Error::invalid_argument(
                    "an unbound intersection function buffer requires offset zero",
                ));
            }
        }
        require_selector(
            self.as_inner(),
            sel!(setBuffer:offset:atIndex:),
            "MTL::IntersectionFunctionTable::setBuffer",
        )?;
        // SAFETY: selector presence and buffer offset bounds are checked; the
        // optional buffer remains retained for the message.
        unsafe {
            let _: () = msg_send![self.as_inner(), setBuffer: buffer.map(Buffer::as_any_object), offset: offset, atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's parallel buffer/offset arrays.
    pub fn set_buffers(
        &self,
        bindings: &[(Option<&Buffer>, usize)],
        start_index: usize,
    ) -> Result<(), Error> {
        let indices = checked_start_len(
            start_index,
            bindings.len(),
            "intersection function buffer table",
        )?;
        for (index, (buffer, offset)) in indices.zip(bindings.iter().copied()) {
            self.set_buffer(buffer, offset, index)?;
        }
        Ok(())
    }

    /// Binds or clears one function-table slot.
    pub fn set_function(
        &self,
        function: Option<&FunctionHandle>,
        index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setFunction:atIndex:),
            "MTL::IntersectionFunctionTable::setFunction",
        )?;
        // SAFETY: selector presence is checked and the optional retained
        // function handle has the declared Objective-C identity.
        unsafe {
            let _: () = msg_send![self.as_inner(), setFunction: function.map(FunctionHandle::as_inner), atIndex: index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's function-handle pointer array.
    pub fn set_functions(
        &self,
        functions: &[Option<&FunctionHandle>],
        start_index: usize,
    ) -> Result<(), Error> {
        let indices =
            checked_start_len(start_index, functions.len(), "intersection function table")?;
        for (index, function) in indices.zip(functions.iter().copied()) {
            self.set_function(function, index)?;
        }
        Ok(())
    }

    /// Installs an opaque triangle function at one slot.
    pub fn set_opaque_triangle_function(
        &self,
        signature: IntersectionFunctionSignature,
        index: usize,
    ) -> Result<(), Error> {
        let signature = validate_signature(signature)?;
        require_selector(
            self.as_inner(),
            sel!(setOpaqueTriangleIntersectionFunctionWithSignature:atIndex:),
            "MTL::IntersectionFunctionTable::setOpaqueTriangleIntersectionFunction",
        )?;
        // SAFETY: selector presence and option bits are checked; index uses the
        // declared NSUInteger representation.
        unsafe {
            let _: () = msg_send![self.as_inner(), setOpaqueTriangleIntersectionFunctionWithSignature: signature, atIndex: index];
        }
        Ok(())
    }

    /// Installs an opaque triangle function across a checked Rust range.
    pub fn set_opaque_triangle_function_range(
        &self,
        signature: IntersectionFunctionSignature,
        range: Range<usize>,
    ) -> Result<(), Error> {
        validate_signature(signature)?;
        require_selector(
            self.as_inner(),
            sel!(setOpaqueTriangleIntersectionFunctionWithSignature:atIndex:),
            "MTL::IntersectionFunctionTable::setOpaqueTriangleIntersectionFunction",
        )?;
        for index in checked_indices(range, "opaque triangle function")? {
            self.set_opaque_triangle_function(signature, index)?;
        }
        Ok(())
    }

    /// Installs an opaque curve function at one slot.
    pub fn set_opaque_curve_function(
        &self,
        signature: IntersectionFunctionSignature,
        index: usize,
    ) -> Result<(), Error> {
        let signature = validate_signature(signature)?;
        require_selector(
            self.as_inner(),
            sel!(setOpaqueCurveIntersectionFunctionWithSignature:atIndex:),
            "MTL::IntersectionFunctionTable::setOpaqueCurveIntersectionFunction",
        )?;
        // SAFETY: selector presence and option bits are checked; index uses the
        // declared NSUInteger representation.
        unsafe {
            let _: () = msg_send![self.as_inner(), setOpaqueCurveIntersectionFunctionWithSignature: signature, atIndex: index];
        }
        Ok(())
    }

    /// Installs an opaque curve function across a checked Rust range.
    pub fn set_opaque_curve_function_range(
        &self,
        signature: IntersectionFunctionSignature,
        range: Range<usize>,
    ) -> Result<(), Error> {
        validate_signature(signature)?;
        require_selector(
            self.as_inner(),
            sel!(setOpaqueCurveIntersectionFunctionWithSignature:atIndex:),
            "MTL::IntersectionFunctionTable::setOpaqueCurveIntersectionFunction",
        )?;
        for index in checked_indices(range, "opaque curve function")? {
            self.set_opaque_curve_function(signature, index)?;
        }
        Ok(())
    }

    /// Binds or clears one visible-function table at a buffer index.
    pub fn set_visible_function_table(
        &self,
        table: Option<&VisibleFunctionTable>,
        buffer_index: usize,
    ) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setVisibleFunctionTable:atBufferIndex:),
            "MTL::IntersectionFunctionTable::setVisibleFunctionTable",
        )?;
        // SAFETY: selector presence is checked and the optional table remains retained.
        unsafe {
            let _: () = msg_send![self.as_inner(), setVisibleFunctionTable: table.map(VisibleFunctionTable::as_inner), atBufferIndex: buffer_index];
        }
        Ok(())
    }

    /// Safe slice substitute for Metal's visible-function-table pointer array.
    pub fn set_visible_function_tables(
        &self,
        tables: &[Option<&VisibleFunctionTable>],
        start_buffer_index: usize,
    ) -> Result<(), Error> {
        let indices = checked_start_len(
            start_buffer_index,
            tables.len(),
            "visible function table buffer binding",
        )?;
        for (index, table) in indices.zip(tables.iter().copied()) {
            self.set_visible_function_table(table, index)?;
        }
        Ok(())
    }
}

impl FunctionDescriptor {
    /// Returns associated binary archives as owned wrappers.
    pub fn binary_archives_vec(&self) -> Result<Vec<BinaryArchive>, Error> {
        require_selector(
            self.as_inner(),
            sel!(binaryArchives),
            "MTL::FunctionDescriptor::binaryArchives",
        )?;
        // SAFETY: selector presence and its nullable NSArray return are checked.
        let array: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), binaryArchives] };
        Ok(array_objects(array)
            .into_iter()
            .map(BinaryArchive::from_inner)
            .collect())
    }

    /// Replaces associated binary archives from a Rust slice.
    pub fn set_binary_archives_slice(&self, values: &[BinaryArchive]) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(setBinaryArchives:),
            "MTL::FunctionDescriptor::setBinaryArchives",
        )?;
        let array = object_array(values.iter().map(BinaryArchive::as_inner));
        // SAFETY: selector presence is checked and every entry conforms to MTLBinaryArchive.
        unsafe {
            let _: () = msg_send![self.as_inner(), setBinaryArchives: &*array];
        }
        Ok(())
    }
}

const MAX_PIPELINE_BUFFER_BINDINGS: usize = 31;

impl PipelineBufferDescriptorArray {
    /// Returns one optional pipeline-buffer descriptor after checking the Metal index limit.
    pub fn buffer(&self, index: usize) -> Result<Option<PipelineBufferDescriptor>, Error> {
        if index >= MAX_PIPELINE_BUFFER_BINDINGS {
            return Err(Error::invalid_argument(
                "pipeline buffer index must be below 31",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(objectAtIndexedSubscript:),
            "MTL::PipelineBufferDescriptorArray::object",
        )?;
        // SAFETY: selector presence and the framework's 31-slot index limit are checked;
        // objc2 retains the nullable returned descriptor.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
        Ok(value.map(PipelineBufferDescriptor::from_inner))
    }

    /// Replaces or clears one pipeline-buffer descriptor at a checked index.
    pub fn set_buffer(
        &self,
        index: usize,
        value: Option<&PipelineBufferDescriptor>,
    ) -> Result<(), Error> {
        if index >= MAX_PIPELINE_BUFFER_BINDINGS {
            return Err(Error::invalid_argument(
                "pipeline buffer index must be below 31",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setObject:atIndexedSubscript:),
            "MTL::PipelineBufferDescriptorArray::setObject",
        )?;
        // SAFETY: selector presence and index bounds are checked and the optional
        // descriptor remains retained for the message.
        unsafe {
            let _: () = msg_send![self.as_inner(), setObject: value.map(PipelineBufferDescriptor::as_inner), atIndexedSubscript: index];
        }
        Ok(())
    }
}

impl SamplerDescriptor {
    /// Returns the R-coordinate address mode after selector and enum checks.
    pub fn r_address_mode(&self) -> Result<SamplerAddressMode, Error> {
        require_selector(
            self.as_inner(),
            sel!(rAddressMode),
            "MTL::SamplerDescriptor::rAddressMode",
        )?;
        // SAFETY: selector presence is checked and NSUInteger is the declared enum ABI.
        let raw: usize = unsafe { msg_send![self.as_inner(), rAddressMode] };
        let value = SamplerAddressMode::from_system_raw(raw);
        if value.is_valid() {
            Ok(value)
        } else {
            Err(Error::unsupported(
                "MTL::SamplerDescriptor::rAddressMode returned an unknown value",
            ))
        }
    }

    /// Sets the R-coordinate address mode after validating the enum value.
    pub fn set_r_address_mode(&self, value: SamplerAddressMode) -> Result<(), Error> {
        if !value.is_valid() {
            return Err(Error::invalid_argument(
                "sampler R address mode is undeclared",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(setRAddressMode:),
            "MTL::SamplerDescriptor::setRAddressMode",
        )?;
        let raw = value.as_raw();
        // SAFETY: selector presence and enum validity are checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), setRAddressMode: raw];
        }
        Ok(())
    }
}

impl DepthStencilDescriptor {
    /// Safe canonical alias for Metal's deprecated `depthWriteEnabled` getter.
    pub fn depth_write_enabled(&self) -> Result<bool, Error> {
        self.is_depth_write_enabled()
    }
}

impl DepthStencilState {
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        gpu_resource_id(self.as_inner(), "MTL::DepthStencilState::gpuResourceID")
    }
}

impl SamplerState {
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        gpu_resource_id(self.as_inner(), "MTL::SamplerState::gpuResourceID")
    }
}

impl FunctionHandle {
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<ResourceID, Error> {
        gpu_resource_id(self.as_inner(), "MTL::FunctionHandle::gpuResourceID")
    }
}

impl CounterSet {
    /// Returns the counters in this set as owned wrappers.
    pub fn counters_vec(&self) -> Result<Vec<Counter>, Error> {
        require_selector(self.as_inner(), sel!(counters), "MTL::CounterSet::counters")?;
        // SAFETY: selector presence and its nullable NSArray return are checked.
        let array: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), counters] };
        Ok(array_objects(array)
            .into_iter()
            .map(Counter::from_inner)
            .collect())
    }
}

impl CounterSampleBuffer {
    /// Resolves a checked sample range into owned bytes when CPU resolution is available.
    pub fn resolve_counter_range(&self, range: Range<usize>) -> Result<Option<Vec<u8>>, Error> {
        let count = self.sample_count()?;
        let range = checked_indices(range, "counter sample")?;
        if range.end > count {
            return Err(Error::invalid_argument(
                "counter sample range is out of bounds",
            ));
        }
        require_selector(
            self.as_inner(),
            sel!(resolveCounterRange:),
            "MTL::CounterSampleBuffer::resolveCounterRange",
        )?;
        // SAFETY: selector presence and sample bounds are checked; NSData is retained.
        let data: Option<Retained<NSData>> = unsafe {
            msg_send![self.as_inner(), resolveCounterRange: NSRange::new(range.start, range.len())]
        };
        let Some(data) = data else {
            return Ok(None);
        };
        let length = data.length();
        if length == 0 {
            return Ok(Some(Vec::new()));
        }
        // SAFETY: immutable NSData guarantees `bytes` points to at least `length`
        // bytes for the retained data lifetime; the bytes are copied immediately.
        let pointer: *const u8 = unsafe { msg_send![&*data, bytes] };
        if pointer.is_null() {
            return Err(Error::unsupported(
                "Metal returned non-empty counter data without bytes",
            ));
        }
        // SAFETY: pointer non-nullness and NSData length are checked above.
        let bytes = unsafe { std::slice::from_raw_parts(pointer, length) };
        Ok(Some(bytes.to_vec()))
    }
}

impl DynamicLibrary {
    /// Serializes this dynamic library to a Rust file path.
    pub fn serialize_to_file(&self, path: &Path) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(serializeToURL:error:),
            "MTL::DynamicLibrary::serializeToURL",
        )?;
        let url = file_url(path)?;
        // SAFETY: selector presence is checked, NSURL remains live, and objc2
        // owns the NSError out parameter.
        let result: Result<(), Retained<NSError>> =
            unsafe { msg_send![self.as_inner(), serializeToURL: &*url, error: _] };
        result.map_err(|error| metal_error(&error))
    }
}

impl FunctionLogDebugLocation {
    /// Returns the source URL as an owned absolute string.
    pub fn source_url(&self) -> Result<Option<String>, Error> {
        require_selector(
            self.as_inner(),
            sel!(URL),
            "MTL::FunctionLogDebugLocation::URL",
        )?;
        // SAFETY: the selector is declared to return a nullable NSURL retained by objc2.
        let url: Option<Retained<NSURL>> = unsafe { msg_send![self.as_inner(), URL] };
        Ok(url
            .and_then(|value| value.absoluteString())
            .map(|value| value.to_string()))
    }
}

impl FunctionReflection {
    /// Returns reflected bindings as owned safe wrappers.
    pub fn bindings_vec(&self) -> Result<Vec<Binding>, Error> {
        require_selector(
            self.as_inner(),
            sel!(bindings),
            "MTL::FunctionReflection::bindings",
        )?;
        // SAFETY: selector presence and its non-null NSArray return are checked.
        let array: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), bindings] };
        Ok(array_objects(array)
            .into_iter()
            .map(Binding::from_inner)
            .collect())
    }
}

impl VertexAttribute {
    /// Safe canonical alias for Metal's deprecated `active` getter.
    pub fn active(&self) -> Result<bool, Error> {
        self.is_active()
    }

    /// Safe canonical alias for Metal's deprecated `patchControlPointData` getter.
    pub fn patch_control_point_data(&self) -> Result<bool, Error> {
        self.is_patch_control_point_data()
    }

    /// Safe canonical alias for Metal's deprecated `patchData` getter.
    pub fn patch_data(&self) -> Result<bool, Error> {
        self.is_patch_data()
    }
}

impl Attribute {
    /// Safe canonical alias for Metal's deprecated `active` getter.
    pub fn active(&self) -> Result<bool, Error> {
        self.is_active()
    }

    /// Safe canonical alias for Metal's deprecated `patchControlPointData` getter.
    pub fn patch_control_point_data(&self) -> Result<bool, Error> {
        self.is_patch_control_point_data()
    }

    /// Safe canonical alias for Metal's deprecated `patchData` getter.
    pub fn patch_data(&self) -> Result<bool, Error> {
        self.is_patch_data()
    }
}

impl Function {
    fn checked_argument_buffer_index(index: usize) -> Result<(), Error> {
        if index >= MAX_PIPELINE_BUFFER_BINDINGS {
            Err(Error::invalid_argument(
                "argument-buffer index must be below 31",
            ))
        } else {
            Ok(())
        }
    }

    /// Creates an argument encoder for a checked buffer binding index.
    pub fn new_argument_encoder(&self, buffer_index: usize) -> Result<ArgumentEncoder, Error> {
        Self::checked_argument_buffer_index(buffer_index)?;
        require_selector(
            self.as_any_object(),
            sel!(newArgumentEncoderWithBufferIndex:),
            "MTL::Function::newArgumentEncoder",
        )?;
        // SAFETY: selector presence and Metal's buffer-index limit are checked;
        // the `new` family returns an owned argument encoder.
        let inner: Retained<AnyObject> = unsafe {
            msg_send![self.as_any_object(), newArgumentEncoderWithBufferIndex: buffer_index]
        };
        Ok(ArgumentEncoder::from_inner(inner))
    }

    /// Creates an argument encoder and returns its optional owned reflection.
    pub fn new_argument_encoder_with_reflection(
        &self,
        buffer_index: usize,
    ) -> Result<(ArgumentEncoder, Option<Argument>), Error> {
        Self::checked_argument_buffer_index(buffer_index)?;
        require_selector(
            self.as_any_object(),
            sel!(newArgumentEncoderWithBufferIndex:reflection:),
            "MTL::Function::newArgumentEncoder(reflection)",
        )?;
        let mut reflection: *mut AnyObject = std::ptr::null_mut();
        // SAFETY: selector presence and the binding index are checked. The local
        // out pointer has the declared autoreleasing object-pointer ABI.
        let inner: Retained<AnyObject> = unsafe {
            msg_send![self.as_any_object(), newArgumentEncoderWithBufferIndex: buffer_index, reflection: &mut reflection]
        };
        // SAFETY: a non-null autoreleased reflection object is retained before
        // leaving the message scope and MTL declares it as MTLArgument.
        let reflection = unsafe { Retained::retain(reflection) }.map(Argument::from_inner);
        Ok((ArgumentEncoder::from_inner(inner), reflection))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checked_table_ranges_reject_overflow_and_reverse_ranges() {
        assert!(checked_start_len(usize::MAX, 1, "table").is_err());
        assert!(checked_indices(Range { start: 2, end: 1 }, "table",).is_err());
        assert_eq!(checked_start_len(7, 3, "table").unwrap(), 7..10);
    }

    #[test]
    fn argument_encoder_indices_enforce_metal_buffer_limit() {
        assert!(Function::checked_argument_buffer_index(30).is_ok());
        assert!(Function::checked_argument_buffer_index(31).is_err());
    }
}
