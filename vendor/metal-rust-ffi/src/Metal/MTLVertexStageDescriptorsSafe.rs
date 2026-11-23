//! Checked indexed access for vertex and stage-input descriptor arrays.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{
    AttributeDescriptor, AttributeDescriptorArray, BufferLayoutDescriptor,
    BufferLayoutDescriptorArray, StageInputOutputDescriptor, VertexAttributeDescriptor,
    VertexAttributeDescriptorArray, VertexBufferLayoutDescriptor,
    VertexBufferLayoutDescriptorArray, VertexDescriptor,
};
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{msg_send, sel};

/// Metal exposes 31 vertex/stage attribute and buffer binding slots.
const MAX_DESCRIPTOR_SLOTS: usize = 31;

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

fn checked_slot(index: usize, kind: &str) -> Result<usize, Error> {
    if index < MAX_DESCRIPTOR_SLOTS {
        Ok(index)
    } else {
        Err(Error::invalid_argument(format!(
            "{kind} index must be below {MAX_DESCRIPTOR_SLOTS}"
        )))
    }
}

macro_rules! descriptor_array_access {
    ($array:ty, $item:ty, $kind:literal, $context:literal) => {
        impl $array {
            /// Returns the descriptor at a checked Metal binding slot.
            pub fn get(&self, index: usize) -> Result<$item, Error> {
                let index = checked_slot(index, $kind)?;
                require_selector(
                    self.as_inner(),
                    sel!(objectAtIndexedSubscript:),
                    concat!($context, "::object"),
                )?;
                // SAFETY: selector presence and Metal's fixed 31-slot bound are
                // checked; objc2 retains the non-null descriptor result.
                let value: Retained<AnyObject> =
                    unsafe { msg_send![self.as_inner(), objectAtIndexedSubscript: index] };
                Ok(<$item>::from_inner(value))
            }

            /// Sets or clears the descriptor at a checked Metal binding slot.
            pub fn set(&self, index: usize, value: Option<&$item>) -> Result<(), Error> {
                let index = checked_slot(index, $kind)?;
                require_selector(
                    self.as_inner(),
                    sel!(setObject:atIndexedSubscript:),
                    concat!($context, "::setObject"),
                )?;
                // SAFETY: selector presence, fixed slot bound, nullable class-
                // preserving wrapper, and argument ABI are checked above.
                unsafe {
                    let _: () = msg_send![self.as_inner(), setObject: value.map(<$item>::as_inner), atIndexedSubscript: index];
                }
                Ok(())
            }
        }
    };
}

descriptor_array_access!(
    VertexBufferLayoutDescriptorArray,
    VertexBufferLayoutDescriptor,
    "vertex buffer layout",
    "MTL::VertexBufferLayoutDescriptorArray"
);
descriptor_array_access!(
    VertexAttributeDescriptorArray,
    VertexAttributeDescriptor,
    "vertex attribute",
    "MTL::VertexAttributeDescriptorArray"
);
descriptor_array_access!(
    BufferLayoutDescriptorArray,
    BufferLayoutDescriptor,
    "stage buffer layout",
    "MTL::BufferLayoutDescriptorArray"
);
descriptor_array_access!(
    AttributeDescriptorArray,
    AttributeDescriptor,
    "stage attribute",
    "MTL::AttributeDescriptorArray"
);

macro_rules! descriptor_factory_and_reset {
    ($type:ty, $class:literal, $factory:ident, $factory_name:literal, $reset_name:literal) => {
        impl $type {
            /// Creates the framework's autoreleased convenience descriptor as an owned wrapper.
            pub fn default_descriptor() -> Result<Self, Error> {
                let class = AnyClass::get($class).ok_or_else(|| {
                    Error::unsupported(concat!($factory_name, " class is unavailable"))
                })?;
                // SAFETY: Objective-C class objects implement respondsToSelector:
                // with the stable selector/bool ABI.
                let available: bool =
                    unsafe { msg_send![class, respondsToSelector: sel!($factory)] };
                if !available {
                    return Err(Error::unsupported(concat!($factory_name, " is unavailable")));
                }
                // SAFETY: class and selector availability are checked; the
                // convenience factory returns the declared non-null descriptor,
                // retained by objc2 before wrapping.
                let value: Retained<AnyObject> = unsafe { msg_send![class, $factory] };
                Ok(Self::from_inner(value))
            }

            /// Resets the descriptor after checking runtime selector availability.
            pub fn reset_safe(&self) -> Result<(), Error> {
                require_selector(self.as_inner(), sel!(reset), $reset_name)?;
                // SAFETY: selector presence is checked and reset has no arguments/result.
                unsafe {
                    let _: () = msg_send![self.as_inner(), reset];
                }
                Ok(())
            }
        }
    };
}

descriptor_factory_and_reset!(
    VertexDescriptor,
    c"MTLVertexDescriptor",
    vertexDescriptor,
    "MTL::VertexDescriptor::vertexDescriptor",
    "MTL::VertexDescriptor::reset"
);
descriptor_factory_and_reset!(
    StageInputOutputDescriptor,
    c"MTLStageInputOutputDescriptor",
    stageInputOutputDescriptor,
    "MTL::StageInputOutputDescriptor::stageInputOutputDescriptor",
    "MTL::StageInputOutputDescriptor::reset"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descriptor_slots_use_metal_fixed_bounds() {
        assert_eq!(checked_slot(0, "slot").unwrap(), 0);
        assert_eq!(checked_slot(30, "slot").unwrap(), 30);
        assert!(checked_slot(31, "slot").is_err());
        assert!(checked_slot(usize::MAX, "slot").is_err());
    }
}
