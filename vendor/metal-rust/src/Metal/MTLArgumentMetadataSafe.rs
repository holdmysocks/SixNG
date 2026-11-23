//! Safe argument reflection metadata and texture-view descriptors.

use super::{
    Argument, ArgumentType, ArrayType, BindingAccess, DataType, PixelFormat, PointerType,
    StructMember, StructType, TensorAuxiliaryPlaneType, TensorBinding, TensorReferenceType,
    TextureReferenceType, TextureSwizzleChannels, TextureType, TextureViewDescriptor,
};
use crate::Error;
use std::ops::Range;

/// An owned, internally consistent snapshot of one Metal argument.
#[derive(Clone)]
pub struct ArgumentMetadata {
    /// The optional source-level argument name.
    pub name: Option<String>,
    /// The binding index.
    pub index: usize,
    /// The array element count reported by Metal.
    pub array_length: usize,
    /// The shader access mode.
    pub access: BindingAccess,
    /// Whether the reflected argument is active.
    pub active: bool,
    /// The reflected argument kind.
    pub kind: ArgumentType,
    /// Kind-specific metadata.
    pub details: ArgumentDetails,
}

/// Kind-specific metadata for [`ArgumentMetadata`].
#[derive(Clone)]
pub enum ArgumentDetails {
    /// Buffer-specific layout and pointee information.
    Buffer {
        /// Required byte alignment.
        alignment: usize,
        /// Reflected byte size.
        data_size: usize,
        /// Reflected element data type.
        data_type: DataType,
        /// Optional pointer reflection.
        pointer_type: Option<PointerType>,
        /// Optional structure reflection.
        struct_type: Option<StructType>,
    },
    /// Texture-specific type information.
    Texture {
        /// Reflected texel data type.
        data_type: DataType,
        /// Texture dimensionality.
        texture_type: TextureType,
        /// Whether this is a depth texture.
        is_depth: bool,
    },
    /// Threadgroup-memory-specific layout.
    ThreadgroupMemory {
        /// Required byte alignment.
        alignment: usize,
        /// Reflected byte size.
        data_size: usize,
    },
    /// An argument kind without additional `MTL::Argument` metadata.
    Other,
}

impl Argument {
    /// Copies all applicable properties into a safe Rust metadata snapshot.
    pub fn metadata(&self) -> Result<ArgumentMetadata, Error> {
        let kind = self.r#type()?;
        let details = if kind == ArgumentType::ArgumentTypeBuffer {
            ArgumentDetails::Buffer {
                alignment: self.buffer_alignment()?,
                data_size: self.buffer_data_size()?,
                data_type: self.buffer_data_type()?,
                pointer_type: self.buffer_pointer_type()?,
                struct_type: self.buffer_struct_type()?,
            }
        } else if kind == ArgumentType::ArgumentTypeTexture {
            ArgumentDetails::Texture {
                data_type: self.texture_data_type()?,
                texture_type: self.texture_type()?,
                is_depth: self.is_depth_texture()?,
            }
        } else if kind == ArgumentType::ArgumentTypeThreadgroupMemory {
            ArgumentDetails::ThreadgroupMemory {
                alignment: self.threadgroup_memory_alignment()?,
                data_size: self.threadgroup_memory_data_size()?,
            }
        } else {
            ArgumentDetails::Other
        };
        Ok(ArgumentMetadata {
            name: self.name()?,
            index: self.index()?,
            array_length: self.array_length()?,
            access: self.access()?,
            active: self.is_active()?,
            kind,
            details,
        })
    }
}

impl StructMember {
    /// Returns optional array reflection for this member.
    pub fn array_type(&self) -> Result<Option<ArrayType>, Error> {
        self.inner
            .array_type()
            .map(|value| value.map(ArrayType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional pointer reflection for this member.
    pub fn pointer_type(&self) -> Result<Option<PointerType>, Error> {
        self.inner
            .pointer_type()
            .map(|value| value.map(PointerType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional structure reflection for this member.
    pub fn struct_type(&self) -> Result<Option<StructType>, Error> {
        self.inner
            .struct_type()
            .map(|value| value.map(StructType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional tensor-reference reflection for this member.
    pub fn tensor_reference_type(&self) -> Result<Option<TensorReferenceType>, Error> {
        self.inner
            .tensor_reference_type()
            .map(|value| value.map(TensorReferenceType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional texture-reference reflection for this member.
    pub fn texture_reference_type(&self) -> Result<Option<TextureReferenceType>, Error> {
        self.inner
            .texture_reference_type()
            .map(|value| value.map(TextureReferenceType::from_ffi))
            .map_err(Error::from_ffi)
    }
}

impl StructType {
    /// Looks up a reflected member by its Rust string name.
    pub fn member_by_name(&self, name: &str) -> Result<Option<StructMember>, Error> {
        self.inner
            .member_by_name(name)
            .map(|value| value.map(StructMember::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns all reflected members as an owned Rust vector.
    pub fn members(&self) -> Result<Vec<StructMember>, Error> {
        self.inner
            .members_vec()
            .map(|values| values.into_iter().map(StructMember::from_ffi).collect())
            .map_err(Error::from_ffi)
    }
}

impl ArrayType {
    /// Returns optional nested array reflection.
    pub fn element_array_type(&self) -> Result<Option<ArrayType>, Error> {
        self.inner
            .element_array_type()
            .map(|value| value.map(ArrayType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional element pointer reflection.
    pub fn element_pointer_type(&self) -> Result<Option<PointerType>, Error> {
        self.inner
            .element_pointer_type()
            .map(|value| value.map(PointerType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional element structure reflection.
    pub fn element_struct_type(&self) -> Result<Option<StructType>, Error> {
        self.inner
            .element_struct_type()
            .map(|value| value.map(StructType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional element tensor-reference reflection.
    pub fn element_tensor_reference_type(&self) -> Result<Option<TensorReferenceType>, Error> {
        self.inner
            .element_tensor_reference_type()
            .map(|value| value.map(TensorReferenceType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional element texture-reference reflection.
    pub fn element_texture_reference_type(&self) -> Result<Option<TextureReferenceType>, Error> {
        self.inner
            .element_texture_reference_type()
            .map(|value| value.map(TextureReferenceType::from_ffi))
            .map_err(Error::from_ffi)
    }
}

impl PointerType {
    /// Returns optional nested array reflection.
    pub fn element_array_type(&self) -> Result<Option<ArrayType>, Error> {
        self.inner
            .element_array_type()
            .map(|value| value.map(ArrayType::from_ffi))
            .map_err(Error::from_ffi)
    }

    /// Returns optional element structure reflection.
    pub fn element_struct_type(&self) -> Result<Option<StructType>, Error> {
        self.inner
            .element_struct_type()
            .map(|value| value.map(StructType::from_ffi))
            .map_err(Error::from_ffi)
    }
}

impl TensorReferenceType {
    /// Returns auxiliary-plane metadata as an owned Rust vector.
    pub fn auxiliary_planes(&self) -> Result<Vec<TensorAuxiliaryPlaneType>, Error> {
        self.inner
            .auxiliary_plane_types()
            .map(|values| {
                values
                    .into_iter()
                    .map(TensorAuxiliaryPlaneType::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }
}

impl TensorBinding {
    /// Returns auxiliary-plane metadata as an owned Rust vector.
    pub fn auxiliary_planes(&self) -> Result<Vec<TensorAuxiliaryPlaneType>, Error> {
        self.inner
            .auxiliary_plane_types()
            .map(|values| {
                values
                    .into_iter()
                    .map(TensorAuxiliaryPlaneType::from_ffi)
                    .collect()
            })
            .map_err(Error::from_ffi)
    }
}

impl TextureViewDescriptor {
    /// Creates a descriptor with every view property checked before use.
    pub fn with_properties(
        pixel_format: PixelFormat,
        texture_type: TextureType,
        levels: Range<usize>,
        slices: Range<usize>,
        swizzle: &TextureSwizzleChannels,
    ) -> Result<Self, Error> {
        metal_rust_ffi::__private::objects::metal::TextureViewDescriptor::with_properties(
            pixel_format,
            texture_type,
            levels,
            slices,
            swizzle,
        )
        .map(Self::from_ffi)
        .map_err(Error::from_ffi)
    }

    /// Returns the descriptor mip-level range.
    pub fn level_range(&self) -> Result<Range<usize>, Error> {
        self.inner.level_range().map_err(Error::from_ffi)
    }

    /// Sets a checked non-empty mip-level range.
    pub fn set_level_range(&self, value: Range<usize>) -> Result<(), Error> {
        self.inner.set_level_range(value).map_err(Error::from_ffi)
    }

    /// Returns the descriptor slice range.
    pub fn slice_range(&self) -> Result<Range<usize>, Error> {
        self.inner.slice_range().map_err(Error::from_ffi)
    }

    /// Sets a checked non-empty texture-slice range.
    pub fn set_slice_range(&self, value: Range<usize>) -> Result<(), Error> {
        self.inner.set_slice_range(value).map_err(Error::from_ffi)
    }

    /// Returns the descriptor channel swizzle.
    pub fn swizzle(&self) -> Result<TextureSwizzleChannels, Error> {
        self.inner.swizzle().map_err(Error::from_ffi)
    }

    /// Sets the channel swizzle after validating every component.
    pub fn set_swizzle(&self, value: &TextureSwizzleChannels) -> Result<(), Error> {
        self.inner.set_swizzle(value).map_err(Error::from_ffi)
    }
}
