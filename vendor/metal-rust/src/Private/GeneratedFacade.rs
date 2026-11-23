//! Generated canonical facade wrappers.
//!
//! This module is private; framework modules re-export its safe types.

#![allow(non_camel_case_types)]
#![allow(dead_code)]

/// Safe generated implementation for the `foundation` canonical module.
pub(crate) mod foundation {
    /// Canonical safe structure `OperatingSystemVersion`.
    pub use metal_rust_ffi::__private::structs::OperatingSystemVersion;
    /// Canonical safe structure `Range`.
    pub use metal_rust_ffi::__private::structs::Range;
    /// Canonical safe value type `ActivityOptions`.
    pub use metal_rust_ffi::__private::values::ActivityOptions;
    /// Canonical safe value type `ComparisonResult`.
    pub use metal_rust_ffi::__private::values::ComparisonResult;
    /// Canonical safe value type `ProcessInfoThermalState`.
    pub use metal_rust_ffi::__private::values::ProcessInfoThermalState;
    /// Canonical safe value type `StringCompareOptions`.
    pub use metal_rust_ffi::__private::values::StringCompareOptions;
    /// Canonical safe value type `StringEncoding`.
    pub use metal_rust_ffi::__private::values::StringEncoding;
    /// Safe substitution for `NS::Integer`.
    pub type Integer = isize;
    /// Safe substitution for `NS::TimeInterval`.
    pub type TimeInterval = f64;
    /// Safe substitution for `NS::UInteger`.
    pub type UInteger = usize;
    /// Safe substitution for `NS::unichar`.
    pub type unichar = u16;
}

/// Safe generated implementation for the `metal` canonical module.
pub(crate) mod metal {
    /// Canonical safe structure `AccelerationStructureInstanceDescriptor`.
    pub use metal_rust_ffi::__private::structs::AccelerationStructureInstanceDescriptor;
    /// Canonical safe structure `AccelerationStructureMotionInstanceDescriptor`.
    pub use metal_rust_ffi::__private::structs::AccelerationStructureMotionInstanceDescriptor;
    /// Canonical safe structure `AccelerationStructureSizes`.
    pub use metal_rust_ffi::__private::structs::AccelerationStructureSizes;
    /// Canonical safe structure `AccelerationStructureUserIDInstanceDescriptor`.
    pub use metal_rust_ffi::__private::structs::AccelerationStructureUserIDInstanceDescriptor;
    /// Canonical safe structure `AxisAlignedBoundingBox`.
    pub use metal_rust_ffi::__private::structs::AxisAlignedBoundingBox;
    /// Canonical safe structure `ComponentTransform`.
    pub use metal_rust_ffi::__private::structs::ComponentTransform;
    /// Canonical safe structure `CounterResultStageUtilization`.
    pub use metal_rust_ffi::__private::structs::CounterResultStageUtilization;
    /// Canonical safe structure `CounterResultStatistic`.
    pub use metal_rust_ffi::__private::structs::CounterResultStatistic;
    /// Canonical safe structure `CounterResultTimestamp`.
    pub use metal_rust_ffi::__private::structs::CounterResultTimestamp;
    /// Canonical safe structure `DispatchThreadgroupsIndirectArguments`.
    pub use metal_rust_ffi::__private::structs::DispatchThreadgroupsIndirectArguments;
    /// Canonical safe structure `DispatchThreadsIndirectArguments`.
    pub use metal_rust_ffi::__private::structs::DispatchThreadsIndirectArguments;
    /// Canonical safe structure `DrawIndexedPrimitivesIndirectArguments`.
    pub use metal_rust_ffi::__private::structs::DrawIndexedPrimitivesIndirectArguments;
    /// Canonical safe structure `DrawPatchIndirectArguments`.
    pub use metal_rust_ffi::__private::structs::DrawPatchIndirectArguments;
    /// Canonical safe structure `DrawPrimitivesIndirectArguments`.
    pub use metal_rust_ffi::__private::structs::DrawPrimitivesIndirectArguments;
    /// Canonical safe structure `IndirectAccelerationStructureInstanceDescriptor`.
    pub use metal_rust_ffi::__private::structs::IndirectAccelerationStructureInstanceDescriptor;
    /// Canonical safe structure `IndirectAccelerationStructureMotionInstanceDescriptor`.
    pub use metal_rust_ffi::__private::structs::IndirectAccelerationStructureMotionInstanceDescriptor;
    /// Canonical safe structure `IndirectCommandBufferExecutionRange`.
    pub use metal_rust_ffi::__private::structs::IndirectCommandBufferExecutionRange;
    /// Canonical safe structure `IntersectionFunctionBufferArguments`.
    pub use metal_rust_ffi::__private::structs::IntersectionFunctionBufferArguments;
    /// Canonical safe structure `MapIndirectArguments`.
    pub use metal_rust_ffi::__private::structs::MapIndirectArguments;
    /// Canonical safe structure `PackedFloat3`.
    pub use metal_rust_ffi::__private::structs::PackedFloat3;
    /// Canonical safe structure `PackedFloat4x3`.
    pub use metal_rust_ffi::__private::structs::PackedFloat4x3;
    /// Canonical safe structure `PackedFloatQuaternion`.
    pub use metal_rust_ffi::__private::structs::PackedFloatQuaternion;
    /// Canonical safe structure `QuadTessellationFactorsHalf`.
    pub use metal_rust_ffi::__private::structs::QuadTessellationFactorsHalf;
    /// Canonical safe structure `ResourceID`.
    pub use metal_rust_ffi::__private::structs::ResourceID;
    /// Canonical safe structure `SamplePosition`.
    pub use metal_rust_ffi::__private::structs::SamplePosition;
    /// Canonical safe structure `ScissorRect`.
    pub use metal_rust_ffi::__private::structs::ScissorRect;
    /// Canonical safe structure `SizeAndAlign`.
    pub use metal_rust_ffi::__private::structs::SizeAndAlign;
    /// Canonical safe structure `StageInRegionIndirectArguments`.
    pub use metal_rust_ffi::__private::structs::StageInRegionIndirectArguments;
    /// Canonical safe structure `TextureSwizzleChannels`.
    pub use metal_rust_ffi::__private::structs::TextureSwizzleChannels;
    /// Canonical safe structure `TriangleTessellationFactorsHalf`.
    pub use metal_rust_ffi::__private::structs::TriangleTessellationFactorsHalf;
    /// Canonical safe structure `VertexAmplificationViewMapping`.
    pub use metal_rust_ffi::__private::structs::VertexAmplificationViewMapping;
    /// Canonical safe value type `AccelerationStructureInstanceDescriptorType`.
    pub use metal_rust_ffi::__private::values::AccelerationStructureInstanceDescriptorType;
    /// Canonical safe value type `AccelerationStructureInstanceOptions`.
    pub use metal_rust_ffi::__private::values::AccelerationStructureInstanceOptions;
    /// Canonical safe value type `AccelerationStructureRefitOptions`.
    pub use metal_rust_ffi::__private::values::AccelerationStructureRefitOptions;
    /// Canonical safe value type `AccelerationStructureUsage`.
    pub use metal_rust_ffi::__private::values::AccelerationStructureUsage;
    /// Canonical safe value type `ArgumentBuffersTier`.
    pub use metal_rust_ffi::__private::values::ArgumentBuffersTier;
    /// Canonical safe value type `ArgumentType`.
    pub use metal_rust_ffi::__private::values::ArgumentType;
    /// Canonical safe value type `AttributeFormat`.
    pub use metal_rust_ffi::__private::values::AttributeFormat;
    /// Canonical safe value type `BarrierScope`.
    pub use metal_rust_ffi::__private::values::BarrierScope;
    /// Canonical safe value type `BinaryArchiveError`.
    pub use metal_rust_ffi::__private::values::BinaryArchiveError;
    /// Canonical safe value type `BindingAccess`.
    pub use metal_rust_ffi::__private::values::BindingAccess;
    /// Canonical safe value type `BindingType`.
    pub use metal_rust_ffi::__private::values::BindingType;
    /// Canonical safe value type `BlendFactor`.
    pub use metal_rust_ffi::__private::values::BlendFactor;
    /// Canonical safe value type `BlendOperation`.
    pub use metal_rust_ffi::__private::values::BlendOperation;
    /// Canonical safe value type `BlitOption`.
    pub use metal_rust_ffi::__private::values::BlitOption;
    /// Canonical safe value type `BufferSparseTier`.
    pub use metal_rust_ffi::__private::values::BufferSparseTier;
    /// Canonical safe value type `CPUCacheMode`.
    pub use metal_rust_ffi::__private::values::CPUCacheMode;
    /// Canonical safe value type `CaptureDestination`.
    pub use metal_rust_ffi::__private::values::CaptureDestination;
    /// Canonical safe value type `CaptureError`.
    pub use metal_rust_ffi::__private::values::CaptureError;
    /// Canonical safe value type `ColorWriteMask`.
    pub use metal_rust_ffi::__private::values::ColorWriteMask;
    /// Canonical safe value type `CommandBufferError`.
    pub use metal_rust_ffi::__private::values::CommandBufferError;
    /// Canonical safe value type `CommandBufferErrorOption`.
    pub use metal_rust_ffi::__private::values::CommandBufferErrorOption;
    /// Canonical safe value type `CommandEncoderErrorState`.
    pub use metal_rust_ffi::__private::values::CommandEncoderErrorState;
    /// Canonical safe value type `CompareFunction`.
    pub use metal_rust_ffi::__private::values::CompareFunction;
    /// Canonical safe value type `CompileSymbolVisibility`.
    pub use metal_rust_ffi::__private::values::CompileSymbolVisibility;
    /// Canonical safe value type `CounterSampleBufferError`.
    pub use metal_rust_ffi::__private::values::CounterSampleBufferError;
    /// Canonical safe value type `CounterSamplingPoint`.
    pub use metal_rust_ffi::__private::values::CounterSamplingPoint;
    /// Canonical safe value type `CullMode`.
    pub use metal_rust_ffi::__private::values::CullMode;
    /// Canonical safe value type `CurveBasis`.
    pub use metal_rust_ffi::__private::values::CurveBasis;
    /// Canonical safe value type `CurveEndCaps`.
    pub use metal_rust_ffi::__private::values::CurveEndCaps;
    /// Canonical safe value type `CurveType`.
    pub use metal_rust_ffi::__private::values::CurveType;
    /// Canonical safe value type `DataType`.
    pub use metal_rust_ffi::__private::values::DataType;
    /// Canonical safe value type `DepthClipMode`.
    pub use metal_rust_ffi::__private::values::DepthClipMode;
    /// Canonical safe value type `DeviceError`.
    pub use metal_rust_ffi::__private::values::DeviceError;
    /// Canonical safe value type `DeviceLocation`.
    pub use metal_rust_ffi::__private::values::DeviceLocation;
    /// Canonical safe value type `DispatchType`.
    pub use metal_rust_ffi::__private::values::DispatchType;
    /// Canonical safe value type `DynamicLibraryError`.
    pub use metal_rust_ffi::__private::values::DynamicLibraryError;
    /// Canonical safe value type `FeatureSet`.
    pub use metal_rust_ffi::__private::values::FeatureSet;
    /// Canonical safe value type `FloatingPointConversionRoundingMode`.
    pub use metal_rust_ffi::__private::values::FloatingPointConversionRoundingMode;
    /// Canonical safe value type `FunctionLogType`.
    pub use metal_rust_ffi::__private::values::FunctionLogType;
    /// Canonical safe value type `FunctionOptions`.
    pub use metal_rust_ffi::__private::values::FunctionOptions;
    /// Canonical safe value type `FunctionType`.
    pub use metal_rust_ffi::__private::values::FunctionType;
    /// Canonical safe value type `GPUFamily`.
    pub use metal_rust_ffi::__private::values::GPUFamily;
    /// Canonical safe value type `HazardTrackingMode`.
    pub use metal_rust_ffi::__private::values::HazardTrackingMode;
    /// Canonical safe value type `HeapType`.
    pub use metal_rust_ffi::__private::values::HeapType;
    /// Canonical safe value type `IOCommandQueueType`.
    pub use metal_rust_ffi::__private::values::IOCommandQueueType;
    /// Canonical safe value type `IOCompressionMethod`.
    pub use metal_rust_ffi::__private::values::IOCompressionMethod;
    /// Canonical safe value type `IOCompressionStatus`.
    pub use metal_rust_ffi::__private::values::IOCompressionStatus;
    /// Canonical safe value type `IOError`.
    pub use metal_rust_ffi::__private::values::IOError;
    /// Canonical safe value type `IOPriority`.
    pub use metal_rust_ffi::__private::values::IOPriority;
    /// Canonical safe value type `IOStatus`.
    pub use metal_rust_ffi::__private::values::IOStatus;
    /// Canonical safe value type `IndexType`.
    pub use metal_rust_ffi::__private::values::IndexType;
    /// Canonical safe value type `IndirectCommandType`.
    pub use metal_rust_ffi::__private::values::IndirectCommandType;
    /// Canonical safe value type `IntersectionFunctionSignature`.
    pub use metal_rust_ffi::__private::values::IntersectionFunctionSignature;
    /// Canonical safe value type `LanguageVersion`.
    pub use metal_rust_ffi::__private::values::LanguageVersion;
    /// Canonical safe value type `LibraryError`.
    pub use metal_rust_ffi::__private::values::LibraryError;
    /// Canonical safe value type `LibraryOptimizationLevel`.
    pub use metal_rust_ffi::__private::values::LibraryOptimizationLevel;
    /// Canonical safe value type `LibraryType`.
    pub use metal_rust_ffi::__private::values::LibraryType;
    /// Canonical safe value type `LoadAction`.
    pub use metal_rust_ffi::__private::values::LoadAction;
    /// Canonical safe value type `LogLevel`.
    pub use metal_rust_ffi::__private::values::LogLevel;
    /// Canonical safe value type `LogStateError`.
    pub use metal_rust_ffi::__private::values::LogStateError;
    /// Canonical safe value type `MathFloatingPointFunctions`.
    pub use metal_rust_ffi::__private::values::MathFloatingPointFunctions;
    /// Canonical safe value type `MathMode`.
    pub use metal_rust_ffi::__private::values::MathMode;
    /// Canonical safe value type `MatrixLayout`.
    pub use metal_rust_ffi::__private::values::MatrixLayout;
    /// Canonical safe value type `MotionBorderMode`.
    pub use metal_rust_ffi::__private::values::MotionBorderMode;
    /// Canonical safe value type `MultisampleDepthResolveFilter`.
    pub use metal_rust_ffi::__private::values::MultisampleDepthResolveFilter;
    /// Canonical safe value type `MultisampleStencilResolveFilter`.
    pub use metal_rust_ffi::__private::values::MultisampleStencilResolveFilter;
    /// Canonical safe value type `Mutability`.
    pub use metal_rust_ffi::__private::values::Mutability;
    /// Canonical safe value type `PatchType`.
    pub use metal_rust_ffi::__private::values::PatchType;
    /// Canonical safe value type `PipelineOption`.
    pub use metal_rust_ffi::__private::values::PipelineOption;
    /// Canonical safe value type `PrimitiveTopologyClass`.
    pub use metal_rust_ffi::__private::values::PrimitiveTopologyClass;
    /// Canonical safe value type `PurgeableState`.
    pub use metal_rust_ffi::__private::values::PurgeableState;
    /// Canonical safe value type `ReadWriteTextureTier`.
    pub use metal_rust_ffi::__private::values::ReadWriteTextureTier;
    /// Canonical safe value type `RenderStages`.
    pub use metal_rust_ffi::__private::values::RenderStages;
    /// Canonical safe value type `ResourceUsage`.
    pub use metal_rust_ffi::__private::values::ResourceUsage;
    /// Canonical safe value type `SamplerAddressMode`.
    pub use metal_rust_ffi::__private::values::SamplerAddressMode;
    /// Canonical safe value type `SamplerBorderColor`.
    pub use metal_rust_ffi::__private::values::SamplerBorderColor;
    /// Canonical safe value type `SamplerMinMagFilter`.
    pub use metal_rust_ffi::__private::values::SamplerMinMagFilter;
    /// Canonical safe value type `SamplerMipFilter`.
    pub use metal_rust_ffi::__private::values::SamplerMipFilter;
    /// Canonical safe value type `SamplerReductionMode`.
    pub use metal_rust_ffi::__private::values::SamplerReductionMode;
    /// Canonical safe value type `ShaderValidation`.
    pub use metal_rust_ffi::__private::values::ShaderValidation;
    /// Canonical safe value type `SparsePageSize`.
    pub use metal_rust_ffi::__private::values::SparsePageSize;
    /// Canonical safe value type `SparseTextureMappingMode`.
    pub use metal_rust_ffi::__private::values::SparseTextureMappingMode;
    /// Canonical safe value type `SparseTextureRegionAlignmentMode`.
    pub use metal_rust_ffi::__private::values::SparseTextureRegionAlignmentMode;
    /// Canonical safe value type `Stages`.
    pub use metal_rust_ffi::__private::values::Stages;
    /// Canonical safe value type `StencilOperation`.
    pub use metal_rust_ffi::__private::values::StencilOperation;
    /// Canonical safe value type `StepFunction`.
    pub use metal_rust_ffi::__private::values::StepFunction;
    /// Canonical safe value type `StitchedLibraryOptions`.
    pub use metal_rust_ffi::__private::values::StitchedLibraryOptions;
    /// Canonical safe value type `StoreAction`.
    pub use metal_rust_ffi::__private::values::StoreAction;
    /// Canonical safe value type `StoreActionOptions`.
    pub use metal_rust_ffi::__private::values::StoreActionOptions;
    /// Canonical safe value type `TensorDataType`.
    pub use metal_rust_ffi::__private::values::TensorDataType;
    /// Canonical safe value type `TensorError`.
    pub use metal_rust_ffi::__private::values::TensorError;
    /// Canonical safe value type `TensorPlaneType`.
    pub use metal_rust_ffi::__private::values::TensorPlaneType;
    /// Canonical safe value type `TensorUsage`.
    pub use metal_rust_ffi::__private::values::TensorUsage;
    /// Canonical safe value type `TessellationControlPointIndexType`.
    pub use metal_rust_ffi::__private::values::TessellationControlPointIndexType;
    /// Canonical safe value type `TessellationFactorFormat`.
    pub use metal_rust_ffi::__private::values::TessellationFactorFormat;
    /// Canonical safe value type `TessellationFactorStepFunction`.
    pub use metal_rust_ffi::__private::values::TessellationFactorStepFunction;
    /// Canonical safe value type `TessellationPartitionMode`.
    pub use metal_rust_ffi::__private::values::TessellationPartitionMode;
    /// Canonical safe value type `TextureCompressionType`.
    pub use metal_rust_ffi::__private::values::TextureCompressionType;
    /// Canonical safe value type `TextureSparseTier`.
    pub use metal_rust_ffi::__private::values::TextureSparseTier;
    /// Canonical safe value type `TextureSwizzle`.
    pub use metal_rust_ffi::__private::values::TextureSwizzle;
    /// Canonical safe value type `TransformType`.
    pub use metal_rust_ffi::__private::values::TransformType;
    /// Canonical safe value type `TriangleFillMode`.
    pub use metal_rust_ffi::__private::values::TriangleFillMode;
    /// Canonical safe value type `VertexFormat`.
    pub use metal_rust_ffi::__private::values::VertexFormat;
    /// Canonical safe value type `VertexStepFunction`.
    pub use metal_rust_ffi::__private::values::VertexStepFunction;
    /// Canonical safe value type `VisibilityResultMode`.
    pub use metal_rust_ffi::__private::values::VisibilityResultMode;
    /// Canonical safe value type `VisibilityResultType`.
    pub use metal_rust_ffi::__private::values::VisibilityResultType;
    /// Canonical safe value type `Winding`.
    pub use metal_rust_ffi::__private::values::Winding;
    /// Safe substitution for `MTL::Coordinate2D`.
    pub type Coordinate2D = crate::metal::SamplePosition;
    /// Safe substitution for `MTL::GPUAddress`.
    pub type GPUAddress = u64;
    /// Safe substitution for `MTL::Timestamp`.
    pub type Timestamp = u64;
    /// Safe owned facade for `MTL::AccelerationStructure`.
    #[derive(Clone)]
    pub struct AccelerationStructure {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructure,
    }

    impl AccelerationStructure {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructure,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `size` property with availability checks.
        pub fn size(&self) -> Result<usize, crate::Error> {
            self.inner.size().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureBoundingBoxGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureBoundingBoxGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureBoundingBoxGeometryDescriptor,
    }

    impl AccelerationStructureBoundingBoxGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureBoundingBoxGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureBoundingBoxGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureBoundingBoxGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_buffer_offset` property with availability checks.
        pub fn bounding_box_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_buffer_offset` property after validation.
        pub fn set_bounding_box_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_count` property with availability checks.
        pub fn bounding_box_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_count` property after validation.
        pub fn set_bounding_box_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_stride` property with availability checks.
        pub fn bounding_box_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_stride` property after validation.
        pub fn set_bounding_box_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `bounding_box_buffer` object property.
        pub fn bounding_box_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .bounding_box_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_buffer` object property.
        pub fn set_bounding_box_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureCommandEncoder`.
    #[derive(Clone)]
    pub struct AccelerationStructureCommandEncoder {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::AccelerationStructureCommandEncoder,
    }

    impl AccelerationStructureCommandEncoder {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureCommandEncoder,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureCurveGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureCurveGeometryDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::AccelerationStructureCurveGeometryDescriptor,
    }

    impl AccelerationStructureCurveGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureCurveGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureCurveGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureCurveGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_buffer_offset` property with availability checks.
        pub fn control_point_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_buffer_offset` property after validation.
        pub fn set_control_point_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_count` property with availability checks.
        pub fn control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_count` property after validation.
        pub fn set_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_stride` property with availability checks.
        pub fn control_point_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_stride` property after validation.
        pub fn set_control_point_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_buffer_offset` property with availability checks.
        pub fn index_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .index_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer_offset` property after validation.
        pub fn set_index_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_buffer_offset` property with availability checks.
        pub fn radius_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .radius_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_buffer_offset` property after validation.
        pub fn set_radius_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_radius_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_stride` property with availability checks.
        pub fn radius_stride(&self) -> Result<usize, crate::Error> {
            self.inner.radius_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_stride` property after validation.
        pub fn set_radius_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_radius_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_control_point_count` property with availability checks.
        pub fn segment_control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .segment_control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_control_point_count` property after validation.
        pub fn set_segment_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_count` property with availability checks.
        pub fn segment_count(&self) -> Result<usize, crate::Error> {
            self.inner.segment_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_count` property after validation.
        pub fn set_segment_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_format` property with availability checks.
        pub fn control_point_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner
                .control_point_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_format` property after validation.
        pub fn set_control_point_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_basis` property with availability checks.
        pub fn curve_basis(&self) -> Result<crate::metal::CurveBasis, crate::Error> {
            self.inner.curve_basis().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_basis` property after validation.
        pub fn set_curve_basis(&self, value: crate::metal::CurveBasis) -> Result<(), crate::Error> {
            self.inner
                .set_curve_basis(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_end_caps` property with availability checks.
        pub fn curve_end_caps(&self) -> Result<crate::metal::CurveEndCaps, crate::Error> {
            self.inner.curve_end_caps().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_end_caps` property after validation.
        pub fn set_curve_end_caps(
            &self,
            value: crate::metal::CurveEndCaps,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_curve_end_caps(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_type` property with availability checks.
        pub fn curve_type(&self) -> Result<crate::metal::CurveType, crate::Error> {
            self.inner.curve_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_type` property after validation.
        pub fn set_curve_type(&self, value: crate::metal::CurveType) -> Result<(), crate::Error> {
            self.inner
                .set_curve_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_format` property with availability checks.
        pub fn radius_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.radius_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_format` property after validation.
        pub fn set_radius_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_radius_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `control_point_buffer` object property.
        pub fn control_point_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .control_point_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_buffer` object property.
        pub fn set_control_point_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `index_buffer` object property.
        pub fn index_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .index_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer` object property.
        pub fn set_index_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `radius_buffer` object property.
        pub fn radius_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .radius_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_buffer` object property.
        pub fn set_radius_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_radius_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::AccelerationStructureDescriptor,
    }

    impl AccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `usage` property with availability checks.
        pub fn usage(&self) -> Result<crate::metal::AccelerationStructureUsage, crate::Error> {
            self.inner.usage().map_err(crate::Error::from_ffi)
        }
        /// Writes the `usage` property after validation.
        pub fn set_usage(
            &self,
            value: crate::metal::AccelerationStructureUsage,
        ) -> Result<(), crate::Error> {
            self.inner.set_usage(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureGeometryDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::AccelerationStructureGeometryDescriptor,
    }

    impl AccelerationStructureGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureGeometryDescriptor::new(
            )
            .map(Self::from_ffi)
            .map_err(crate::Error::from_ffi)
        }
        /// Reads the `allow_duplicate_intersection_function_invocation` property with availability checks.
        pub fn allow_duplicate_intersection_function_invocation(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .allow_duplicate_intersection_function_invocation()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `intersection_function_table_offset` property with availability checks.
        pub fn intersection_function_table_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .intersection_function_table_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `intersection_function_table_offset` property after validation.
        pub fn set_intersection_function_table_offset(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_intersection_function_table_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `opaque` property with availability checks.
        pub fn opaque(&self) -> Result<bool, crate::Error> {
            self.inner.opaque().map_err(crate::Error::from_ffi)
        }
        /// Reads the `primitive_data_buffer_offset` property with availability checks.
        pub fn primitive_data_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .primitive_data_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `primitive_data_buffer_offset` property after validation.
        pub fn set_primitive_data_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_primitive_data_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `primitive_data_element_size` property with availability checks.
        pub fn primitive_data_element_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .primitive_data_element_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `primitive_data_element_size` property after validation.
        pub fn set_primitive_data_element_size(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_primitive_data_element_size(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `primitive_data_stride` property with availability checks.
        pub fn primitive_data_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .primitive_data_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `primitive_data_stride` property after validation.
        pub fn set_primitive_data_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_primitive_data_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `primitive_data_buffer` object property.
        pub fn primitive_data_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .primitive_data_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `primitive_data_buffer` object property.
        pub fn set_primitive_data_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_primitive_data_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_allow_duplicate_intersection_function_invocation(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_allow_duplicate_intersection_function_invocation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_opaque(&self, value: bool) -> Result<(), crate::Error> {
            self.inner.set_opaque(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureMotionBoundingBoxGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureMotionBoundingBoxGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    }

    impl AccelerationStructureMotionBoundingBoxGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionBoundingBoxGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureMotionBoundingBoxGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionBoundingBoxGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_count` property with availability checks.
        pub fn bounding_box_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_count` property after validation.
        pub fn set_bounding_box_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_stride` property with availability checks.
        pub fn bounding_box_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_stride` property after validation.
        pub fn set_bounding_box_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_stride(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureMotionCurveGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureMotionCurveGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionCurveGeometryDescriptor,
    }

    impl AccelerationStructureMotionCurveGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionCurveGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureMotionCurveGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionCurveGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_count` property with availability checks.
        pub fn control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_count` property after validation.
        pub fn set_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_stride` property with availability checks.
        pub fn control_point_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_stride` property after validation.
        pub fn set_control_point_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_buffer_offset` property with availability checks.
        pub fn index_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .index_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer_offset` property after validation.
        pub fn set_index_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_stride` property with availability checks.
        pub fn radius_stride(&self) -> Result<usize, crate::Error> {
            self.inner.radius_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_stride` property after validation.
        pub fn set_radius_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_radius_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_control_point_count` property with availability checks.
        pub fn segment_control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .segment_control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_control_point_count` property after validation.
        pub fn set_segment_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_count` property with availability checks.
        pub fn segment_count(&self) -> Result<usize, crate::Error> {
            self.inner.segment_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_count` property after validation.
        pub fn set_segment_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_format` property with availability checks.
        pub fn control_point_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner
                .control_point_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_format` property after validation.
        pub fn set_control_point_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_basis` property with availability checks.
        pub fn curve_basis(&self) -> Result<crate::metal::CurveBasis, crate::Error> {
            self.inner.curve_basis().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_basis` property after validation.
        pub fn set_curve_basis(&self, value: crate::metal::CurveBasis) -> Result<(), crate::Error> {
            self.inner
                .set_curve_basis(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_end_caps` property with availability checks.
        pub fn curve_end_caps(&self) -> Result<crate::metal::CurveEndCaps, crate::Error> {
            self.inner.curve_end_caps().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_end_caps` property after validation.
        pub fn set_curve_end_caps(
            &self,
            value: crate::metal::CurveEndCaps,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_curve_end_caps(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_type` property with availability checks.
        pub fn curve_type(&self) -> Result<crate::metal::CurveType, crate::Error> {
            self.inner.curve_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_type` property after validation.
        pub fn set_curve_type(&self, value: crate::metal::CurveType) -> Result<(), crate::Error> {
            self.inner
                .set_curve_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_format` property with availability checks.
        pub fn radius_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.radius_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_format` property after validation.
        pub fn set_radius_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_radius_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `index_buffer` object property.
        pub fn index_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .index_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer` object property.
        pub fn set_index_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureMotionTriangleGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureMotionTriangleGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionTriangleGeometryDescriptor,
    }

    impl AccelerationStructureMotionTriangleGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionTriangleGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureMotionTriangleGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureMotionTriangleGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_buffer_offset` property with availability checks.
        pub fn index_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .index_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer_offset` property after validation.
        pub fn set_index_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transformation_matrix_buffer_offset` property with availability checks.
        pub fn transformation_matrix_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .transformation_matrix_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_buffer_offset` property after validation.
        pub fn set_transformation_matrix_buffer_offset(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `triangle_count` property with availability checks.
        pub fn triangle_count(&self) -> Result<usize, crate::Error> {
            self.inner.triangle_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `triangle_count` property after validation.
        pub fn set_triangle_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_triangle_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_stride` property with availability checks.
        pub fn vertex_stride(&self) -> Result<usize, crate::Error> {
            self.inner.vertex_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_stride` property after validation.
        pub fn set_vertex_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transformation_matrix_layout` property with availability checks.
        pub fn transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_layout` property after validation.
        pub fn set_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_format` property with availability checks.
        pub fn vertex_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.vertex_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_format` property after validation.
        pub fn set_vertex_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `index_buffer` object property.
        pub fn index_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .index_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer` object property.
        pub fn set_index_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `transformation_matrix_buffer` object property.
        pub fn transformation_matrix_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .transformation_matrix_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_buffer` object property.
        pub fn set_transformation_matrix_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructurePassDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructurePassDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::AccelerationStructurePassDescriptor,
    }

    impl AccelerationStructurePassDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructurePassDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructurePassDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructurePassDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer_attachments` object property.
        pub fn sample_buffer_attachments(
            &self,
        ) -> Result<
            Option<crate::metal::AccelerationStructurePassSampleBufferAttachmentDescriptorArray>,
            crate::Error,
        > {
            self.inner.sample_buffer_attachments()
                .map(|value| value.map(crate::metal::AccelerationStructurePassSampleBufferAttachmentDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructurePassSampleBufferAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructurePassSampleBufferAttachmentDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructurePassSampleBufferAttachmentDescriptor,
    }

    impl AccelerationStructurePassSampleBufferAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructurePassSampleBufferAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructurePassSampleBufferAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructurePassSampleBufferAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `end_of_encoder_sample_index` property with availability checks.
        pub fn end_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .end_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `end_of_encoder_sample_index` property after validation.
        pub fn set_end_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_end_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `start_of_encoder_sample_index` property with availability checks.
        pub fn start_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .start_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `start_of_encoder_sample_index` property after validation.
        pub fn set_start_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_start_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer` object property.
        pub fn sample_buffer(
            &self,
        ) -> Result<Option<crate::metal::CounterSampleBuffer>, crate::Error> {
            self.inner
                .sample_buffer()
                .map(|value| value.map(crate::metal::CounterSampleBuffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `sample_buffer` object property.
        pub fn set_sample_buffer(
            &self,
            value: Option<&crate::metal::CounterSampleBuffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_sample_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructurePassSampleBufferAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct AccelerationStructurePassSampleBufferAttachmentDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructurePassSampleBufferAttachmentDescriptorArray,
    }

    impl AccelerationStructurePassSampleBufferAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructurePassSampleBufferAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructurePassSampleBufferAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructurePassSampleBufferAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AccelerationStructureTriangleGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureTriangleGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureTriangleGeometryDescriptor,
    }

    impl AccelerationStructureTriangleGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AccelerationStructureTriangleGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AccelerationStructureTriangleGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AccelerationStructureTriangleGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_buffer_offset` property with availability checks.
        pub fn index_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .index_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer_offset` property after validation.
        pub fn set_index_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transformation_matrix_buffer_offset` property with availability checks.
        pub fn transformation_matrix_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .transformation_matrix_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_buffer_offset` property after validation.
        pub fn set_transformation_matrix_buffer_offset(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `triangle_count` property with availability checks.
        pub fn triangle_count(&self) -> Result<usize, crate::Error> {
            self.inner.triangle_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `triangle_count` property after validation.
        pub fn set_triangle_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_triangle_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_buffer_offset` property with availability checks.
        pub fn vertex_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .vertex_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_buffer_offset` property after validation.
        pub fn set_vertex_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_stride` property with availability checks.
        pub fn vertex_stride(&self) -> Result<usize, crate::Error> {
            self.inner.vertex_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_stride` property after validation.
        pub fn set_vertex_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transformation_matrix_layout` property with availability checks.
        pub fn transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_layout` property after validation.
        pub fn set_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_format` property with availability checks.
        pub fn vertex_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.vertex_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_format` property after validation.
        pub fn set_vertex_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `index_buffer` object property.
        pub fn index_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .index_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer` object property.
        pub fn set_index_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `transformation_matrix_buffer` object property.
        pub fn transformation_matrix_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .transformation_matrix_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_buffer` object property.
        pub fn set_transformation_matrix_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `vertex_buffer` object property.
        pub fn vertex_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .vertex_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_buffer` object property.
        pub fn set_vertex_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Allocation`.
    #[derive(Clone)]
    pub struct Allocation {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Allocation,
    }

    impl Allocation {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Allocation,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `allocated_size` property with availability checks.
        pub fn allocated_size(&self) -> Result<usize, crate::Error> {
            self.inner.allocated_size().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Architecture`.
    #[derive(Clone)]
    pub struct Architecture {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Architecture,
    }

    impl Architecture {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Architecture,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::Architecture` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::Architecture::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Argument`.
    #[derive(Clone)]
    pub struct Argument {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Argument,
    }

    impl Argument {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Argument,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::Argument` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::Argument::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `array_length` property with availability checks.
        pub fn array_length(&self) -> Result<usize, crate::Error> {
            self.inner.array_length().map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_alignment` property with availability checks.
        pub fn buffer_alignment(&self) -> Result<usize, crate::Error> {
            self.inner
                .buffer_alignment()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_data_size` property with availability checks.
        pub fn buffer_data_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .buffer_data_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index` property with availability checks.
        pub fn index(&self) -> Result<usize, crate::Error> {
            self.inner.index().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_active` property with availability checks.
        pub fn is_active(&self) -> Result<bool, crate::Error> {
            self.inner.is_active().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_depth_texture` property with availability checks.
        pub fn is_depth_texture(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_depth_texture()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `threadgroup_memory_alignment` property with availability checks.
        pub fn threadgroup_memory_alignment(&self) -> Result<usize, crate::Error> {
            self.inner
                .threadgroup_memory_alignment()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `threadgroup_memory_data_size` property with availability checks.
        pub fn threadgroup_memory_data_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .threadgroup_memory_data_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `access` property with availability checks.
        pub fn access(&self) -> Result<crate::metal::BindingAccess, crate::Error> {
            self.inner.access().map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_data_type` property with availability checks.
        pub fn buffer_data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner
                .buffer_data_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_data_type` property with availability checks.
        pub fn texture_data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner
                .texture_data_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_type` property with availability checks.
        pub fn texture_type(&self) -> Result<crate::metal::TextureType, crate::Error> {
            self.inner.texture_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal::ArgumentType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffer_pointer_type` object property.
        pub fn buffer_pointer_type(
            &self,
        ) -> Result<Option<crate::metal::PointerType>, crate::Error> {
            self.inner
                .buffer_pointer_type()
                .map(|value| value.map(crate::metal::PointerType::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffer_struct_type` object property.
        pub fn buffer_struct_type(&self) -> Result<Option<crate::metal::StructType>, crate::Error> {
            self.inner
                .buffer_struct_type()
                .map(|value| value.map(crate::metal::StructType::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ArgumentDescriptor`.
    #[derive(Clone)]
    pub struct ArgumentDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ArgumentDescriptor,
    }

    impl ArgumentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ArgumentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ArgumentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ArgumentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `array_length` property with availability checks.
        pub fn array_length(&self) -> Result<usize, crate::Error> {
            self.inner.array_length().map_err(crate::Error::from_ffi)
        }
        /// Writes the `array_length` property after validation.
        pub fn set_array_length(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_array_length(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `constant_block_alignment` property with availability checks.
        pub fn constant_block_alignment(&self) -> Result<usize, crate::Error> {
            self.inner
                .constant_block_alignment()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `constant_block_alignment` property after validation.
        pub fn set_constant_block_alignment(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_constant_block_alignment(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index` property with availability checks.
        pub fn index(&self) -> Result<usize, crate::Error> {
            self.inner.index().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index` property after validation.
        pub fn set_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_index(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `access` property with availability checks.
        pub fn access(&self) -> Result<crate::metal::BindingAccess, crate::Error> {
            self.inner.access().map_err(crate::Error::from_ffi)
        }
        /// Writes the `access` property after validation.
        pub fn set_access(&self, value: crate::metal::BindingAccess) -> Result<(), crate::Error> {
            self.inner.set_access(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `data_type` property after validation.
        pub fn set_data_type(&self, value: crate::metal::DataType) -> Result<(), crate::Error> {
            self.inner
                .set_data_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_type` property with availability checks.
        pub fn texture_type(&self) -> Result<crate::metal::TextureType, crate::Error> {
            self.inner.texture_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `texture_type` property after validation.
        pub fn set_texture_type(
            &self,
            value: crate::metal::TextureType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_texture_type(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ArgumentEncoder`.
    #[derive(Clone)]
    pub struct ArgumentEncoder {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ArgumentEncoder,
    }

    impl ArgumentEncoder {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ArgumentEncoder,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `alignment` property with availability checks.
        pub fn alignment(&self) -> Result<usize, crate::Error> {
            self.inner.alignment().map_err(crate::Error::from_ffi)
        }
        /// Reads the `encoded_length` property with availability checks.
        pub fn encoded_length(&self) -> Result<usize, crate::Error> {
            self.inner.encoded_length().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ArrayType`.
    #[derive(Clone)]
    pub struct ArrayType {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ArrayType,
    }

    impl ArrayType {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ArrayType,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ArrayType` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ArrayType::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `argument_index_stride` property with availability checks.
        pub fn argument_index_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .argument_index_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `array_length` property with availability checks.
        pub fn array_length(&self) -> Result<usize, crate::Error> {
            self.inner.array_length().map_err(crate::Error::from_ffi)
        }
        /// Reads the `stride` property with availability checks.
        pub fn stride(&self) -> Result<usize, crate::Error> {
            self.inner.stride().map_err(crate::Error::from_ffi)
        }
        /// Reads the `element_type` property with availability checks.
        pub fn element_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.element_type().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Attribute`.
    #[derive(Clone)]
    pub struct Attribute {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Attribute,
    }

    impl Attribute {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Attribute,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::Attribute` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::Attribute::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `attribute_index` property with availability checks.
        pub fn attribute_index(&self) -> Result<usize, crate::Error> {
            self.inner.attribute_index().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_active` property with availability checks.
        pub fn is_active(&self) -> Result<bool, crate::Error> {
            self.inner.is_active().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_patch_control_point_data` property with availability checks.
        pub fn is_patch_control_point_data(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_patch_control_point_data()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_patch_data` property with availability checks.
        pub fn is_patch_data(&self) -> Result<bool, crate::Error> {
            self.inner.is_patch_data().map_err(crate::Error::from_ffi)
        }
        /// Reads the `attribute_type` property with availability checks.
        pub fn attribute_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.attribute_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AttributeDescriptor`.
    #[derive(Clone)]
    pub struct AttributeDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AttributeDescriptor,
    }

    impl AttributeDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AttributeDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AttributeDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AttributeDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_index` property with availability checks.
        pub fn buffer_index(&self) -> Result<usize, crate::Error> {
            self.inner.buffer_index().map_err(crate::Error::from_ffi)
        }
        /// Writes the `buffer_index` property after validation.
        pub fn set_buffer_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_buffer_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `offset` property with availability checks.
        pub fn offset(&self) -> Result<usize, crate::Error> {
            self.inner.offset().map_err(crate::Error::from_ffi)
        }
        /// Writes the `offset` property after validation.
        pub fn set_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_offset(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `format` property with availability checks.
        pub fn format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `format` property after validation.
        pub fn set_format(&self, value: crate::metal::AttributeFormat) -> Result<(), crate::Error> {
            self.inner.set_format(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::AttributeDescriptorArray`.
    #[derive(Clone)]
    pub struct AttributeDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::AttributeDescriptorArray,
    }

    impl AttributeDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::AttributeDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::AttributeDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::AttributeDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BinaryArchive`.
    #[derive(Clone)]
    pub struct BinaryArchive {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::BinaryArchive,
    }

    impl BinaryArchive {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BinaryArchive,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BinaryArchiveDescriptor`.
    #[derive(Clone)]
    pub struct BinaryArchiveDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::BinaryArchiveDescriptor,
    }

    impl BinaryArchiveDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BinaryArchiveDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::BinaryArchiveDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::BinaryArchiveDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Binding`.
    #[derive(Clone)]
    pub struct Binding {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Binding,
    }

    impl Binding {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Binding,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `index` property with availability checks.
        pub fn index(&self) -> Result<usize, crate::Error> {
            self.inner.index().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_argument` property with availability checks.
        pub fn is_argument(&self) -> Result<bool, crate::Error> {
            self.inner.is_argument().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_used` property with availability checks.
        pub fn is_used(&self) -> Result<bool, crate::Error> {
            self.inner.is_used().map_err(crate::Error::from_ffi)
        }
        /// Reads the `access` property with availability checks.
        pub fn access(&self) -> Result<crate::metal::BindingAccess, crate::Error> {
            self.inner.access().map_err(crate::Error::from_ffi)
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal::BindingType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BlitPassDescriptor`.
    #[derive(Clone)]
    pub struct BlitPassDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::BlitPassDescriptor,
    }

    impl BlitPassDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BlitPassDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::BlitPassDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::BlitPassDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer_attachments` object property.
        pub fn sample_buffer_attachments(
            &self,
        ) -> Result<Option<crate::metal::BlitPassSampleBufferAttachmentDescriptorArray>, crate::Error>
        {
            self.inner
                .sample_buffer_attachments()
                .map(|value| {
                    value.map(crate::metal::BlitPassSampleBufferAttachmentDescriptorArray::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BlitPassSampleBufferAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct BlitPassSampleBufferAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::BlitPassSampleBufferAttachmentDescriptor,
    }

    impl BlitPassSampleBufferAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BlitPassSampleBufferAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::BlitPassSampleBufferAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::BlitPassSampleBufferAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `end_of_encoder_sample_index` property with availability checks.
        pub fn end_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .end_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `end_of_encoder_sample_index` property after validation.
        pub fn set_end_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_end_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `start_of_encoder_sample_index` property with availability checks.
        pub fn start_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .start_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `start_of_encoder_sample_index` property after validation.
        pub fn set_start_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_start_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer` object property.
        pub fn sample_buffer(
            &self,
        ) -> Result<Option<crate::metal::CounterSampleBuffer>, crate::Error> {
            self.inner
                .sample_buffer()
                .map(|value| value.map(crate::metal::CounterSampleBuffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `sample_buffer` object property.
        pub fn set_sample_buffer(
            &self,
            value: Option<&crate::metal::CounterSampleBuffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_sample_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BlitPassSampleBufferAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct BlitPassSampleBufferAttachmentDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::BlitPassSampleBufferAttachmentDescriptorArray,
    }

    impl BlitPassSampleBufferAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BlitPassSampleBufferAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::BlitPassSampleBufferAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::BlitPassSampleBufferAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BufferBinding`.
    #[derive(Clone)]
    pub struct BufferBinding {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::BufferBinding,
    }

    impl BufferBinding {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BufferBinding,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `buffer_alignment` property with availability checks.
        pub fn buffer_alignment(&self) -> Result<usize, crate::Error> {
            self.inner
                .buffer_alignment()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_data_size` property with availability checks.
        pub fn buffer_data_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .buffer_data_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_data_type` property with availability checks.
        pub fn buffer_data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner
                .buffer_data_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffer_pointer_type` object property.
        pub fn buffer_pointer_type(
            &self,
        ) -> Result<Option<crate::metal::PointerType>, crate::Error> {
            self.inner
                .buffer_pointer_type()
                .map(|value| value.map(crate::metal::PointerType::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffer_struct_type` object property.
        pub fn buffer_struct_type(&self) -> Result<Option<crate::metal::StructType>, crate::Error> {
            self.inner
                .buffer_struct_type()
                .map(|value| value.map(crate::metal::StructType::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BufferLayoutDescriptor`.
    #[derive(Clone)]
    pub struct BufferLayoutDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::BufferLayoutDescriptor,
    }

    impl BufferLayoutDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BufferLayoutDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::BufferLayoutDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::BufferLayoutDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `step_rate` property with availability checks.
        pub fn step_rate(&self) -> Result<usize, crate::Error> {
            self.inner.step_rate().map_err(crate::Error::from_ffi)
        }
        /// Writes the `step_rate` property after validation.
        pub fn set_step_rate(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_step_rate(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `stride` property with availability checks.
        pub fn stride(&self) -> Result<usize, crate::Error> {
            self.inner.stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `stride` property after validation.
        pub fn set_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_stride(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `step_function` property with availability checks.
        pub fn step_function(&self) -> Result<crate::metal::StepFunction, crate::Error> {
            self.inner.step_function().map_err(crate::Error::from_ffi)
        }
        /// Writes the `step_function` property after validation.
        pub fn set_step_function(
            &self,
            value: crate::metal::StepFunction,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_step_function(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::BufferLayoutDescriptorArray`.
    #[derive(Clone)]
    pub struct BufferLayoutDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::BufferLayoutDescriptorArray,
    }

    impl BufferLayoutDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::BufferLayoutDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::BufferLayoutDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::BufferLayoutDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CaptureDescriptor`.
    #[derive(Clone)]
    pub struct CaptureDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CaptureDescriptor,
    }

    impl CaptureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CaptureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::CaptureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::CaptureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `destination` property with availability checks.
        pub fn destination(&self) -> Result<crate::metal::CaptureDestination, crate::Error> {
            self.inner.destination().map_err(crate::Error::from_ffi)
        }
        /// Writes the `destination` property after validation.
        pub fn set_destination(
            &self,
            value: crate::metal::CaptureDestination,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_destination(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CaptureScope`.
    #[derive(Clone)]
    pub struct CaptureScope {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CaptureScope,
    }

    impl CaptureScope {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CaptureScope,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CommandBufferDescriptor`.
    #[derive(Clone)]
    pub struct CommandBufferDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CommandBufferDescriptor,
    }

    impl CommandBufferDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CommandBufferDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::CommandBufferDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::CommandBufferDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `retained_references` property with availability checks.
        pub fn retained_references(&self) -> Result<bool, crate::Error> {
            self.inner
                .retained_references()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `error_options` property with availability checks.
        pub fn error_options(
            &self,
        ) -> Result<crate::metal::CommandBufferErrorOption, crate::Error> {
            self.inner.error_options().map_err(crate::Error::from_ffi)
        }
        /// Writes the `error_options` property after validation.
        pub fn set_error_options(
            &self,
            value: crate::metal::CommandBufferErrorOption,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_error_options(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `log_state` object property.
        pub fn log_state(&self) -> Result<Option<crate::metal::LogState>, crate::Error> {
            self.inner
                .log_state()
                .map(|value| value.map(crate::metal::LogState::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `log_state` object property.
        pub fn set_log_state(
            &self,
            value: Option<&crate::metal::LogState>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_log_state(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_retained_references(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_retained_references(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CommandBufferEncoderInfo`.
    #[derive(Clone)]
    pub struct CommandBufferEncoderInfo {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CommandBufferEncoderInfo,
    }

    impl CommandBufferEncoderInfo {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CommandBufferEncoderInfo,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `error_state` property with availability checks.
        pub fn error_state(&self) -> Result<crate::metal::CommandEncoderErrorState, crate::Error> {
            self.inner.error_state().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CommandEncoder`.
    #[derive(Clone)]
    pub struct CommandEncoder {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CommandEncoder,
    }

    impl CommandEncoder {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CommandEncoder,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CommandQueueDescriptor`.
    #[derive(Clone)]
    pub struct CommandQueueDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CommandQueueDescriptor,
    }

    impl CommandQueueDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CommandQueueDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::CommandQueueDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::CommandQueueDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_command_buffer_count` property with availability checks.
        pub fn max_command_buffer_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_command_buffer_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_command_buffer_count` property after validation.
        pub fn set_max_command_buffer_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_command_buffer_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `log_state` object property.
        pub fn log_state(&self) -> Result<Option<crate::metal::LogState>, crate::Error> {
            self.inner
                .log_state()
                .map(|value| value.map(crate::metal::LogState::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `log_state` object property.
        pub fn set_log_state(
            &self,
            value: Option<&crate::metal::LogState>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_log_state(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ComputePassSampleBufferAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct ComputePassSampleBufferAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::ComputePassSampleBufferAttachmentDescriptor,
    }

    impl ComputePassSampleBufferAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ComputePassSampleBufferAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ComputePassSampleBufferAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ComputePassSampleBufferAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `end_of_encoder_sample_index` property with availability checks.
        pub fn end_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .end_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `end_of_encoder_sample_index` property after validation.
        pub fn set_end_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_end_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `start_of_encoder_sample_index` property with availability checks.
        pub fn start_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .start_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `start_of_encoder_sample_index` property after validation.
        pub fn set_start_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_start_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer` object property.
        pub fn sample_buffer(
            &self,
        ) -> Result<Option<crate::metal::CounterSampleBuffer>, crate::Error> {
            self.inner
                .sample_buffer()
                .map(|value| value.map(crate::metal::CounterSampleBuffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `sample_buffer` object property.
        pub fn set_sample_buffer(
            &self,
            value: Option<&crate::metal::CounterSampleBuffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_sample_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ComputePassSampleBufferAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct ComputePassSampleBufferAttachmentDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ComputePassSampleBufferAttachmentDescriptorArray,
    }

    impl ComputePassSampleBufferAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ComputePassSampleBufferAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ComputePassSampleBufferAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ComputePassSampleBufferAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ComputePipelineDescriptor`.
    #[derive(Clone)]
    pub struct ComputePipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ComputePipelineDescriptor,
    }

    impl ComputePipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ComputePipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ComputePipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ComputePipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_call_stack_depth` property with availability checks.
        pub fn max_call_stack_depth(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_call_stack_depth()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_call_stack_depth` property after validation.
        pub fn set_max_call_stack_depth(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_call_stack_depth(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_threadgroup` property with availability checks.
        pub fn max_total_threads_per_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_threadgroup` property after validation.
        pub fn set_max_total_threads_per_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_adding_binary_functions` property with availability checks.
        pub fn support_adding_binary_functions(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_adding_binary_functions()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_indirect_command_buffers` property with availability checks.
        pub fn support_indirect_command_buffers(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_indirect_command_buffers()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `thread_group_size_is_multiple_of_thread_execution_width` property with availability checks.
        pub fn thread_group_size_is_multiple_of_thread_execution_width(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .thread_group_size_is_multiple_of_thread_execution_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `shader_validation` property with availability checks.
        pub fn shader_validation(&self) -> Result<crate::metal::ShaderValidation, crate::Error> {
            self.inner
                .shader_validation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `shader_validation` property after validation.
        pub fn set_shader_validation(
            &self,
            value: crate::metal::ShaderValidation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_shader_validation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffers` object property.
        pub fn buffers(
            &self,
        ) -> Result<Option<crate::metal::PipelineBufferDescriptorArray>, crate::Error> {
            self.inner
                .buffers()
                .map(|value| value.map(crate::metal::PipelineBufferDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `compute_function` object property.
        pub fn compute_function(&self) -> Result<Option<crate::metal::Function>, crate::Error> {
            self.inner
                .compute_function()
                .map(|value| value.map(crate::metal::Function::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `compute_function` object property.
        pub fn set_compute_function(
            &self,
            value: Option<&crate::metal::Function>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_compute_function(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `linked_functions` object property.
        pub fn linked_functions(
            &self,
        ) -> Result<Option<crate::metal::LinkedFunctions>, crate::Error> {
            self.inner
                .linked_functions()
                .map(|value| value.map(crate::metal::LinkedFunctions::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `linked_functions` object property.
        pub fn set_linked_functions(
            &self,
            value: Option<&crate::metal::LinkedFunctions>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_linked_functions(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `stage_input_descriptor` object property.
        pub fn stage_input_descriptor(
            &self,
        ) -> Result<Option<crate::metal::StageInputOutputDescriptor>, crate::Error> {
            self.inner
                .stage_input_descriptor()
                .map(|value| value.map(crate::metal::StageInputOutputDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `stage_input_descriptor` object property.
        pub fn set_stage_input_descriptor(
            &self,
            value: Option<&crate::metal::StageInputOutputDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_stage_input_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_adding_binary_functions(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_adding_binary_functions(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_indirect_command_buffers(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_indirect_command_buffers(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_thread_group_size_is_multiple_of_thread_execution_width(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_thread_group_size_is_multiple_of_thread_execution_width(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ComputePipelineReflection`.
    #[derive(Clone)]
    pub struct ComputePipelineReflection {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ComputePipelineReflection,
    }

    impl ComputePipelineReflection {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ComputePipelineReflection,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ComputePipelineReflection` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ComputePipelineReflection::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Counter`.
    #[derive(Clone)]
    pub struct Counter {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Counter,
    }

    impl Counter {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Counter,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CounterSampleBuffer`.
    #[derive(Clone)]
    pub struct CounterSampleBuffer {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CounterSampleBuffer,
    }

    impl CounterSampleBuffer {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CounterSampleBuffer,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `sample_count` property with availability checks.
        pub fn sample_count(&self) -> Result<usize, crate::Error> {
            self.inner.sample_count().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CounterSampleBufferDescriptor`.
    #[derive(Clone)]
    pub struct CounterSampleBufferDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CounterSampleBufferDescriptor,
    }

    impl CounterSampleBufferDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CounterSampleBufferDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::CounterSampleBufferDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::CounterSampleBufferDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `sample_count` property with availability checks.
        pub fn sample_count(&self) -> Result<usize, crate::Error> {
            self.inner.sample_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `sample_count` property after validation.
        pub fn set_sample_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_sample_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `storage_mode` property with availability checks.
        pub fn storage_mode(&self) -> Result<crate::metal::StorageMode, crate::Error> {
            self.inner.storage_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `storage_mode` property after validation.
        pub fn set_storage_mode(
            &self,
            value: crate::metal::StorageMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_storage_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `counter_set` object property.
        pub fn counter_set(&self) -> Result<Option<crate::metal::CounterSet>, crate::Error> {
            self.inner
                .counter_set()
                .map(|value| value.map(crate::metal::CounterSet::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `counter_set` object property.
        pub fn set_counter_set(
            &self,
            value: Option<&crate::metal::CounterSet>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_counter_set(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::CounterSet`.
    #[derive(Clone)]
    pub struct CounterSet {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::CounterSet,
    }

    impl CounterSet {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::CounterSet,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::DepthStencilDescriptor`.
    #[derive(Clone)]
    pub struct DepthStencilDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::DepthStencilDescriptor,
    }

    impl DepthStencilDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::DepthStencilDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::DepthStencilDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::DepthStencilDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_depth_write_enabled` property with availability checks.
        pub fn is_depth_write_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_depth_write_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_compare_function` property with availability checks.
        pub fn depth_compare_function(
            &self,
        ) -> Result<crate::metal::CompareFunction, crate::Error> {
            self.inner
                .depth_compare_function()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_compare_function` property after validation.
        pub fn set_depth_compare_function(
            &self,
            value: crate::metal::CompareFunction,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_compare_function(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `back_face_stencil` object property.
        pub fn back_face_stencil(
            &self,
        ) -> Result<Option<crate::metal::StencilDescriptor>, crate::Error> {
            self.inner
                .back_face_stencil()
                .map(|value| value.map(crate::metal::StencilDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `back_face_stencil` object property.
        pub fn set_back_face_stencil(
            &self,
            value: Option<&crate::metal::StencilDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_back_face_stencil(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `front_face_stencil` object property.
        pub fn front_face_stencil(
            &self,
        ) -> Result<Option<crate::metal::StencilDescriptor>, crate::Error> {
            self.inner
                .front_face_stencil()
                .map(|value| value.map(crate::metal::StencilDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `front_face_stencil` object property.
        pub fn set_front_face_stencil(
            &self,
            value: Option<&crate::metal::StencilDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_front_face_stencil(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_depth_write_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_depth_write_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::DepthStencilState`.
    #[derive(Clone)]
    pub struct DepthStencilState {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::DepthStencilState,
    }

    impl DepthStencilState {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::DepthStencilState,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Drawable`.
    #[derive(Clone)]
    pub struct Drawable {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Drawable,
    }

    impl Drawable {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Drawable,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `drawable_id` property with availability checks.
        pub fn drawable_id(&self) -> Result<usize, crate::Error> {
            self.inner.drawable_id().map_err(crate::Error::from_ffi)
        }
        /// Reads the `presented_time` property with availability checks.
        pub fn presented_time(&self) -> Result<f64, crate::Error> {
            self.inner.presented_time().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::DynamicLibrary`.
    #[derive(Clone)]
    pub struct DynamicLibrary {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::DynamicLibrary,
    }

    impl DynamicLibrary {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::DynamicLibrary,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `install_name` string property.
        pub fn install_name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.install_name().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Event`.
    #[derive(Clone)]
    pub struct Event {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Event,
    }

    impl Event {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Event,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Fence`.
    #[derive(Clone)]
    pub struct Fence {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Fence,
    }

    impl Fence {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Fence,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionConstant`.
    #[derive(Clone)]
    pub struct FunctionConstant {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionConstant,
    }

    impl FunctionConstant {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionConstant,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionConstant` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionConstant::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index` property with availability checks.
        pub fn index(&self) -> Result<usize, crate::Error> {
            self.inner.index().map_err(crate::Error::from_ffi)
        }
        /// Reads the `required` property with availability checks.
        pub fn required(&self) -> Result<bool, crate::Error> {
            self.inner.required().map_err(crate::Error::from_ffi)
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionConstantValues`.
    #[derive(Clone)]
    pub struct FunctionConstantValues {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionConstantValues,
    }

    impl FunctionConstantValues {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionConstantValues,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionConstantValues` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionConstantValues::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionDescriptor`.
    #[derive(Clone)]
    pub struct FunctionDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionDescriptor,
    }

    impl FunctionDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `options` property with availability checks.
        pub fn options(&self) -> Result<crate::metal::FunctionOptions, crate::Error> {
            self.inner.options().map_err(crate::Error::from_ffi)
        }
        /// Writes the `options` property after validation.
        pub fn set_options(
            &self,
            value: crate::metal::FunctionOptions,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_options(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `constant_values` object property.
        pub fn constant_values(
            &self,
        ) -> Result<Option<crate::metal::FunctionConstantValues>, crate::Error> {
            self.inner
                .constant_values()
                .map(|value| value.map(crate::metal::FunctionConstantValues::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `constant_values` object property.
        pub fn set_constant_values(
            &self,
            value: Option<&crate::metal::FunctionConstantValues>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_constant_values(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
        /// Writes the `name` string property.
        pub fn set_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_name(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `specialized_name` string property.
        pub fn specialized_name(&self) -> Result<Option<String>, crate::Error> {
            self.inner
                .specialized_name()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `specialized_name` string property.
        pub fn set_specialized_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner
                .set_specialized_name(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionHandle`.
    #[derive(Clone)]
    pub struct FunctionHandle {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionHandle,
    }

    impl FunctionHandle {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionHandle,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `function_type` property with availability checks.
        pub fn function_type(&self) -> Result<crate::metal::FunctionType, crate::Error> {
            self.inner.function_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionLog`.
    #[derive(Clone)]
    pub struct FunctionLog {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionLog,
    }

    impl FunctionLog {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionLog,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal::FunctionLogType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `debug_location` object property.
        pub fn debug_location(
            &self,
        ) -> Result<Option<crate::metal::FunctionLogDebugLocation>, crate::Error> {
            self.inner
                .debug_location()
                .map(|value| value.map(crate::metal::FunctionLogDebugLocation::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `encoder_label` string property.
        pub fn encoder_label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.encoder_label().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `function` object property.
        pub fn function(&self) -> Result<Option<crate::metal::Function>, crate::Error> {
            self.inner
                .function()
                .map(|value| value.map(crate::metal::Function::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionLogDebugLocation`.
    #[derive(Clone)]
    pub struct FunctionLogDebugLocation {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionLogDebugLocation,
    }

    impl FunctionLogDebugLocation {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionLogDebugLocation,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `column` property with availability checks.
        pub fn column(&self) -> Result<usize, crate::Error> {
            self.inner.column().map_err(crate::Error::from_ffi)
        }
        /// Reads the `line` property with availability checks.
        pub fn line(&self) -> Result<usize, crate::Error> {
            self.inner.line().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `function_name` string property.
        pub fn function_name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.function_name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionReflection`.
    #[derive(Clone)]
    pub struct FunctionReflection {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionReflection,
    }

    impl FunctionReflection {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionReflection,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionReflection` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionReflection::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `user_annotation` string property.
        pub fn user_annotation(&self) -> Result<Option<String>, crate::Error> {
            self.inner.user_annotation().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionStitchingAttribute`.
    #[derive(Clone)]
    pub struct FunctionStitchingAttribute {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingAttribute,
    }

    impl FunctionStitchingAttribute {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingAttribute,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::FunctionStitchingAttributeAlwaysInline`.
    #[derive(Clone)]
    pub struct FunctionStitchingAttributeAlwaysInline {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::FunctionStitchingAttributeAlwaysInline,
    }

    impl FunctionStitchingAttributeAlwaysInline {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingAttributeAlwaysInline,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionStitchingAttributeAlwaysInline` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionStitchingAttributeAlwaysInline::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionStitchingFunctionNode`.
    #[derive(Clone)]
    pub struct FunctionStitchingFunctionNode {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingFunctionNode,
    }

    impl FunctionStitchingFunctionNode {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingFunctionNode,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionStitchingFunctionNode` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionStitchingFunctionNode::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
        /// Writes the `name` string property.
        pub fn set_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_name(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionStitchingGraph`.
    #[derive(Clone)]
    pub struct FunctionStitchingGraph {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingGraph,
    }

    impl FunctionStitchingGraph {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingGraph,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionStitchingGraph` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionStitchingGraph::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `function_name` string property.
        pub fn function_name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.function_name().map_err(crate::Error::from_ffi)
        }
        /// Writes the `function_name` string property.
        pub fn set_function_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner
                .set_function_name(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `output_node` object property.
        pub fn output_node(
            &self,
        ) -> Result<Option<crate::metal::FunctionStitchingFunctionNode>, crate::Error> {
            self.inner
                .output_node()
                .map(|value| value.map(crate::metal::FunctionStitchingFunctionNode::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_node` object property.
        pub fn set_output_node(
            &self,
            value: Option<&crate::metal::FunctionStitchingFunctionNode>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_output_node(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionStitchingInputNode`.
    #[derive(Clone)]
    pub struct FunctionStitchingInputNode {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingInputNode,
    }

    impl FunctionStitchingInputNode {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingInputNode,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::FunctionStitchingInputNode` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::FunctionStitchingInputNode::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `argument_index` property with availability checks.
        pub fn argument_index(&self) -> Result<usize, crate::Error> {
            self.inner.argument_index().map_err(crate::Error::from_ffi)
        }
        /// Writes the `argument_index` property after validation.
        pub fn set_argument_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_argument_index(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::FunctionStitchingNode`.
    #[derive(Clone)]
    pub struct FunctionStitchingNode {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingNode,
    }

    impl FunctionStitchingNode {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::FunctionStitchingNode,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::Heap`.
    #[derive(Clone)]
    pub struct Heap {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Heap,
    }

    impl Heap {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Heap,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `current_allocated_size` property with availability checks.
        pub fn current_allocated_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .current_allocated_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `size` property with availability checks.
        pub fn size(&self) -> Result<usize, crate::Error> {
            self.inner.size().map_err(crate::Error::from_ffi)
        }
        /// Reads the `used_size` property with availability checks.
        pub fn used_size(&self) -> Result<usize, crate::Error> {
            self.inner.used_size().map_err(crate::Error::from_ffi)
        }
        /// Reads the `cpu_cache_mode` property with availability checks.
        pub fn cpu_cache_mode(&self) -> Result<crate::metal::CPUCacheMode, crate::Error> {
            self.inner.cpu_cache_mode().map_err(crate::Error::from_ffi)
        }
        /// Reads the `hazard_tracking_mode` property with availability checks.
        pub fn hazard_tracking_mode(
            &self,
        ) -> Result<crate::metal::HazardTrackingMode, crate::Error> {
            self.inner
                .hazard_tracking_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `resource_options` property with availability checks.
        pub fn resource_options(&self) -> Result<crate::metal::ResourceOptions, crate::Error> {
            self.inner
                .resource_options()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `storage_mode` property with availability checks.
        pub fn storage_mode(&self) -> Result<crate::metal::StorageMode, crate::Error> {
            self.inner.storage_mode().map_err(crate::Error::from_ffi)
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal::HeapType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::HeapDescriptor`.
    #[derive(Clone)]
    pub struct HeapDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::HeapDescriptor,
    }

    impl HeapDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::HeapDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::HeapDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::HeapDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `size` property with availability checks.
        pub fn size(&self) -> Result<usize, crate::Error> {
            self.inner.size().map_err(crate::Error::from_ffi)
        }
        /// Writes the `size` property after validation.
        pub fn set_size(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_size(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `cpu_cache_mode` property with availability checks.
        pub fn cpu_cache_mode(&self) -> Result<crate::metal::CPUCacheMode, crate::Error> {
            self.inner.cpu_cache_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `cpu_cache_mode` property after validation.
        pub fn set_cpu_cache_mode(
            &self,
            value: crate::metal::CPUCacheMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_cpu_cache_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `hazard_tracking_mode` property with availability checks.
        pub fn hazard_tracking_mode(
            &self,
        ) -> Result<crate::metal::HazardTrackingMode, crate::Error> {
            self.inner
                .hazard_tracking_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `hazard_tracking_mode` property after validation.
        pub fn set_hazard_tracking_mode(
            &self,
            value: crate::metal::HazardTrackingMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_hazard_tracking_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_compatible_placement_sparse_page_size` property with availability checks.
        pub fn max_compatible_placement_sparse_page_size(
            &self,
        ) -> Result<crate::metal::SparsePageSize, crate::Error> {
            self.inner
                .max_compatible_placement_sparse_page_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_compatible_placement_sparse_page_size` property after validation.
        pub fn set_max_compatible_placement_sparse_page_size(
            &self,
            value: crate::metal::SparsePageSize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_compatible_placement_sparse_page_size(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `resource_options` property with availability checks.
        pub fn resource_options(&self) -> Result<crate::metal::ResourceOptions, crate::Error> {
            self.inner
                .resource_options()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `resource_options` property after validation.
        pub fn set_resource_options(
            &self,
            value: crate::metal::ResourceOptions,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_resource_options(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `sparse_page_size` property with availability checks.
        pub fn sparse_page_size(&self) -> Result<crate::metal::SparsePageSize, crate::Error> {
            self.inner
                .sparse_page_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `sparse_page_size` property after validation.
        pub fn set_sparse_page_size(
            &self,
            value: crate::metal::SparsePageSize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_sparse_page_size(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `storage_mode` property with availability checks.
        pub fn storage_mode(&self) -> Result<crate::metal::StorageMode, crate::Error> {
            self.inner.storage_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `storage_mode` property after validation.
        pub fn set_storage_mode(
            &self,
            value: crate::metal::StorageMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_storage_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal::HeapType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `r#type` property after validation.
        pub fn set_type(&self, value: crate::metal::HeapType) -> Result<(), crate::Error> {
            self.inner.set_type(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IOCommandBuffer`.
    #[derive(Clone)]
    pub struct IOCommandBuffer {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IOCommandBuffer,
    }

    impl IOCommandBuffer {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IOCommandBuffer,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `status` property with availability checks.
        pub fn status(&self) -> Result<crate::metal::IOStatus, crate::Error> {
            self.inner.status().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IOCommandQueue`.
    #[derive(Clone)]
    pub struct IOCommandQueue {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IOCommandQueue,
    }

    impl IOCommandQueue {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IOCommandQueue,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IOCommandQueueDescriptor`.
    #[derive(Clone)]
    pub struct IOCommandQueueDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IOCommandQueueDescriptor,
    }

    impl IOCommandQueueDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IOCommandQueueDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::IOCommandQueueDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::IOCommandQueueDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_command_buffer_count` property with availability checks.
        pub fn max_command_buffer_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_command_buffer_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_command_buffer_count` property after validation.
        pub fn set_max_command_buffer_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_command_buffer_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_commands_in_flight` property with availability checks.
        pub fn max_commands_in_flight(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_commands_in_flight()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_commands_in_flight` property after validation.
        pub fn set_max_commands_in_flight(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_commands_in_flight(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `priority` property with availability checks.
        pub fn priority(&self) -> Result<crate::metal::IOPriority, crate::Error> {
            self.inner.priority().map_err(crate::Error::from_ffi)
        }
        /// Writes the `priority` property after validation.
        pub fn set_priority(&self, value: crate::metal::IOPriority) -> Result<(), crate::Error> {
            self.inner
                .set_priority(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal::IOCommandQueueType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `r#type` property after validation.
        pub fn set_type(
            &self,
            value: crate::metal::IOCommandQueueType,
        ) -> Result<(), crate::Error> {
            self.inner.set_type(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `scratch_buffer_allocator` object property.
        pub fn scratch_buffer_allocator(
            &self,
        ) -> Result<Option<crate::metal::IOScratchBufferAllocator>, crate::Error> {
            self.inner
                .scratch_buffer_allocator()
                .map(|value| value.map(crate::metal::IOScratchBufferAllocator::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `scratch_buffer_allocator` object property.
        pub fn set_scratch_buffer_allocator(
            &self,
            value: Option<&crate::metal::IOScratchBufferAllocator>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_scratch_buffer_allocator(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IOFileHandle`.
    #[derive(Clone)]
    pub struct IOFileHandle {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IOFileHandle,
    }

    impl IOFileHandle {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IOFileHandle,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IOScratchBuffer`.
    #[derive(Clone)]
    pub struct IOScratchBuffer {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IOScratchBuffer,
    }

    impl IOScratchBuffer {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IOScratchBuffer,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `buffer` object property.
        pub fn buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IOScratchBufferAllocator`.
    #[derive(Clone)]
    pub struct IOScratchBufferAllocator {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IOScratchBufferAllocator,
    }

    impl IOScratchBufferAllocator {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IOScratchBufferAllocator,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::IndirectCommandBuffer`.
    #[derive(Clone)]
    pub struct IndirectCommandBuffer {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IndirectCommandBuffer,
    }

    impl IndirectCommandBuffer {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IndirectCommandBuffer,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `size` property with availability checks.
        pub fn size(&self) -> Result<usize, crate::Error> {
            self.inner.size().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IndirectCommandBufferDescriptor`.
    #[derive(Clone)]
    pub struct IndirectCommandBufferDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::IndirectCommandBufferDescriptor,
    }

    impl IndirectCommandBufferDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IndirectCommandBufferDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::IndirectCommandBufferDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::IndirectCommandBufferDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_buffers` property with availability checks.
        pub fn inherit_buffers(&self) -> Result<bool, crate::Error> {
            self.inner.inherit_buffers().map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_cull_mode` property with availability checks.
        pub fn inherit_cull_mode(&self) -> Result<bool, crate::Error> {
            self.inner
                .inherit_cull_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_depth_bias` property with availability checks.
        pub fn inherit_depth_bias(&self) -> Result<bool, crate::Error> {
            self.inner
                .inherit_depth_bias()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_depth_clip_mode` property with availability checks.
        pub fn inherit_depth_clip_mode(&self) -> Result<bool, crate::Error> {
            self.inner
                .inherit_depth_clip_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_depth_stencil_state` property with availability checks.
        pub fn inherit_depth_stencil_state(&self) -> Result<bool, crate::Error> {
            self.inner
                .inherit_depth_stencil_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_front_facing_winding` property with availability checks.
        pub fn inherit_front_facing_winding(&self) -> Result<bool, crate::Error> {
            self.inner
                .inherit_front_facing_winding()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_pipeline_state` property with availability checks.
        pub fn inherit_pipeline_state(&self) -> Result<bool, crate::Error> {
            self.inner
                .inherit_pipeline_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `inherit_triangle_fill_mode` property with availability checks.
        pub fn inherit_triangle_fill_mode(&self) -> Result<bool, crate::Error> {
            self.inner
                .inherit_triangle_fill_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_fragment_buffer_bind_count` property with availability checks.
        pub fn max_fragment_buffer_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_fragment_buffer_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_fragment_buffer_bind_count` property after validation.
        pub fn set_max_fragment_buffer_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_fragment_buffer_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_kernel_buffer_bind_count` property with availability checks.
        pub fn max_kernel_buffer_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_kernel_buffer_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_kernel_buffer_bind_count` property after validation.
        pub fn set_max_kernel_buffer_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_kernel_buffer_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_kernel_threadgroup_memory_bind_count` property with availability checks.
        pub fn max_kernel_threadgroup_memory_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_kernel_threadgroup_memory_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_kernel_threadgroup_memory_bind_count` property after validation.
        pub fn set_max_kernel_threadgroup_memory_bind_count(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_kernel_threadgroup_memory_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_mesh_buffer_bind_count` property with availability checks.
        pub fn max_mesh_buffer_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_mesh_buffer_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_mesh_buffer_bind_count` property after validation.
        pub fn set_max_mesh_buffer_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_mesh_buffer_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_object_buffer_bind_count` property with availability checks.
        pub fn max_object_buffer_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_object_buffer_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_object_buffer_bind_count` property after validation.
        pub fn set_max_object_buffer_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_object_buffer_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_object_threadgroup_memory_bind_count` property with availability checks.
        pub fn max_object_threadgroup_memory_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_object_threadgroup_memory_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_object_threadgroup_memory_bind_count` property after validation.
        pub fn set_max_object_threadgroup_memory_bind_count(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_object_threadgroup_memory_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_vertex_buffer_bind_count` property with availability checks.
        pub fn max_vertex_buffer_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_vertex_buffer_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_vertex_buffer_bind_count` property after validation.
        pub fn set_max_vertex_buffer_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_vertex_buffer_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_color_attachment_mapping` property with availability checks.
        pub fn support_color_attachment_mapping(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_color_attachment_mapping()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_dynamic_attribute_stride` property with availability checks.
        pub fn support_dynamic_attribute_stride(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_dynamic_attribute_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_ray_tracing` property with availability checks.
        pub fn support_ray_tracing(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_ray_tracing()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `command_types` property with availability checks.
        pub fn command_types(&self) -> Result<crate::metal::IndirectCommandType, crate::Error> {
            self.inner.command_types().map_err(crate::Error::from_ffi)
        }
        /// Writes the `command_types` property after validation.
        pub fn set_command_types(
            &self,
            value: crate::metal::IndirectCommandType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_command_types(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_buffers(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_buffers(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_cull_mode(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_cull_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_depth_bias(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_depth_bias(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_depth_clip_mode(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_depth_clip_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_depth_stencil_state(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_depth_stencil_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_front_facing_winding(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_front_facing_winding(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_pipeline_state(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_pipeline_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_inherit_triangle_fill_mode(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_inherit_triangle_fill_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_color_attachment_mapping(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_color_attachment_mapping(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_dynamic_attribute_stride(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_dynamic_attribute_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_ray_tracing(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_ray_tracing(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IndirectComputeCommand`.
    #[derive(Clone)]
    pub struct IndirectComputeCommand {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IndirectComputeCommand,
    }

    impl IndirectComputeCommand {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IndirectComputeCommand,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::IndirectInstanceAccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct IndirectInstanceAccelerationStructureDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IndirectInstanceAccelerationStructureDescriptor,
    }

    impl IndirectInstanceAccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IndirectInstanceAccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::IndirectInstanceAccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::IndirectInstanceAccelerationStructureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_count_buffer_offset` property with availability checks.
        pub fn instance_count_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .instance_count_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_count_buffer_offset` property after validation.
        pub fn set_instance_count_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_instance_count_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_buffer_offset` property with availability checks.
        pub fn instance_descriptor_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .instance_descriptor_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_buffer_offset` property after validation.
        pub fn set_instance_descriptor_buffer_offset(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_stride` property with availability checks.
        pub fn instance_descriptor_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .instance_descriptor_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_stride` property after validation.
        pub fn set_instance_descriptor_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_instance_count` property with availability checks.
        pub fn max_instance_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_instance_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_instance_count` property after validation.
        pub fn set_max_instance_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_instance_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_motion_transform_count` property with availability checks.
        pub fn max_motion_transform_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_motion_transform_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_motion_transform_count` property after validation.
        pub fn set_max_motion_transform_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_motion_transform_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_buffer_offset` property with availability checks.
        pub fn motion_transform_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_buffer_offset` property after validation.
        pub fn set_motion_transform_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_count_buffer_offset` property with availability checks.
        pub fn motion_transform_count_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_count_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_count_buffer_offset` property after validation.
        pub fn set_motion_transform_count_buffer_offset(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_count_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_stride` property with availability checks.
        pub fn motion_transform_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_stride` property after validation.
        pub fn set_motion_transform_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_type` property with availability checks.
        pub fn instance_descriptor_type(
            &self,
        ) -> Result<crate::metal::AccelerationStructureInstanceDescriptorType, crate::Error>
        {
            self.inner
                .instance_descriptor_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_type` property after validation.
        pub fn set_instance_descriptor_type(
            &self,
            value: crate::metal::AccelerationStructureInstanceDescriptorType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_transformation_matrix_layout` property with availability checks.
        pub fn instance_transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .instance_transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_transformation_matrix_layout` property after validation.
        pub fn set_instance_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_type` property with availability checks.
        pub fn motion_transform_type(&self) -> Result<crate::metal::TransformType, crate::Error> {
            self.inner
                .motion_transform_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_type` property after validation.
        pub fn set_motion_transform_type(
            &self,
            value: crate::metal::TransformType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `instance_count_buffer` object property.
        pub fn instance_count_buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .instance_count_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_count_buffer` object property.
        pub fn set_instance_count_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_count_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `instance_descriptor_buffer` object property.
        pub fn instance_descriptor_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .instance_descriptor_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_buffer` object property.
        pub fn set_instance_descriptor_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `motion_transform_buffer` object property.
        pub fn motion_transform_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .motion_transform_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_buffer` object property.
        pub fn set_motion_transform_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `motion_transform_count_buffer` object property.
        pub fn motion_transform_count_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .motion_transform_count_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_count_buffer` object property.
        pub fn set_motion_transform_count_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_count_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IndirectRenderCommand`.
    #[derive(Clone)]
    pub struct IndirectRenderCommand {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IndirectRenderCommand,
    }

    impl IndirectRenderCommand {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IndirectRenderCommand,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::InstanceAccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct InstanceAccelerationStructureDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::InstanceAccelerationStructureDescriptor,
    }

    impl InstanceAccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::InstanceAccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::InstanceAccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::InstanceAccelerationStructureDescriptor::new(
            )
            .map(Self::from_ffi)
            .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_count` property with availability checks.
        pub fn instance_count(&self) -> Result<usize, crate::Error> {
            self.inner.instance_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_count` property after validation.
        pub fn set_instance_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_instance_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_buffer_offset` property with availability checks.
        pub fn instance_descriptor_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .instance_descriptor_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_buffer_offset` property after validation.
        pub fn set_instance_descriptor_buffer_offset(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_stride` property with availability checks.
        pub fn instance_descriptor_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .instance_descriptor_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_stride` property after validation.
        pub fn set_instance_descriptor_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_buffer_offset` property with availability checks.
        pub fn motion_transform_buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_buffer_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_buffer_offset` property after validation.
        pub fn set_motion_transform_buffer_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_buffer_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_count` property with availability checks.
        pub fn motion_transform_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_count` property after validation.
        pub fn set_motion_transform_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_stride` property with availability checks.
        pub fn motion_transform_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_stride` property after validation.
        pub fn set_motion_transform_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_type` property with availability checks.
        pub fn instance_descriptor_type(
            &self,
        ) -> Result<crate::metal::AccelerationStructureInstanceDescriptorType, crate::Error>
        {
            self.inner
                .instance_descriptor_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_type` property after validation.
        pub fn set_instance_descriptor_type(
            &self,
            value: crate::metal::AccelerationStructureInstanceDescriptorType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_transformation_matrix_layout` property with availability checks.
        pub fn instance_transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .instance_transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_transformation_matrix_layout` property after validation.
        pub fn set_instance_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_type` property with availability checks.
        pub fn motion_transform_type(&self) -> Result<crate::metal::TransformType, crate::Error> {
            self.inner
                .motion_transform_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_type` property after validation.
        pub fn set_motion_transform_type(
            &self,
            value: crate::metal::TransformType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `instance_descriptor_buffer` object property.
        pub fn instance_descriptor_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .instance_descriptor_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_buffer` object property.
        pub fn set_instance_descriptor_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `motion_transform_buffer` object property.
        pub fn motion_transform_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .motion_transform_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_buffer` object property.
        pub fn set_motion_transform_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IntersectionFunctionDescriptor`.
    #[derive(Clone)]
    pub struct IntersectionFunctionDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IntersectionFunctionDescriptor,
    }

    impl IntersectionFunctionDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IntersectionFunctionDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::IntersectionFunctionDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::IntersectionFunctionDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::IntersectionFunctionTable`.
    #[derive(Clone)]
    pub struct IntersectionFunctionTable {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::IntersectionFunctionTable,
    }

    impl IntersectionFunctionTable {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IntersectionFunctionTable,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::IntersectionFunctionTableDescriptor`.
    #[derive(Clone)]
    pub struct IntersectionFunctionTableDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::IntersectionFunctionTableDescriptor,
    }

    impl IntersectionFunctionTableDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::IntersectionFunctionTableDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::IntersectionFunctionTableDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::IntersectionFunctionTableDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `function_count` property with availability checks.
        pub fn function_count(&self) -> Result<usize, crate::Error> {
            self.inner.function_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `function_count` property after validation.
        pub fn set_function_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_function_count(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::LinkedFunctions`.
    #[derive(Clone)]
    pub struct LinkedFunctions {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::LinkedFunctions,
    }

    impl LinkedFunctions {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::LinkedFunctions,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::LinkedFunctions` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::LinkedFunctions::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::LogContainer`.
    #[derive(Clone)]
    pub struct LogContainer {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::LogContainer,
    }

    impl LogContainer {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::LogContainer,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::LogState`.
    #[derive(Clone)]
    pub struct LogState {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::LogState,
    }

    impl LogState {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::LogState,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::LogStateDescriptor`.
    #[derive(Clone)]
    pub struct LogStateDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::LogStateDescriptor,
    }

    impl LogStateDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::LogStateDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::LogStateDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::LogStateDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_size` property with availability checks.
        pub fn buffer_size(&self) -> Result<isize, crate::Error> {
            self.inner.buffer_size().map_err(crate::Error::from_ffi)
        }
        /// Writes the `buffer_size` property after validation.
        pub fn set_buffer_size(&self, value: isize) -> Result<(), crate::Error> {
            self.inner
                .set_buffer_size(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `level` property with availability checks.
        pub fn level(&self) -> Result<crate::metal::LogLevel, crate::Error> {
            self.inner.level().map_err(crate::Error::from_ffi)
        }
        /// Writes the `level` property after validation.
        pub fn set_level(&self, value: crate::metal::LogLevel) -> Result<(), crate::Error> {
            self.inner.set_level(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::LogicalToPhysicalColorAttachmentMap`.
    #[derive(Clone)]
    pub struct LogicalToPhysicalColorAttachmentMap {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::LogicalToPhysicalColorAttachmentMap,
    }

    impl LogicalToPhysicalColorAttachmentMap {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::LogicalToPhysicalColorAttachmentMap,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::LogicalToPhysicalColorAttachmentMap` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::LogicalToPhysicalColorAttachmentMap::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::MeshRenderPipelineDescriptor`.
    #[derive(Clone)]
    pub struct MeshRenderPipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::MeshRenderPipelineDescriptor,
    }

    impl MeshRenderPipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::MeshRenderPipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::MeshRenderPipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::MeshRenderPipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_alpha_to_coverage_enabled` property with availability checks.
        pub fn is_alpha_to_coverage_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_alpha_to_coverage_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_alpha_to_one_enabled` property with availability checks.
        pub fn is_alpha_to_one_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_alpha_to_one_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_rasterization_enabled` property with availability checks.
        pub fn is_rasterization_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_rasterization_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threadgroups_per_mesh_grid` property with availability checks.
        pub fn max_total_threadgroups_per_mesh_grid(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threadgroups_per_mesh_grid()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threadgroups_per_mesh_grid` property after validation.
        pub fn set_max_total_threadgroups_per_mesh_grid(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threadgroups_per_mesh_grid(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_mesh_threadgroup` property with availability checks.
        pub fn max_total_threads_per_mesh_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_mesh_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_mesh_threadgroup` property after validation.
        pub fn set_max_total_threads_per_mesh_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_mesh_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_object_threadgroup` property with availability checks.
        pub fn max_total_threads_per_object_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_object_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_object_threadgroup` property after validation.
        pub fn set_max_total_threads_per_object_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_object_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_vertex_amplification_count` property with availability checks.
        pub fn max_vertex_amplification_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_vertex_amplification_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_vertex_amplification_count` property after validation.
        pub fn set_max_vertex_amplification_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_vertex_amplification_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `mesh_threadgroup_size_is_multiple_of_thread_execution_width` property with availability checks.
        pub fn mesh_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .mesh_threadgroup_size_is_multiple_of_thread_execution_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `object_threadgroup_size_is_multiple_of_thread_execution_width` property with availability checks.
        pub fn object_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .object_threadgroup_size_is_multiple_of_thread_execution_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `payload_memory_length` property with availability checks.
        pub fn payload_memory_length(&self) -> Result<usize, crate::Error> {
            self.inner
                .payload_memory_length()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `payload_memory_length` property after validation.
        pub fn set_payload_memory_length(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_payload_memory_length(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `raster_sample_count` property with availability checks.
        pub fn raster_sample_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .raster_sample_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `raster_sample_count` property after validation.
        pub fn set_raster_sample_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_raster_sample_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_indirect_command_buffers` property with availability checks.
        pub fn support_indirect_command_buffers(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_indirect_command_buffers()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_attachment_pixel_format` property with availability checks.
        pub fn depth_attachment_pixel_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .depth_attachment_pixel_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_attachment_pixel_format` property after validation.
        pub fn set_depth_attachment_pixel_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_attachment_pixel_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `shader_validation` property with availability checks.
        pub fn shader_validation(&self) -> Result<crate::metal::ShaderValidation, crate::Error> {
            self.inner
                .shader_validation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `shader_validation` property after validation.
        pub fn set_shader_validation(
            &self,
            value: crate::metal::ShaderValidation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_shader_validation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `stencil_attachment_pixel_format` property with availability checks.
        pub fn stencil_attachment_pixel_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .stencil_attachment_pixel_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `stencil_attachment_pixel_format` property after validation.
        pub fn set_stencil_attachment_pixel_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_stencil_attachment_pixel_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_attachments` object property.
        pub fn color_attachments(
            &self,
        ) -> Result<Option<crate::metal::RenderPipelineColorAttachmentDescriptorArray>, crate::Error>
        {
            self.inner
                .color_attachments()
                .map(|value| {
                    value.map(crate::metal::RenderPipelineColorAttachmentDescriptorArray::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_buffers` object property.
        pub fn fragment_buffers(
            &self,
        ) -> Result<Option<crate::metal::PipelineBufferDescriptorArray>, crate::Error> {
            self.inner
                .fragment_buffers()
                .map(|value| value.map(crate::metal::PipelineBufferDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_function` object property.
        pub fn fragment_function(&self) -> Result<Option<crate::metal::Function>, crate::Error> {
            self.inner
                .fragment_function()
                .map(|value| value.map(crate::metal::Function::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fragment_function` object property.
        pub fn set_fragment_function(
            &self,
            value: Option<&crate::metal::Function>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_fragment_function(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_linked_functions` object property.
        pub fn fragment_linked_functions(
            &self,
        ) -> Result<Option<crate::metal::LinkedFunctions>, crate::Error> {
            self.inner
                .fragment_linked_functions()
                .map(|value| value.map(crate::metal::LinkedFunctions::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fragment_linked_functions` object property.
        pub fn set_fragment_linked_functions(
            &self,
            value: Option<&crate::metal::LinkedFunctions>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_fragment_linked_functions(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `mesh_buffers` object property.
        pub fn mesh_buffers(
            &self,
        ) -> Result<Option<crate::metal::PipelineBufferDescriptorArray>, crate::Error> {
            self.inner
                .mesh_buffers()
                .map(|value| value.map(crate::metal::PipelineBufferDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `mesh_function` object property.
        pub fn mesh_function(&self) -> Result<Option<crate::metal::Function>, crate::Error> {
            self.inner
                .mesh_function()
                .map(|value| value.map(crate::metal::Function::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `mesh_function` object property.
        pub fn set_mesh_function(
            &self,
            value: Option<&crate::metal::Function>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mesh_function(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `mesh_linked_functions` object property.
        pub fn mesh_linked_functions(
            &self,
        ) -> Result<Option<crate::metal::LinkedFunctions>, crate::Error> {
            self.inner
                .mesh_linked_functions()
                .map(|value| value.map(crate::metal::LinkedFunctions::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `mesh_linked_functions` object property.
        pub fn set_mesh_linked_functions(
            &self,
            value: Option<&crate::metal::LinkedFunctions>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mesh_linked_functions(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `object_buffers` object property.
        pub fn object_buffers(
            &self,
        ) -> Result<Option<crate::metal::PipelineBufferDescriptorArray>, crate::Error> {
            self.inner
                .object_buffers()
                .map(|value| value.map(crate::metal::PipelineBufferDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `object_function` object property.
        pub fn object_function(&self) -> Result<Option<crate::metal::Function>, crate::Error> {
            self.inner
                .object_function()
                .map(|value| value.map(crate::metal::Function::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `object_function` object property.
        pub fn set_object_function(
            &self,
            value: Option<&crate::metal::Function>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_object_function(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `object_linked_functions` object property.
        pub fn object_linked_functions(
            &self,
        ) -> Result<Option<crate::metal::LinkedFunctions>, crate::Error> {
            self.inner
                .object_linked_functions()
                .map(|value| value.map(crate::metal::LinkedFunctions::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `object_linked_functions` object property.
        pub fn set_object_linked_functions(
            &self,
            value: Option<&crate::metal::LinkedFunctions>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_object_linked_functions(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_alpha_to_coverage_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_to_coverage_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_alpha_to_one_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_to_one_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_mesh_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mesh_threadgroup_size_is_multiple_of_thread_execution_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_object_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_object_threadgroup_size_is_multiple_of_thread_execution_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_rasterization_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_rasterization_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_indirect_command_buffers(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_indirect_command_buffers(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::MotionKeyframeData`.
    #[derive(Clone)]
    pub struct MotionKeyframeData {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::MotionKeyframeData,
    }

    impl MotionKeyframeData {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::MotionKeyframeData,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::MotionKeyframeData` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::MotionKeyframeData::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `offset` property with availability checks.
        pub fn offset(&self) -> Result<usize, crate::Error> {
            self.inner.offset().map_err(crate::Error::from_ffi)
        }
        /// Writes the `offset` property after validation.
        pub fn set_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_offset(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffer` object property.
        pub fn buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `buffer` object property.
        pub fn set_buffer(&self, value: Option<&crate::metal::Buffer>) -> Result<(), crate::Error> {
            self.inner
                .set_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ObjectPayloadBinding`.
    #[derive(Clone)]
    pub struct ObjectPayloadBinding {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ObjectPayloadBinding,
    }

    impl ObjectPayloadBinding {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ObjectPayloadBinding,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `object_payload_alignment` property with availability checks.
        pub fn object_payload_alignment(&self) -> Result<usize, crate::Error> {
            self.inner
                .object_payload_alignment()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `object_payload_data_size` property with availability checks.
        pub fn object_payload_data_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .object_payload_data_size()
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ParallelRenderCommandEncoder`.
    #[derive(Clone)]
    pub struct ParallelRenderCommandEncoder {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ParallelRenderCommandEncoder,
    }

    impl ParallelRenderCommandEncoder {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ParallelRenderCommandEncoder,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::PipelineBufferDescriptor`.
    #[derive(Clone)]
    pub struct PipelineBufferDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::PipelineBufferDescriptor,
    }

    impl PipelineBufferDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::PipelineBufferDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::PipelineBufferDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::PipelineBufferDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `mutability` property with availability checks.
        pub fn mutability(&self) -> Result<crate::metal::Mutability, crate::Error> {
            self.inner.mutability().map_err(crate::Error::from_ffi)
        }
        /// Writes the `mutability` property after validation.
        pub fn set_mutability(&self, value: crate::metal::Mutability) -> Result<(), crate::Error> {
            self.inner
                .set_mutability(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::PipelineBufferDescriptorArray`.
    #[derive(Clone)]
    pub struct PipelineBufferDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::PipelineBufferDescriptorArray,
    }

    impl PipelineBufferDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::PipelineBufferDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::PipelineBufferDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::PipelineBufferDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::PointerType`.
    #[derive(Clone)]
    pub struct PointerType {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::PointerType,
    }

    impl PointerType {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::PointerType,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::PointerType` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::PointerType::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `alignment` property with availability checks.
        pub fn alignment(&self) -> Result<usize, crate::Error> {
            self.inner.alignment().map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_size` property with availability checks.
        pub fn data_size(&self) -> Result<usize, crate::Error> {
            self.inner.data_size().map_err(crate::Error::from_ffi)
        }
        /// Reads the `element_is_argument_buffer` property with availability checks.
        pub fn element_is_argument_buffer(&self) -> Result<bool, crate::Error> {
            self.inner
                .element_is_argument_buffer()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `access` property with availability checks.
        pub fn access(&self) -> Result<crate::metal::BindingAccess, crate::Error> {
            self.inner.access().map_err(crate::Error::from_ffi)
        }
        /// Reads the `element_type` property with availability checks.
        pub fn element_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.element_type().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::PrimitiveAccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct PrimitiveAccelerationStructureDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::PrimitiveAccelerationStructureDescriptor,
    }

    impl PrimitiveAccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::PrimitiveAccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::PrimitiveAccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::PrimitiveAccelerationStructureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_end_time` property with availability checks.
        pub fn motion_end_time(&self) -> Result<f32, crate::Error> {
            self.inner.motion_end_time().map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_end_time` property after validation.
        pub fn set_motion_end_time(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_end_time(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_keyframe_count` property with availability checks.
        pub fn motion_keyframe_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_keyframe_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_keyframe_count` property after validation.
        pub fn set_motion_keyframe_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_keyframe_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_start_time` property with availability checks.
        pub fn motion_start_time(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_start_time()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_start_time` property after validation.
        pub fn set_motion_start_time(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_start_time(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_end_border_mode` property with availability checks.
        pub fn motion_end_border_mode(
            &self,
        ) -> Result<crate::metal::MotionBorderMode, crate::Error> {
            self.inner
                .motion_end_border_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_end_border_mode` property after validation.
        pub fn set_motion_end_border_mode(
            &self,
            value: crate::metal::MotionBorderMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_end_border_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_start_border_mode` property with availability checks.
        pub fn motion_start_border_mode(
            &self,
        ) -> Result<crate::metal::MotionBorderMode, crate::Error> {
            self.inner
                .motion_start_border_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_start_border_mode` property after validation.
        pub fn set_motion_start_border_mode(
            &self,
            value: crate::metal::MotionBorderMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_start_border_mode(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RasterizationRateLayerArray`.
    #[derive(Clone)]
    pub struct RasterizationRateLayerArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::RasterizationRateLayerArray,
    }

    impl RasterizationRateLayerArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RasterizationRateLayerArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RasterizationRateLayerArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RasterizationRateLayerArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RasterizationRateLayerDescriptor`.
    #[derive(Clone)]
    pub struct RasterizationRateLayerDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RasterizationRateLayerDescriptor,
    }

    impl RasterizationRateLayerDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RasterizationRateLayerDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RasterizationRateLayerDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RasterizationRateLayerDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `horizontal` object property.
        pub fn horizontal(
            &self,
        ) -> Result<Option<crate::metal::RasterizationRateSampleArray>, crate::Error> {
            self.inner
                .horizontal()
                .map(|value| value.map(crate::metal::RasterizationRateSampleArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `vertical` object property.
        pub fn vertical(
            &self,
        ) -> Result<Option<crate::metal::RasterizationRateSampleArray>, crate::Error> {
            self.inner
                .vertical()
                .map(|value| value.map(crate::metal::RasterizationRateSampleArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RasterizationRateMap`.
    #[derive(Clone)]
    pub struct RasterizationRateMap {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::RasterizationRateMap,
    }

    impl RasterizationRateMap {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RasterizationRateMap,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `layer_count` property with availability checks.
        pub fn layer_count(&self) -> Result<usize, crate::Error> {
            self.inner.layer_count().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RasterizationRateMapDescriptor`.
    #[derive(Clone)]
    pub struct RasterizationRateMapDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::RasterizationRateMapDescriptor,
    }

    impl RasterizationRateMapDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RasterizationRateMapDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RasterizationRateMapDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RasterizationRateMapDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `layer_count` property with availability checks.
        pub fn layer_count(&self) -> Result<usize, crate::Error> {
            self.inner.layer_count().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `layers` object property.
        pub fn layers(
            &self,
        ) -> Result<Option<crate::metal::RasterizationRateLayerArray>, crate::Error> {
            self.inner
                .layers()
                .map(|value| value.map(crate::metal::RasterizationRateLayerArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RasterizationRateSampleArray`.
    #[derive(Clone)]
    pub struct RasterizationRateSampleArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::RasterizationRateSampleArray,
    }

    impl RasterizationRateSampleArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RasterizationRateSampleArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RasterizationRateSampleArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RasterizationRateSampleArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPassAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct RenderPassAttachmentDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::RenderPassAttachmentDescriptor,
    }

    impl RenderPassAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPassAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPassAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPassAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_plane` property with availability checks.
        pub fn depth_plane(&self) -> Result<usize, crate::Error> {
            self.inner.depth_plane().map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_plane` property after validation.
        pub fn set_depth_plane(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_depth_plane(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `level` property with availability checks.
        pub fn level(&self) -> Result<usize, crate::Error> {
            self.inner.level().map_err(crate::Error::from_ffi)
        }
        /// Writes the `level` property after validation.
        pub fn set_level(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_level(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `resolve_depth_plane` property with availability checks.
        pub fn resolve_depth_plane(&self) -> Result<usize, crate::Error> {
            self.inner
                .resolve_depth_plane()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `resolve_depth_plane` property after validation.
        pub fn set_resolve_depth_plane(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_resolve_depth_plane(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `resolve_level` property with availability checks.
        pub fn resolve_level(&self) -> Result<usize, crate::Error> {
            self.inner.resolve_level().map_err(crate::Error::from_ffi)
        }
        /// Writes the `resolve_level` property after validation.
        pub fn set_resolve_level(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_resolve_level(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `resolve_slice` property with availability checks.
        pub fn resolve_slice(&self) -> Result<usize, crate::Error> {
            self.inner.resolve_slice().map_err(crate::Error::from_ffi)
        }
        /// Writes the `resolve_slice` property after validation.
        pub fn set_resolve_slice(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_resolve_slice(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `slice` property with availability checks.
        pub fn slice(&self) -> Result<usize, crate::Error> {
            self.inner.slice().map_err(crate::Error::from_ffi)
        }
        /// Writes the `slice` property after validation.
        pub fn set_slice(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_slice(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `load_action` property with availability checks.
        pub fn load_action(&self) -> Result<crate::metal::LoadAction, crate::Error> {
            self.inner.load_action().map_err(crate::Error::from_ffi)
        }
        /// Writes the `load_action` property after validation.
        pub fn set_load_action(&self, value: crate::metal::LoadAction) -> Result<(), crate::Error> {
            self.inner
                .set_load_action(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `store_action` property with availability checks.
        pub fn store_action(&self) -> Result<crate::metal::StoreAction, crate::Error> {
            self.inner.store_action().map_err(crate::Error::from_ffi)
        }
        /// Writes the `store_action` property after validation.
        pub fn set_store_action(
            &self,
            value: crate::metal::StoreAction,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_store_action(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `store_action_options` property with availability checks.
        pub fn store_action_options(
            &self,
        ) -> Result<crate::metal::StoreActionOptions, crate::Error> {
            self.inner
                .store_action_options()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `store_action_options` property after validation.
        pub fn set_store_action_options(
            &self,
            value: crate::metal::StoreActionOptions,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_store_action_options(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `resolve_texture` object property.
        pub fn resolve_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .resolve_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `resolve_texture` object property.
        pub fn set_resolve_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_resolve_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `texture` object property.
        pub fn texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `texture` object property.
        pub fn set_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPassColorAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct RenderPassColorAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPassColorAttachmentDescriptor,
    }

    impl RenderPassColorAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPassColorAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPassColorAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPassColorAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPassColorAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct RenderPassColorAttachmentDescriptorArray {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPassColorAttachmentDescriptorArray,
    }

    impl RenderPassColorAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPassColorAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPassColorAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPassColorAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPassDepthAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct RenderPassDepthAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPassDepthAttachmentDescriptor,
    }

    impl RenderPassDepthAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPassDepthAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPassDepthAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPassDepthAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `clear_depth` property with availability checks.
        pub fn clear_depth(&self) -> Result<f64, crate::Error> {
            self.inner.clear_depth().map_err(crate::Error::from_ffi)
        }
        /// Writes the `clear_depth` property after validation.
        pub fn set_clear_depth(&self, value: f64) -> Result<(), crate::Error> {
            self.inner
                .set_clear_depth(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the inherited depth attachment texture.
        pub fn set_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the inherited depth load action.
        pub fn set_load_action(&self, value: crate::metal::LoadAction) -> Result<(), crate::Error> {
            self.inner
                .set_load_action(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the inherited depth store action.
        pub fn set_store_action(
            &self,
            value: crate::metal::StoreAction,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_store_action(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_resolve_filter` property with availability checks.
        pub fn depth_resolve_filter(
            &self,
        ) -> Result<crate::metal::MultisampleDepthResolveFilter, crate::Error> {
            self.inner
                .depth_resolve_filter()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_resolve_filter` property after validation.
        pub fn set_depth_resolve_filter(
            &self,
            value: crate::metal::MultisampleDepthResolveFilter,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_resolve_filter(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPassSampleBufferAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct RenderPassSampleBufferAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPassSampleBufferAttachmentDescriptor,
    }

    impl RenderPassSampleBufferAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPassSampleBufferAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPassSampleBufferAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPassSampleBufferAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `end_of_fragment_sample_index` property with availability checks.
        pub fn end_of_fragment_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .end_of_fragment_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `end_of_fragment_sample_index` property after validation.
        pub fn set_end_of_fragment_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_end_of_fragment_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `end_of_vertex_sample_index` property with availability checks.
        pub fn end_of_vertex_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .end_of_vertex_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `end_of_vertex_sample_index` property after validation.
        pub fn set_end_of_vertex_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_end_of_vertex_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `start_of_fragment_sample_index` property with availability checks.
        pub fn start_of_fragment_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .start_of_fragment_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `start_of_fragment_sample_index` property after validation.
        pub fn set_start_of_fragment_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_start_of_fragment_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `start_of_vertex_sample_index` property with availability checks.
        pub fn start_of_vertex_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .start_of_vertex_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `start_of_vertex_sample_index` property after validation.
        pub fn set_start_of_vertex_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_start_of_vertex_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer` object property.
        pub fn sample_buffer(
            &self,
        ) -> Result<Option<crate::metal::CounterSampleBuffer>, crate::Error> {
            self.inner
                .sample_buffer()
                .map(|value| value.map(crate::metal::CounterSampleBuffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `sample_buffer` object property.
        pub fn set_sample_buffer(
            &self,
            value: Option<&crate::metal::CounterSampleBuffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_sample_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPassSampleBufferAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct RenderPassSampleBufferAttachmentDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::RenderPassSampleBufferAttachmentDescriptorArray,
    }

    impl RenderPassSampleBufferAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPassSampleBufferAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPassSampleBufferAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPassSampleBufferAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPassStencilAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct RenderPassStencilAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPassStencilAttachmentDescriptor,
    }

    impl RenderPassStencilAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPassStencilAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPassStencilAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPassStencilAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `clear_stencil` property with availability checks.
        pub fn clear_stencil(&self) -> Result<u32, crate::Error> {
            self.inner.clear_stencil().map_err(crate::Error::from_ffi)
        }
        /// Writes the `clear_stencil` property after validation.
        pub fn set_clear_stencil(&self, value: u32) -> Result<(), crate::Error> {
            self.inner
                .set_clear_stencil(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `stencil_resolve_filter` property with availability checks.
        pub fn stencil_resolve_filter(
            &self,
        ) -> Result<crate::metal::MultisampleStencilResolveFilter, crate::Error> {
            self.inner
                .stencil_resolve_filter()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `stencil_resolve_filter` property after validation.
        pub fn set_stencil_resolve_filter(
            &self,
            value: crate::metal::MultisampleStencilResolveFilter,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_stencil_resolve_filter(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPipelineColorAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct RenderPipelineColorAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPipelineColorAttachmentDescriptor,
    }

    impl RenderPipelineColorAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPipelineColorAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPipelineColorAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPipelineColorAttachmentDescriptor::new(
            )
            .map(Self::from_ffi)
            .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_blending_enabled` property with availability checks.
        pub fn is_blending_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_blending_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `alpha_blend_operation` property with availability checks.
        pub fn alpha_blend_operation(&self) -> Result<crate::metal::BlendOperation, crate::Error> {
            self.inner
                .alpha_blend_operation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `alpha_blend_operation` property after validation.
        pub fn set_alpha_blend_operation(
            &self,
            value: crate::metal::BlendOperation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_blend_operation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `destination_alpha_blend_factor` property with availability checks.
        pub fn destination_alpha_blend_factor(
            &self,
        ) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .destination_alpha_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `destination_alpha_blend_factor` property after validation.
        pub fn set_destination_alpha_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_destination_alpha_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `destination_rgb_blend_factor` property with availability checks.
        pub fn destination_rgb_blend_factor(
            &self,
        ) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .destination_rgb_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `destination_rgb_blend_factor` property after validation.
        pub fn set_destination_rgb_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_destination_rgb_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `pixel_format` property with availability checks.
        pub fn pixel_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner.pixel_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `pixel_format` property after validation.
        pub fn set_pixel_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_pixel_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `rgb_blend_operation` property with availability checks.
        pub fn rgb_blend_operation(&self) -> Result<crate::metal::BlendOperation, crate::Error> {
            self.inner
                .rgb_blend_operation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `rgb_blend_operation` property after validation.
        pub fn set_rgb_blend_operation(
            &self,
            value: crate::metal::BlendOperation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_rgb_blend_operation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `source_alpha_blend_factor` property with availability checks.
        pub fn source_alpha_blend_factor(&self) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .source_alpha_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `source_alpha_blend_factor` property after validation.
        pub fn set_source_alpha_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_source_alpha_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `source_rgb_blend_factor` property with availability checks.
        pub fn source_rgb_blend_factor(&self) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .source_rgb_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `source_rgb_blend_factor` property after validation.
        pub fn set_source_rgb_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_source_rgb_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `write_mask` property with availability checks.
        pub fn write_mask(&self) -> Result<crate::metal::ColorWriteMask, crate::Error> {
            self.inner.write_mask().map_err(crate::Error::from_ffi)
        }
        /// Writes the `write_mask` property after validation.
        pub fn set_write_mask(
            &self,
            value: crate::metal::ColorWriteMask,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_write_mask(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_blending_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_blending_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPipelineColorAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct RenderPipelineColorAttachmentDescriptorArray {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPipelineColorAttachmentDescriptorArray,
    }

    impl RenderPipelineColorAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPipelineColorAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPipelineColorAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPipelineColorAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPipelineFunctionsDescriptor`.
    #[derive(Clone)]
    pub struct RenderPipelineFunctionsDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::RenderPipelineFunctionsDescriptor,
    }

    impl RenderPipelineFunctionsDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPipelineFunctionsDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPipelineFunctionsDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPipelineFunctionsDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::RenderPipelineReflection`.
    #[derive(Clone)]
    pub struct RenderPipelineReflection {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::RenderPipelineReflection,
    }

    impl RenderPipelineReflection {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::RenderPipelineReflection,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::RenderPipelineReflection` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::RenderPipelineReflection::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ResidencySet`.
    #[derive(Clone)]
    pub struct ResidencySet {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResidencySet,
    }

    impl ResidencySet {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResidencySet,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `allocated_size` property with availability checks.
        pub fn allocated_size(&self) -> Result<u64, crate::Error> {
            self.inner.allocated_size().map_err(crate::Error::from_ffi)
        }
        /// Reads the `allocation_count` property with availability checks.
        pub fn allocation_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .allocation_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ResidencySetDescriptor`.
    #[derive(Clone)]
    pub struct ResidencySetDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResidencySetDescriptor,
    }

    impl ResidencySetDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResidencySetDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ResidencySetDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ResidencySetDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `initial_capacity` property with availability checks.
        pub fn initial_capacity(&self) -> Result<usize, crate::Error> {
            self.inner
                .initial_capacity()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `initial_capacity` property after validation.
        pub fn set_initial_capacity(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_initial_capacity(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Resource`.
    #[derive(Clone)]
    pub struct Resource {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Resource,
    }

    impl Resource {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Resource,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `allocated_size` property with availability checks.
        pub fn allocated_size(&self) -> Result<usize, crate::Error> {
            self.inner.allocated_size().map_err(crate::Error::from_ffi)
        }
        /// Reads the `heap_offset` property with availability checks.
        pub fn heap_offset(&self) -> Result<usize, crate::Error> {
            self.inner.heap_offset().map_err(crate::Error::from_ffi)
        }
        /// Reads the `cpu_cache_mode` property with availability checks.
        pub fn cpu_cache_mode(&self) -> Result<crate::metal::CPUCacheMode, crate::Error> {
            self.inner.cpu_cache_mode().map_err(crate::Error::from_ffi)
        }
        /// Reads the `hazard_tracking_mode` property with availability checks.
        pub fn hazard_tracking_mode(
            &self,
        ) -> Result<crate::metal::HazardTrackingMode, crate::Error> {
            self.inner
                .hazard_tracking_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `resource_options` property with availability checks.
        pub fn resource_options(&self) -> Result<crate::metal::ResourceOptions, crate::Error> {
            self.inner
                .resource_options()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `storage_mode` property with availability checks.
        pub fn storage_mode(&self) -> Result<crate::metal::StorageMode, crate::Error> {
            self.inner.storage_mode().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `heap` object property.
        pub fn heap(&self) -> Result<Option<crate::metal::Heap>, crate::Error> {
            self.inner
                .heap()
                .map(|value| value.map(crate::metal::Heap::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ResourceStateCommandEncoder`.
    #[derive(Clone)]
    pub struct ResourceStateCommandEncoder {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResourceStateCommandEncoder,
    }

    impl ResourceStateCommandEncoder {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResourceStateCommandEncoder,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::ResourceStatePassDescriptor`.
    #[derive(Clone)]
    pub struct ResourceStatePassDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResourceStatePassDescriptor,
    }

    impl ResourceStatePassDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResourceStatePassDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ResourceStatePassDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ResourceStatePassDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer_attachments` object property.
        pub fn sample_buffer_attachments(
            &self,
        ) -> Result<
            Option<crate::metal::ResourceStatePassSampleBufferAttachmentDescriptorArray>,
            crate::Error,
        > {
            self.inner.sample_buffer_attachments()
                .map(|value| value.map(crate::metal::ResourceStatePassSampleBufferAttachmentDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ResourceStatePassSampleBufferAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct ResourceStatePassSampleBufferAttachmentDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResourceStatePassSampleBufferAttachmentDescriptor,
    }

    impl ResourceStatePassSampleBufferAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResourceStatePassSampleBufferAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ResourceStatePassSampleBufferAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ResourceStatePassSampleBufferAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `end_of_encoder_sample_index` property with availability checks.
        pub fn end_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .end_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `end_of_encoder_sample_index` property after validation.
        pub fn set_end_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_end_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `start_of_encoder_sample_index` property with availability checks.
        pub fn start_of_encoder_sample_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .start_of_encoder_sample_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `start_of_encoder_sample_index` property after validation.
        pub fn set_start_of_encoder_sample_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_start_of_encoder_sample_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `sample_buffer` object property.
        pub fn sample_buffer(
            &self,
        ) -> Result<Option<crate::metal::CounterSampleBuffer>, crate::Error> {
            self.inner
                .sample_buffer()
                .map(|value| value.map(crate::metal::CounterSampleBuffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `sample_buffer` object property.
        pub fn set_sample_buffer(
            &self,
            value: Option<&crate::metal::CounterSampleBuffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_sample_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ResourceStatePassSampleBufferAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct ResourceStatePassSampleBufferAttachmentDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResourceStatePassSampleBufferAttachmentDescriptorArray,
    }

    impl ResourceStatePassSampleBufferAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResourceStatePassSampleBufferAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ResourceStatePassSampleBufferAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ResourceStatePassSampleBufferAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ResourceViewPool`.
    #[derive(Clone)]
    pub struct ResourceViewPool {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResourceViewPool,
    }

    impl ResourceViewPool {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResourceViewPool,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `resource_view_count` property with availability checks.
        pub fn resource_view_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .resource_view_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::ResourceViewPoolDescriptor`.
    #[derive(Clone)]
    pub struct ResourceViewPoolDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ResourceViewPoolDescriptor,
    }

    impl ResourceViewPoolDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ResourceViewPoolDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::ResourceViewPoolDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::ResourceViewPoolDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `resource_view_count` property with availability checks.
        pub fn resource_view_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .resource_view_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `resource_view_count` property after validation.
        pub fn set_resource_view_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_resource_view_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::SamplerDescriptor`.
    #[derive(Clone)]
    pub struct SamplerDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::SamplerDescriptor,
    }

    impl SamplerDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::SamplerDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::SamplerDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::SamplerDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `lod_average` property with availability checks.
        pub fn lod_average(&self) -> Result<bool, crate::Error> {
            self.inner.lod_average().map_err(crate::Error::from_ffi)
        }
        /// Reads the `lod_bias` property with availability checks.
        pub fn lod_bias(&self) -> Result<f32, crate::Error> {
            self.inner.lod_bias().map_err(crate::Error::from_ffi)
        }
        /// Writes the `lod_bias` property after validation.
        pub fn set_lod_bias(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_lod_bias(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `lod_max_clamp` property with availability checks.
        pub fn lod_max_clamp(&self) -> Result<f32, crate::Error> {
            self.inner.lod_max_clamp().map_err(crate::Error::from_ffi)
        }
        /// Writes the `lod_max_clamp` property after validation.
        pub fn set_lod_max_clamp(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_lod_max_clamp(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `lod_min_clamp` property with availability checks.
        pub fn lod_min_clamp(&self) -> Result<f32, crate::Error> {
            self.inner.lod_min_clamp().map_err(crate::Error::from_ffi)
        }
        /// Writes the `lod_min_clamp` property after validation.
        pub fn set_lod_min_clamp(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_lod_min_clamp(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_anisotropy` property with availability checks.
        pub fn max_anisotropy(&self) -> Result<usize, crate::Error> {
            self.inner.max_anisotropy().map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_anisotropy` property after validation.
        pub fn set_max_anisotropy(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_anisotropy(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `normalized_coordinates` property with availability checks.
        pub fn normalized_coordinates(&self) -> Result<bool, crate::Error> {
            self.inner
                .normalized_coordinates()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_argument_buffers` property with availability checks.
        pub fn support_argument_buffers(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_argument_buffers()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `border_color` property with availability checks.
        pub fn border_color(&self) -> Result<crate::metal::SamplerBorderColor, crate::Error> {
            self.inner.border_color().map_err(crate::Error::from_ffi)
        }
        /// Writes the `border_color` property after validation.
        pub fn set_border_color(
            &self,
            value: crate::metal::SamplerBorderColor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_border_color(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `compare_function` property with availability checks.
        pub fn compare_function(&self) -> Result<crate::metal::CompareFunction, crate::Error> {
            self.inner
                .compare_function()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `compare_function` property after validation.
        pub fn set_compare_function(
            &self,
            value: crate::metal::CompareFunction,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_compare_function(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `mag_filter` property with availability checks.
        pub fn mag_filter(&self) -> Result<crate::metal::SamplerMinMagFilter, crate::Error> {
            self.inner.mag_filter().map_err(crate::Error::from_ffi)
        }
        /// Writes the `mag_filter` property after validation.
        pub fn set_mag_filter(
            &self,
            value: crate::metal::SamplerMinMagFilter,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mag_filter(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `min_filter` property with availability checks.
        pub fn min_filter(&self) -> Result<crate::metal::SamplerMinMagFilter, crate::Error> {
            self.inner.min_filter().map_err(crate::Error::from_ffi)
        }
        /// Writes the `min_filter` property after validation.
        pub fn set_min_filter(
            &self,
            value: crate::metal::SamplerMinMagFilter,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_min_filter(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `mip_filter` property with availability checks.
        pub fn mip_filter(&self) -> Result<crate::metal::SamplerMipFilter, crate::Error> {
            self.inner.mip_filter().map_err(crate::Error::from_ffi)
        }
        /// Writes the `mip_filter` property after validation.
        pub fn set_mip_filter(
            &self,
            value: crate::metal::SamplerMipFilter,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mip_filter(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reduction_mode` property with availability checks.
        pub fn reduction_mode(&self) -> Result<crate::metal::SamplerReductionMode, crate::Error> {
            self.inner.reduction_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `reduction_mode` property after validation.
        pub fn set_reduction_mode(
            &self,
            value: crate::metal::SamplerReductionMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_reduction_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `s_address_mode` property with availability checks.
        pub fn s_address_mode(&self) -> Result<crate::metal::SamplerAddressMode, crate::Error> {
            self.inner.s_address_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `s_address_mode` property after validation.
        pub fn set_s_address_mode(
            &self,
            value: crate::metal::SamplerAddressMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_s_address_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `t_address_mode` property with availability checks.
        pub fn t_address_mode(&self) -> Result<crate::metal::SamplerAddressMode, crate::Error> {
            self.inner.t_address_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `t_address_mode` property after validation.
        pub fn set_t_address_mode(
            &self,
            value: crate::metal::SamplerAddressMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_t_address_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_lod_average(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_lod_average(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_normalized_coordinates(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_normalized_coordinates(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_argument_buffers(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_argument_buffers(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::SamplerState`.
    #[derive(Clone)]
    pub struct SamplerState {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::SamplerState,
    }

    impl SamplerState {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::SamplerState,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::SharedEvent`.
    #[derive(Clone)]
    pub struct SharedEvent {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::SharedEvent,
    }

    impl SharedEvent {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::SharedEvent,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `signaled_value` property with availability checks.
        pub fn signaled_value(&self) -> Result<u64, crate::Error> {
            self.inner.signaled_value().map_err(crate::Error::from_ffi)
        }
        /// Writes the `signaled_value` property after validation.
        pub fn set_signaled_value(&self, value: u64) -> Result<(), crate::Error> {
            self.inner
                .set_signaled_value(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::SharedEventHandle`.
    #[derive(Clone)]
    pub struct SharedEventHandle {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::SharedEventHandle,
    }

    impl SharedEventHandle {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::SharedEventHandle,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::SharedEventHandle` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::SharedEventHandle::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::SharedEventListener`.
    #[derive(Clone)]
    pub struct SharedEventListener {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::SharedEventListener,
    }

    impl SharedEventListener {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::SharedEventListener,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::SharedEventListener` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::SharedEventListener::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::SharedTextureHandle`.
    #[derive(Clone)]
    pub struct SharedTextureHandle {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::SharedTextureHandle,
    }

    impl SharedTextureHandle {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::SharedTextureHandle,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::SharedTextureHandle` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::SharedTextureHandle::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::StageInputOutputDescriptor`.
    #[derive(Clone)]
    pub struct StageInputOutputDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::StageInputOutputDescriptor,
    }

    impl StageInputOutputDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::StageInputOutputDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::StageInputOutputDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::StageInputOutputDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_buffer_index` property with availability checks.
        pub fn index_buffer_index(&self) -> Result<usize, crate::Error> {
            self.inner
                .index_buffer_index()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_buffer_index` property after validation.
        pub fn set_index_buffer_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_index_buffer_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `attributes` object property.
        pub fn attributes(
            &self,
        ) -> Result<Option<crate::metal::AttributeDescriptorArray>, crate::Error> {
            self.inner
                .attributes()
                .map(|value| value.map(crate::metal::AttributeDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `layouts` object property.
        pub fn layouts(
            &self,
        ) -> Result<Option<crate::metal::BufferLayoutDescriptorArray>, crate::Error> {
            self.inner
                .layouts()
                .map(|value| value.map(crate::metal::BufferLayoutDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::StencilDescriptor`.
    #[derive(Clone)]
    pub struct StencilDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::StencilDescriptor,
    }

    impl StencilDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::StencilDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::StencilDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::StencilDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `read_mask` property with availability checks.
        pub fn read_mask(&self) -> Result<u32, crate::Error> {
            self.inner.read_mask().map_err(crate::Error::from_ffi)
        }
        /// Writes the `read_mask` property after validation.
        pub fn set_read_mask(&self, value: u32) -> Result<(), crate::Error> {
            self.inner
                .set_read_mask(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `write_mask` property with availability checks.
        pub fn write_mask(&self) -> Result<u32, crate::Error> {
            self.inner.write_mask().map_err(crate::Error::from_ffi)
        }
        /// Writes the `write_mask` property after validation.
        pub fn set_write_mask(&self, value: u32) -> Result<(), crate::Error> {
            self.inner
                .set_write_mask(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_failure_operation` property with availability checks.
        pub fn depth_failure_operation(
            &self,
        ) -> Result<crate::metal::StencilOperation, crate::Error> {
            self.inner
                .depth_failure_operation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_failure_operation` property after validation.
        pub fn set_depth_failure_operation(
            &self,
            value: crate::metal::StencilOperation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_failure_operation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_stencil_pass_operation` property with availability checks.
        pub fn depth_stencil_pass_operation(
            &self,
        ) -> Result<crate::metal::StencilOperation, crate::Error> {
            self.inner
                .depth_stencil_pass_operation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_stencil_pass_operation` property after validation.
        pub fn set_depth_stencil_pass_operation(
            &self,
            value: crate::metal::StencilOperation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_stencil_pass_operation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `stencil_compare_function` property with availability checks.
        pub fn stencil_compare_function(
            &self,
        ) -> Result<crate::metal::CompareFunction, crate::Error> {
            self.inner
                .stencil_compare_function()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `stencil_compare_function` property after validation.
        pub fn set_stencil_compare_function(
            &self,
            value: crate::metal::CompareFunction,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_stencil_compare_function(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `stencil_failure_operation` property with availability checks.
        pub fn stencil_failure_operation(
            &self,
        ) -> Result<crate::metal::StencilOperation, crate::Error> {
            self.inner
                .stencil_failure_operation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `stencil_failure_operation` property after validation.
        pub fn set_stencil_failure_operation(
            &self,
            value: crate::metal::StencilOperation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_stencil_failure_operation(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::StitchedLibraryDescriptor`.
    #[derive(Clone)]
    pub struct StitchedLibraryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::StitchedLibraryDescriptor,
    }

    impl StitchedLibraryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::StitchedLibraryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::StitchedLibraryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::StitchedLibraryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `options` property with availability checks.
        pub fn options(&self) -> Result<crate::metal::StitchedLibraryOptions, crate::Error> {
            self.inner.options().map_err(crate::Error::from_ffi)
        }
        /// Writes the `options` property after validation.
        pub fn set_options(
            &self,
            value: crate::metal::StitchedLibraryOptions,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_options(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::StructMember`.
    #[derive(Clone)]
    pub struct StructMember {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::StructMember,
    }

    impl StructMember {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::StructMember,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::StructMember` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::StructMember::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `argument_index` property with availability checks.
        pub fn argument_index(&self) -> Result<usize, crate::Error> {
            self.inner.argument_index().map_err(crate::Error::from_ffi)
        }
        /// Reads the `offset` property with availability checks.
        pub fn offset(&self) -> Result<usize, crate::Error> {
            self.inner.offset().map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::StructType`.
    #[derive(Clone)]
    pub struct StructType {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::StructType,
    }

    impl StructType {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::StructType,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::StructType` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::StructType::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Tensor`.
    #[derive(Clone)]
    pub struct Tensor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Tensor,
    }

    impl Tensor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Tensor,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `buffer_offset` property with availability checks.
        pub fn buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner.buffer_offset().map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::TensorDataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the `usage` property with availability checks.
        pub fn usage(&self) -> Result<crate::metal::TensorUsage, crate::Error> {
            self.inner.usage().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffer` object property.
        pub fn buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `dimensions` object property.
        pub fn dimensions(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .dimensions()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `strides` object property.
        pub fn strides(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .strides()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorAuxiliaryPlane`.
    #[derive(Clone)]
    pub struct TensorAuxiliaryPlane {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlane,
    }

    impl TensorAuxiliaryPlane {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlane,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `buffer_offset` property with availability checks.
        pub fn buffer_offset(&self) -> Result<usize, crate::Error> {
            self.inner.buffer_offset().map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::TensorDataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the `plane_type` property with availability checks.
        pub fn plane_type(&self) -> Result<crate::metal::TensorPlaneType, crate::Error> {
            self.inner.plane_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `block_factors` object property.
        pub fn block_factors(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .block_factors()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `buffer` object property.
        pub fn buffer(&self) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorAuxiliaryPlaneDescriptor`.
    #[derive(Clone)]
    pub struct TensorAuxiliaryPlaneDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneDescriptor,
    }

    impl TensorAuxiliaryPlaneDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TensorAuxiliaryPlaneDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::TensorDataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `data_type` property after validation.
        pub fn set_data_type(
            &self,
            value: crate::metal::TensorDataType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_data_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `block_factors` object property.
        pub fn block_factors(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .block_factors()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `block_factors` object property.
        pub fn set_block_factors(
            &self,
            value: Option<&crate::metal::TensorExtents>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_block_factors(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorAuxiliaryPlaneDescriptorMap`.
    #[derive(Clone)]
    pub struct TensorAuxiliaryPlaneDescriptorMap {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneDescriptorMap,
    }

    impl TensorAuxiliaryPlaneDescriptorMap {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneDescriptorMap,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TensorAuxiliaryPlaneDescriptorMap` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneDescriptorMap::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorAuxiliaryPlaneType`.
    #[derive(Clone)]
    pub struct TensorAuxiliaryPlaneType {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneType,
    }

    impl TensorAuxiliaryPlaneType {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneType,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TensorAuxiliaryPlaneType` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TensorAuxiliaryPlaneType::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::TensorDataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the `plane_type` property with availability checks.
        pub fn plane_type(&self) -> Result<crate::metal::TensorPlaneType, crate::Error> {
            self.inner.plane_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `block_factors` object property.
        pub fn block_factors(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .block_factors()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorBinding`.
    #[derive(Clone)]
    pub struct TensorBinding {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorBinding,
    }

    impl TensorBinding {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorBinding,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the `tensor_data_type` property with availability checks.
        pub fn tensor_data_type(&self) -> Result<crate::metal::TensorDataType, crate::Error> {
            self.inner
                .tensor_data_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `dimensions` object property.
        pub fn dimensions(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .dimensions()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorBufferAttachments`.
    #[derive(Clone)]
    pub struct TensorBufferAttachments {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorBufferAttachments,
    }

    impl TensorBufferAttachments {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorBufferAttachments,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TensorBufferAttachments` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TensorBufferAttachments::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorDescriptor`.
    #[derive(Clone)]
    pub struct TensorDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorDescriptor,
    }

    impl TensorDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TensorDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TensorDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `cpu_cache_mode` property with availability checks.
        pub fn cpu_cache_mode(&self) -> Result<crate::metal::CPUCacheMode, crate::Error> {
            self.inner.cpu_cache_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `cpu_cache_mode` property after validation.
        pub fn set_cpu_cache_mode(
            &self,
            value: crate::metal::CPUCacheMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_cpu_cache_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::TensorDataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `data_type` property after validation.
        pub fn set_data_type(
            &self,
            value: crate::metal::TensorDataType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_data_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `hazard_tracking_mode` property with availability checks.
        pub fn hazard_tracking_mode(
            &self,
        ) -> Result<crate::metal::HazardTrackingMode, crate::Error> {
            self.inner
                .hazard_tracking_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `hazard_tracking_mode` property after validation.
        pub fn set_hazard_tracking_mode(
            &self,
            value: crate::metal::HazardTrackingMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_hazard_tracking_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `resource_options` property with availability checks.
        pub fn resource_options(&self) -> Result<crate::metal::ResourceOptions, crate::Error> {
            self.inner
                .resource_options()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `resource_options` property after validation.
        pub fn set_resource_options(
            &self,
            value: crate::metal::ResourceOptions,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_resource_options(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `storage_mode` property with availability checks.
        pub fn storage_mode(&self) -> Result<crate::metal::StorageMode, crate::Error> {
            self.inner.storage_mode().map_err(crate::Error::from_ffi)
        }
        /// Writes the `storage_mode` property after validation.
        pub fn set_storage_mode(
            &self,
            value: crate::metal::StorageMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_storage_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `usage` property with availability checks.
        pub fn usage(&self) -> Result<crate::metal::TensorUsage, crate::Error> {
            self.inner.usage().map_err(crate::Error::from_ffi)
        }
        /// Writes the `usage` property after validation.
        pub fn set_usage(&self, value: crate::metal::TensorUsage) -> Result<(), crate::Error> {
            self.inner.set_usage(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `auxiliary_planes` object property.
        pub fn auxiliary_planes(
            &self,
        ) -> Result<Option<crate::metal::TensorAuxiliaryPlaneDescriptorMap>, crate::Error> {
            self.inner
                .auxiliary_planes()
                .map(|value| value.map(crate::metal::TensorAuxiliaryPlaneDescriptorMap::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `auxiliary_planes` object property.
        pub fn set_auxiliary_planes(
            &self,
            value: Option<&crate::metal::TensorAuxiliaryPlaneDescriptorMap>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_auxiliary_planes(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `dimensions` object property.
        pub fn dimensions(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .dimensions()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `dimensions` object property.
        pub fn set_dimensions(
            &self,
            value: Option<&crate::metal::TensorExtents>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_dimensions(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `strides` object property.
        pub fn strides(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .strides()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `strides` object property.
        pub fn set_strides(
            &self,
            value: Option<&crate::metal::TensorExtents>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_strides(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorExtents`.
    #[derive(Clone)]
    pub struct TensorExtents {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorExtents,
    }

    impl TensorExtents {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorExtents,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TensorExtents` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TensorExtents::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `rank` property with availability checks.
        pub fn rank(&self) -> Result<usize, crate::Error> {
            self.inner.rank().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TensorReferenceType`.
    #[derive(Clone)]
    pub struct TensorReferenceType {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TensorReferenceType,
    }

    impl TensorReferenceType {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TensorReferenceType,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TensorReferenceType` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TensorReferenceType::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `access` property with availability checks.
        pub fn access(&self) -> Result<crate::metal::BindingAccess, crate::Error> {
            self.inner.access().map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the `tensor_data_type` property with availability checks.
        pub fn tensor_data_type(&self) -> Result<crate::metal::TensorDataType, crate::Error> {
            self.inner
                .tensor_data_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `dimensions` object property.
        pub fn dimensions(&self) -> Result<Option<crate::metal::TensorExtents>, crate::Error> {
            self.inner
                .dimensions()
                .map(|value| value.map(crate::metal::TensorExtents::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TextureBinding`.
    #[derive(Clone)]
    pub struct TextureBinding {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TextureBinding,
    }

    impl TextureBinding {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TextureBinding,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `array_length` property with availability checks.
        pub fn array_length(&self) -> Result<usize, crate::Error> {
            self.inner.array_length().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_depth_texture` property with availability checks.
        pub fn is_depth_texture(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_depth_texture()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_data_type` property with availability checks.
        pub fn texture_data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner
                .texture_data_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_type` property with availability checks.
        pub fn texture_type(&self) -> Result<crate::metal::TextureType, crate::Error> {
            self.inner.texture_type().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TextureReferenceType`.
    #[derive(Clone)]
    pub struct TextureReferenceType {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TextureReferenceType,
    }

    impl TextureReferenceType {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TextureReferenceType,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TextureReferenceType` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TextureReferenceType::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_depth_texture` property with availability checks.
        pub fn is_depth_texture(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_depth_texture()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `access` property with availability checks.
        pub fn access(&self) -> Result<crate::metal::BindingAccess, crate::Error> {
            self.inner.access().map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_data_type` property with availability checks.
        pub fn texture_data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner
                .texture_data_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_type` property with availability checks.
        pub fn texture_type(&self) -> Result<crate::metal::TextureType, crate::Error> {
            self.inner.texture_type().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TextureViewDescriptor`.
    #[derive(Clone)]
    pub struct TextureViewDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TextureViewDescriptor,
    }

    impl TextureViewDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TextureViewDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TextureViewDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TextureViewDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `pixel_format` property with availability checks.
        pub fn pixel_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner.pixel_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `pixel_format` property after validation.
        pub fn set_pixel_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_pixel_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `texture_type` property with availability checks.
        pub fn texture_type(&self) -> Result<crate::metal::TextureType, crate::Error> {
            self.inner.texture_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `texture_type` property after validation.
        pub fn set_texture_type(
            &self,
            value: crate::metal::TextureType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_texture_type(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TextureViewPool`.
    #[derive(Clone)]
    pub struct TextureViewPool {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TextureViewPool,
    }

    impl TextureViewPool {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TextureViewPool,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::ThreadgroupBinding`.
    #[derive(Clone)]
    pub struct ThreadgroupBinding {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::ThreadgroupBinding,
    }

    impl ThreadgroupBinding {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::ThreadgroupBinding,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `threadgroup_memory_alignment` property with availability checks.
        pub fn threadgroup_memory_alignment(&self) -> Result<usize, crate::Error> {
            self.inner
                .threadgroup_memory_alignment()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `threadgroup_memory_data_size` property with availability checks.
        pub fn threadgroup_memory_data_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .threadgroup_memory_data_size()
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TileRenderPipelineColorAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct TileRenderPipelineColorAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::TileRenderPipelineColorAttachmentDescriptor,
    }

    impl TileRenderPipelineColorAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TileRenderPipelineColorAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TileRenderPipelineColorAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TileRenderPipelineColorAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `pixel_format` property with availability checks.
        pub fn pixel_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner.pixel_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `pixel_format` property after validation.
        pub fn set_pixel_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_pixel_format(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TileRenderPipelineColorAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct TileRenderPipelineColorAttachmentDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TileRenderPipelineColorAttachmentDescriptorArray,
    }

    impl TileRenderPipelineColorAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TileRenderPipelineColorAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TileRenderPipelineColorAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TileRenderPipelineColorAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::TileRenderPipelineDescriptor`.
    #[derive(Clone)]
    pub struct TileRenderPipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::TileRenderPipelineDescriptor,
    }

    impl TileRenderPipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::TileRenderPipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::TileRenderPipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::TileRenderPipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_call_stack_depth` property with availability checks.
        pub fn max_call_stack_depth(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_call_stack_depth()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_call_stack_depth` property after validation.
        pub fn set_max_call_stack_depth(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_call_stack_depth(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_threadgroup` property with availability checks.
        pub fn max_total_threads_per_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_threadgroup` property after validation.
        pub fn set_max_total_threads_per_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `raster_sample_count` property with availability checks.
        pub fn raster_sample_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .raster_sample_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `raster_sample_count` property after validation.
        pub fn set_raster_sample_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_raster_sample_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_adding_binary_functions` property with availability checks.
        pub fn support_adding_binary_functions(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_adding_binary_functions()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `threadgroup_size_matches_tile_size` property with availability checks.
        pub fn threadgroup_size_matches_tile_size(&self) -> Result<bool, crate::Error> {
            self.inner
                .threadgroup_size_matches_tile_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `shader_validation` property with availability checks.
        pub fn shader_validation(&self) -> Result<crate::metal::ShaderValidation, crate::Error> {
            self.inner
                .shader_validation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `shader_validation` property after validation.
        pub fn set_shader_validation(
            &self,
            value: crate::metal::ShaderValidation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_shader_validation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_attachments` object property.
        pub fn color_attachments(
            &self,
        ) -> Result<
            Option<crate::metal::TileRenderPipelineColorAttachmentDescriptorArray>,
            crate::Error,
        > {
            self.inner
                .color_attachments()
                .map(|value| {
                    value.map(
                        crate::metal::TileRenderPipelineColorAttachmentDescriptorArray::from_ffi,
                    )
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `linked_functions` object property.
        pub fn linked_functions(
            &self,
        ) -> Result<Option<crate::metal::LinkedFunctions>, crate::Error> {
            self.inner
                .linked_functions()
                .map(|value| value.map(crate::metal::LinkedFunctions::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `linked_functions` object property.
        pub fn set_linked_functions(
            &self,
            value: Option<&crate::metal::LinkedFunctions>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_linked_functions(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `tile_buffers` object property.
        pub fn tile_buffers(
            &self,
        ) -> Result<Option<crate::metal::PipelineBufferDescriptorArray>, crate::Error> {
            self.inner
                .tile_buffers()
                .map(|value| value.map(crate::metal::PipelineBufferDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `tile_function` object property.
        pub fn tile_function(&self) -> Result<Option<crate::metal::Function>, crate::Error> {
            self.inner
                .tile_function()
                .map(|value| value.map(crate::metal::Function::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `tile_function` object property.
        pub fn set_tile_function(
            &self,
            value: Option<&crate::metal::Function>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_tile_function(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_adding_binary_functions(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_adding_binary_functions(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_threadgroup_size_matches_tile_size(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_threadgroup_size_matches_tile_size(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::Type`.
    #[derive(Clone)]
    pub struct Type {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::Type,
    }

    impl Type {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::Type,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::Type` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::Type::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `data_type` property with availability checks.
        pub fn data_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.data_type().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::VertexAttribute`.
    #[derive(Clone)]
    pub struct VertexAttribute {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::VertexAttribute,
    }

    impl VertexAttribute {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VertexAttribute,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::VertexAttribute` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::VertexAttribute::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `attribute_index` property with availability checks.
        pub fn attribute_index(&self) -> Result<usize, crate::Error> {
            self.inner.attribute_index().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_active` property with availability checks.
        pub fn is_active(&self) -> Result<bool, crate::Error> {
            self.inner.is_active().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_patch_control_point_data` property with availability checks.
        pub fn is_patch_control_point_data(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_patch_control_point_data()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_patch_data` property with availability checks.
        pub fn is_patch_data(&self) -> Result<bool, crate::Error> {
            self.inner.is_patch_data().map_err(crate::Error::from_ffi)
        }
        /// Reads the `attribute_type` property with availability checks.
        pub fn attribute_type(&self) -> Result<crate::metal::DataType, crate::Error> {
            self.inner.attribute_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::VertexAttributeDescriptor`.
    #[derive(Clone)]
    pub struct VertexAttributeDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::VertexAttributeDescriptor,
    }

    impl VertexAttributeDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VertexAttributeDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::VertexAttributeDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::VertexAttributeDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `buffer_index` property with availability checks.
        pub fn buffer_index(&self) -> Result<usize, crate::Error> {
            self.inner.buffer_index().map_err(crate::Error::from_ffi)
        }
        /// Writes the `buffer_index` property after validation.
        pub fn set_buffer_index(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_buffer_index(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `offset` property with availability checks.
        pub fn offset(&self) -> Result<usize, crate::Error> {
            self.inner.offset().map_err(crate::Error::from_ffi)
        }
        /// Writes the `offset` property after validation.
        pub fn set_offset(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_offset(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `format` property with availability checks.
        pub fn format(&self) -> Result<crate::metal::VertexFormat, crate::Error> {
            self.inner.format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `format` property after validation.
        pub fn set_format(&self, value: crate::metal::VertexFormat) -> Result<(), crate::Error> {
            self.inner.set_format(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::VertexAttributeDescriptorArray`.
    #[derive(Clone)]
    pub struct VertexAttributeDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::VertexAttributeDescriptorArray,
    }

    impl VertexAttributeDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VertexAttributeDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::VertexAttributeDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::VertexAttributeDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::VertexBufferLayoutDescriptor`.
    #[derive(Clone)]
    pub struct VertexBufferLayoutDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::VertexBufferLayoutDescriptor,
    }

    impl VertexBufferLayoutDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VertexBufferLayoutDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::VertexBufferLayoutDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::VertexBufferLayoutDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `step_rate` property with availability checks.
        pub fn step_rate(&self) -> Result<usize, crate::Error> {
            self.inner.step_rate().map_err(crate::Error::from_ffi)
        }
        /// Writes the `step_rate` property after validation.
        pub fn set_step_rate(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_step_rate(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `stride` property with availability checks.
        pub fn stride(&self) -> Result<usize, crate::Error> {
            self.inner.stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `stride` property after validation.
        pub fn set_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_stride(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `step_function` property with availability checks.
        pub fn step_function(&self) -> Result<crate::metal::VertexStepFunction, crate::Error> {
            self.inner.step_function().map_err(crate::Error::from_ffi)
        }
        /// Writes the `step_function` property after validation.
        pub fn set_step_function(
            &self,
            value: crate::metal::VertexStepFunction,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_step_function(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::VertexBufferLayoutDescriptorArray`.
    #[derive(Clone)]
    pub struct VertexBufferLayoutDescriptorArray {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal::VertexBufferLayoutDescriptorArray,
    }

    impl VertexBufferLayoutDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VertexBufferLayoutDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::VertexBufferLayoutDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::VertexBufferLayoutDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::VertexDescriptor`.
    #[derive(Clone)]
    pub struct VertexDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::VertexDescriptor,
    }

    impl VertexDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VertexDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::VertexDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::VertexDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `attributes` object property.
        pub fn attributes(
            &self,
        ) -> Result<Option<crate::metal::VertexAttributeDescriptorArray>, crate::Error> {
            self.inner
                .attributes()
                .map(|value| value.map(crate::metal::VertexAttributeDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `layouts` object property.
        pub fn layouts(
            &self,
        ) -> Result<Option<crate::metal::VertexBufferLayoutDescriptorArray>, crate::Error> {
            self.inner
                .layouts()
                .map(|value| value.map(crate::metal::VertexBufferLayoutDescriptorArray::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL::VisibleFunctionTable`.
    #[derive(Clone)]
    pub struct VisibleFunctionTable {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::VisibleFunctionTable,
    }

    impl VisibleFunctionTable {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VisibleFunctionTable,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL::VisibleFunctionTableDescriptor`.
    #[derive(Clone)]
    pub struct VisibleFunctionTableDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal::VisibleFunctionTableDescriptor,
    }

    impl VisibleFunctionTableDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal::VisibleFunctionTableDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL::VisibleFunctionTableDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal::VisibleFunctionTableDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `function_count` property with availability checks.
        pub fn function_count(&self) -> Result<usize, crate::Error> {
            self.inner.function_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `function_count` property after validation.
        pub fn set_function_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_function_count(value)
                .map_err(crate::Error::from_ffi)
        }
    }
}

/// Safe generated implementation for the `metal4` canonical module.
pub(crate) mod metal4 {
    /// Canonical safe structure `BufferRange`.
    pub use metal_rust_ffi::__private::structs::BufferRange;
    /// Canonical safe structure `CopySparseBufferMappingOperation`.
    pub use metal_rust_ffi::__private::structs::CopySparseBufferMappingOperation;
    /// Canonical safe structure `CopySparseTextureMappingOperation`.
    pub use metal_rust_ffi::__private::structs::CopySparseTextureMappingOperation;
    /// Canonical safe structure `TimestampHeapEntry`.
    pub use metal_rust_ffi::__private::structs::TimestampHeapEntry;
    /// Canonical safe structure `UpdateSparseBufferMappingOperation`.
    pub use metal_rust_ffi::__private::structs::UpdateSparseBufferMappingOperation;
    /// Canonical safe structure `UpdateSparseTextureMappingOperation`.
    pub use metal_rust_ffi::__private::structs::UpdateSparseTextureMappingOperation;
    /// Canonical safe value type `AlphaToCoverageState`.
    pub use metal_rust_ffi::__private::values::AlphaToCoverageState;
    /// Canonical safe value type `AlphaToOneState`.
    pub use metal_rust_ffi::__private::values::AlphaToOneState;
    /// Canonical safe value type `BinaryFunctionOptions`.
    pub use metal_rust_ffi::__private::values::BinaryFunctionOptions;
    /// Canonical safe value type `BlendState`.
    pub use metal_rust_ffi::__private::values::BlendState;
    /// Canonical safe value type `CommandQueueError`.
    pub use metal_rust_ffi::__private::values::CommandQueueError;
    /// Canonical safe value type `CompilerTaskStatus`.
    pub use metal_rust_ffi::__private::values::CompilerTaskStatus;
    /// Canonical safe value type `CounterHeapType`.
    pub use metal_rust_ffi::__private::values::CounterHeapType;
    /// Canonical safe value type `IndirectCommandBufferSupportState`.
    pub use metal_rust_ffi::__private::values::IndirectCommandBufferSupportState;
    /// Canonical safe value type `LogicalToPhysicalColorAttachmentMappingState`.
    pub use metal_rust_ffi::__private::values::LogicalToPhysicalColorAttachmentMappingState;
    /// Canonical safe value type `PipelineDataSetSerializerConfiguration`.
    pub use metal_rust_ffi::__private::values::PipelineDataSetSerializerConfiguration;
    /// Canonical safe value type `RenderEncoderOptions`.
    pub use metal_rust_ffi::__private::values::RenderEncoderOptions;
    /// Canonical safe value type `ShaderReflection`.
    pub use metal_rust_ffi::__private::values::ShaderReflection;
    /// Canonical safe value type `TimestampGranularity`.
    pub use metal_rust_ffi::__private::values::TimestampGranularity;
    /// Canonical safe value type `VisibilityOptions`.
    pub use metal_rust_ffi::__private::values::VisibilityOptions;
    /// Safe owned facade for `MTL4::AccelerationStructureBoundingBoxGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureBoundingBoxGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureBoundingBoxGeometryDescriptor,
    }

    impl AccelerationStructureBoundingBoxGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureBoundingBoxGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureBoundingBoxGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureBoundingBoxGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_count` property with availability checks.
        pub fn bounding_box_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_count` property after validation.
        pub fn set_bounding_box_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_stride` property with availability checks.
        pub fn bounding_box_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_stride` property after validation.
        pub fn set_bounding_box_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_stride(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::AccelerationStructureCurveGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureCurveGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureCurveGeometryDescriptor,
    }

    impl AccelerationStructureCurveGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureCurveGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureCurveGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureCurveGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_count` property with availability checks.
        pub fn control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_count` property after validation.
        pub fn set_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_stride` property with availability checks.
        pub fn control_point_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_stride` property after validation.
        pub fn set_control_point_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_stride` property with availability checks.
        pub fn radius_stride(&self) -> Result<usize, crate::Error> {
            self.inner.radius_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_stride` property after validation.
        pub fn set_radius_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_radius_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_control_point_count` property with availability checks.
        pub fn segment_control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .segment_control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_control_point_count` property after validation.
        pub fn set_segment_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_count` property with availability checks.
        pub fn segment_count(&self) -> Result<usize, crate::Error> {
            self.inner.segment_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_count` property after validation.
        pub fn set_segment_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_format` property with availability checks.
        pub fn control_point_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner
                .control_point_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_format` property after validation.
        pub fn set_control_point_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_basis` property with availability checks.
        pub fn curve_basis(&self) -> Result<crate::metal::CurveBasis, crate::Error> {
            self.inner.curve_basis().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_basis` property after validation.
        pub fn set_curve_basis(&self, value: crate::metal::CurveBasis) -> Result<(), crate::Error> {
            self.inner
                .set_curve_basis(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_end_caps` property with availability checks.
        pub fn curve_end_caps(&self) -> Result<crate::metal::CurveEndCaps, crate::Error> {
            self.inner.curve_end_caps().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_end_caps` property after validation.
        pub fn set_curve_end_caps(
            &self,
            value: crate::metal::CurveEndCaps,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_curve_end_caps(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_type` property with availability checks.
        pub fn curve_type(&self) -> Result<crate::metal::CurveType, crate::Error> {
            self.inner.curve_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_type` property after validation.
        pub fn set_curve_type(&self, value: crate::metal::CurveType) -> Result<(), crate::Error> {
            self.inner
                .set_curve_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_format` property with availability checks.
        pub fn radius_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.radius_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_format` property after validation.
        pub fn set_radius_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_radius_format(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::AccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureDescriptor,
    }

    impl AccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::AccelerationStructureGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureGeometryDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureGeometryDescriptor,
    }

    impl AccelerationStructureGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `allow_duplicate_intersection_function_invocation` property with availability checks.
        pub fn allow_duplicate_intersection_function_invocation(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .allow_duplicate_intersection_function_invocation()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `intersection_function_table_offset` property with availability checks.
        pub fn intersection_function_table_offset(&self) -> Result<usize, crate::Error> {
            self.inner
                .intersection_function_table_offset()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `intersection_function_table_offset` property after validation.
        pub fn set_intersection_function_table_offset(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_intersection_function_table_offset(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `opaque` property with availability checks.
        pub fn opaque(&self) -> Result<bool, crate::Error> {
            self.inner.opaque().map_err(crate::Error::from_ffi)
        }
        /// Reads the `primitive_data_element_size` property with availability checks.
        pub fn primitive_data_element_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .primitive_data_element_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `primitive_data_element_size` property after validation.
        pub fn set_primitive_data_element_size(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_primitive_data_element_size(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `primitive_data_stride` property with availability checks.
        pub fn primitive_data_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .primitive_data_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `primitive_data_stride` property after validation.
        pub fn set_primitive_data_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_primitive_data_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_allow_duplicate_intersection_function_invocation(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_allow_duplicate_intersection_function_invocation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_opaque(&self, value: bool) -> Result<(), crate::Error> {
            self.inner.set_opaque(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::AccelerationStructureMotionBoundingBoxGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureMotionBoundingBoxGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionBoundingBoxGeometryDescriptor,
    }

    impl AccelerationStructureMotionBoundingBoxGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionBoundingBoxGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureMotionBoundingBoxGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionBoundingBoxGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_count` property with availability checks.
        pub fn bounding_box_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_count` property after validation.
        pub fn set_bounding_box_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `bounding_box_stride` property with availability checks.
        pub fn bounding_box_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .bounding_box_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `bounding_box_stride` property after validation.
        pub fn set_bounding_box_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_bounding_box_stride(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::AccelerationStructureMotionCurveGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureMotionCurveGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionCurveGeometryDescriptor,
    }

    impl AccelerationStructureMotionCurveGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionCurveGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureMotionCurveGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionCurveGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_count` property with availability checks.
        pub fn control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_count` property after validation.
        pub fn set_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_stride` property with availability checks.
        pub fn control_point_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .control_point_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_stride` property after validation.
        pub fn set_control_point_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_stride` property with availability checks.
        pub fn radius_stride(&self) -> Result<usize, crate::Error> {
            self.inner.radius_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_stride` property after validation.
        pub fn set_radius_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_radius_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_control_point_count` property with availability checks.
        pub fn segment_control_point_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .segment_control_point_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_control_point_count` property after validation.
        pub fn set_segment_control_point_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_control_point_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `segment_count` property with availability checks.
        pub fn segment_count(&self) -> Result<usize, crate::Error> {
            self.inner.segment_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `segment_count` property after validation.
        pub fn set_segment_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_segment_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `control_point_format` property with availability checks.
        pub fn control_point_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner
                .control_point_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `control_point_format` property after validation.
        pub fn set_control_point_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_control_point_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_basis` property with availability checks.
        pub fn curve_basis(&self) -> Result<crate::metal::CurveBasis, crate::Error> {
            self.inner.curve_basis().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_basis` property after validation.
        pub fn set_curve_basis(&self, value: crate::metal::CurveBasis) -> Result<(), crate::Error> {
            self.inner
                .set_curve_basis(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_end_caps` property with availability checks.
        pub fn curve_end_caps(&self) -> Result<crate::metal::CurveEndCaps, crate::Error> {
            self.inner.curve_end_caps().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_end_caps` property after validation.
        pub fn set_curve_end_caps(
            &self,
            value: crate::metal::CurveEndCaps,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_curve_end_caps(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `curve_type` property with availability checks.
        pub fn curve_type(&self) -> Result<crate::metal::CurveType, crate::Error> {
            self.inner.curve_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `curve_type` property after validation.
        pub fn set_curve_type(&self, value: crate::metal::CurveType) -> Result<(), crate::Error> {
            self.inner
                .set_curve_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `radius_format` property with availability checks.
        pub fn radius_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.radius_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `radius_format` property after validation.
        pub fn set_radius_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_radius_format(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::AccelerationStructureMotionTriangleGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureMotionTriangleGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionTriangleGeometryDescriptor,
    }

    impl AccelerationStructureMotionTriangleGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionTriangleGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureMotionTriangleGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureMotionTriangleGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `triangle_count` property with availability checks.
        pub fn triangle_count(&self) -> Result<usize, crate::Error> {
            self.inner.triangle_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `triangle_count` property after validation.
        pub fn set_triangle_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_triangle_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_stride` property with availability checks.
        pub fn vertex_stride(&self) -> Result<usize, crate::Error> {
            self.inner.vertex_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_stride` property after validation.
        pub fn set_vertex_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transformation_matrix_layout` property with availability checks.
        pub fn transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_layout` property after validation.
        pub fn set_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_format` property with availability checks.
        pub fn vertex_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.vertex_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_format` property after validation.
        pub fn set_vertex_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_format(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::AccelerationStructureTriangleGeometryDescriptor`.
    #[derive(Clone)]
    pub struct AccelerationStructureTriangleGeometryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureTriangleGeometryDescriptor,
    }

    impl AccelerationStructureTriangleGeometryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::AccelerationStructureTriangleGeometryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::AccelerationStructureTriangleGeometryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::AccelerationStructureTriangleGeometryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `triangle_count` property with availability checks.
        pub fn triangle_count(&self) -> Result<usize, crate::Error> {
            self.inner.triangle_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `triangle_count` property after validation.
        pub fn set_triangle_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_triangle_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_stride` property with availability checks.
        pub fn vertex_stride(&self) -> Result<usize, crate::Error> {
            self.inner.vertex_stride().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_stride` property after validation.
        pub fn set_vertex_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `index_type` property with availability checks.
        pub fn index_type(&self) -> Result<crate::metal::IndexType, crate::Error> {
            self.inner.index_type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `index_type` property after validation.
        pub fn set_index_type(&self, value: crate::metal::IndexType) -> Result<(), crate::Error> {
            self.inner
                .set_index_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transformation_matrix_layout` property with availability checks.
        pub fn transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transformation_matrix_layout` property after validation.
        pub fn set_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `vertex_format` property with availability checks.
        pub fn vertex_format(&self) -> Result<crate::metal::AttributeFormat, crate::Error> {
            self.inner.vertex_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_format` property after validation.
        pub fn set_vertex_format(
            &self,
            value: crate::metal::AttributeFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_format(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::Archive`.
    #[derive(Clone)]
    pub struct Archive {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::Archive,
    }

    impl Archive {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::Archive,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::ArgumentTable`.
    #[derive(Clone)]
    pub struct ArgumentTable {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::ArgumentTable,
    }

    impl ArgumentTable {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::ArgumentTable,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::ArgumentTableDescriptor`.
    #[derive(Clone)]
    pub struct ArgumentTableDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::ArgumentTableDescriptor,
    }

    impl ArgumentTableDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::ArgumentTableDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::ArgumentTableDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::ArgumentTableDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `initialize_bindings` property with availability checks.
        pub fn initialize_bindings(&self) -> Result<bool, crate::Error> {
            self.inner
                .initialize_bindings()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_buffer_bind_count` property with availability checks.
        pub fn max_buffer_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_buffer_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_buffer_bind_count` property after validation.
        pub fn set_max_buffer_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_buffer_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_sampler_state_bind_count` property with availability checks.
        pub fn max_sampler_state_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_sampler_state_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_sampler_state_bind_count` property after validation.
        pub fn set_max_sampler_state_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_sampler_state_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_texture_bind_count` property with availability checks.
        pub fn max_texture_bind_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_texture_bind_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_texture_bind_count` property after validation.
        pub fn set_max_texture_bind_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_texture_bind_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_attribute_strides` property with availability checks.
        pub fn support_attribute_strides(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_attribute_strides()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_initialize_bindings(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_initialize_bindings(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_attribute_strides(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_attribute_strides(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::BinaryFunction`.
    #[derive(Clone)]
    pub struct BinaryFunction {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::BinaryFunction,
    }

    impl BinaryFunction {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::BinaryFunction,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `function_type` property with availability checks.
        pub fn function_type(&self) -> Result<crate::metal::FunctionType, crate::Error> {
            self.inner.function_type().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::BinaryFunctionDescriptor`.
    #[derive(Clone)]
    pub struct BinaryFunctionDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::BinaryFunctionDescriptor,
    }

    impl BinaryFunctionDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::BinaryFunctionDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::BinaryFunctionDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::BinaryFunctionDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `options` property with availability checks.
        pub fn options(&self) -> Result<crate::metal4::BinaryFunctionOptions, crate::Error> {
            self.inner.options().map_err(crate::Error::from_ffi)
        }
        /// Writes the `options` property after validation.
        pub fn set_options(
            &self,
            value: crate::metal4::BinaryFunctionOptions,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_options(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `function_descriptor` object property.
        pub fn function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `function_descriptor` object property.
        pub fn set_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
        /// Writes the `name` string property.
        pub fn set_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_name(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CommandAllocator`.
    #[derive(Clone)]
    pub struct CommandAllocator {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CommandAllocator,
    }

    impl CommandAllocator {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CommandAllocator,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CommandAllocatorDescriptor`.
    #[derive(Clone)]
    pub struct CommandAllocatorDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CommandAllocatorDescriptor,
    }

    impl CommandAllocatorDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CommandAllocatorDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::CommandAllocatorDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::CommandAllocatorDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CommandBufferOptions`.
    #[derive(Clone)]
    pub struct CommandBufferOptions {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CommandBufferOptions,
    }

    impl CommandBufferOptions {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CommandBufferOptions,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::CommandBufferOptions` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::CommandBufferOptions::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `log_state` object property.
        pub fn log_state(&self) -> Result<Option<crate::metal::LogState>, crate::Error> {
            self.inner
                .log_state()
                .map(|value| value.map(crate::metal::LogState::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `log_state` object property.
        pub fn set_log_state(
            &self,
            value: Option<&crate::metal::LogState>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_log_state(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CommandQueue`.
    #[derive(Clone)]
    pub struct CommandQueue {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CommandQueue,
    }

    impl CommandQueue {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CommandQueue,
        ) -> Self {
            Self { inner }
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CommandQueueDescriptor`.
    #[derive(Clone)]
    pub struct CommandQueueDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CommandQueueDescriptor,
    }

    impl CommandQueueDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CommandQueueDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::CommandQueueDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::CommandQueueDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CommitFeedback`.
    #[derive(Clone)]
    pub struct CommitFeedback {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CommitFeedback,
    }

    impl CommitFeedback {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CommitFeedback,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `gpu_end_time` property with availability checks.
        pub fn gpu_end_time(&self) -> Result<f64, crate::Error> {
            self.inner.gpu_end_time().map_err(crate::Error::from_ffi)
        }
        /// Reads the `gpu_start_time` property with availability checks.
        pub fn gpu_start_time(&self) -> Result<f64, crate::Error> {
            self.inner.gpu_start_time().map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CommitOptions`.
    #[derive(Clone)]
    pub struct CommitOptions {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CommitOptions,
    }

    impl CommitOptions {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CommitOptions,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::CommitOptions` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::CommitOptions::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::Compiler`.
    #[derive(Clone)]
    pub struct Compiler {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::Compiler,
    }

    impl Compiler {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::Compiler,
        ) -> Self {
            Self { inner }
        }
        pub(crate) fn as_ffi(&self) -> &metal_rust_ffi::__private::objects::metal4::Compiler {
            &self.inner
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `pipeline_data_set_serializer` object property.
        pub fn pipeline_data_set_serializer(
            &self,
        ) -> Result<Option<crate::metal4::PipelineDataSetSerializer>, crate::Error> {
            self.inner
                .pipeline_data_set_serializer()
                .map(|value| value.map(crate::metal4::PipelineDataSetSerializer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CompilerDescriptor`.
    #[derive(Clone)]
    pub struct CompilerDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CompilerDescriptor,
    }

    impl CompilerDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CompilerDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::CompilerDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::CompilerDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `pipeline_data_set_serializer` object property.
        pub fn pipeline_data_set_serializer(
            &self,
        ) -> Result<Option<crate::metal4::PipelineDataSetSerializer>, crate::Error> {
            self.inner
                .pipeline_data_set_serializer()
                .map(|value| value.map(crate::metal4::PipelineDataSetSerializer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `pipeline_data_set_serializer` object property.
        pub fn set_pipeline_data_set_serializer(
            &self,
            value: Option<&crate::metal4::PipelineDataSetSerializer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_pipeline_data_set_serializer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CompilerTask`.
    #[derive(Clone)]
    pub struct CompilerTask {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CompilerTask,
    }

    impl CompilerTask {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CompilerTask,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `status` property with availability checks.
        pub fn status(&self) -> Result<crate::metal4::CompilerTaskStatus, crate::Error> {
            self.inner.status().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `compiler` object property.
        pub fn compiler(&self) -> Result<Option<crate::metal4::Compiler>, crate::Error> {
            self.inner
                .compiler()
                .map(|value| value.map(crate::metal4::Compiler::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CompilerTaskOptions`.
    #[derive(Clone)]
    pub struct CompilerTaskOptions {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CompilerTaskOptions,
    }

    impl CompilerTaskOptions {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CompilerTaskOptions,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::CompilerTaskOptions` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::CompilerTaskOptions::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::ComputePipelineDescriptor`.
    #[derive(Clone)]
    pub struct ComputePipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::ComputePipelineDescriptor,
    }

    impl ComputePipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::ComputePipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::ComputePipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::ComputePipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_threadgroup` property with availability checks.
        pub fn max_total_threads_per_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_threadgroup` property after validation.
        pub fn set_max_total_threads_per_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_binary_linking` property with availability checks.
        pub fn support_binary_linking(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_binary_linking()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `thread_group_size_is_multiple_of_thread_execution_width` property with availability checks.
        pub fn thread_group_size_is_multiple_of_thread_execution_width(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .thread_group_size_is_multiple_of_thread_execution_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_indirect_command_buffers` property with availability checks.
        pub fn support_indirect_command_buffers(
            &self,
        ) -> Result<crate::metal4::IndirectCommandBufferSupportState, crate::Error> {
            self.inner
                .support_indirect_command_buffers()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `support_indirect_command_buffers` property after validation.
        pub fn set_support_indirect_command_buffers(
            &self,
            value: crate::metal4::IndirectCommandBufferSupportState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_indirect_command_buffers(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `compute_function_descriptor` object property.
        pub fn compute_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .compute_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `compute_function_descriptor` object property.
        pub fn set_compute_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_compute_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `static_linking_descriptor` object property.
        pub fn static_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::StaticLinkingDescriptor>, crate::Error> {
            self.inner
                .static_linking_descriptor()
                .map(|value| value.map(crate::metal4::StaticLinkingDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `static_linking_descriptor` object property.
        pub fn set_static_linking_descriptor(
            &self,
            value: Option<&crate::metal4::StaticLinkingDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_static_linking_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_binary_linking(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_binary_linking(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_thread_group_size_is_multiple_of_thread_execution_width(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_thread_group_size_is_multiple_of_thread_execution_width(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::CounterHeapDescriptor`.
    #[derive(Clone)]
    pub struct CounterHeapDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::CounterHeapDescriptor,
    }

    impl CounterHeapDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::CounterHeapDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::CounterHeapDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::CounterHeapDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `count` property with availability checks.
        pub fn count(&self) -> Result<usize, crate::Error> {
            self.inner.count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `count` property after validation.
        pub fn set_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner.set_count(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the `r#type` property with availability checks.
        pub fn r#type(&self) -> Result<crate::metal4::CounterHeapType, crate::Error> {
            self.inner.r#type().map_err(crate::Error::from_ffi)
        }
        /// Writes the `r#type` property after validation.
        pub fn set_type(&self, value: crate::metal4::CounterHeapType) -> Result<(), crate::Error> {
            self.inner.set_type(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::FunctionDescriptor`.
    #[derive(Clone)]
    pub struct FunctionDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::FunctionDescriptor,
    }

    impl FunctionDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::FunctionDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::FunctionDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::FunctionDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::IndirectInstanceAccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct IndirectInstanceAccelerationStructureDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::IndirectInstanceAccelerationStructureDescriptor,
    }

    impl IndirectInstanceAccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::IndirectInstanceAccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::IndirectInstanceAccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::IndirectInstanceAccelerationStructureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_stride` property with availability checks.
        pub fn instance_descriptor_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .instance_descriptor_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_stride` property after validation.
        pub fn set_instance_descriptor_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_instance_count` property with availability checks.
        pub fn max_instance_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_instance_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_instance_count` property after validation.
        pub fn set_max_instance_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_instance_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_motion_transform_count` property with availability checks.
        pub fn max_motion_transform_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_motion_transform_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_motion_transform_count` property after validation.
        pub fn set_max_motion_transform_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_motion_transform_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_stride` property with availability checks.
        pub fn motion_transform_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_stride` property after validation.
        pub fn set_motion_transform_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_type` property with availability checks.
        pub fn instance_descriptor_type(
            &self,
        ) -> Result<crate::metal::AccelerationStructureInstanceDescriptorType, crate::Error>
        {
            self.inner
                .instance_descriptor_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_type` property after validation.
        pub fn set_instance_descriptor_type(
            &self,
            value: crate::metal::AccelerationStructureInstanceDescriptorType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_transformation_matrix_layout` property with availability checks.
        pub fn instance_transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .instance_transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_transformation_matrix_layout` property after validation.
        pub fn set_instance_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_type` property with availability checks.
        pub fn motion_transform_type(&self) -> Result<crate::metal::TransformType, crate::Error> {
            self.inner
                .motion_transform_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_type` property after validation.
        pub fn set_motion_transform_type(
            &self,
            value: crate::metal::TransformType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_type(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::InstanceAccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct InstanceAccelerationStructureDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::InstanceAccelerationStructureDescriptor,
    }

    impl InstanceAccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::InstanceAccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::InstanceAccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::InstanceAccelerationStructureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_count` property with availability checks.
        pub fn instance_count(&self) -> Result<usize, crate::Error> {
            self.inner.instance_count().map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_count` property after validation.
        pub fn set_instance_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_instance_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_stride` property with availability checks.
        pub fn instance_descriptor_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .instance_descriptor_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_stride` property after validation.
        pub fn set_instance_descriptor_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_count` property with availability checks.
        pub fn motion_transform_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_count` property after validation.
        pub fn set_motion_transform_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_stride` property with availability checks.
        pub fn motion_transform_stride(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_transform_stride()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_stride` property after validation.
        pub fn set_motion_transform_stride(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_stride(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_descriptor_type` property with availability checks.
        pub fn instance_descriptor_type(
            &self,
        ) -> Result<crate::metal::AccelerationStructureInstanceDescriptorType, crate::Error>
        {
            self.inner
                .instance_descriptor_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_descriptor_type` property after validation.
        pub fn set_instance_descriptor_type(
            &self,
            value: crate::metal::AccelerationStructureInstanceDescriptorType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_descriptor_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `instance_transformation_matrix_layout` property with availability checks.
        pub fn instance_transformation_matrix_layout(
            &self,
        ) -> Result<crate::metal::MatrixLayout, crate::Error> {
            self.inner
                .instance_transformation_matrix_layout()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `instance_transformation_matrix_layout` property after validation.
        pub fn set_instance_transformation_matrix_layout(
            &self,
            value: crate::metal::MatrixLayout,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_instance_transformation_matrix_layout(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_transform_type` property with availability checks.
        pub fn motion_transform_type(&self) -> Result<crate::metal::TransformType, crate::Error> {
            self.inner
                .motion_transform_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_transform_type` property after validation.
        pub fn set_motion_transform_type(
            &self,
            value: crate::metal::TransformType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_transform_type(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::LibraryDescriptor`.
    #[derive(Clone)]
    pub struct LibraryDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::LibraryDescriptor,
    }

    impl LibraryDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::LibraryDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::LibraryDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::LibraryDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
        /// Writes the `name` string property.
        pub fn set_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_name(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `options` object property.
        pub fn options(&self) -> Result<Option<crate::metal::CompileOptions>, crate::Error> {
            self.inner
                .options()
                .map(|value| value.map(crate::metal::CompileOptions::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `options` object property.
        pub fn set_options(
            &self,
            value: Option<&crate::metal::CompileOptions>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_options(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `source` string property.
        pub fn source(&self) -> Result<Option<String>, crate::Error> {
            self.inner.source().map_err(crate::Error::from_ffi)
        }
        /// Writes the `source` string property.
        pub fn set_source(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_source(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::LibraryFunctionDescriptor`.
    #[derive(Clone)]
    pub struct LibraryFunctionDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::LibraryFunctionDescriptor,
    }

    impl LibraryFunctionDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::LibraryFunctionDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::LibraryFunctionDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::LibraryFunctionDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `library` object property.
        pub fn library(&self) -> Result<Option<crate::metal::Library>, crate::Error> {
            self.inner
                .library()
                .map(|value| value.map(crate::metal::Library::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `library` object property.
        pub fn set_library(
            &self,
            value: Option<&crate::metal::Library>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_library(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `name` string property.
        pub fn name(&self) -> Result<Option<String>, crate::Error> {
            self.inner.name().map_err(crate::Error::from_ffi)
        }
        /// Writes the `name` string property.
        pub fn set_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_name(value).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::MachineLearningPipelineDescriptor`.
    #[derive(Clone)]
    pub struct MachineLearningPipelineDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineDescriptor,
    }

    impl MachineLearningPipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::MachineLearningPipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `machine_learning_function_descriptor` object property.
        pub fn machine_learning_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .machine_learning_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `machine_learning_function_descriptor` object property.
        pub fn set_machine_learning_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_machine_learning_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::MachineLearningPipelineReflection`.
    #[derive(Clone)]
    pub struct MachineLearningPipelineReflection {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineReflection,
    }

    impl MachineLearningPipelineReflection {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineReflection,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::MachineLearningPipelineReflection` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineReflection::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::MachineLearningPipelineState`.
    #[derive(Clone)]
    pub struct MachineLearningPipelineState {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineState,
    }

    impl MachineLearningPipelineState {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::MachineLearningPipelineState,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `intermediates_heap_size` property with availability checks.
        pub fn intermediates_heap_size(&self) -> Result<usize, crate::Error> {
            self.inner
                .intermediates_heap_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `device` object property.
        pub fn device(&self) -> Result<Option<crate::metal::Device>, crate::Error> {
            self.inner
                .device()
                .map(|value| value.map(crate::metal::Device::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `reflection` object property.
        pub fn reflection(
            &self,
        ) -> Result<Option<crate::metal4::MachineLearningPipelineReflection>, crate::Error>
        {
            self.inner
                .reflection()
                .map(|value| value.map(crate::metal4::MachineLearningPipelineReflection::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::MeshRenderPipelineDescriptor`.
    #[derive(Clone)]
    pub struct MeshRenderPipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::MeshRenderPipelineDescriptor,
    }

    impl MeshRenderPipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::MeshRenderPipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::MeshRenderPipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::MeshRenderPipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_rasterization_enabled` property with availability checks.
        pub fn is_rasterization_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_rasterization_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threadgroups_per_mesh_grid` property with availability checks.
        pub fn max_total_threadgroups_per_mesh_grid(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threadgroups_per_mesh_grid()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threadgroups_per_mesh_grid` property after validation.
        pub fn set_max_total_threadgroups_per_mesh_grid(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threadgroups_per_mesh_grid(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_mesh_threadgroup` property with availability checks.
        pub fn max_total_threads_per_mesh_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_mesh_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_mesh_threadgroup` property after validation.
        pub fn set_max_total_threads_per_mesh_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_mesh_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_object_threadgroup` property with availability checks.
        pub fn max_total_threads_per_object_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_object_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_object_threadgroup` property after validation.
        pub fn set_max_total_threads_per_object_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_object_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_vertex_amplification_count` property with availability checks.
        pub fn max_vertex_amplification_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_vertex_amplification_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_vertex_amplification_count` property after validation.
        pub fn set_max_vertex_amplification_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_vertex_amplification_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `mesh_threadgroup_size_is_multiple_of_thread_execution_width` property with availability checks.
        pub fn mesh_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .mesh_threadgroup_size_is_multiple_of_thread_execution_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `object_threadgroup_size_is_multiple_of_thread_execution_width` property with availability checks.
        pub fn object_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
        ) -> Result<bool, crate::Error> {
            self.inner
                .object_threadgroup_size_is_multiple_of_thread_execution_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `payload_memory_length` property with availability checks.
        pub fn payload_memory_length(&self) -> Result<usize, crate::Error> {
            self.inner
                .payload_memory_length()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `payload_memory_length` property after validation.
        pub fn set_payload_memory_length(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_payload_memory_length(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `raster_sample_count` property with availability checks.
        pub fn raster_sample_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .raster_sample_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `raster_sample_count` property after validation.
        pub fn set_raster_sample_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_raster_sample_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_fragment_binary_linking` property with availability checks.
        pub fn support_fragment_binary_linking(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_fragment_binary_linking()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_mesh_binary_linking` property with availability checks.
        pub fn support_mesh_binary_linking(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_mesh_binary_linking()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_object_binary_linking` property with availability checks.
        pub fn support_object_binary_linking(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_object_binary_linking()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `alpha_to_coverage_state` property with availability checks.
        pub fn alpha_to_coverage_state(
            &self,
        ) -> Result<crate::metal4::AlphaToCoverageState, crate::Error> {
            self.inner
                .alpha_to_coverage_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `alpha_to_coverage_state` property after validation.
        pub fn set_alpha_to_coverage_state(
            &self,
            value: crate::metal4::AlphaToCoverageState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_to_coverage_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `alpha_to_one_state` property with availability checks.
        pub fn alpha_to_one_state(&self) -> Result<crate::metal4::AlphaToOneState, crate::Error> {
            self.inner
                .alpha_to_one_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `alpha_to_one_state` property after validation.
        pub fn set_alpha_to_one_state(
            &self,
            value: crate::metal4::AlphaToOneState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_to_one_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_attachment_mapping_state` property with availability checks.
        pub fn color_attachment_mapping_state(
            &self,
        ) -> Result<crate::metal4::LogicalToPhysicalColorAttachmentMappingState, crate::Error>
        {
            self.inner
                .color_attachment_mapping_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_attachment_mapping_state` property after validation.
        pub fn set_color_attachment_mapping_state(
            &self,
            value: crate::metal4::LogicalToPhysicalColorAttachmentMappingState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_attachment_mapping_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_indirect_command_buffers` property with availability checks.
        pub fn support_indirect_command_buffers(
            &self,
        ) -> Result<crate::metal4::IndirectCommandBufferSupportState, crate::Error> {
            self.inner
                .support_indirect_command_buffers()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `support_indirect_command_buffers` property after validation.
        pub fn set_support_indirect_command_buffers(
            &self,
            value: crate::metal4::IndirectCommandBufferSupportState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_indirect_command_buffers(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_attachments` object property.
        pub fn color_attachments(
            &self,
        ) -> Result<Option<crate::metal4::RenderPipelineColorAttachmentDescriptorArray>, crate::Error>
        {
            self.inner
                .color_attachments()
                .map(|value| {
                    value.map(crate::metal4::RenderPipelineColorAttachmentDescriptorArray::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_function_descriptor` object property.
        pub fn fragment_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .fragment_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fragment_function_descriptor` object property.
        pub fn set_fragment_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_fragment_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_static_linking_descriptor` object property.
        pub fn fragment_static_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::StaticLinkingDescriptor>, crate::Error> {
            self.inner
                .fragment_static_linking_descriptor()
                .map(|value| value.map(crate::metal4::StaticLinkingDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fragment_static_linking_descriptor` object property.
        pub fn set_fragment_static_linking_descriptor(
            &self,
            value: Option<&crate::metal4::StaticLinkingDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_fragment_static_linking_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `mesh_function_descriptor` object property.
        pub fn mesh_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .mesh_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `mesh_function_descriptor` object property.
        pub fn set_mesh_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mesh_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `mesh_static_linking_descriptor` object property.
        pub fn mesh_static_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::StaticLinkingDescriptor>, crate::Error> {
            self.inner
                .mesh_static_linking_descriptor()
                .map(|value| value.map(crate::metal4::StaticLinkingDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `mesh_static_linking_descriptor` object property.
        pub fn set_mesh_static_linking_descriptor(
            &self,
            value: Option<&crate::metal4::StaticLinkingDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mesh_static_linking_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `object_function_descriptor` object property.
        pub fn object_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .object_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `object_function_descriptor` object property.
        pub fn set_object_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_object_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `object_static_linking_descriptor` object property.
        pub fn object_static_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::StaticLinkingDescriptor>, crate::Error> {
            self.inner
                .object_static_linking_descriptor()
                .map(|value| value.map(crate::metal4::StaticLinkingDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `object_static_linking_descriptor` object property.
        pub fn set_object_static_linking_descriptor(
            &self,
            value: Option<&crate::metal4::StaticLinkingDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_object_static_linking_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_mesh_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_mesh_threadgroup_size_is_multiple_of_thread_execution_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_object_threadgroup_size_is_multiple_of_thread_execution_width(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_object_threadgroup_size_is_multiple_of_thread_execution_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_rasterization_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_rasterization_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_fragment_binary_linking(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_fragment_binary_linking(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_mesh_binary_linking(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_mesh_binary_linking(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_object_binary_linking(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_object_binary_linking(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::PipelineDataSetSerializer`.
    #[derive(Clone)]
    pub struct PipelineDataSetSerializer {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::PipelineDataSetSerializer,
    }

    impl PipelineDataSetSerializer {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::PipelineDataSetSerializer,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTL4::PipelineDataSetSerializerDescriptor`.
    #[derive(Clone)]
    pub struct PipelineDataSetSerializerDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::PipelineDataSetSerializerDescriptor,
    }

    impl PipelineDataSetSerializerDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::PipelineDataSetSerializerDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::PipelineDataSetSerializerDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::PipelineDataSetSerializerDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `configuration` property with availability checks.
        pub fn configuration(
            &self,
        ) -> Result<crate::metal4::PipelineDataSetSerializerConfiguration, crate::Error> {
            self.inner.configuration().map_err(crate::Error::from_ffi)
        }
        /// Writes the `configuration` property after validation.
        pub fn set_configuration(
            &self,
            value: crate::metal4::PipelineDataSetSerializerConfiguration,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_configuration(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::PipelineDescriptor`.
    #[derive(Clone)]
    pub struct PipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::PipelineDescriptor,
    }

    impl PipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::PipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::PipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::PipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `label` string property.
        pub fn label(&self) -> Result<Option<String>, crate::Error> {
            self.inner.label().map_err(crate::Error::from_ffi)
        }
        /// Writes the `label` string property.
        pub fn set_label(&self, value: &str) -> Result<(), crate::Error> {
            self.inner.set_label(value).map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `options` object property.
        pub fn options(&self) -> Result<Option<crate::metal4::PipelineOptions>, crate::Error> {
            self.inner
                .options()
                .map(|value| value.map(crate::metal4::PipelineOptions::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `options` object property.
        pub fn set_options(
            &self,
            value: Option<&crate::metal4::PipelineOptions>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_options(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::PipelineOptions`.
    #[derive(Clone)]
    pub struct PipelineOptions {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::PipelineOptions,
    }

    impl PipelineOptions {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::PipelineOptions,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::PipelineOptions` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::PipelineOptions::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `shader_reflection` property with availability checks.
        pub fn shader_reflection(&self) -> Result<crate::metal4::ShaderReflection, crate::Error> {
            self.inner
                .shader_reflection()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `shader_reflection` property after validation.
        pub fn set_shader_reflection(
            &self,
            value: crate::metal4::ShaderReflection,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_shader_reflection(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `shader_validation` property with availability checks.
        pub fn shader_validation(&self) -> Result<crate::metal::ShaderValidation, crate::Error> {
            self.inner
                .shader_validation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `shader_validation` property after validation.
        pub fn set_shader_validation(
            &self,
            value: crate::metal::ShaderValidation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_shader_validation(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::PipelineStageDynamicLinkingDescriptor`.
    #[derive(Clone)]
    pub struct PipelineStageDynamicLinkingDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::PipelineStageDynamicLinkingDescriptor,
    }

    impl PipelineStageDynamicLinkingDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::PipelineStageDynamicLinkingDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::PipelineStageDynamicLinkingDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::PipelineStageDynamicLinkingDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_call_stack_depth` property with availability checks.
        pub fn max_call_stack_depth(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_call_stack_depth()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_call_stack_depth` property after validation.
        pub fn set_max_call_stack_depth(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_call_stack_depth(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::PrimitiveAccelerationStructureDescriptor`.
    #[derive(Clone)]
    pub struct PrimitiveAccelerationStructureDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::PrimitiveAccelerationStructureDescriptor,
    }

    impl PrimitiveAccelerationStructureDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::PrimitiveAccelerationStructureDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::PrimitiveAccelerationStructureDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::PrimitiveAccelerationStructureDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_end_time` property with availability checks.
        pub fn motion_end_time(&self) -> Result<f32, crate::Error> {
            self.inner.motion_end_time().map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_end_time` property after validation.
        pub fn set_motion_end_time(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_end_time(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_keyframe_count` property with availability checks.
        pub fn motion_keyframe_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_keyframe_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_keyframe_count` property after validation.
        pub fn set_motion_keyframe_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_keyframe_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_start_time` property with availability checks.
        pub fn motion_start_time(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_start_time()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_start_time` property after validation.
        pub fn set_motion_start_time(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_start_time(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_end_border_mode` property with availability checks.
        pub fn motion_end_border_mode(
            &self,
        ) -> Result<crate::metal::MotionBorderMode, crate::Error> {
            self.inner
                .motion_end_border_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_end_border_mode` property after validation.
        pub fn set_motion_end_border_mode(
            &self,
            value: crate::metal::MotionBorderMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_end_border_mode(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_start_border_mode` property with availability checks.
        pub fn motion_start_border_mode(
            &self,
        ) -> Result<crate::metal::MotionBorderMode, crate::Error> {
            self.inner
                .motion_start_border_mode()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_start_border_mode` property after validation.
        pub fn set_motion_start_border_mode(
            &self,
            value: crate::metal::MotionBorderMode,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_start_border_mode(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::RenderPassDescriptor`.
    #[derive(Clone)]
    pub struct RenderPassDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::RenderPassDescriptor,
    }

    impl RenderPassDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::RenderPassDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::RenderPassDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::RenderPassDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `default_raster_sample_count` property with availability checks.
        pub fn default_raster_sample_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .default_raster_sample_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `default_raster_sample_count` property after validation.
        pub fn set_default_raster_sample_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_default_raster_sample_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `imageblock_sample_length` property with availability checks.
        pub fn imageblock_sample_length(&self) -> Result<usize, crate::Error> {
            self.inner
                .imageblock_sample_length()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `imageblock_sample_length` property after validation.
        pub fn set_imageblock_sample_length(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_imageblock_sample_length(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `render_target_array_length` property with availability checks.
        pub fn render_target_array_length(&self) -> Result<usize, crate::Error> {
            self.inner
                .render_target_array_length()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `render_target_array_length` property after validation.
        pub fn set_render_target_array_length(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_render_target_array_length(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `render_target_height` property with availability checks.
        pub fn render_target_height(&self) -> Result<usize, crate::Error> {
            self.inner
                .render_target_height()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `render_target_height` property after validation.
        pub fn set_render_target_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_render_target_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `render_target_width` property with availability checks.
        pub fn render_target_width(&self) -> Result<usize, crate::Error> {
            self.inner
                .render_target_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `render_target_width` property after validation.
        pub fn set_render_target_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_render_target_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_color_attachment_mapping` property with availability checks.
        pub fn support_color_attachment_mapping(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_color_attachment_mapping()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `threadgroup_memory_length` property with availability checks.
        pub fn threadgroup_memory_length(&self) -> Result<usize, crate::Error> {
            self.inner
                .threadgroup_memory_length()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `threadgroup_memory_length` property after validation.
        pub fn set_threadgroup_memory_length(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_threadgroup_memory_length(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `tile_height` property with availability checks.
        pub fn tile_height(&self) -> Result<usize, crate::Error> {
            self.inner.tile_height().map_err(crate::Error::from_ffi)
        }
        /// Writes the `tile_height` property after validation.
        pub fn set_tile_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_tile_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `tile_width` property with availability checks.
        pub fn tile_width(&self) -> Result<usize, crate::Error> {
            self.inner.tile_width().map_err(crate::Error::from_ffi)
        }
        /// Writes the `tile_width` property after validation.
        pub fn set_tile_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_tile_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `visibility_result_type` property with availability checks.
        pub fn visibility_result_type(
            &self,
        ) -> Result<crate::metal::VisibilityResultType, crate::Error> {
            self.inner
                .visibility_result_type()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `visibility_result_type` property after validation.
        pub fn set_visibility_result_type(
            &self,
            value: crate::metal::VisibilityResultType,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_visibility_result_type(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_attachments` object property.
        pub fn color_attachments(
            &self,
        ) -> Result<Option<crate::metal::RenderPassColorAttachmentDescriptorArray>, crate::Error>
        {
            self.inner
                .color_attachments()
                .map(|value| {
                    value.map(crate::metal::RenderPassColorAttachmentDescriptorArray::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `depth_attachment` object property.
        pub fn depth_attachment(
            &self,
        ) -> Result<Option<crate::metal::RenderPassDepthAttachmentDescriptor>, crate::Error>
        {
            self.inner
                .depth_attachment()
                .map(|value| value.map(crate::metal::RenderPassDepthAttachmentDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_attachment` object property.
        pub fn set_depth_attachment(
            &self,
            value: Option<&crate::metal::RenderPassDepthAttachmentDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_attachment(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `rasterization_rate_map` object property.
        pub fn rasterization_rate_map(
            &self,
        ) -> Result<Option<crate::metal::RasterizationRateMap>, crate::Error> {
            self.inner
                .rasterization_rate_map()
                .map(|value| value.map(crate::metal::RasterizationRateMap::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `rasterization_rate_map` object property.
        pub fn set_rasterization_rate_map(
            &self,
            value: Option<&crate::metal::RasterizationRateMap>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_rasterization_rate_map(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `stencil_attachment` object property.
        pub fn stencil_attachment(
            &self,
        ) -> Result<Option<crate::metal::RenderPassStencilAttachmentDescriptor>, crate::Error>
        {
            self.inner
                .stencil_attachment()
                .map(|value| {
                    value.map(crate::metal::RenderPassStencilAttachmentDescriptor::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `stencil_attachment` object property.
        pub fn set_stencil_attachment(
            &self,
            value: Option<&crate::metal::RenderPassStencilAttachmentDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_stencil_attachment(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `visibility_result_buffer` object property.
        pub fn visibility_result_buffer(
            &self,
        ) -> Result<Option<crate::metal::Buffer>, crate::Error> {
            self.inner
                .visibility_result_buffer()
                .map(|value| value.map(crate::metal::Buffer::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `visibility_result_buffer` object property.
        pub fn set_visibility_result_buffer(
            &self,
            value: Option<&crate::metal::Buffer>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_visibility_result_buffer(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_color_attachment_mapping(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_color_attachment_mapping(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::RenderPipelineBinaryFunctionsDescriptor`.
    #[derive(Clone)]
    pub struct RenderPipelineBinaryFunctionsDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::RenderPipelineBinaryFunctionsDescriptor,
    }

    impl RenderPipelineBinaryFunctionsDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::RenderPipelineBinaryFunctionsDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::RenderPipelineBinaryFunctionsDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::RenderPipelineBinaryFunctionsDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::RenderPipelineColorAttachmentDescriptor`.
    #[derive(Clone)]
    pub struct RenderPipelineColorAttachmentDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::RenderPipelineColorAttachmentDescriptor,
    }

    impl RenderPipelineColorAttachmentDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::RenderPipelineColorAttachmentDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::RenderPipelineColorAttachmentDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::RenderPipelineColorAttachmentDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `alpha_blend_operation` property with availability checks.
        pub fn alpha_blend_operation(&self) -> Result<crate::metal::BlendOperation, crate::Error> {
            self.inner
                .alpha_blend_operation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `alpha_blend_operation` property after validation.
        pub fn set_alpha_blend_operation(
            &self,
            value: crate::metal::BlendOperation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_blend_operation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `blending_state` property with availability checks.
        pub fn blending_state(&self) -> Result<crate::metal4::BlendState, crate::Error> {
            self.inner.blending_state().map_err(crate::Error::from_ffi)
        }
        /// Writes the `blending_state` property after validation.
        pub fn set_blending_state(
            &self,
            value: crate::metal4::BlendState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_blending_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `destination_alpha_blend_factor` property with availability checks.
        pub fn destination_alpha_blend_factor(
            &self,
        ) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .destination_alpha_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `destination_alpha_blend_factor` property after validation.
        pub fn set_destination_alpha_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_destination_alpha_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `destination_rgb_blend_factor` property with availability checks.
        pub fn destination_rgb_blend_factor(
            &self,
        ) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .destination_rgb_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `destination_rgb_blend_factor` property after validation.
        pub fn set_destination_rgb_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_destination_rgb_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `pixel_format` property with availability checks.
        pub fn pixel_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner.pixel_format().map_err(crate::Error::from_ffi)
        }
        /// Writes the `pixel_format` property after validation.
        pub fn set_pixel_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_pixel_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `rgb_blend_operation` property with availability checks.
        pub fn rgb_blend_operation(&self) -> Result<crate::metal::BlendOperation, crate::Error> {
            self.inner
                .rgb_blend_operation()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `rgb_blend_operation` property after validation.
        pub fn set_rgb_blend_operation(
            &self,
            value: crate::metal::BlendOperation,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_rgb_blend_operation(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `source_alpha_blend_factor` property with availability checks.
        pub fn source_alpha_blend_factor(&self) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .source_alpha_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `source_alpha_blend_factor` property after validation.
        pub fn set_source_alpha_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_source_alpha_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `source_rgb_blend_factor` property with availability checks.
        pub fn source_rgb_blend_factor(&self) -> Result<crate::metal::BlendFactor, crate::Error> {
            self.inner
                .source_rgb_blend_factor()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `source_rgb_blend_factor` property after validation.
        pub fn set_source_rgb_blend_factor(
            &self,
            value: crate::metal::BlendFactor,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_source_rgb_blend_factor(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `write_mask` property with availability checks.
        pub fn write_mask(&self) -> Result<crate::metal::ColorWriteMask, crate::Error> {
            self.inner.write_mask().map_err(crate::Error::from_ffi)
        }
        /// Writes the `write_mask` property after validation.
        pub fn set_write_mask(
            &self,
            value: crate::metal::ColorWriteMask,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_write_mask(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::RenderPipelineColorAttachmentDescriptorArray`.
    #[derive(Clone)]
    pub struct RenderPipelineColorAttachmentDescriptorArray {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::RenderPipelineColorAttachmentDescriptorArray,
    }

    impl RenderPipelineColorAttachmentDescriptorArray {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::RenderPipelineColorAttachmentDescriptorArray,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::RenderPipelineColorAttachmentDescriptorArray` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::RenderPipelineColorAttachmentDescriptorArray::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::RenderPipelineDescriptor`.
    #[derive(Clone)]
    pub struct RenderPipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::RenderPipelineDescriptor,
    }

    impl RenderPipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::RenderPipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::RenderPipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::RenderPipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_rasterization_enabled` property with availability checks.
        pub fn is_rasterization_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_rasterization_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_vertex_amplification_count` property with availability checks.
        pub fn max_vertex_amplification_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_vertex_amplification_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_vertex_amplification_count` property after validation.
        pub fn set_max_vertex_amplification_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_max_vertex_amplification_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `raster_sample_count` property with availability checks.
        pub fn raster_sample_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .raster_sample_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `raster_sample_count` property after validation.
        pub fn set_raster_sample_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_raster_sample_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_fragment_binary_linking` property with availability checks.
        pub fn support_fragment_binary_linking(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_fragment_binary_linking()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_vertex_binary_linking` property with availability checks.
        pub fn support_vertex_binary_linking(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_vertex_binary_linking()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `alpha_to_coverage_state` property with availability checks.
        pub fn alpha_to_coverage_state(
            &self,
        ) -> Result<crate::metal4::AlphaToCoverageState, crate::Error> {
            self.inner
                .alpha_to_coverage_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `alpha_to_coverage_state` property after validation.
        pub fn set_alpha_to_coverage_state(
            &self,
            value: crate::metal4::AlphaToCoverageState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_to_coverage_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `alpha_to_one_state` property with availability checks.
        pub fn alpha_to_one_state(&self) -> Result<crate::metal4::AlphaToOneState, crate::Error> {
            self.inner
                .alpha_to_one_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `alpha_to_one_state` property after validation.
        pub fn set_alpha_to_one_state(
            &self,
            value: crate::metal4::AlphaToOneState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_alpha_to_one_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_attachment_mapping_state` property with availability checks.
        pub fn color_attachment_mapping_state(
            &self,
        ) -> Result<crate::metal4::LogicalToPhysicalColorAttachmentMappingState, crate::Error>
        {
            self.inner
                .color_attachment_mapping_state()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_attachment_mapping_state` property after validation.
        pub fn set_color_attachment_mapping_state(
            &self,
            value: crate::metal4::LogicalToPhysicalColorAttachmentMappingState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_attachment_mapping_state(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_primitive_topology` property with availability checks.
        pub fn input_primitive_topology(
            &self,
        ) -> Result<crate::metal::PrimitiveTopologyClass, crate::Error> {
            self.inner
                .input_primitive_topology()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_primitive_topology` property after validation.
        pub fn set_input_primitive_topology(
            &self,
            value: crate::metal::PrimitiveTopologyClass,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_input_primitive_topology(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_indirect_command_buffers` property with availability checks.
        pub fn support_indirect_command_buffers(
            &self,
        ) -> Result<crate::metal4::IndirectCommandBufferSupportState, crate::Error> {
            self.inner
                .support_indirect_command_buffers()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `support_indirect_command_buffers` property after validation.
        pub fn set_support_indirect_command_buffers(
            &self,
            value: crate::metal4::IndirectCommandBufferSupportState,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_support_indirect_command_buffers(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_attachments` object property.
        pub fn color_attachments(
            &self,
        ) -> Result<Option<crate::metal4::RenderPipelineColorAttachmentDescriptorArray>, crate::Error>
        {
            self.inner
                .color_attachments()
                .map(|value| {
                    value.map(crate::metal4::RenderPipelineColorAttachmentDescriptorArray::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_function_descriptor` object property.
        pub fn fragment_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .fragment_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fragment_function_descriptor` object property.
        pub fn set_fragment_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_fragment_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_static_linking_descriptor` object property.
        pub fn fragment_static_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::StaticLinkingDescriptor>, crate::Error> {
            self.inner
                .fragment_static_linking_descriptor()
                .map(|value| value.map(crate::metal4::StaticLinkingDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fragment_static_linking_descriptor` object property.
        pub fn set_fragment_static_linking_descriptor(
            &self,
            value: Option<&crate::metal4::StaticLinkingDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_fragment_static_linking_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `vertex_descriptor` object property.
        pub fn vertex_descriptor(
            &self,
        ) -> Result<Option<crate::metal::VertexDescriptor>, crate::Error> {
            self.inner
                .vertex_descriptor()
                .map(|value| value.map(crate::metal::VertexDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_descriptor` object property.
        pub fn set_vertex_descriptor(
            &self,
            value: Option<&crate::metal::VertexDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `vertex_function_descriptor` object property.
        pub fn vertex_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .vertex_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_function_descriptor` object property.
        pub fn set_vertex_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `vertex_static_linking_descriptor` object property.
        pub fn vertex_static_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::StaticLinkingDescriptor>, crate::Error> {
            self.inner
                .vertex_static_linking_descriptor()
                .map(|value| value.map(crate::metal4::StaticLinkingDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `vertex_static_linking_descriptor` object property.
        pub fn set_vertex_static_linking_descriptor(
            &self,
            value: Option<&crate::metal4::StaticLinkingDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_vertex_static_linking_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_rasterization_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_rasterization_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_fragment_binary_linking(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_fragment_binary_linking(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_vertex_binary_linking(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_vertex_binary_linking(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::RenderPipelineDynamicLinkingDescriptor`.
    #[derive(Clone)]
    pub struct RenderPipelineDynamicLinkingDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal4::RenderPipelineDynamicLinkingDescriptor,
    }

    impl RenderPipelineDynamicLinkingDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::RenderPipelineDynamicLinkingDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::RenderPipelineDynamicLinkingDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::RenderPipelineDynamicLinkingDescriptor::new(
            )
            .map(Self::from_ffi)
            .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fragment_linking_descriptor` object property.
        pub fn fragment_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::PipelineStageDynamicLinkingDescriptor>, crate::Error>
        {
            self.inner
                .fragment_linking_descriptor()
                .map(|value| {
                    value.map(crate::metal4::PipelineStageDynamicLinkingDescriptor::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `mesh_linking_descriptor` object property.
        pub fn mesh_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::PipelineStageDynamicLinkingDescriptor>, crate::Error>
        {
            self.inner
                .mesh_linking_descriptor()
                .map(|value| {
                    value.map(crate::metal4::PipelineStageDynamicLinkingDescriptor::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `object_linking_descriptor` object property.
        pub fn object_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::PipelineStageDynamicLinkingDescriptor>, crate::Error>
        {
            self.inner
                .object_linking_descriptor()
                .map(|value| {
                    value.map(crate::metal4::PipelineStageDynamicLinkingDescriptor::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `tile_linking_descriptor` object property.
        pub fn tile_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::PipelineStageDynamicLinkingDescriptor>, crate::Error>
        {
            self.inner
                .tile_linking_descriptor()
                .map(|value| {
                    value.map(crate::metal4::PipelineStageDynamicLinkingDescriptor::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `vertex_linking_descriptor` object property.
        pub fn vertex_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::PipelineStageDynamicLinkingDescriptor>, crate::Error>
        {
            self.inner
                .vertex_linking_descriptor()
                .map(|value| {
                    value.map(crate::metal4::PipelineStageDynamicLinkingDescriptor::from_ffi)
                })
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::SpecializedFunctionDescriptor`.
    #[derive(Clone)]
    pub struct SpecializedFunctionDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::SpecializedFunctionDescriptor,
    }

    impl SpecializedFunctionDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::SpecializedFunctionDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::SpecializedFunctionDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::SpecializedFunctionDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `constant_values` object property.
        pub fn constant_values(
            &self,
        ) -> Result<Option<crate::metal::FunctionConstantValues>, crate::Error> {
            self.inner
                .constant_values()
                .map(|value| value.map(crate::metal::FunctionConstantValues::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `constant_values` object property.
        pub fn set_constant_values(
            &self,
            value: Option<&crate::metal::FunctionConstantValues>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_constant_values(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `function_descriptor` object property.
        pub fn function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `function_descriptor` object property.
        pub fn set_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `specialized_name` string property.
        pub fn specialized_name(&self) -> Result<Option<String>, crate::Error> {
            self.inner
                .specialized_name()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `specialized_name` string property.
        pub fn set_specialized_name(&self, value: &str) -> Result<(), crate::Error> {
            self.inner
                .set_specialized_name(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::StaticLinkingDescriptor`.
    #[derive(Clone)]
    pub struct StaticLinkingDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::StaticLinkingDescriptor,
    }

    impl StaticLinkingDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::StaticLinkingDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::StaticLinkingDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::StaticLinkingDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::StitchedFunctionDescriptor`.
    #[derive(Clone)]
    pub struct StitchedFunctionDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::StitchedFunctionDescriptor,
    }

    impl StitchedFunctionDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::StitchedFunctionDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::StitchedFunctionDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::StitchedFunctionDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `function_graph` object property.
        pub fn function_graph(
            &self,
        ) -> Result<Option<crate::metal::FunctionStitchingGraph>, crate::Error> {
            self.inner
                .function_graph()
                .map(|value| value.map(crate::metal::FunctionStitchingGraph::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `function_graph` object property.
        pub fn set_function_graph(
            &self,
            value: Option<&crate::metal::FunctionStitchingGraph>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_function_graph(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4::TileRenderPipelineDescriptor`.
    #[derive(Clone)]
    pub struct TileRenderPipelineDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4::TileRenderPipelineDescriptor,
    }

    impl TileRenderPipelineDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4::TileRenderPipelineDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTL4::TileRenderPipelineDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal4::TileRenderPipelineDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `max_total_threads_per_threadgroup` property with availability checks.
        pub fn max_total_threads_per_threadgroup(&self) -> Result<usize, crate::Error> {
            self.inner
                .max_total_threads_per_threadgroup()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `max_total_threads_per_threadgroup` property after validation.
        pub fn set_max_total_threads_per_threadgroup(
            &self,
            value: usize,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_max_total_threads_per_threadgroup(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `raster_sample_count` property with availability checks.
        pub fn raster_sample_count(&self) -> Result<usize, crate::Error> {
            self.inner
                .raster_sample_count()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `raster_sample_count` property after validation.
        pub fn set_raster_sample_count(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_raster_sample_count(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `support_binary_linking` property with availability checks.
        pub fn support_binary_linking(&self) -> Result<bool, crate::Error> {
            self.inner
                .support_binary_linking()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `threadgroup_size_matches_tile_size` property with availability checks.
        pub fn threadgroup_size_matches_tile_size(&self) -> Result<bool, crate::Error> {
            self.inner
                .threadgroup_size_matches_tile_size()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_attachments` object property.
        pub fn color_attachments(
            &self,
        ) -> Result<
            Option<crate::metal::TileRenderPipelineColorAttachmentDescriptorArray>,
            crate::Error,
        > {
            self.inner
                .color_attachments()
                .map(|value| {
                    value.map(
                        crate::metal::TileRenderPipelineColorAttachmentDescriptorArray::from_ffi,
                    )
                })
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `static_linking_descriptor` object property.
        pub fn static_linking_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::StaticLinkingDescriptor>, crate::Error> {
            self.inner
                .static_linking_descriptor()
                .map(|value| value.map(crate::metal4::StaticLinkingDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `static_linking_descriptor` object property.
        pub fn set_static_linking_descriptor(
            &self,
            value: Option<&crate::metal4::StaticLinkingDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_static_linking_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `tile_function_descriptor` object property.
        pub fn tile_function_descriptor(
            &self,
        ) -> Result<Option<crate::metal4::FunctionDescriptor>, crate::Error> {
            self.inner
                .tile_function_descriptor()
                .map(|value| value.map(crate::metal4::FunctionDescriptor::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `tile_function_descriptor` object property.
        pub fn set_tile_function_descriptor(
            &self,
            value: Option<&crate::metal4::FunctionDescriptor>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_tile_function_descriptor(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_support_binary_linking(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_support_binary_linking(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_threadgroup_size_matches_tile_size(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_threadgroup_size_matches_tile_size(value)
                .map_err(crate::Error::from_ffi)
        }
    }
}

/// Safe generated implementation for the `metal4_fx` canonical module.
pub(crate) mod metal4_fx {
    /// Safe owned facade for `MTL4FX::FrameInterpolator`.
    #[derive(Clone)]
    pub struct FrameInterpolator {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4_fx::FrameInterpolator,
    }

    impl FrameInterpolator {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4_fx::FrameInterpolator,
        ) -> Self {
            Self { inner }
        }
        /// Encodes work into an active Metal 4 recording scope.
        pub fn encode_to_command_buffer(
            &self,
            command_buffer: &mut crate::metal4::RecordingCommandBuffer,
        ) -> Result<(), crate::Error> {
            command_buffer
                .as_ffi_mut()
                .encode_frame_interpolator(&self.inner)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4FX::SpatialScaler`.
    #[derive(Clone)]
    pub struct SpatialScaler {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4_fx::SpatialScaler,
    }

    impl SpatialScaler {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4_fx::SpatialScaler,
        ) -> Self {
            Self { inner }
        }
        /// Encodes work into an active Metal 4 recording scope.
        pub fn encode_to_command_buffer(
            &self,
            command_buffer: &mut crate::metal4::RecordingCommandBuffer,
        ) -> Result<(), crate::Error> {
            command_buffer
                .as_ffi_mut()
                .encode_spatial_scaler(&self.inner)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4FX::TemporalDenoisedScaler`.
    #[derive(Clone)]
    pub struct TemporalDenoisedScaler {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4_fx::TemporalDenoisedScaler,
    }

    impl TemporalDenoisedScaler {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4_fx::TemporalDenoisedScaler,
        ) -> Self {
            Self { inner }
        }
        /// Encodes work into an active Metal 4 recording scope.
        pub fn encode_to_command_buffer(
            &self,
            command_buffer: &mut crate::metal4::RecordingCommandBuffer,
        ) -> Result<(), crate::Error> {
            command_buffer
                .as_ffi_mut()
                .encode_temporal_denoised_scaler(&self.inner)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTL4FX::TemporalScaler`.
    #[derive(Clone)]
    pub struct TemporalScaler {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal4_fx::TemporalScaler,
    }

    impl TemporalScaler {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal4_fx::TemporalScaler,
        ) -> Self {
            Self { inner }
        }
        pub fn set_color_texture(&self, texture: Option<&crate::metal::Texture>) -> Result<(), crate::Error> {
            self.inner.set_color_texture(texture.map(|v| &v.inner)).map_err(crate::Error::from_ffi)
        }
        pub fn set_depth_texture(&self, texture: Option<&crate::metal::Texture>) -> Result<(), crate::Error> {
            self.inner.set_depth_texture(texture.map(|v| &v.inner)).map_err(crate::Error::from_ffi)
        }
        pub fn set_motion_texture(&self, texture: Option<&crate::metal::Texture>) -> Result<(), crate::Error> {
            self.inner.set_motion_texture(texture.map(|v| &v.inner)).map_err(crate::Error::from_ffi)
        }
        pub fn set_output_texture(&self, texture: Option<&crate::metal::Texture>) -> Result<(), crate::Error> {
            self.inner.set_output_texture(texture.map(|v| &v.inner)).map_err(crate::Error::from_ffi)
        }
        pub fn set_jitter_offset_x(&self, value: f32) -> Result<(), crate::Error> { self.inner.set_jitter_offset_x(value).map_err(crate::Error::from_ffi) }
        pub fn set_jitter_offset_y(&self, value: f32) -> Result<(), crate::Error> { self.inner.set_jitter_offset_y(value).map_err(crate::Error::from_ffi) }
        pub fn set_motion_vector_scale_x(&self, value: f32) -> Result<(), crate::Error> { self.inner.set_motion_vector_scale_x(value).map_err(crate::Error::from_ffi) }
        pub fn set_motion_vector_scale_y(&self, value: f32) -> Result<(), crate::Error> { self.inner.set_motion_vector_scale_y(value).map_err(crate::Error::from_ffi) }
        pub fn set_input_content_width(&self, value: usize) -> Result<(), crate::Error> { self.inner.set_input_content_width(value).map_err(crate::Error::from_ffi) }
        pub fn set_input_content_height(&self, value: usize) -> Result<(), crate::Error> { self.inner.set_input_content_height(value).map_err(crate::Error::from_ffi) }
        pub fn set_depth_reversed(&self, value: bool) -> Result<(), crate::Error> { self.inner.set_depth_reversed(value).map_err(crate::Error::from_ffi) }
        pub fn set_reset(&self, value: bool) -> Result<(), crate::Error> { self.inner.set_reset(value).map_err(crate::Error::from_ffi) }
        /// Encodes work into an active Metal 4 recording scope.
        pub fn encode_to_command_buffer(
            &self,
            command_buffer: &mut crate::metal4::RecordingCommandBuffer,
        ) -> Result<(), crate::Error> {
            command_buffer
                .as_ffi_mut()
                .encode_temporal_scaler(&self.inner)
                .map_err(crate::Error::from_ffi)
        }
    }
}

/// Safe generated implementation for the `metal_fx` canonical module.
pub(crate) mod metal_fx {
    /// Safe owned facade for `MTLFX::FrameInterpolatableScaler`.
    #[derive(Clone)]
    pub struct FrameInterpolatableScaler {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatableScaler,
    }

    impl FrameInterpolatableScaler {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatableScaler,
        ) -> Self {
            Self { inner }
        }
    }

    /// Safe owned facade for `MTLFX::FrameInterpolator`.
    #[derive(Clone)]
    pub struct FrameInterpolator {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolator,
    }

    impl FrameInterpolator {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolator,
        ) -> Self {
            Self { inner }
        }
        /// Encodes work into a mutably borrowed Metal command buffer.
        pub fn encode_to_command_buffer(
            &self,
            command_buffer: &mut crate::metal::CommandBuffer,
        ) -> Result<(), crate::Error> {
            self.inner
                .encode_to_command_buffer(&mut command_buffer.inner)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTLFX::FrameInterpolatorBase`.
    #[derive(Clone)]
    pub struct FrameInterpolatorBase {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatorBase,
    }

    impl FrameInterpolatorBase {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatorBase,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `aspect_ratio` property with availability checks.
        pub fn aspect_ratio(&self) -> Result<f32, crate::Error> {
            self.inner.aspect_ratio().map_err(crate::Error::from_ffi)
        }
        /// Writes the `aspect_ratio` property after validation.
        pub fn set_aspect_ratio(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_aspect_ratio(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `content_height` property with availability checks.
        pub fn content_height(&self) -> Result<usize, crate::Error> {
            self.inner.content_height().map_err(crate::Error::from_ffi)
        }
        /// Writes the `content_height` property after validation.
        pub fn set_content_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_content_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `content_width` property with availability checks.
        pub fn content_width(&self) -> Result<usize, crate::Error> {
            self.inner.content_width().map_err(crate::Error::from_ffi)
        }
        /// Writes the `content_width` property after validation.
        pub fn set_content_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_content_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `delta_time` property with availability checks.
        pub fn delta_time(&self) -> Result<f32, crate::Error> {
            self.inner.delta_time().map_err(crate::Error::from_ffi)
        }
        /// Writes the `delta_time` property after validation.
        pub fn set_delta_time(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_delta_time(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_content_offset_x` property with availability checks.
        pub fn depth_content_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner
                .depth_content_offset_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_content_offset_x` property after validation.
        pub fn set_depth_content_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_depth_content_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_content_offset_y` property with availability checks.
        pub fn depth_content_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner
                .depth_content_offset_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_content_offset_y` property after validation.
        pub fn set_depth_content_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_depth_content_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `distortion_height` property with availability checks.
        pub fn distortion_height(&self) -> Result<usize, crate::Error> {
            self.inner
                .distortion_height()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `distortion_height` property after validation.
        pub fn set_distortion_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_distortion_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `distortion_offset_x` property with availability checks.
        pub fn distortion_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner
                .distortion_offset_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `distortion_offset_x` property after validation.
        pub fn set_distortion_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_distortion_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `distortion_offset_y` property with availability checks.
        pub fn distortion_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner
                .distortion_offset_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `distortion_offset_y` property after validation.
        pub fn set_distortion_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_distortion_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `distortion_width` property with availability checks.
        pub fn distortion_width(&self) -> Result<usize, crate::Error> {
            self.inner
                .distortion_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `distortion_width` property after validation.
        pub fn set_distortion_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_distortion_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `far_plane` property with availability checks.
        pub fn far_plane(&self) -> Result<f32, crate::Error> {
            self.inner.far_plane().map_err(crate::Error::from_ffi)
        }
        /// Writes the `far_plane` property after validation.
        pub fn set_far_plane(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_far_plane(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `field_of_view` property with availability checks.
        pub fn field_of_view(&self) -> Result<f32, crate::Error> {
            self.inner.field_of_view().map_err(crate::Error::from_ffi)
        }
        /// Writes the `field_of_view` property after validation.
        pub fn set_field_of_view(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_field_of_view(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_height` property with availability checks.
        pub fn input_height(&self) -> Result<usize, crate::Error> {
            self.inner.input_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_width` property with availability checks.
        pub fn input_width(&self) -> Result<usize, crate::Error> {
            self.inner.input_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_depth_reversed` property with availability checks.
        pub fn is_depth_reversed(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_depth_reversed()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_ui_texture_composited` property with availability checks.
        pub fn is_ui_texture_composited(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_ui_texture_composited()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `jitter_offset_x` property with availability checks.
        pub fn jitter_offset_x(&self) -> Result<f32, crate::Error> {
            self.inner.jitter_offset_x().map_err(crate::Error::from_ffi)
        }
        /// Writes the `jitter_offset_x` property after validation.
        pub fn set_jitter_offset_x(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_jitter_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `jitter_offset_y` property with availability checks.
        pub fn jitter_offset_y(&self) -> Result<f32, crate::Error> {
            self.inner.jitter_offset_y().map_err(crate::Error::from_ffi)
        }
        /// Writes the `jitter_offset_y` property after validation.
        pub fn set_jitter_offset_y(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_jitter_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_content_offset_x` property with availability checks.
        pub fn motion_content_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_content_offset_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_content_offset_x` property after validation.
        pub fn set_motion_content_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_content_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_content_offset_y` property with availability checks.
        pub fn motion_content_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_content_offset_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_content_offset_y` property after validation.
        pub fn set_motion_content_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_content_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_vector_scale_x` property with availability checks.
        pub fn motion_vector_scale_x(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_vector_scale_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_vector_scale_x` property after validation.
        pub fn set_motion_vector_scale_x(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_vector_scale_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_vector_scale_y` property with availability checks.
        pub fn motion_vector_scale_y(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_vector_scale_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_vector_scale_y` property after validation.
        pub fn set_motion_vector_scale_y(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_vector_scale_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `near_plane` property with availability checks.
        pub fn near_plane(&self) -> Result<f32, crate::Error> {
            self.inner.near_plane().map_err(crate::Error::from_ffi)
        }
        /// Writes the `near_plane` property after validation.
        pub fn set_near_plane(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_near_plane(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_height` property with availability checks.
        pub fn output_height(&self) -> Result<usize, crate::Error> {
            self.inner.output_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_offset_x` property with availability checks.
        pub fn output_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner.output_offset_x().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_offset_x` property after validation.
        pub fn set_output_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_offset_y` property with availability checks.
        pub fn output_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner.output_offset_y().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_offset_y` property after validation.
        pub fn set_output_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_width` property with availability checks.
        pub fn output_width(&self) -> Result<usize, crate::Error> {
            self.inner.output_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `should_reset_history` property with availability checks.
        pub fn should_reset_history(&self) -> Result<bool, crate::Error> {
            self.inner
                .should_reset_history()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_format` property with availability checks.
        pub fn color_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .color_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_usage` property with availability checks.
        pub fn color_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .color_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_format` property with availability checks.
        pub fn depth_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .depth_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_usage` property with availability checks.
        pub fn depth_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .depth_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_format` property with availability checks.
        pub fn motion_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .motion_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_usage` property with availability checks.
        pub fn motion_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .motion_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_format` property with availability checks.
        pub fn output_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .output_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_usage` property with availability checks.
        pub fn output_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .output_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `ui_texture_format` property with availability checks.
        pub fn ui_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .ui_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `ui_texture_usage` property with availability checks.
        pub fn ui_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .ui_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_texture` object property.
        pub fn color_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .color_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_texture` object property.
        pub fn set_color_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `depth_texture` object property.
        pub fn depth_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .depth_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_texture` object property.
        pub fn set_depth_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `distortion_texture` object property.
        pub fn distortion_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .distortion_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `distortion_texture` object property.
        pub fn set_distortion_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_distortion_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fence` object property.
        pub fn fence(&self) -> Result<Option<crate::metal::Fence>, crate::Error> {
            self.inner
                .fence()
                .map(|value| value.map(crate::metal::Fence::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fence` object property.
        pub fn set_fence(&self, value: Option<&crate::metal::Fence>) -> Result<(), crate::Error> {
            self.inner
                .set_fence(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `motion_texture` object property.
        pub fn motion_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .motion_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_texture` object property.
        pub fn set_motion_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `output_texture` object property.
        pub fn output_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .output_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_texture` object property.
        pub fn set_output_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_output_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `prev_color_texture` object property.
        pub fn prev_color_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .prev_color_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `prev_color_texture` object property.
        pub fn set_prev_color_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_prev_color_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `ui_texture` object property.
        pub fn ui_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .ui_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `ui_texture` object property.
        pub fn set_ui_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_ui_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_depth_reversed(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_depth_reversed(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_is_ui_texture_composited(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_is_ui_texture_composited(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_should_reset_history(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_should_reset_history(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTLFX::FrameInterpolatorDescriptor`.
    #[derive(Clone)]
    pub struct FrameInterpolatorDescriptor {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatorDescriptor,
    }

    impl FrameInterpolatorDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatorDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Creates a default `MTLFX::FrameInterpolatorDescriptor` after availability checks.
        pub fn new() -> Result<Self, crate::Error> {
            metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatorDescriptor::new()
                .map(Self::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_height` property with availability checks.
        pub fn input_height(&self) -> Result<usize, crate::Error> {
            self.inner.input_height().map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_height` property after validation.
        pub fn set_input_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_width` property with availability checks.
        pub fn input_width(&self) -> Result<usize, crate::Error> {
            self.inner.input_width().map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_width` property after validation.
        pub fn set_input_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_distortion_texture_enabled` property with availability checks.
        pub fn is_distortion_texture_enabled(&self) -> Result<usize, crate::Error> {
            self.inner
                .is_distortion_texture_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_height` property with availability checks.
        pub fn output_height(&self) -> Result<usize, crate::Error> {
            self.inner.output_height().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_height` property after validation.
        pub fn set_output_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_width` property with availability checks.
        pub fn output_width(&self) -> Result<usize, crate::Error> {
            self.inner.output_width().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_width` property after validation.
        pub fn set_output_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_format` property with availability checks.
        pub fn color_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .color_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_texture_format` property after validation.
        pub fn set_color_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_format` property with availability checks.
        pub fn depth_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .depth_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_texture_format` property after validation.
        pub fn set_depth_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_format` property with availability checks.
        pub fn motion_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .motion_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_texture_format` property after validation.
        pub fn set_motion_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_format` property with availability checks.
        pub fn output_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .output_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_texture_format` property after validation.
        pub fn set_output_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_output_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `ui_texture_format` property with availability checks.
        pub fn ui_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .ui_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `ui_texture_format` property after validation.
        pub fn set_ui_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_ui_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `scaler` object property.
        pub fn scaler(
            &self,
        ) -> Result<Option<crate::metal_fx::FrameInterpolatableScaler>, crate::Error> {
            self.inner
                .scaler()
                .map(|value| value.map(crate::metal_fx::FrameInterpolatableScaler::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `scaler` object property.
        pub fn set_scaler(
            &self,
            value: Option<&crate::metal_fx::FrameInterpolatableScaler>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_scaler(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_distortion_texture_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_distortion_texture_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Creates a `MTL4FX::FrameInterpolator` from a device and Metal 4 compiler.
        pub fn new_frame_interpolator(
            &self,
            device: &crate::metal::Device,
            compiler: &crate::metal4::Compiler,
        ) -> Result<crate::metal4_fx::FrameInterpolator, crate::Error> {
            self.inner
                .new_frame_interpolator(&device.inner, &compiler.inner)
                .map(crate::metal4_fx::FrameInterpolator::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Runs a static MetalFX device query after availability checks.
        pub fn supports_device(device: &crate::metal::Device) -> Result<bool, crate::Error> {
            metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatorDescriptor::supports_device(&device.inner).map_err(crate::Error::from_ffi)
        }
        /// Runs a static MetalFX device query after availability checks.
        pub fn supports_metal4_fx(device: &crate::metal::Device) -> Result<bool, crate::Error> {
            metal_rust_ffi::__private::objects::metal_fx::FrameInterpolatorDescriptor::supports_metal4_fx(&device.inner).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTLFX::SpatialScalerBase`.
    #[derive(Clone)]
    pub struct SpatialScalerBase {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::SpatialScalerBase,
    }

    impl SpatialScalerBase {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::SpatialScalerBase,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `input_content_height` property with availability checks.
        pub fn input_content_height(&self) -> Result<usize, crate::Error> {
            self.inner
                .input_content_height()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_content_height` property after validation.
        pub fn set_input_content_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_content_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_content_width` property with availability checks.
        pub fn input_content_width(&self) -> Result<usize, crate::Error> {
            self.inner
                .input_content_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_content_width` property after validation.
        pub fn set_input_content_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_content_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_height` property with availability checks.
        pub fn input_height(&self) -> Result<usize, crate::Error> {
            self.inner.input_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_width` property with availability checks.
        pub fn input_width(&self) -> Result<usize, crate::Error> {
            self.inner.input_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_height` property with availability checks.
        pub fn output_height(&self) -> Result<usize, crate::Error> {
            self.inner.output_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_width` property with availability checks.
        pub fn output_width(&self) -> Result<usize, crate::Error> {
            self.inner.output_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_format` property with availability checks.
        pub fn color_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .color_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_usage` property with availability checks.
        pub fn color_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .color_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_format` property with availability checks.
        pub fn output_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .output_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_usage` property with availability checks.
        pub fn output_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .output_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_texture` object property.
        pub fn color_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .color_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_texture` object property.
        pub fn set_color_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fence` object property.
        pub fn fence(&self) -> Result<Option<crate::metal::Fence>, crate::Error> {
            self.inner
                .fence()
                .map(|value| value.map(crate::metal::Fence::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fence` object property.
        pub fn set_fence(&self, value: Option<&crate::metal::Fence>) -> Result<(), crate::Error> {
            self.inner
                .set_fence(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `output_texture` object property.
        pub fn output_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .output_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_texture` object property.
        pub fn set_output_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_output_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTLFX::TemporalDenoisedScaler`.
    #[derive(Clone)]
    pub struct TemporalDenoisedScaler {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScaler,
    }

    impl TemporalDenoisedScaler {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScaler,
        ) -> Self {
            Self { inner }
        }
        /// Encodes work into a mutably borrowed Metal command buffer.
        pub fn encode_to_command_buffer(
            &self,
            command_buffer: &mut crate::metal::CommandBuffer,
        ) -> Result<(), crate::Error> {
            self.inner
                .encode_to_command_buffer(&mut command_buffer.inner)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTLFX::TemporalDenoisedScalerBase`.
    #[derive(Clone)]
    pub struct TemporalDenoisedScalerBase {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerBase,
    }

    impl TemporalDenoisedScalerBase {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerBase,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `input_content_max_scale` property with availability checks.
        pub fn input_content_max_scale(&self) -> Result<f32, crate::Error> {
            self.inner
                .input_content_max_scale()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_content_min_scale` property with availability checks.
        pub fn input_content_min_scale(&self) -> Result<f32, crate::Error> {
            self.inner
                .input_content_min_scale()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_height` property with availability checks.
        pub fn input_height(&self) -> Result<usize, crate::Error> {
            self.inner.input_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_width` property with availability checks.
        pub fn input_width(&self) -> Result<usize, crate::Error> {
            self.inner.input_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_depth_reversed` property with availability checks.
        pub fn is_depth_reversed(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_depth_reversed()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `jitter_offset_x` property with availability checks.
        pub fn jitter_offset_x(&self) -> Result<f32, crate::Error> {
            self.inner.jitter_offset_x().map_err(crate::Error::from_ffi)
        }
        /// Writes the `jitter_offset_x` property after validation.
        pub fn set_jitter_offset_x(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_jitter_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `jitter_offset_y` property with availability checks.
        pub fn jitter_offset_y(&self) -> Result<f32, crate::Error> {
            self.inner.jitter_offset_y().map_err(crate::Error::from_ffi)
        }
        /// Writes the `jitter_offset_y` property after validation.
        pub fn set_jitter_offset_y(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_jitter_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_vector_scale_x` property with availability checks.
        pub fn motion_vector_scale_x(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_vector_scale_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_vector_scale_x` property after validation.
        pub fn set_motion_vector_scale_x(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_vector_scale_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_vector_scale_y` property with availability checks.
        pub fn motion_vector_scale_y(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_vector_scale_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_vector_scale_y` property after validation.
        pub fn set_motion_vector_scale_y(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_vector_scale_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_height` property with availability checks.
        pub fn output_height(&self) -> Result<usize, crate::Error> {
            self.inner.output_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_width` property with availability checks.
        pub fn output_width(&self) -> Result<usize, crate::Error> {
            self.inner.output_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `pre_exposure` property with availability checks.
        pub fn pre_exposure(&self) -> Result<f32, crate::Error> {
            self.inner.pre_exposure().map_err(crate::Error::from_ffi)
        }
        /// Writes the `pre_exposure` property after validation.
        pub fn set_pre_exposure(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_pre_exposure(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `should_reset_history` property with availability checks.
        pub fn should_reset_history(&self) -> Result<bool, crate::Error> {
            self.inner
                .should_reset_history()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_format` property with availability checks.
        pub fn color_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .color_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_usage` property with availability checks.
        pub fn color_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .color_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `denoise_strength_mask_texture_format` property with availability checks.
        pub fn denoise_strength_mask_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .denoise_strength_mask_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `denoise_strength_mask_texture_usage` property with availability checks.
        pub fn denoise_strength_mask_texture_usage(
            &self,
        ) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .denoise_strength_mask_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_format` property with availability checks.
        pub fn depth_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .depth_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_usage` property with availability checks.
        pub fn depth_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .depth_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `diffuse_albedo_texture_format` property with availability checks.
        pub fn diffuse_albedo_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .diffuse_albedo_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `diffuse_albedo_texture_usage` property with availability checks.
        pub fn diffuse_albedo_texture_usage(
            &self,
        ) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .diffuse_albedo_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_format` property with availability checks.
        pub fn motion_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .motion_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_usage` property with availability checks.
        pub fn motion_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .motion_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `normal_texture_format` property with availability checks.
        pub fn normal_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .normal_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `normal_texture_usage` property with availability checks.
        pub fn normal_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .normal_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_format` property with availability checks.
        pub fn output_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .output_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_usage` property with availability checks.
        pub fn output_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .output_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reactive_mask_texture_format` property with availability checks.
        pub fn reactive_mask_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .reactive_mask_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reactive_mask_texture_usage` property with availability checks.
        pub fn reactive_mask_texture_usage(
            &self,
        ) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .reactive_mask_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `roughness_texture_format` property with availability checks.
        pub fn roughness_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .roughness_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `roughness_texture_usage` property with availability checks.
        pub fn roughness_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .roughness_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `specular_albedo_texture_format` property with availability checks.
        pub fn specular_albedo_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .specular_albedo_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `specular_albedo_texture_usage` property with availability checks.
        pub fn specular_albedo_texture_usage(
            &self,
        ) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .specular_albedo_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `specular_hit_distance_texture_format` property with availability checks.
        pub fn specular_hit_distance_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .specular_hit_distance_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `specular_hit_distance_texture_usage` property with availability checks.
        pub fn specular_hit_distance_texture_usage(
            &self,
        ) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .specular_hit_distance_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transparency_overlay_texture_format` property with availability checks.
        pub fn transparency_overlay_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .transparency_overlay_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transparency_overlay_texture_usage` property with availability checks.
        pub fn transparency_overlay_texture_usage(
            &self,
        ) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .transparency_overlay_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_texture` object property.
        pub fn color_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .color_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_texture` object property.
        pub fn set_color_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `denoise_strength_mask_texture` object property.
        pub fn denoise_strength_mask_texture(
            &self,
        ) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .denoise_strength_mask_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `denoise_strength_mask_texture` object property.
        pub fn set_denoise_strength_mask_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_denoise_strength_mask_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `depth_texture` object property.
        pub fn depth_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .depth_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_texture` object property.
        pub fn set_depth_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `diffuse_albedo_texture` object property.
        pub fn diffuse_albedo_texture(
            &self,
        ) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .diffuse_albedo_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `diffuse_albedo_texture` object property.
        pub fn set_diffuse_albedo_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_diffuse_albedo_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `exposure_texture` object property.
        pub fn exposure_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .exposure_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `exposure_texture` object property.
        pub fn set_exposure_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_exposure_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fence` object property.
        pub fn fence(&self) -> Result<Option<crate::metal::Fence>, crate::Error> {
            self.inner
                .fence()
                .map(|value| value.map(crate::metal::Fence::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fence` object property.
        pub fn set_fence(&self, value: Option<&crate::metal::Fence>) -> Result<(), crate::Error> {
            self.inner
                .set_fence(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `motion_texture` object property.
        pub fn motion_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .motion_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_texture` object property.
        pub fn set_motion_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `normal_texture` object property.
        pub fn normal_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .normal_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `normal_texture` object property.
        pub fn set_normal_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_normal_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `output_texture` object property.
        pub fn output_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .output_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_texture` object property.
        pub fn set_output_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_output_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `reactive_mask_texture` object property.
        pub fn reactive_mask_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .reactive_mask_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `reactive_mask_texture` object property.
        pub fn set_reactive_mask_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_reactive_mask_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `roughness_texture` object property.
        pub fn roughness_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .roughness_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `roughness_texture` object property.
        pub fn set_roughness_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_roughness_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `specular_albedo_texture` object property.
        pub fn specular_albedo_texture(
            &self,
        ) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .specular_albedo_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `specular_albedo_texture` object property.
        pub fn set_specular_albedo_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_specular_albedo_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `specular_hit_distance_texture` object property.
        pub fn specular_hit_distance_texture(
            &self,
        ) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .specular_hit_distance_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `specular_hit_distance_texture` object property.
        pub fn set_specular_hit_distance_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_specular_hit_distance_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `transparency_overlay_texture` object property.
        pub fn transparency_overlay_texture(
            &self,
        ) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .transparency_overlay_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transparency_overlay_texture` object property.
        pub fn set_transparency_overlay_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transparency_overlay_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_depth_reversed(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_depth_reversed(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_should_reset_history(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_should_reset_history(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads an aligned MetalFX transformation matrix.
        pub fn world_to_view_matrix(&self) -> Result<crate::metal_fx::Matrix4x4, crate::Error> {
            self.inner
                .world_to_view_matrix()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes a finite aligned MetalFX transformation matrix.
        pub fn set_world_to_view_matrix(
            &self,
            value: crate::metal_fx::Matrix4x4,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_world_to_view_matrix(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads an aligned MetalFX transformation matrix.
        pub fn view_to_clip_matrix(&self) -> Result<crate::metal_fx::Matrix4x4, crate::Error> {
            self.inner
                .view_to_clip_matrix()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes a finite aligned MetalFX transformation matrix.
        pub fn set_view_to_clip_matrix(
            &self,
            value: crate::metal_fx::Matrix4x4,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_view_to_clip_matrix(value)
                .map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTLFX::TemporalDenoisedScalerDescriptor`.
    #[derive(Clone)]
    pub struct TemporalDenoisedScalerDescriptor {
        pub(crate) inner:
            metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerDescriptor,
    }

    impl TemporalDenoisedScalerDescriptor {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerDescriptor,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `input_content_max_scale` property with availability checks.
        pub fn input_content_max_scale(&self) -> Result<f32, crate::Error> {
            self.inner
                .input_content_max_scale()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_content_max_scale` property after validation.
        pub fn set_input_content_max_scale(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_input_content_max_scale(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_content_min_scale` property with availability checks.
        pub fn input_content_min_scale(&self) -> Result<f32, crate::Error> {
            self.inner
                .input_content_min_scale()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_content_min_scale` property after validation.
        pub fn set_input_content_min_scale(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_input_content_min_scale(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_height` property with availability checks.
        pub fn input_height(&self) -> Result<usize, crate::Error> {
            self.inner.input_height().map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_height` property after validation.
        pub fn set_input_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_width` property with availability checks.
        pub fn input_width(&self) -> Result<usize, crate::Error> {
            self.inner.input_width().map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_width` property after validation.
        pub fn set_input_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_auto_exposure_enabled` property with availability checks.
        pub fn is_auto_exposure_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_auto_exposure_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_denoise_strength_mask_texture_enabled` property with availability checks.
        pub fn is_denoise_strength_mask_texture_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_denoise_strength_mask_texture_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_input_content_properties_enabled` property with availability checks.
        pub fn is_input_content_properties_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_input_content_properties_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_reactive_mask_texture_enabled` property with availability checks.
        pub fn is_reactive_mask_texture_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_reactive_mask_texture_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_specular_hit_distance_texture_enabled` property with availability checks.
        pub fn is_specular_hit_distance_texture_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_specular_hit_distance_texture_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_transparency_overlay_texture_enabled` property with availability checks.
        pub fn is_transparency_overlay_texture_enabled(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_transparency_overlay_texture_enabled()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_height` property with availability checks.
        pub fn output_height(&self) -> Result<usize, crate::Error> {
            self.inner.output_height().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_height` property after validation.
        pub fn set_output_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_width` property with availability checks.
        pub fn output_width(&self) -> Result<usize, crate::Error> {
            self.inner.output_width().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_width` property after validation.
        pub fn set_output_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `requires_synchronous_initialization` property with availability checks.
        pub fn requires_synchronous_initialization(&self) -> Result<bool, crate::Error> {
            self.inner
                .requires_synchronous_initialization()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_format` property with availability checks.
        pub fn color_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .color_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_texture_format` property after validation.
        pub fn set_color_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `denoise_strength_mask_texture_format` property with availability checks.
        pub fn denoise_strength_mask_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .denoise_strength_mask_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `denoise_strength_mask_texture_format` property after validation.
        pub fn set_denoise_strength_mask_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_denoise_strength_mask_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_format` property with availability checks.
        pub fn depth_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .depth_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_texture_format` property after validation.
        pub fn set_depth_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `diffuse_albedo_texture_format` property with availability checks.
        pub fn diffuse_albedo_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .diffuse_albedo_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `diffuse_albedo_texture_format` property after validation.
        pub fn set_diffuse_albedo_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_diffuse_albedo_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_format` property with availability checks.
        pub fn motion_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .motion_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_texture_format` property after validation.
        pub fn set_motion_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `normal_texture_format` property with availability checks.
        pub fn normal_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .normal_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `normal_texture_format` property after validation.
        pub fn set_normal_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_normal_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_format` property with availability checks.
        pub fn output_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .output_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_texture_format` property after validation.
        pub fn set_output_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_output_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reactive_mask_texture_format` property with availability checks.
        pub fn reactive_mask_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .reactive_mask_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `reactive_mask_texture_format` property after validation.
        pub fn set_reactive_mask_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_reactive_mask_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `roughness_texture_format` property with availability checks.
        pub fn roughness_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .roughness_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `roughness_texture_format` property after validation.
        pub fn set_roughness_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_roughness_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `specular_albedo_texture_format` property with availability checks.
        pub fn specular_albedo_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .specular_albedo_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `specular_albedo_texture_format` property after validation.
        pub fn set_specular_albedo_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_specular_albedo_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `specular_hit_distance_texture_format` property with availability checks.
        pub fn specular_hit_distance_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .specular_hit_distance_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `specular_hit_distance_texture_format` property after validation.
        pub fn set_specular_hit_distance_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_specular_hit_distance_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `transparency_overlay_texture_format` property with availability checks.
        pub fn transparency_overlay_texture_format(
            &self,
        ) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .transparency_overlay_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `transparency_overlay_texture_format` property after validation.
        pub fn set_transparency_overlay_texture_format(
            &self,
            value: crate::metal::PixelFormat,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transparency_overlay_texture_format(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_auto_exposure_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_auto_exposure_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_denoise_strength_mask_texture_enabled(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_denoise_strength_mask_texture_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_input_content_properties_enabled(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_input_content_properties_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_reactive_mask_texture_enabled(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_reactive_mask_texture_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_requires_synchronous_initialization(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_requires_synchronous_initialization(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_specular_hit_distance_texture_enabled(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_specular_hit_distance_texture_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_transparency_overlay_texture_enabled(
            &self,
            value: bool,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_transparency_overlay_texture_enabled(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Creates a `MTL4FX::TemporalDenoisedScaler` from a device and Metal 4 compiler.
        pub fn new_temporal_denoised_scaler(
            &self,
            device: &crate::metal::Device,
            compiler: &crate::metal4::Compiler,
        ) -> Result<crate::metal4_fx::TemporalDenoisedScaler, crate::Error> {
            self.inner
                .new_temporal_denoised_scaler(&device.inner, &compiler.inner)
                .map(crate::metal4_fx::TemporalDenoisedScaler::from_ffi)
                .map_err(crate::Error::from_ffi)
        }
        /// Runs a static MetalFX device query after availability checks.
        pub fn supported_input_content_max_scale(
            device: &crate::metal::Device,
        ) -> Result<f32, crate::Error> {
            metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerDescriptor::supported_input_content_max_scale(&device.inner).map_err(crate::Error::from_ffi)
        }
        /// Runs a static MetalFX device query after availability checks.
        pub fn supported_input_content_min_scale(
            device: &crate::metal::Device,
        ) -> Result<f32, crate::Error> {
            metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerDescriptor::supported_input_content_min_scale(&device.inner).map_err(crate::Error::from_ffi)
        }
        /// Runs a static MetalFX device query after availability checks.
        pub fn supports_device(device: &crate::metal::Device) -> Result<bool, crate::Error> {
            metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerDescriptor::supports_device(&device.inner).map_err(crate::Error::from_ffi)
        }
        /// Runs a static MetalFX device query after availability checks.
        pub fn supports_metal4_fx(device: &crate::metal::Device) -> Result<bool, crate::Error> {
            metal_rust_ffi::__private::objects::metal_fx::TemporalDenoisedScalerDescriptor::supports_metal4_fx(&device.inner).map_err(crate::Error::from_ffi)
        }
    }

    /// Safe owned facade for `MTLFX::TemporalScalerBase`.
    #[derive(Clone)]
    pub struct TemporalScalerBase {
        pub(crate) inner: metal_rust_ffi::__private::objects::metal_fx::TemporalScalerBase,
    }

    impl TemporalScalerBase {
        pub(crate) const fn from_ffi(
            inner: metal_rust_ffi::__private::objects::metal_fx::TemporalScalerBase,
        ) -> Self {
            Self { inner }
        }
        /// Reads the `color_content_offset_x` property with availability checks.
        pub fn color_content_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner
                .color_content_offset_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_content_offset_x` property after validation.
        pub fn set_color_content_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_color_content_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_content_offset_y` property with availability checks.
        pub fn color_content_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner
                .color_content_offset_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_content_offset_y` property after validation.
        pub fn set_color_content_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_color_content_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_content_offset_x` property with availability checks.
        pub fn depth_content_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner
                .depth_content_offset_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_content_offset_x` property after validation.
        pub fn set_depth_content_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_depth_content_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_content_offset_y` property with availability checks.
        pub fn depth_content_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner
                .depth_content_offset_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_content_offset_y` property after validation.
        pub fn set_depth_content_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_depth_content_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_content_height` property with availability checks.
        pub fn input_content_height(&self) -> Result<usize, crate::Error> {
            self.inner
                .input_content_height()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_content_height` property after validation.
        pub fn set_input_content_height(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_content_height(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_content_max_scale` property with availability checks.
        pub fn input_content_max_scale(&self) -> Result<f32, crate::Error> {
            self.inner
                .input_content_max_scale()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_content_min_scale` property with availability checks.
        pub fn input_content_min_scale(&self) -> Result<f32, crate::Error> {
            self.inner
                .input_content_min_scale()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_content_width` property with availability checks.
        pub fn input_content_width(&self) -> Result<usize, crate::Error> {
            self.inner
                .input_content_width()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `input_content_width` property after validation.
        pub fn set_input_content_width(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_input_content_width(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_height` property with availability checks.
        pub fn input_height(&self) -> Result<usize, crate::Error> {
            self.inner.input_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `input_width` property with availability checks.
        pub fn input_width(&self) -> Result<usize, crate::Error> {
            self.inner.input_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `is_depth_reversed` property with availability checks.
        pub fn is_depth_reversed(&self) -> Result<bool, crate::Error> {
            self.inner
                .is_depth_reversed()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `jitter_offset_x` property with availability checks.
        pub fn jitter_offset_x(&self) -> Result<f32, crate::Error> {
            self.inner.jitter_offset_x().map_err(crate::Error::from_ffi)
        }
        /// Writes the `jitter_offset_x` property after validation.
        pub fn set_jitter_offset_x(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_jitter_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `jitter_offset_y` property with availability checks.
        pub fn jitter_offset_y(&self) -> Result<f32, crate::Error> {
            self.inner.jitter_offset_y().map_err(crate::Error::from_ffi)
        }
        /// Writes the `jitter_offset_y` property after validation.
        pub fn set_jitter_offset_y(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_jitter_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_content_offset_x` property with availability checks.
        pub fn motion_content_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_content_offset_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_content_offset_x` property after validation.
        pub fn set_motion_content_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_content_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_content_offset_y` property with availability checks.
        pub fn motion_content_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner
                .motion_content_offset_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_content_offset_y` property after validation.
        pub fn set_motion_content_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_motion_content_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_vector_scale_x` property with availability checks.
        pub fn motion_vector_scale_x(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_vector_scale_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_vector_scale_x` property after validation.
        pub fn set_motion_vector_scale_x(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_vector_scale_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_vector_scale_y` property with availability checks.
        pub fn motion_vector_scale_y(&self) -> Result<f32, crate::Error> {
            self.inner
                .motion_vector_scale_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_vector_scale_y` property after validation.
        pub fn set_motion_vector_scale_y(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_motion_vector_scale_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_height` property with availability checks.
        pub fn output_height(&self) -> Result<usize, crate::Error> {
            self.inner.output_height().map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_offset_x` property with availability checks.
        pub fn output_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner.output_offset_x().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_offset_x` property after validation.
        pub fn set_output_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_offset_y` property with availability checks.
        pub fn output_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner.output_offset_y().map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_offset_y` property after validation.
        pub fn set_output_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_output_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_width` property with availability checks.
        pub fn output_width(&self) -> Result<usize, crate::Error> {
            self.inner.output_width().map_err(crate::Error::from_ffi)
        }
        /// Reads the `pre_exposure` property with availability checks.
        pub fn pre_exposure(&self) -> Result<f32, crate::Error> {
            self.inner.pre_exposure().map_err(crate::Error::from_ffi)
        }
        /// Writes the `pre_exposure` property after validation.
        pub fn set_pre_exposure(&self, value: f32) -> Result<(), crate::Error> {
            self.inner
                .set_pre_exposure(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reactive_mask_content_offset_x` property with availability checks.
        pub fn reactive_mask_content_offset_x(&self) -> Result<usize, crate::Error> {
            self.inner
                .reactive_mask_content_offset_x()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `reactive_mask_content_offset_x` property after validation.
        pub fn set_reactive_mask_content_offset_x(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_reactive_mask_content_offset_x(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reactive_mask_content_offset_y` property with availability checks.
        pub fn reactive_mask_content_offset_y(&self) -> Result<usize, crate::Error> {
            self.inner
                .reactive_mask_content_offset_y()
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `reactive_mask_content_offset_y` property after validation.
        pub fn set_reactive_mask_content_offset_y(&self, value: usize) -> Result<(), crate::Error> {
            self.inner
                .set_reactive_mask_content_offset_y(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reset` property with availability checks.
        pub fn reset(&self) -> Result<bool, crate::Error> {
            self.inner.reset().map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_format` property with availability checks.
        pub fn color_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .color_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `color_texture_usage` property with availability checks.
        pub fn color_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .color_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_format` property with availability checks.
        pub fn depth_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .depth_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `depth_texture_usage` property with availability checks.
        pub fn depth_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .depth_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_format` property with availability checks.
        pub fn motion_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .motion_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `motion_texture_usage` property with availability checks.
        pub fn motion_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .motion_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_format` property with availability checks.
        pub fn output_texture_format(&self) -> Result<crate::metal::PixelFormat, crate::Error> {
            self.inner
                .output_texture_format()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `output_texture_usage` property with availability checks.
        pub fn output_texture_usage(&self) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .output_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the `reactive_mask_texture_usage` property with availability checks.
        pub fn reactive_mask_texture_usage(
            &self,
        ) -> Result<crate::metal::TextureUsage, crate::Error> {
            self.inner
                .reactive_mask_texture_usage()
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `color_texture` object property.
        pub fn color_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .color_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `color_texture` object property.
        pub fn set_color_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_color_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `depth_texture` object property.
        pub fn depth_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .depth_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `depth_texture` object property.
        pub fn set_depth_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_depth_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `exposure_texture` object property.
        pub fn exposure_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .exposure_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `exposure_texture` object property.
        pub fn set_exposure_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_exposure_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `fence` object property.
        pub fn fence(&self) -> Result<Option<crate::metal::Fence>, crate::Error> {
            self.inner
                .fence()
                .map(|value| value.map(crate::metal::Fence::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `fence` object property.
        pub fn set_fence(&self, value: Option<&crate::metal::Fence>) -> Result<(), crate::Error> {
            self.inner
                .set_fence(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `motion_texture` object property.
        pub fn motion_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .motion_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `motion_texture` object property.
        pub fn set_motion_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_motion_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `output_texture` object property.
        pub fn output_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .output_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `output_texture` object property.
        pub fn set_output_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_output_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Reads the optional `reactive_mask_texture` object property.
        pub fn reactive_mask_texture(&self) -> Result<Option<crate::metal::Texture>, crate::Error> {
            self.inner
                .reactive_mask_texture()
                .map(|value| value.map(crate::metal::Texture::from_ffi))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the `reactive_mask_texture` object property.
        pub fn set_reactive_mask_texture(
            &self,
            value: Option<&crate::metal::Texture>,
        ) -> Result<(), crate::Error> {
            self.inner
                .set_reactive_mask_texture(value.map(|value| &value.inner))
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_depth_reversed(&self, value: bool) -> Result<(), crate::Error> {
            self.inner
                .set_depth_reversed(value)
                .map_err(crate::Error::from_ffi)
        }
        /// Writes the Boolean property after availability checks.
        pub fn set_reset(&self, value: bool) -> Result<(), crate::Error> {
            self.inner.set_reset(value).map_err(crate::Error::from_ffi)
        }
    }
}
