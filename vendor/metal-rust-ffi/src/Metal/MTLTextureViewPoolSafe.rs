//! Audited lightweight texture-view-pool operations.

use crate::foundation::Error;
use crate::metal::generated_object_types::metal::{TextureViewDescriptor, TextureViewPool};
use crate::metal::generated_struct_types::ResourceID;
use crate::metal::{Buffer, Texture, TextureDescriptor};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{msg_send, sel};
use objc2_metal::MTLResourceID;

fn responds_to(object: &AnyObject, selector: Sel) -> bool {
    // SAFETY: every Objective-C object implements respondsToSelector:, and
    // selector and bool use the runtime's declared ABI encodings.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

fn require_selector(object: &AnyObject, selector: Sel, message: &'static str) -> Result<(), Error> {
    if responds_to(object, selector) {
        Ok(())
    } else {
        Err(Error::unsupported(message))
    }
}

fn device(object: &AnyObject, subject: &'static str) -> Result<Retained<AnyObject>, Error> {
    require_selector(
        object,
        sel!(device),
        "Metal object device query is unavailable",
    )?;
    // SAFETY: the selector is present and Metal resource/view-pool protocols
    // declare a non-null object result.
    let value: Option<Retained<AnyObject>> = unsafe { msg_send![object, device] };
    value.ok_or_else(|| Error::unsupported(format!("{subject} device is unavailable")))
}

fn require_same_device(
    pool: &TextureViewPool,
    resource: &AnyObject,
    subject: &'static str,
) -> Result<(), Error> {
    let pool_device = device(pool.as_inner(), "texture-view pool")?;
    let resource_device = device(resource, subject)?;
    if std::ptr::eq(&*pool_device, &*resource_device) {
        Ok(())
    } else {
        Err(Error::invalid_argument(
            "texture-view pool and source resource must belong to the same Metal device",
        ))
    }
}

fn checked_slot(pool: &TextureViewPool, index: usize) -> Result<(), Error> {
    require_selector(
        pool.as_inner(),
        sel!(resourceViewCount),
        "texture-view pool size queries are unavailable",
    )?;
    // SAFETY: the selector is present and returns NSUInteger.
    let count: usize = unsafe { msg_send![pool.as_inner(), resourceViewCount] };
    if index < count {
        Ok(())
    } else {
        Err(Error::invalid_argument(
            "texture-view pool index is out of bounds",
        ))
    }
}

fn resource_id_snapshot(value: MTLResourceID) -> ResourceID {
    ResourceID {
        _impl: value.to_raw(),
    }
}

fn validate_buffer_view_layout(
    buffer: &Buffer,
    descriptor: &TextureDescriptor,
    offset: usize,
    bytes_per_row: usize,
) -> Result<(), Error> {
    if offset >= buffer.length() || bytes_per_row == 0 || bytes_per_row > buffer.length() - offset {
        return Err(Error::invalid_argument(
            "buffer texture offset or first row exceeds the source buffer",
        ));
    }

    let buffer_device = device(buffer.as_any_object(), "buffer")?;
    let format = descriptor.pixel_format().as_raw();
    require_selector(
        &buffer_device,
        sel!(minimumLinearTextureAlignmentForPixelFormat:),
        "linear texture row-alignment queries are unavailable",
    )?;
    require_selector(
        &buffer_device,
        sel!(minimumTextureBufferAlignmentForPixelFormat:),
        "texture-buffer offset-alignment queries are unavailable",
    )?;
    // SAFETY: both selectors are present and receive a checked Metal pixel-format value.
    let row_alignment: usize = unsafe {
        msg_send![
            &*buffer_device,
            minimumLinearTextureAlignmentForPixelFormat: format
        ]
    };
    // SAFETY: selector availability and argument representation are checked above.
    let offset_alignment: usize = unsafe {
        msg_send![
            &*buffer_device,
            minimumTextureBufferAlignmentForPixelFormat: format
        ]
    };
    if row_alignment == 0
        || offset_alignment == 0
        || !bytes_per_row.is_multiple_of(row_alignment)
        || !offset.is_multiple_of(offset_alignment)
    {
        return Err(Error::invalid_argument(
            "buffer texture offset or row stride violates device alignment",
        ));
    }
    Ok(())
}

impl TextureViewPool {
    /// Copies a texture's default view into one checked pool slot.
    pub fn set_texture_view(&self, texture: &Texture, index: usize) -> Result<ResourceID, Error> {
        checked_slot(self, index)?;
        require_same_device(self, texture.as_any_object(), "texture")?;
        require_selector(
            self.as_inner(),
            sel!(setTextureView:atIndex:),
            "texture-view pool writes are unavailable",
        )?;
        // SAFETY: the selector is present, the texture remains borrowed for
        // the call, both objects belong to the same device, and index is in range.
        let value: MTLResourceID = unsafe {
            msg_send![
                self.as_inner(),
                setTextureView: texture.as_any_object(),
                atIndex: index
            ]
        };
        Ok(resource_id_snapshot(value))
    }

    /// Creates a validated texture view, then copies it into one pool slot.
    ///
    /// This safe substitute deliberately routes descriptor validation through
    /// the existing checked texture-view factory instead of accepting a native
    /// descriptor directly at the final Objective-C call.
    pub fn set_texture_view_with_descriptor(
        &self,
        texture: &Texture,
        descriptor: &TextureViewDescriptor,
        index: usize,
    ) -> Result<ResourceID, Error> {
        checked_slot(self, index)?;
        require_same_device(self, texture.as_any_object(), "texture")?;
        let validated_view = texture.view_with_descriptor(descriptor)?;
        self.set_texture_view(&validated_view, index)
    }

    /// Creates a checked buffer-backed texture view, then copies it into a pool slot.
    ///
    /// The temporary owned texture closes the descriptor, offset, row-stride,
    /// and resource lifetime invariants before the pool sees the view.
    pub fn set_texture_view_from_buffer(
        &self,
        buffer: &Buffer,
        descriptor: &TextureDescriptor,
        offset: usize,
        bytes_per_row: usize,
        index: usize,
    ) -> Result<ResourceID, Error> {
        checked_slot(self, index)?;
        require_same_device(self, buffer.as_any_object(), "buffer")?;
        validate_buffer_view_layout(buffer, descriptor, offset, bytes_per_row)?;
        let validated_view = buffer.new_texture(descriptor, offset, bytes_per_row)?;
        self.set_texture_view(&validated_view, index)
    }
}
