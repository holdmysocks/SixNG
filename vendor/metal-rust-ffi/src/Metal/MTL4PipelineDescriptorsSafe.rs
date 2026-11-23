//! Safe bridges for Metal 4 pipeline descriptor values and collections.

use crate::foundation::Error;
use crate::metal::generated_object_types::{metal::Binding, metal4};
use crate::metal::{CheckedTensorExtents, Size};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};
use objc2_foundation::NSRange;
use objc2_metal::MTLSize;

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

fn checked_size(value: Size, name: &str) -> Result<MTLSize, Error> {
    if value.width == 0 || value.height == 0 || value.depth == 0 {
        return Err(Error::invalid_argument(format!(
            "{name} dimensions must all be non-zero"
        )));
    }
    Ok(value.into())
}

fn from_mtl_size(value: MTLSize) -> Size {
    Size::new(value.width, value.height, value.depth)
}

fn tensor_extents_object(value: &CheckedTensorExtents) -> Result<Retained<AnyObject>, Error> {
    let class = AnyClass::get(c"MTLTensorExtents")
        .ok_or_else(|| Error::unsupported("MTLTensorExtents is unavailable on this system"))?;
    // SAFETY: class objects implement instancesRespondToSelector: with the
    // declared selector/bool ABI.
    let available: bool =
        unsafe { msg_send![class, instancesRespondToSelector: sel!(initWithRank:values:)] };
    if !available {
        return Err(Error::unsupported(
            "MTLTensorExtents initializer is unavailable",
        ));
    }
    let values: Vec<isize> = value
        .as_slice()
        .iter()
        .map(|&dimension| dimension as isize)
        .collect();
    // SAFETY: alloc returns a retained uninitialized object of the checked class.
    let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
    let pointer = if values.is_empty() {
        std::ptr::null()
    } else {
        values.as_ptr()
    };
    // SAFETY: CheckedTensorExtents constrains rank and NSInteger conversion;
    // pointer covers exactly rank elements until this initializer returns.
    let inner: Option<Retained<AnyObject>> =
        unsafe { msg_send![allocated, initWithRank: values.len(), values: pointer] };
    inner.ok_or_else(|| Error::invalid_argument("Metal rejected tensor extents"))
}

fn checked_extents_from_object(object: Retained<AnyObject>) -> Result<CheckedTensorExtents, Error> {
    require_selector(&object, sel!(rank), "MTL::TensorExtents::rank")?;
    require_selector(
        &object,
        sel!(extentAtDimensionIndex:),
        "MTL::TensorExtents::extentAtDimensionIndex",
    )?;
    // SAFETY: selector presence is checked and rank returns NSUInteger.
    let rank: usize = unsafe { msg_send![&*object, rank] };
    if rank > crate::metal::MAX_TENSOR_RANK {
        return Err(Error::invalid_argument(
            "Metal returned tensor extents above the supported maximum rank",
        ));
    }
    let values = (0..rank)
        .map(|dimension| {
            // SAFETY: dimension is below the rank read from the same immutable object.
            unsafe { msg_send![&*object, extentAtDimensionIndex: dimension] }
        })
        .collect::<Vec<usize>>();
    CheckedTensorExtents::new(&values)
}

fn object_array(values: &[Retained<AnyObject>]) -> Retained<AnyObject> {
    let class = AnyClass::get(c"NSMutableArray")
        .expect("Foundation provides NSMutableArray whenever Metal is loaded");
    // SAFETY: NSMutableArray implements new and returns a retained empty array.
    let array: Retained<AnyObject> = unsafe { msg_send![class, new] };
    for value in values {
        // SAFETY: the array and non-null object remain live for the message.
        unsafe {
            let _: () = msg_send![&*array, addObject: &**value];
        }
    }
    array
}

impl metal4::MeshRenderPipelineDescriptor {
    /// Returns required mesh threadgroup dimensions after availability checks.
    pub fn required_threads_per_mesh_threadgroup_safe(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(requiredThreadsPerMeshThreadgroup),
            "MTL4::MeshRenderPipelineDescriptor::requiredThreadsPerMeshThreadgroup",
        )?;
        // SAFETY: selector presence is checked and MTLSize is the SDK ABI type.
        let value: MTLSize =
            unsafe { msg_send![self.as_inner(), requiredThreadsPerMeshThreadgroup] };
        Ok(from_mtl_size(value))
    }

    /// Sets validated required mesh threadgroup dimensions.
    pub fn set_required_threads_per_mesh_threadgroup_safe(&self, value: Size) -> Result<(), Error> {
        let value = checked_size(value, "required mesh threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(setRequiredThreadsPerMeshThreadgroup:),
            "MTL4::MeshRenderPipelineDescriptor::setRequiredThreadsPerMeshThreadgroup",
        )?;
        // SAFETY: selector presence and non-zero MTLSize value are checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), setRequiredThreadsPerMeshThreadgroup: value];
        }
        Ok(())
    }

    /// Returns required object threadgroup dimensions after availability checks.
    pub fn required_threads_per_object_threadgroup_safe(&self) -> Result<Size, Error> {
        require_selector(
            self.as_inner(),
            sel!(requiredThreadsPerObjectThreadgroup),
            "MTL4::MeshRenderPipelineDescriptor::requiredThreadsPerObjectThreadgroup",
        )?;
        // SAFETY: selector presence is checked and MTLSize is the SDK ABI type.
        let value: MTLSize =
            unsafe { msg_send![self.as_inner(), requiredThreadsPerObjectThreadgroup] };
        Ok(from_mtl_size(value))
    }

    /// Sets validated required object threadgroup dimensions.
    pub fn set_required_threads_per_object_threadgroup_safe(
        &self,
        value: Size,
    ) -> Result<(), Error> {
        let value = checked_size(value, "required object threadgroup")?;
        require_selector(
            self.as_inner(),
            sel!(setRequiredThreadsPerObjectThreadgroup:),
            "MTL4::MeshRenderPipelineDescriptor::setRequiredThreadsPerObjectThreadgroup",
        )?;
        // SAFETY: selector presence and non-zero MTLSize value are checked.
        unsafe {
            let _: () = msg_send![self.as_inner(), setRequiredThreadsPerObjectThreadgroup: value];
        }
        Ok(())
    }

    /// Resets the descriptor after checking runtime availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL4::MeshRenderPipelineDescriptor::reset",
        )?;
        // SAFETY: selector presence is checked and reset has no arguments/result.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

impl metal4::MachineLearningPipelineDescriptor {
    /// Returns optional checked input dimensions for a non-negative buffer index.
    pub fn input_dimensions(
        &self,
        buffer_index: usize,
    ) -> Result<Option<CheckedTensorExtents>, Error> {
        let buffer_index = isize::try_from(buffer_index).map_err(|_| {
            Error::invalid_argument("machine-learning buffer index exceeds NSInteger")
        })?;
        require_selector(
            self.as_inner(),
            sel!(inputDimensionsAtBufferIndex:),
            "MTL4::MachineLearningPipelineDescriptor::inputDimensionsAtBufferIndex",
        )?;
        // SAFETY: selector presence and non-negative NSInteger conversion are checked;
        // objc2 retains the nullable MTLTensorExtents result.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![self.as_inner(), inputDimensionsAtBufferIndex: buffer_index] };
        value.map(checked_extents_from_object).transpose()
    }

    /// Sets or clears checked input dimensions for a non-negative buffer index.
    pub fn set_input_dimensions(
        &self,
        buffer_index: usize,
        dimensions: Option<&CheckedTensorExtents>,
    ) -> Result<(), Error> {
        let buffer_index = isize::try_from(buffer_index).map_err(|_| {
            Error::invalid_argument("machine-learning buffer index exceeds NSInteger")
        })?;
        let dimensions = dimensions.map(tensor_extents_object).transpose()?;
        require_selector(
            self.as_inner(),
            sel!(setInputDimensions:atBufferIndex:),
            "MTL4::MachineLearningPipelineDescriptor::setInputDimensions",
        )?;
        // SAFETY: selector presence, NSInteger conversion, nullable object class,
        // and extents memory ownership are checked above.
        unsafe {
            let _: () = msg_send![self.as_inner(), setInputDimensions: dimensions.as_deref(), atBufferIndex: buffer_index];
        }
        Ok(())
    }

    /// Sets a contiguous range of checked input dimensions from a Rust slice.
    pub fn set_input_dimensions_slice(
        &self,
        start_index: usize,
        dimensions: &[CheckedTensorExtents],
    ) -> Result<(), Error> {
        let end = start_index
            .checked_add(dimensions.len())
            .ok_or_else(|| Error::invalid_argument("machine-learning buffer range overflows"))?;
        if start_index > isize::MAX as usize || end > isize::MAX as usize {
            return Err(Error::invalid_argument(
                "machine-learning buffer range exceeds NSInteger",
            ));
        }
        let dimensions = dimensions
            .iter()
            .map(tensor_extents_object)
            .collect::<Result<Vec<_>, _>>()?;
        let array = object_array(&dimensions);
        require_selector(
            self.as_inner(),
            sel!(setInputDimensions:withRange:),
            "MTL4::MachineLearningPipelineDescriptor::setInputDimensions",
        )?;
        // SAFETY: selector presence, object element classes, range overflow,
        // and the exact NSArray/NSRange ABI types are checked above.
        unsafe {
            let _: () = msg_send![self.as_inner(), setInputDimensions: &*array, withRange: NSRange::new(start_index, dimensions.len())];
        }
        Ok(())
    }

    /// Resets the descriptor after checking runtime availability.
    pub fn reset_safe(&self) -> Result<(), Error> {
        require_selector(
            self.as_inner(),
            sel!(reset),
            "MTL4::MachineLearningPipelineDescriptor::reset",
        )?;
        // SAFETY: selector presence is checked and reset has no arguments/result.
        unsafe {
            let _: () = msg_send![self.as_inner(), reset];
        }
        Ok(())
    }
}

impl metal4::MachineLearningPipelineReflection {
    /// Returns reflection bindings as owned safe facade inputs.
    pub fn bindings_vec(&self) -> Result<Vec<Binding>, Error> {
        require_selector(
            self.as_inner(),
            sel!(bindings),
            "MTL4::MachineLearningPipelineReflection::bindings",
        )?;
        // SAFETY: selector presence is checked and the property is a nullable NSArray.
        let array: Option<Retained<AnyObject>> = unsafe { msg_send![self.as_inner(), bindings] };
        let Some(array) = array else {
            return Ok(Vec::new());
        };
        // SAFETY: the returned object is declared NSArray and count uses NSUInteger.
        let count: usize = unsafe { msg_send![&*array, count] };
        (0..count)
            .map(|index| {
                // SAFETY: index is below count and the array is declared to contain MTLBinding.
                let value = unsafe { msg_send![&*array, objectAtIndex: index] };
                Ok(Binding::from_inner(value))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metal4_required_threadgroup_dimensions_reject_zero() {
        assert!(checked_size(Size::new(1, 2, 3), "threads").is_ok());
        assert!(checked_size(Size::new(0, 2, 3), "threads").is_err());
    }
}
