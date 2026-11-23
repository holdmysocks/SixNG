//! Safe Metal shader library and pipeline objects.

use super::PixelFormat;
use crate::Error;

/// Copyable, validated scalar options for a render pipeline descriptor.
pub use metal_rust_ffi::RenderPipelineOptions;

/// An owned Metal shader library.
#[derive(Clone)]
pub struct Library {
    pub(crate) inner: metal_rust_ffi::Library,
}

impl Library {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::Library) -> Self {
        Self { inner }
    }

    /// Looks up a named shader function.
    pub fn function(&self, name: &str) -> Result<Function, Error> {
        self.inner
            .function(name)
            .map(|inner| Function { inner })
            .map_err(Error::from_ffi)
    }
    /// Returns every function name as an owned Rust string.
    #[must_use]
    pub fn function_names(&self) -> Vec<String> {
        self.inner.function_names()
    }
    /// Creates a specialized function from named constant values.
    pub fn specialized_function(
        &self,
        name: &str,
        constants: &super::FunctionConstantValues,
    ) -> Result<Function, Error> {
        self.inner
            .specialized_function(name, &constants.inner)
            .map(Function::from_ffi)
            .map_err(Error::from_ffi)
    }
    /// Starts one-shot asynchronous specialization from named constants.
    pub fn specialized_function_async(
        &self,
        name: &str,
        constants: &super::FunctionConstantValues,
        handler: impl FnOnce(Result<Function, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .specialized_function_async(name, &constants.inner, move |result| {
                handler(result.map(Function::from_ffi).map_err(Error::from_ffi));
            })
            .map_err(Error::from_ffi)
    }
    /// Creates a function from a function descriptor.
    pub fn function_with_descriptor(
        &self,
        descriptor: &super::FunctionDescriptor,
    ) -> Result<Function, Error> {
        self.inner
            .function_with_descriptor(&descriptor.inner)
            .map(Function::from_ffi)
            .map_err(Error::from_ffi)
    }
    /// Starts one-shot asynchronous descriptor-based function creation.
    pub fn function_with_descriptor_async(
        &self,
        descriptor: &super::FunctionDescriptor,
        handler: impl FnOnce(Result<Function, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .function_with_descriptor_async(&descriptor.inner, move |result| {
                handler(result.map(Function::from_ffi).map_err(Error::from_ffi));
            })
            .map_err(Error::from_ffi)
    }
    /// Creates an intersection function from a descriptor.
    pub fn intersection_function_with_descriptor(
        &self,
        descriptor: &super::IntersectionFunctionDescriptor,
    ) -> Result<Function, Error> {
        self.inner
            .intersection_function_with_descriptor(&descriptor.inner)
            .map(Function::from_ffi)
            .map_err(Error::from_ffi)
    }
    /// Starts one-shot asynchronous intersection-function creation.
    pub fn intersection_function_with_descriptor_async(
        &self,
        descriptor: &super::IntersectionFunctionDescriptor,
        handler: impl FnOnce(Result<Function, Error>) + Send + 'static,
    ) -> Result<(), Error> {
        self.inner
            .intersection_function_with_descriptor_async(&descriptor.inner, move |result| {
                handler(result.map(Function::from_ffi).map_err(Error::from_ffi));
            })
            .map_err(Error::from_ffi)
    }
    /// Returns reflection metadata for a named function.
    pub fn reflection_for_function(
        &self,
        name: &str,
    ) -> Result<Option<super::FunctionReflection>, Error> {
        self.inner
            .reflection_for_function(name)
            .map(|value| value.map(super::FunctionReflection::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> super::Device {
        super::Device::from_ffi(self.inner.device())
    }
    /// Returns the optional install name.
    #[must_use]
    pub fn install_name(&self) -> Option<String> {
        self.inner.install_name()
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }
    /// Sets or clears the label.
    pub fn set_label(&self, value: Option<&str>) {
        self.inner.set_label(value);
    }
    /// Returns the library type.
    #[must_use]
    pub fn library_type(&self) -> super::LibraryType {
        self.inner.library_type()
    }
}

/// A named shader function.
#[derive(Clone)]
pub struct Function {
    pub(crate) inner: metal_rust_ffi::Function,
}

impl Function {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::Function) -> Self {
        Self { inner }
    }

    /// Returns the function's Metal name.
    #[must_use]
    pub fn name(&self) -> String {
        self.inner.name()
    }
    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> super::Device {
        super::Device::from_ffi(self.inner.device())
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }
    /// Sets or clears the label.
    pub fn set_label(&self, value: Option<&str>) {
        self.inner.set_label(value);
    }
    /// Returns the function type.
    #[must_use]
    pub fn function_type(&self) -> super::FunctionType {
        self.inner.function_type()
    }
    /// Returns function options.
    #[must_use]
    pub fn options(&self) -> super::FunctionOptions {
        self.inner.options()
    }
    /// Returns patch control-point count.
    #[must_use]
    pub fn patch_control_point_count(&self) -> isize {
        self.inner.patch_control_point_count()
    }
    /// Returns patch type.
    #[must_use]
    pub fn patch_type(&self) -> super::PatchType {
        self.inner.patch_type()
    }
    /// Returns the declared function-constant names.
    #[must_use]
    pub fn function_constant_names(&self) -> Vec<String> {
        self.inner.function_constant_names()
    }
    /// Returns the declared vertex-attribute names.
    #[must_use]
    pub fn vertex_attribute_names(&self) -> Vec<String> {
        self.inner.vertex_attribute_names()
    }
    /// Returns the declared stage-input attribute names.
    #[must_use]
    pub fn stage_input_attribute_names(&self) -> Vec<String> {
        self.inner.stage_input_attribute_names()
    }
}

/// Options for compiling Metal Shading Language.
pub struct CompileOptions {
    pub(crate) inner: metal_rust_ffi::CompileOptions,
}

impl CompileOptions {
    pub(crate) const fn from_ffi(inner: metal_rust_ffi::CompileOptions) -> Self {
        Self { inner }
    }

    /// Creates compiler options using Metal defaults.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: metal_rust_ffi::CompileOptions::new(),
        }
    }

    /// Creates strict, non-fast-math compiler options.
    #[must_use]
    pub fn strict() -> Self {
        Self {
            inner: metal_rust_ffi::CompileOptions::strict(),
        }
    }

    /// Returns and sets fast-math behavior.
    #[must_use]
    pub fn fast_math_enabled(&self) -> bool {
        self.inner.fast_math_enabled()
    }
    /// Sets fast-math behavior.
    pub fn set_fast_math_enabled(&self, value: bool) {
        self.inner.set_fast_math_enabled(value);
    }
    /// Returns undefined-symbol reference behavior.
    #[must_use]
    pub fn allows_referencing_undefined_symbols(&self) -> bool {
        self.inner.allows_referencing_undefined_symbols()
    }
    /// Sets undefined-symbol reference behavior.
    pub fn set_allows_referencing_undefined_symbols(&self, value: bool) {
        self.inner.set_allows_referencing_undefined_symbols(value);
    }
    /// Returns whether compiler logging is enabled.
    #[must_use]
    pub fn logging_enabled(&self) -> bool {
        self.inner.logging_enabled()
    }
    /// Sets compiler logging.
    pub fn set_logging_enabled(&self, value: bool) {
        self.inner.set_logging_enabled(value);
    }
    /// Returns invariance preservation behavior.
    #[must_use]
    pub fn preserves_invariance(&self) -> bool {
        self.inner.preserves_invariance()
    }
    /// Sets invariance preservation behavior.
    pub fn set_preserves_invariance(&self, value: bool) {
        self.inner.set_preserves_invariance(value);
    }
    /// Returns the optional install name.
    #[must_use]
    pub fn install_name(&self) -> Option<String> {
        self.inner.install_name()
    }
    /// Sets or clears the install name.
    pub fn set_install_name(&self, value: Option<&str>) {
        self.inner.set_install_name(value);
    }
    /// Returns the maximum total threads per threadgroup.
    #[must_use]
    pub fn max_total_threads_per_threadgroup(&self) -> usize {
        self.inner.max_total_threads_per_threadgroup()
    }
    /// Sets a non-zero maximum total thread count.
    pub fn set_max_total_threads_per_threadgroup(&self, value: usize) -> Result<(), Error> {
        self.inner
            .set_max_total_threads_per_threadgroup(value)
            .map_err(Error::from_ffi)
    }
    /// Returns required threadgroup dimensions.
    #[must_use]
    pub fn required_threads_per_threadgroup(&self) -> super::Size {
        self.inner.required_threads_per_threadgroup()
    }
    /// Sets non-zero required threadgroup dimensions.
    pub fn set_required_threads_per_threadgroup(&self, value: super::Size) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_threadgroup(value)
            .map_err(Error::from_ffi)
    }
    /// Returns the math mode.
    #[must_use]
    pub fn math_mode(&self) -> super::MathMode {
        self.inner.math_mode()
    }
    /// Sets the math mode.
    pub fn set_math_mode(&self, value: super::MathMode) {
        self.inner.set_math_mode(value);
    }
    /// Returns floating-point function options.
    #[must_use]
    pub fn math_floating_point_functions(&self) -> super::MathFloatingPointFunctions {
        self.inner.math_floating_point_functions()
    }
    /// Sets floating-point function options.
    pub fn set_math_floating_point_functions(&self, value: super::MathFloatingPointFunctions) {
        self.inner.set_math_floating_point_functions(value);
    }
    /// Returns the Metal language version.
    #[must_use]
    pub fn language_version(&self) -> super::LanguageVersion {
        self.inner.language_version()
    }
    /// Sets the Metal language version.
    pub fn set_language_version(&self, value: super::LanguageVersion) {
        self.inner.set_language_version(value);
    }
    /// Returns the library type.
    #[must_use]
    pub fn library_type(&self) -> super::LibraryType {
        self.inner.library_type()
    }
    /// Sets the library type.
    pub fn set_library_type(&self, value: super::LibraryType) {
        self.inner.set_library_type(value);
    }
    /// Returns the optimization level.
    #[must_use]
    pub fn optimization_level(&self) -> super::LibraryOptimizationLevel {
        self.inner.optimization_level()
    }
    /// Sets the optimization level.
    pub fn set_optimization_level(&self, value: super::LibraryOptimizationLevel) {
        self.inner.set_optimization_level(value);
    }
    /// Returns compile-symbol visibility.
    #[must_use]
    pub fn compile_symbol_visibility(&self) -> super::CompileSymbolVisibility {
        self.inner.compile_symbol_visibility()
    }
    /// Sets compile-symbol visibility.
    pub fn set_compile_symbol_visibility(&self, value: super::CompileSymbolVisibility) {
        self.inner.set_compile_symbol_visibility(value);
    }
    /// Returns dependent dynamic libraries as owned wrappers.
    pub fn libraries(&self) -> Result<Vec<super::DynamicLibrary>, Error> {
        self.inner
            .libraries()
            .map(|values| {
                values
                    .into_iter()
                    .map(super::DynamicLibrary::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }
    /// Sets or clears dependent dynamic libraries from a Rust slice.
    pub fn set_libraries(&self, libraries: Option<&[super::DynamicLibrary]>) -> Result<(), Error> {
        let libraries: Option<Vec<metal_rust_ffi::__private::objects::metal::DynamicLibrary>> =
            libraries.map(|values| values.iter().map(|value| value.inner.clone()).collect());
        self.inner
            .set_libraries(libraries.as_deref())
            .map_err(Error::from_ffi)
    }
    /// Returns preprocessor macros as owned Rust strings.
    pub fn preprocessor_macros(&self) -> Result<std::collections::HashMap<String, String>, Error> {
        self.inner.preprocessor_macros().map_err(Error::from_ffi)
    }
    /// Sets or clears preprocessor macros from Rust strings.
    pub fn set_preprocessor_macros(
        &self,
        macros: Option<&std::collections::HashMap<String, String>>,
    ) -> Result<(), Error> {
        self.inner
            .set_preprocessor_macros(macros)
            .map_err(Error::from_ffi)
    }
    /// Returns floating-point conversion rounding when available.
    pub fn floating_point_conversion_rounding_mode(
        &self,
    ) -> Result<super::FloatingPointConversionRoundingMode, Error> {
        self.inner
            .floating_point_conversion_rounding_mode()
            .map_err(Error::from_ffi)
    }
    /// Sets floating-point conversion rounding when available.
    pub fn set_floating_point_conversion_rounding_mode(
        &self,
        value: super::FloatingPointConversionRoundingMode,
    ) -> Result<(), Error> {
        self.inner
            .set_floating_point_conversion_rounding_mode(value)
            .map_err(Error::from_ffi)
    }
}

impl Default for CompileOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A compiled render pipeline state.
#[derive(Clone)]
pub struct RenderPipelineState {
    pub(crate) inner: metal_rust_ffi::RenderPipelineState,
}

impl RenderPipelineState {
    /// Returns the maximum threadgroup size reported by Metal.
    #[must_use]
    pub fn max_total_threads_per_threadgroup(&self) -> usize {
        self.inner.max_total_threads_per_threadgroup()
    }
    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> super::Device {
        super::Device::from_ffi(self.inner.device())
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }
    /// Returns indirect-command-buffer support.
    #[must_use]
    pub fn supports_indirect_command_buffers(&self) -> bool {
        self.inner.supports_indirect_command_buffers()
    }
    /// Returns whether tile threadgroup size matches tile size.
    #[must_use]
    pub fn threadgroup_size_matches_tile_size(&self) -> bool {
        self.inner.threadgroup_size_matches_tile_size()
    }
    /// Returns imageblock sample length.
    #[must_use]
    pub fn imageblock_sample_length(&self) -> usize {
        self.inner.imageblock_sample_length()
    }
    /// Returns shader validation when available.
    pub fn shader_validation(&self) -> Result<super::ShaderValidation, Error> {
        self.inner.shader_validation().map_err(Error::from_ffi)
    }
    /// Returns tile, mesh, and object required threadgroup sizes.
    #[must_use]
    pub fn required_threadgroups(&self) -> (super::Size, super::Size, super::Size) {
        self.inner.required_threadgroups()
    }
    /// Returns imageblock memory length for validated dimensions.
    pub fn imageblock_memory_length(&self, dimensions: super::Size) -> Result<usize, Error> {
        self.inner
            .imageblock_memory_length(dimensions)
            .map_err(Error::from_ffi)
    }
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<u64, Error> {
        self.inner.gpu_resource_id().map_err(Error::from_ffi)
    }
    /// Returns the maximum object-shader threadgroup size when supported.
    pub fn max_total_threads_per_object_threadgroup(&self) -> Result<usize, Error> {
        self.inner
            .max_total_threads_per_object_threadgroup()
            .map_err(Error::from_ffi)
    }
    /// Returns the maximum mesh-shader threadgroup size when supported.
    pub fn max_total_threads_per_mesh_threadgroup(&self) -> Result<usize, Error> {
        self.inner
            .max_total_threads_per_mesh_threadgroup()
            .map_err(Error::from_ffi)
    }
    /// Returns the object-shader SIMD width when supported.
    pub fn object_thread_execution_width(&self) -> Result<usize, Error> {
        self.inner
            .object_thread_execution_width()
            .map_err(Error::from_ffi)
    }
    /// Returns the mesh-shader SIMD width when supported.
    pub fn mesh_thread_execution_width(&self) -> Result<usize, Error> {
        self.inner
            .mesh_thread_execution_width()
            .map_err(Error::from_ffi)
    }
    /// Returns the maximum threadgroup count in a mesh grid.
    pub fn max_total_threadgroups_per_mesh_grid(&self) -> Result<usize, Error> {
        self.inner
            .max_total_threadgroups_per_mesh_grid()
            .map_err(Error::from_ffi)
    }
    /// Returns reflection data when retained during pipeline creation.
    pub fn reflection(&self) -> Result<Option<super::RenderPipelineReflection>, Error> {
        self.inner
            .reflection()
            .map(|value| value.map(super::RenderPipelineReflection::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Looks up a linked function handle by name and render stage.
    pub fn function_handle_by_name(
        &self,
        name: &str,
        stage: super::RenderStages,
    ) -> Result<Option<super::FunctionHandle>, Error> {
        self.inner
            .function_handle_by_name(name, stage)
            .map(|value| value.map(super::FunctionHandle::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Looks up a linked handle using a canonical Metal function's name.
    pub fn function_handle_for_function(
        &self,
        function: &Function,
        stage: super::RenderStages,
    ) -> Result<Option<super::FunctionHandle>, Error> {
        self.inner
            .function_handle_for_function(&function.inner, stage)
            .map(|value| value.map(super::FunctionHandle::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Looks up a linked Metal 4 binary function by name.
    pub fn function_handle_for_binary_function(
        &self,
        function: &crate::metal4::BinaryFunction,
        stage: super::RenderStages,
    ) -> Result<Option<super::FunctionHandle>, Error> {
        self.inner
            .function_handle_for_binary_function(&function.inner, stage)
            .map(|value| value.map(super::FunctionHandle::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Creates a visible function table for a render stage.
    pub fn new_visible_function_table(
        &self,
        descriptor: &super::VisibleFunctionTableDescriptor,
        stage: super::RenderStages,
    ) -> Result<Option<super::VisibleFunctionTable>, Error> {
        self.inner
            .new_visible_function_table(&descriptor.inner, stage)
            .map(|value| value.map(super::VisibleFunctionTable::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Creates an intersection function table for a render stage.
    pub fn new_intersection_function_table(
        &self,
        descriptor: &super::IntersectionFunctionTableDescriptor,
        stage: super::RenderStages,
    ) -> Result<Option<super::IntersectionFunctionTable>, Error> {
        self.inner
            .new_intersection_function_table(&descriptor.inner, stage)
            .map(|value| value.map(super::IntersectionFunctionTable::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Returns a Metal 4 descriptor for pipeline specialization.
    pub fn new_render_pipeline_descriptor(
        &self,
    ) -> Result<crate::metal4::PipelineDescriptor, Error> {
        self.inner
            .new_render_pipeline_descriptor()
            .map(crate::metal4::PipelineDescriptor::from_ffi)
            .map_err(Error::from_ffi)
    }
    /// Creates a derived pipeline from Metal 4 binary functions.
    pub fn with_metal4_binary_functions(
        &self,
        descriptor: &crate::metal4::RenderPipelineBinaryFunctionsDescriptor,
    ) -> Result<Self, Error> {
        self.inner
            .with_metal4_binary_functions(&descriptor.inner)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }
    /// Creates a derived pipeline with additional binary functions.
    pub fn with_additional_binary_functions(
        &self,
        descriptor: &super::RenderPipelineFunctionsDescriptor,
    ) -> Result<Self, Error> {
        self.inner
            .with_additional_binary_functions(&descriptor.inner)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }
}

/// A compiled compute pipeline state.
#[derive(Clone)]
pub struct ComputePipelineState {
    pub(crate) inner: metal_rust_ffi::ComputePipelineState,
}

impl ComputePipelineState {
    /// Returns the execution width reported by Metal.
    #[must_use]
    pub fn execution_width(&self) -> usize {
        self.inner.execution_width()
    }

    /// Returns the maximum threadgroup size reported by Metal.
    #[must_use]
    pub fn max_total_threads_per_threadgroup(&self) -> usize {
        self.inner.max_total_threads_per_threadgroup()
    }
    /// Returns the owning device.
    #[must_use]
    pub fn device(&self) -> super::Device {
        super::Device::from_ffi(self.inner.device())
    }
    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }
    /// Returns indirect-command-buffer support.
    #[must_use]
    pub fn supports_indirect_command_buffers(&self) -> bool {
        self.inner.supports_indirect_command_buffers()
    }
    /// Returns static threadgroup memory length.
    #[must_use]
    pub fn static_threadgroup_memory_length(&self) -> usize {
        self.inner.static_threadgroup_memory_length()
    }
    /// Returns required threads per threadgroup.
    #[must_use]
    pub fn required_threads_per_threadgroup(&self) -> super::Size {
        self.inner.required_threads_per_threadgroup()
    }
    /// Returns shader validation when available.
    pub fn shader_validation(&self) -> Result<super::ShaderValidation, Error> {
        self.inner.shader_validation().map_err(Error::from_ffi)
    }
    /// Returns imageblock memory length for validated dimensions.
    pub fn imageblock_memory_length(&self, dimensions: super::Size) -> Result<usize, Error> {
        self.inner
            .imageblock_memory_length(dimensions)
            .map_err(Error::from_ffi)
    }
    /// Returns the opaque GPU resource identifier.
    pub fn gpu_resource_id(&self) -> Result<u64, Error> {
        self.inner.gpu_resource_id().map_err(Error::from_ffi)
    }
    /// Returns reflection data when retained during pipeline creation.
    pub fn reflection(&self) -> Result<Option<super::ComputePipelineReflection>, Error> {
        self.inner
            .reflection()
            .map(|value| value.map(super::ComputePipelineReflection::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Looks up a linked function handle by name.
    pub fn function_handle_by_name(
        &self,
        name: &str,
    ) -> Result<Option<super::FunctionHandle>, Error> {
        self.inner
            .function_handle_by_name(name)
            .map(|value| value.map(super::FunctionHandle::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Looks up a linked handle using a canonical Metal function's name.
    pub fn function_handle_for_function(
        &self,
        function: &Function,
    ) -> Result<Option<super::FunctionHandle>, Error> {
        self.inner
            .function_handle_for_function(&function.inner)
            .map(|value| value.map(super::FunctionHandle::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Looks up a linked Metal 4 binary function by name.
    pub fn function_handle_for_binary_function(
        &self,
        function: &crate::metal4::BinaryFunction,
    ) -> Result<Option<super::FunctionHandle>, Error> {
        self.inner
            .function_handle_for_binary_function(&function.inner)
            .map(|value| value.map(super::FunctionHandle::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Creates a visible function table.
    pub fn new_visible_function_table(
        &self,
        descriptor: &super::VisibleFunctionTableDescriptor,
    ) -> Result<Option<super::VisibleFunctionTable>, Error> {
        self.inner
            .new_visible_function_table(&descriptor.inner)
            .map(|value| value.map(super::VisibleFunctionTable::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Creates an intersection function table.
    pub fn new_intersection_function_table(
        &self,
        descriptor: &super::IntersectionFunctionTableDescriptor,
    ) -> Result<Option<super::IntersectionFunctionTable>, Error> {
        self.inner
            .new_intersection_function_table(&descriptor.inner)
            .map(|value| value.map(super::IntersectionFunctionTable::from_ffi))
            .map_err(Error::from_ffi)
    }
    /// Creates a derived compute pipeline from Metal 4 binary functions.
    pub fn with_metal4_binary_functions(
        &self,
        functions: &[crate::metal4::BinaryFunction],
    ) -> Result<Self, Error> {
        let functions: Vec<metal_rust_ffi::__private::objects::metal4::BinaryFunction> =
            functions.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .with_metal4_binary_functions(&functions)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }
    /// Creates a derived pipeline with additional Metal functions.
    pub fn with_additional_functions(&self, functions: &[Function]) -> Result<Self, Error> {
        let functions: Vec<metal_rust_ffi::Function> =
            functions.iter().map(|value| value.inner.clone()).collect();
        self.inner
            .with_additional_functions(&functions)
            .map(|inner| Self { inner })
            .map_err(Error::from_ffi)
    }
}

/// A render pipeline descriptor.
pub struct RenderPipelineDescriptor {
    pub(crate) inner: metal_rust_ffi::RenderPipelineDescriptor,
}

impl RenderPipelineDescriptor {
    /// Creates a descriptor using Metal defaults.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            inner: metal_rust_ffi::RenderPipelineDescriptor::empty(),
        }
    }

    /// Creates a descriptor from vertex/fragment functions and a color format.
    #[must_use]
    pub fn new(vertex: &Function, fragment: Option<&Function>, color_format: PixelFormat) -> Self {
        Self {
            inner: metal_rust_ffi::RenderPipelineDescriptor::new(
                &vertex.inner,
                fragment.map(|value| &value.inner),
                color_format,
            ),
        }
    }

    /// Returns the optional label.
    #[must_use]
    pub fn label(&self) -> Option<String> {
        self.inner.label()
    }

    /// Sets or clears the label.
    pub fn set_label(&self, value: Option<&str>) {
        self.inner.set_label(value);
    }

    /// Returns the assigned vertex function.
    #[must_use]
    pub fn vertex_function(&self) -> Option<Function> {
        self.inner.vertex_function().map(|inner| Function { inner })
    }

    /// Sets or clears the vertex function.
    pub fn set_vertex_function(&self, value: Option<&Function>) {
        self.inner
            .set_vertex_function(value.map(|function| &function.inner));
    }

    /// Returns the assigned fragment function.
    #[must_use]
    pub fn fragment_function(&self) -> Option<Function> {
        self.inner
            .fragment_function()
            .map(|inner| Function { inner })
    }

    /// Sets or clears the fragment function.
    pub fn set_fragment_function(&self, value: Option<&Function>) {
        self.inner
            .set_fragment_function(value.map(|function| &function.inner));
    }

    /// Resets the descriptor to Metal defaults.
    pub fn reset(&self) {
        self.inner.reset();
    }

    /// Returns all scalar descriptor options when the optional selectors exist.
    pub fn options(&self) -> Result<RenderPipelineOptions, Error> {
        self.inner.options().map_err(Error::from_ffi)
    }

    /// Applies all scalar options after validating their constraints.
    pub fn set_options(&self, value: &RenderPipelineOptions) -> Result<(), Error> {
        self.inner.set_options(value).map_err(Error::from_ffi)
    }
}

impl super::ComputePipelineDescriptor {
    /// Returns required compute threadgroup dimensions when available.
    pub fn required_threads_per_threadgroup(&self) -> Result<super::Size, Error> {
        self.inner
            .required_threads_per_threadgroup()
            .map_err(Error::from_ffi)
    }

    /// Sets required dimensions, or all zeros to disable the requirement.
    pub fn set_required_threads_per_threadgroup(&self, value: super::Size) -> Result<(), Error> {
        self.inner
            .set_required_threads_per_threadgroup(value)
            .map_err(Error::from_ffi)
    }

    /// Restores all descriptor properties to Metal defaults.
    pub fn reset(&self) -> Result<(), Error> {
        self.inner.reset().map_err(Error::from_ffi)
    }
}

impl super::FunctionConstantValues {
    /// Removes all previously assigned function constants.
    pub fn reset(&self) -> Result<(), Error> {
        self.inner.reset().map_err(Error::from_ffi)
    }

    /// Assigns one function constant by index from documented ABI bytes.
    pub fn set_constant_at_index(
        &self,
        data_type: super::DataType,
        index: usize,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.inner
            .set_constant_at_index(data_type, index, bytes)
            .map_err(Error::from_ffi)
    }

    /// Assigns one named function constant from documented ABI bytes.
    pub fn set_constant_named(
        &self,
        data_type: super::DataType,
        name: &str,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.inner
            .set_constant_named(data_type, name, bytes)
            .map_err(Error::from_ffi)
    }

    /// Assigns a contiguous range from packed ABI-sized values.
    pub fn set_constants(
        &self,
        data_type: super::DataType,
        range: std::ops::Range<usize>,
        bytes: &[u8],
    ) -> Result<(), Error> {
        self.inner
            .set_constants(data_type, range, bytes)
            .map_err(Error::from_ffi)
    }
}
