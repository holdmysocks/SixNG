//! Audited reflection metadata and texture-view descriptor boundaries.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    Argument, ArrayType, PointerType, StructMember, StructType, TensorAuxiliaryPlaneType,
    TensorBinding, TensorReferenceType, TextureReferenceType, TextureViewDescriptor,
};
use crate::metal::generated_struct_types::TextureSwizzleChannels;
use crate::metal::generated_value_types::{ArgumentType, BindingAccess, DataType, TextureSwizzle};
use crate::metal::{PixelFormat, TextureType};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{msg_send, sel};
use objc2_foundation::NSString;
use objc2_metal::{MTLTextureSwizzle, MTLTextureSwizzleChannels, MTLTextureViewDescriptor};
use std::ops::Range;

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    name: &str,
) -> Result<(), Error> {
    // SAFETY: every Objective-C object implements `respondsToSelector:` and
    // `Sel` and `bool` have the runtime's stable declared ABIs.
    let available: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if available {
        Ok(())
    } else {
        Err(Error::unsupported(format!("{name} is unavailable")))
    }
}

fn optional_object(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    name: &str,
) -> Result<Option<Retained<AnyObject>>, Error> {
    require_selector(object, selector, name)?;
    // SAFETY: the selector is present and every caller supplies a property
    // declared by Metal to return a nullable Objective-C object.
    Ok(unsafe { msg_send![object, performSelector: selector] })
}

fn array_objects(
    array: Retained<AnyObject>,
    name: &str,
) -> Result<Vec<Retained<AnyObject>>, Error> {
    require_selector(&array, sel!(count), name)?;
    require_selector(&array, sel!(objectAtIndex:), name)?;
    // SAFETY: Metal declared the retained object as an NSArray, whose count
    // getter uses NSUInteger on all supported Apple targets.
    let count: usize = unsafe { msg_send![&*array, count] };
    let mut result = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: `index` is strictly below the NSArray count and the retained
        // result keeps the element alive independently of the array traversal.
        let value: Retained<AnyObject> = unsafe { msg_send![&*array, objectAtIndex: index] };
        result.push(value);
    }
    Ok(result)
}

macro_rules! optional_metadata_object {
    ($owner:ty, $method:ident, $selector:ident, $result:ty, $qualified:literal) => {
        impl $owner {
            #[doc = concat!("Returns the optional `", $qualified, "` reflection object.")]
            pub fn $method(&self) -> Result<Option<$result>, Error> {
                optional_object(self.as_inner(), sel!($selector), $qualified)
                    .map(|value| value.map(<$result>::from_inner))
            }
        }
    };
}

optional_metadata_object!(
    StructMember,
    array_type,
    arrayType,
    ArrayType,
    "MTL::StructMember::arrayType"
);
optional_metadata_object!(
    StructMember,
    pointer_type,
    pointerType,
    PointerType,
    "MTL::StructMember::pointerType"
);
optional_metadata_object!(
    StructMember,
    struct_type,
    structType,
    StructType,
    "MTL::StructMember::structType"
);
optional_metadata_object!(
    StructMember,
    tensor_reference_type,
    tensorReferenceType,
    TensorReferenceType,
    "MTL::StructMember::tensorReferenceType"
);
optional_metadata_object!(
    StructMember,
    texture_reference_type,
    textureReferenceType,
    TextureReferenceType,
    "MTL::StructMember::textureReferenceType"
);

optional_metadata_object!(
    ArrayType,
    element_array_type,
    elementArrayType,
    ArrayType,
    "MTL::ArrayType::elementArrayType"
);
optional_metadata_object!(
    ArrayType,
    element_pointer_type,
    elementPointerType,
    PointerType,
    "MTL::ArrayType::elementPointerType"
);
optional_metadata_object!(
    ArrayType,
    element_struct_type,
    elementStructType,
    StructType,
    "MTL::ArrayType::elementStructType"
);
optional_metadata_object!(
    ArrayType,
    element_tensor_reference_type,
    elementTensorReferenceType,
    TensorReferenceType,
    "MTL::ArrayType::elementTensorReferenceType"
);
optional_metadata_object!(
    ArrayType,
    element_texture_reference_type,
    elementTextureReferenceType,
    TextureReferenceType,
    "MTL::ArrayType::elementTextureReferenceType"
);

optional_metadata_object!(
    PointerType,
    element_array_type,
    elementArrayType,
    ArrayType,
    "MTL::PointerType::elementArrayType"
);
optional_metadata_object!(
    PointerType,
    element_struct_type,
    elementStructType,
    StructType,
    "MTL::PointerType::elementStructType"
);

impl StructType {
    /// Looks up a member by its Rust string name.
    pub fn member_by_name(&self, name: &str) -> Result<Option<StructMember>, Error> {
        require_selector(
            self.as_inner(),
            sel!(memberByName:),
            "MTL::StructType::memberByName",
        )?;
        let name = NSString::from_str(name);
        // SAFETY: selector availability was checked and the live NSString has
        // the exact argument type declared by Metal.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), memberByName: &*name] };
        Ok(value.map(StructMember::from_inner))
    }

    /// Returns all members as an owned Rust vector.
    pub fn members_vec(&self) -> Result<Vec<StructMember>, Error> {
        let Some(array) =
            optional_object(self.as_inner(), sel!(members), "MTL::StructType::members")?
        else {
            return Ok(Vec::new());
        };
        array_objects(array, "MTL::StructType::members")
            .map(|values| values.into_iter().map(StructMember::from_inner).collect())
    }
}

fn auxiliary_plane_types(
    object: &AnyObject,
    name: &str,
) -> Result<Vec<TensorAuxiliaryPlaneType>, Error> {
    let Some(array) = optional_object(object, sel!(auxiliaryPlanes), name)? else {
        return Ok(Vec::new());
    };
    array_objects(array, name).map(|values| {
        values
            .into_iter()
            .map(TensorAuxiliaryPlaneType::from_inner)
            .collect()
    })
}

impl TensorReferenceType {
    /// Returns auxiliary-plane metadata as an owned Rust vector.
    pub fn auxiliary_plane_types(&self) -> Result<Vec<TensorAuxiliaryPlaneType>, Error> {
        auxiliary_plane_types(self.as_inner(), "MTL::TensorReferenceType::auxiliaryPlanes")
    }
}

impl TensorBinding {
    /// Returns auxiliary-plane metadata as an owned Rust vector.
    pub fn auxiliary_plane_types(&self) -> Result<Vec<TensorAuxiliaryPlaneType>, Error> {
        auxiliary_plane_types(self.as_inner(), "MTL::TensorBinding::auxiliaryPlanes")
    }
}

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
    /// Kind-specific metadata, queried only for the matching argument kind.
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
    /// An argument kind without additional metadata in `MTL::Argument`.
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

fn checked_range(value: Range<usize>, name: &str) -> Result<objc2_foundation::NSRange, Error> {
    if value.start >= value.end {
        return Err(Error::invalid_argument(format!(
            "{name} must be a non-empty forward range"
        )));
    }
    let length = value
        .end
        .checked_sub(value.start)
        .ok_or_else(|| Error::invalid_argument(format!("{name} range overflow")))?;
    Ok(objc2_foundation::NSRange::new(value.start, length))
}

fn texture_view_descriptor(
    value: &TextureViewDescriptor,
) -> Result<&MTLTextureViewDescriptor, Error> {
    value
        .as_inner()
        .downcast_ref::<MTLTextureViewDescriptor>()
        .ok_or_else(|| Error::unsupported("texture-view descriptor has an unexpected class"))
}

fn swizzle_to_objc(value: &TextureSwizzleChannels) -> Result<MTLTextureSwizzleChannels, Error> {
    for channel in [value.red, value.green, value.blue, value.alpha] {
        if !channel.is_valid() {
            return Err(Error::invalid_argument(
                "texture swizzle contains an undeclared channel",
            ));
        }
    }
    Ok(MTLTextureSwizzleChannels {
        red: MTLTextureSwizzle(value.red.as_raw()),
        green: MTLTextureSwizzle(value.green.as_raw()),
        blue: MTLTextureSwizzle(value.blue.as_raw()),
        alpha: MTLTextureSwizzle(value.alpha.as_raw()),
    })
}

fn swizzle_from_objc(value: MTLTextureSwizzleChannels) -> TextureSwizzleChannels {
    TextureSwizzleChannels {
        red: TextureSwizzle::from_system_raw(value.red.0),
        green: TextureSwizzle::from_system_raw(value.green.0),
        blue: TextureSwizzle::from_system_raw(value.blue.0),
        alpha: TextureSwizzle::from_system_raw(value.alpha.0),
    }
}

impl TextureViewDescriptor {
    /// Creates a descriptor with all view properties validated before use.
    pub fn with_properties(
        pixel_format: PixelFormat,
        texture_type: TextureType,
        levels: Range<usize>,
        slices: Range<usize>,
        swizzle: &TextureSwizzleChannels,
    ) -> Result<Self, Error> {
        let value = Self::new()?;
        value.set_pixel_format(pixel_format)?;
        value.set_texture_type(texture_type)?;
        value.set_level_range(levels)?;
        value.set_slice_range(slices)?;
        value.set_swizzle(swizzle)?;
        Ok(value)
    }

    /// Returns the descriptor mip-level range as an owned Rust range.
    pub fn level_range(&self) -> Result<Range<usize>, Error> {
        require_selector(
            self.as_inner(),
            sel!(levelRange),
            "MTL::TextureViewDescriptor::levelRange",
        )?;
        let range = texture_view_descriptor(self)?.levelRange();
        let end = range
            .location
            .checked_add(range.length)
            .ok_or_else(|| Error::unsupported("Metal returned an overflowing level range"))?;
        Ok(range.location..end)
    }

    /// Sets a checked non-empty mip-level range.
    pub fn set_level_range(&self, value: Range<usize>) -> Result<(), Error> {
        let value = checked_range(value, "texture-view level range")?;
        require_selector(
            self.as_inner(),
            sel!(setLevelRange:),
            "MTL::TextureViewDescriptor::setLevelRange",
        )?;
        // SAFETY: the range is non-empty, forward, and cannot overflow; final
        // resource-relative bounds are validated when Metal creates the view.
        unsafe { texture_view_descriptor(self)?.setLevelRange(value) };
        Ok(())
    }

    /// Returns the descriptor slice range as an owned Rust range.
    pub fn slice_range(&self) -> Result<Range<usize>, Error> {
        require_selector(
            self.as_inner(),
            sel!(sliceRange),
            "MTL::TextureViewDescriptor::sliceRange",
        )?;
        let range = texture_view_descriptor(self)?.sliceRange();
        let end = range
            .location
            .checked_add(range.length)
            .ok_or_else(|| Error::unsupported("Metal returned an overflowing slice range"))?;
        Ok(range.location..end)
    }

    /// Sets a checked non-empty texture-slice range.
    pub fn set_slice_range(&self, value: Range<usize>) -> Result<(), Error> {
        let value = checked_range(value, "texture-view slice range")?;
        require_selector(
            self.as_inner(),
            sel!(setSliceRange:),
            "MTL::TextureViewDescriptor::setSliceRange",
        )?;
        // SAFETY: the range is non-empty, forward, and cannot overflow; final
        // resource-relative bounds are validated when Metal creates the view.
        unsafe { texture_view_descriptor(self)?.setSliceRange(value) };
        Ok(())
    }

    /// Returns the descriptor's channel swizzle as a safe Rust value.
    pub fn swizzle(&self) -> Result<TextureSwizzleChannels, Error> {
        require_selector(
            self.as_inner(),
            sel!(swizzle),
            "MTL::TextureViewDescriptor::swizzle",
        )?;
        Ok(swizzle_from_objc(texture_view_descriptor(self)?.swizzle()))
    }

    /// Sets the channel swizzle after validating every component.
    pub fn set_swizzle(&self, value: &TextureSwizzleChannels) -> Result<(), Error> {
        let value = swizzle_to_objc(value)?;
        require_selector(
            self.as_inner(),
            sel!(setSwizzle:),
            "MTL::TextureViewDescriptor::setSwizzle",
        )?;
        texture_view_descriptor(self)?.setSwizzle(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texture_view_ranges_must_be_non_empty_and_forward() {
        assert!(checked_range(2..2, "levels").is_err());
        assert!(checked_range(Range { start: 3, end: 2 }, "levels",).is_err());
        let range = checked_range(2..5, "levels").expect("valid range");
        assert_eq!(range.location, 2);
        assert_eq!(range.length, 3);
    }

    #[test]
    fn texture_view_swizzle_rejects_unknown_channels() {
        let value = TextureSwizzleChannels {
            red: TextureSwizzle::from_system_raw(6),
            green: TextureSwizzle::TextureSwizzleGreen,
            blue: TextureSwizzle::TextureSwizzleBlue,
            alpha: TextureSwizzle::TextureSwizzleAlpha,
        };
        assert!(swizzle_to_objc(&value).is_err());
    }
}
