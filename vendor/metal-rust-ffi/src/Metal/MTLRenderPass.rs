//! Audited render-pass descriptor construction.

use crate::ThreadBound;
use crate::foundation::Error;
use crate::metal::generated_struct_types::SamplePosition;
use crate::metal::generated_value_types::{DispatchType, VisibilityResultType};
use crate::metal::{Buffer, ClearColor, Texture};
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObjectProtocol};
use objc2::{msg_send, sel};
use objc2_metal::{
    MTLClearColor, MTLDispatchType, MTLLoadAction, MTLRenderPassDescriptor, MTLResource,
    MTLSamplePosition, MTLStoreAction, MTLVisibilityResultType,
};

use crate::metal::generated_object_types::metal::{
    ComputePassSampleBufferAttachmentDescriptor, RasterizationRateMap,
    RenderPassColorAttachmentDescriptor, RenderPassDepthAttachmentDescriptor,
    RenderPassSampleBufferAttachmentDescriptor, RenderPassStencilAttachmentDescriptor,
};

const MAX_COLOR_ATTACHMENTS: usize = 8;
const MAX_COUNTER_SAMPLE_ATTACHMENTS: usize = 4;

fn validate_attachment_index(index: usize, limit: usize, kind: &str) -> Result<(), Error> {
    if index >= limit {
        return Err(Error::invalid_argument(format!(
            "{kind} attachment index {index} exceeds Metal's limit of {limit}"
        )));
    }
    Ok(())
}

fn require_selector(
    object: &AnyObject,
    selector: objc2::runtime::Sel,
    operation: &str,
) -> Result<(), Error> {
    // SAFETY: Every Objective-C object implements respondsToSelector:, and
    // Sel/bool use the runtime's declared ABI encodings.
    let available: bool = unsafe { msg_send![object, respondsToSelector: selector] };
    if !available {
        return Err(Error::unsupported(format!("{operation} is unavailable")));
    }
    Ok(())
}

/// Copyable scalar configuration for a render pass.
#[derive(Clone, Copy, Debug)]
pub struct RenderPassOptions {
    /// Default raster sample count used when no attachment defines one.
    pub default_raster_sample_count: usize,
    /// Imageblock storage required for each sample.
    pub imageblock_sample_length: usize,
    /// Number of render-target array layers.
    pub render_target_array_length: usize,
    /// Explicit render-target height, or zero for attachment-derived sizing.
    pub render_target_height: usize,
    /// Explicit render-target width, or zero for attachment-derived sizing.
    pub render_target_width: usize,
    /// Enables programmable color-attachment mapping.
    pub support_color_attachment_mapping: bool,
    /// Tile shader threadgroup-memory length.
    pub threadgroup_memory_length: usize,
    /// Explicit tile height, or zero for Metal's default.
    pub tile_height: usize,
    /// Explicit tile width, or zero for Metal's default.
    pub tile_width: usize,
    /// Visibility-result accumulation behavior.
    pub visibility_result_type: VisibilityResultType,
}

/// A render-pass descriptor whose attachment indices are checked before FFI.
pub struct RenderPassDescriptor {
    pub(super) inner: Retained<MTLRenderPassDescriptor>,
    _thread_bound: ThreadBound,
}

impl RenderPassDescriptor {
    /// Creates an empty render-pass descriptor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: MTLRenderPassDescriptor::renderPassDescriptor(),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Sets a color attachment's texture and clear color.
    pub fn set_color_attachment(
        &self,
        index: usize,
        texture: &Texture,
        clear_color: ClearColor,
    ) -> Result<(), Error> {
        validate_attachment_index(index, MAX_COLOR_ATTACHMENTS, "color")?;
        if [
            clear_color.red,
            clear_color.green,
            clear_color.blue,
            clear_color.alpha,
        ]
        .iter()
        .any(|component| !component.is_finite())
        {
            return Err(Error::invalid_argument(
                "clear-color components must be finite",
            ));
        }
        let attachments = self.inner.colorAttachments();
        // SAFETY: Metal's color attachment array has eight entries and index is
        // checked against that fixed API limit.
        let attachment = unsafe { attachments.objectAtIndexedSubscript(index) };
        attachment.setTexture(Some(&texture.inner));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearColor(MTLClearColor {
            red: clear_color.red,
            green: clear_color.green,
            blue: clear_color.blue,
            alpha: clear_color.alpha,
        });
        Ok(())
    }

    /// Returns a color attachment descriptor by checked attachment index.
    pub fn color_attachment(
        &self,
        index: usize,
    ) -> Result<Option<RenderPassColorAttachmentDescriptor>, Error> {
        validate_attachment_index(index, MAX_COLOR_ATTACHMENTS, "color")?;
        require_selector(
            &self.inner,
            sel!(colorAttachments),
            "render-pass color attachments",
        )?;
        // SAFETY: Both selectors have been checked or are required by the
        // indexed attachment-array contract, and the index is within Metal's
        // fixed eight-color-attachment range. The retained result is moved
        // into an owned opaque wrapper.
        let value = unsafe {
            let attachments: Retained<AnyObject> = msg_send![&self.inner, colorAttachments];
            let value: Option<Retained<AnyObject>> =
                msg_send![&*attachments, objectAtIndexedSubscript: index];
            value
        };
        Ok(value.map(RenderPassColorAttachmentDescriptor::from_inner))
    }

    /// Sets or clears a color attachment descriptor at a checked index.
    pub fn set_color_attachment_descriptor(
        &self,
        index: usize,
        value: Option<&RenderPassColorAttachmentDescriptor>,
    ) -> Result<(), Error> {
        validate_attachment_index(index, MAX_COLOR_ATTACHMENTS, "color")?;
        require_selector(
            &self.inner,
            sel!(colorAttachments),
            "render-pass color attachments",
        )?;
        // SAFETY: `colorAttachments` returns the descriptor-owned attachment
        // array, the subscript selector is the documented mutation API, the
        // index is checked, and the optional wrapper preserves object identity.
        unsafe {
            let attachments: Retained<AnyObject> = msg_send![&self.inner, colorAttachments];
            let _: () = msg_send![
                &*attachments,
                setObject: value.map(RenderPassColorAttachmentDescriptor::as_inner),
                atIndexedSubscript: index
            ];
        }
        Ok(())
    }

    /// Returns a color attachment's clear color by checked index.
    pub fn color_attachment_clear_color(&self, index: usize) -> Result<ClearColor, Error> {
        let attachment = self
            .color_attachment(index)?
            .ok_or_else(|| Error::invalid_argument("color attachment is not configured"))?;
        require_selector(
            attachment.as_inner(),
            sel!(clearColor),
            "render-pass color attachment clear color",
        )?;
        // SAFETY: The selector is present and returns the documented
        // MTLClearColor value ABI.
        let value: MTLClearColor = unsafe { msg_send![attachment.as_inner(), clearColor] };
        Ok(ClearColor {
            red: value.red,
            green: value.green,
            blue: value.blue,
            alpha: value.alpha,
        })
    }

    /// Updates a color attachment's clear color by checked index.
    pub fn set_color_attachment_clear_color(
        &self,
        index: usize,
        value: ClearColor,
    ) -> Result<(), Error> {
        if [value.red, value.green, value.blue, value.alpha]
            .iter()
            .any(|component| !component.is_finite())
        {
            return Err(Error::invalid_argument(
                "clear-color components must be finite",
            ));
        }
        let attachment = self
            .color_attachment(index)?
            .ok_or_else(|| Error::invalid_argument("color attachment is not configured"))?;
        require_selector(
            attachment.as_inner(),
            sel!(setClearColor:),
            "render-pass color attachment clear color setter",
        )?;
        let value = MTLClearColor {
            red: value.red,
            green: value.green,
            blue: value.blue,
            alpha: value.alpha,
        };
        // SAFETY: The selector is present and the finite value has the exact
        // MTLClearColor ABI declared by Metal.
        unsafe {
            let _: () = msg_send![attachment.as_inner(), setClearColor: value];
        }
        Ok(())
    }

    /// Returns the optional depth attachment descriptor.
    pub fn depth_attachment(&self) -> Result<Option<RenderPassDepthAttachmentDescriptor>, Error> {
        require_selector(
            &self.inner,
            sel!(depthAttachment),
            "render-pass depth attachment",
        )?;
        // SAFETY: The selector is present and its retained nullable result is
        // transferred into an owned class-identity wrapper.
        let value: Option<Retained<AnyObject>> = unsafe { msg_send![&self.inner, depthAttachment] };
        Ok(value.map(RenderPassDepthAttachmentDescriptor::from_inner))
    }

    /// Sets or clears the depth attachment descriptor.
    pub fn set_depth_attachment(
        &self,
        value: Option<&RenderPassDepthAttachmentDescriptor>,
    ) -> Result<(), Error> {
        require_selector(
            &self.inner,
            sel!(setDepthAttachment:),
            "render-pass depth attachment setter",
        )?;
        // SAFETY: The selector is present and the optional wrapper preserves
        // the Objective-C class identity required by Metal.
        unsafe {
            let _: () = msg_send![
                &self.inner,
                setDepthAttachment: value.map(RenderPassDepthAttachmentDescriptor::as_inner)
            ];
        }
        Ok(())
    }

    /// Returns the optional stencil attachment descriptor.
    pub fn stencil_attachment(
        &self,
    ) -> Result<Option<RenderPassStencilAttachmentDescriptor>, Error> {
        require_selector(
            &self.inner,
            sel!(stencilAttachment),
            "render-pass stencil attachment",
        )?;
        // SAFETY: The selector is present and its retained nullable result is
        // transferred into an owned class-identity wrapper.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![&self.inner, stencilAttachment] };
        Ok(value.map(RenderPassStencilAttachmentDescriptor::from_inner))
    }

    /// Sets or clears the stencil attachment descriptor.
    pub fn set_stencil_attachment(
        &self,
        value: Option<&RenderPassStencilAttachmentDescriptor>,
    ) -> Result<(), Error> {
        require_selector(
            &self.inner,
            sel!(setStencilAttachment:),
            "render-pass stencil attachment setter",
        )?;
        // SAFETY: The selector is present and the optional wrapper preserves
        // the Objective-C class identity required by Metal.
        unsafe {
            let _: () = msg_send![
                &self.inner,
                setStencilAttachment: value.map(RenderPassStencilAttachmentDescriptor::as_inner)
            ];
        }
        Ok(())
    }

    /// Returns the optional rasterization-rate map.
    pub fn rasterization_rate_map(&self) -> Result<Option<RasterizationRateMap>, Error> {
        require_selector(
            &self.inner,
            sel!(rasterizationRateMap),
            "render-pass rasterization-rate map",
        )?;
        // SAFETY: The selector is present and its retained nullable result is
        // transferred into an owned class-identity wrapper.
        let value: Option<Retained<AnyObject>> =
            unsafe { msg_send![&self.inner, rasterizationRateMap] };
        Ok(value.map(RasterizationRateMap::from_inner))
    }

    /// Sets or clears the rasterization-rate map.
    pub fn set_rasterization_rate_map(
        &self,
        value: Option<&RasterizationRateMap>,
    ) -> Result<(), Error> {
        require_selector(
            &self.inner,
            sel!(setRasterizationRateMap:),
            "render-pass rasterization-rate map setter",
        )?;
        // SAFETY: The selector is present and the optional wrapper preserves
        // the Objective-C class identity required by Metal.
        unsafe {
            let _: () = msg_send![
                &self.inner,
                setRasterizationRateMap: value.map(RasterizationRateMap::as_inner)
            ];
        }
        Ok(())
    }

    /// Returns a counter sample-buffer attachment by checked index.
    pub fn sample_buffer_attachment(
        &self,
        index: usize,
    ) -> Result<Option<RenderPassSampleBufferAttachmentDescriptor>, Error> {
        validate_attachment_index(
            index,
            MAX_COUNTER_SAMPLE_ATTACHMENTS,
            "counter sample-buffer",
        )?;
        require_selector(
            &self.inner,
            sel!(sampleBufferAttachments),
            "render-pass sample-buffer attachments",
        )?;
        // SAFETY: The property selector is present, the subscript operation is
        // Metal's documented attachment-array API, and the index is checked.
        let value = unsafe {
            let attachments: Retained<AnyObject> = msg_send![&self.inner, sampleBufferAttachments];
            let value: Option<Retained<AnyObject>> =
                msg_send![&*attachments, objectAtIndexedSubscript: index];
            value
        };
        Ok(value.map(RenderPassSampleBufferAttachmentDescriptor::from_inner))
    }

    /// Sets or clears a counter sample-buffer attachment at a checked index.
    pub fn set_sample_buffer_attachment(
        &self,
        index: usize,
        value: Option<&RenderPassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        validate_attachment_index(
            index,
            MAX_COUNTER_SAMPLE_ATTACHMENTS,
            "counter sample-buffer",
        )?;
        require_selector(
            &self.inner,
            sel!(sampleBufferAttachments),
            "render-pass sample-buffer attachments",
        )?;
        // SAFETY: The array is owned by the descriptor, the subscript selector
        // is documented, the index is checked, and the wrapper preserves class
        // identity for a non-null attachment.
        unsafe {
            let attachments: Retained<AnyObject> = msg_send![&self.inner, sampleBufferAttachments];
            let _: () = msg_send![
                &*attachments,
                setObject: value.map(RenderPassSampleBufferAttachmentDescriptor::as_inner),
                atIndexedSubscript: index
            ];
        }
        Ok(())
    }

    /// Returns the scalar render-pass configuration after availability checks.
    pub fn options(&self) -> Result<RenderPassOptions, Error> {
        for (selector, property) in [
            (
                sel!(defaultRasterSampleCount),
                "default raster sample count",
            ),
            (sel!(imageblockSampleLength), "imageblock sample length"),
            (sel!(renderTargetArrayLength), "render-target array length"),
            (sel!(renderTargetHeight), "render-target height"),
            (sel!(renderTargetWidth), "render-target width"),
            (
                sel!(supportColorAttachmentMapping),
                "color attachment mapping",
            ),
            (sel!(threadgroupMemoryLength), "threadgroup memory length"),
            (sel!(tileHeight), "tile height"),
            (sel!(tileWidth), "tile width"),
            (sel!(visibilityResultType), "visibility result type"),
        ] {
            if !self.inner.respondsToSelector(selector) {
                return Err(Error::unsupported(format!(
                    "render-pass {property} is unavailable"
                )));
            }
        }
        Ok(RenderPassOptions {
            default_raster_sample_count: self.inner.defaultRasterSampleCount(),
            imageblock_sample_length: self.inner.imageblockSampleLength(),
            render_target_array_length: self.inner.renderTargetArrayLength(),
            render_target_height: self.inner.renderTargetHeight(),
            render_target_width: self.inner.renderTargetWidth(),
            support_color_attachment_mapping: self.inner.supportColorAttachmentMapping(),
            threadgroup_memory_length: self.inner.threadgroupMemoryLength(),
            tile_height: self.inner.tileHeight(),
            tile_width: self.inner.tileWidth(),
            visibility_result_type: VisibilityResultType::from_system_raw(
                self.inner.visibilityResultType().0,
            ),
        })
    }

    /// Applies scalar render-pass configuration without partial mutation.
    pub fn set_options(&self, value: &RenderPassOptions) -> Result<(), Error> {
        if value.default_raster_sample_count == 0 {
            return Err(Error::invalid_argument(
                "default raster sample count must be non-zero",
            ));
        }
        for (selector, property) in [
            (
                sel!(setDefaultRasterSampleCount:),
                "default raster sample count",
            ),
            (sel!(setImageblockSampleLength:), "imageblock sample length"),
            (
                sel!(setRenderTargetArrayLength:),
                "render-target array length",
            ),
            (sel!(setRenderTargetHeight:), "render-target height"),
            (sel!(setRenderTargetWidth:), "render-target width"),
            (
                sel!(setSupportColorAttachmentMapping:),
                "color attachment mapping",
            ),
            (
                sel!(setThreadgroupMemoryLength:),
                "threadgroup memory length",
            ),
            (sel!(setTileHeight:), "tile height"),
            (sel!(setTileWidth:), "tile width"),
            (sel!(setVisibilityResultType:), "visibility result type"),
        ] {
            if !self.inner.respondsToSelector(selector) {
                return Err(Error::unsupported(format!(
                    "render-pass {property} is unavailable"
                )));
            }
        }
        self.inner
            .setDefaultRasterSampleCount(value.default_raster_sample_count);
        self.inner
            .setImageblockSampleLength(value.imageblock_sample_length);
        self.inner
            .setRenderTargetArrayLength(value.render_target_array_length);
        self.inner.setRenderTargetHeight(value.render_target_height);
        self.inner.setRenderTargetWidth(value.render_target_width);
        self.inner
            .setSupportColorAttachmentMapping(value.support_color_attachment_mapping);
        self.inner
            .setThreadgroupMemoryLength(value.threadgroup_memory_length);
        self.inner.setTileHeight(value.tile_height);
        self.inner.setTileWidth(value.tile_width);
        self.inner.setVisibilityResultType(MTLVisibilityResultType(
            value.visibility_result_type.as_raw(),
        ));
        Ok(())
    }

    /// Returns up to `count` programmable sample positions as owned values.
    pub fn sample_positions(&self, count: usize) -> Result<Vec<SamplePosition>, Error> {
        if count > 32 {
            return Err(Error::invalid_argument(
                "Metal supports at most 32 programmable sample positions",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(getSamplePositions:count:))
        {
            return Err(Error::unsupported(
                "programmable sample positions are unavailable",
            ));
        }
        let mut positions = vec![MTLSamplePosition { x: 0.0, y: 0.0 }; count];
        // SAFETY: `positions` owns writable storage for exactly `count`
        // MTLSamplePosition values for the duration of the synchronous call.
        let written = unsafe {
            self.inner
                .getSamplePositions_count(positions.as_mut_ptr(), count)
        };
        if written > count {
            return Err(Error::unsupported(
                "Metal returned more sample positions than requested",
            ));
        }
        positions.truncate(written);
        Ok(positions
            .into_iter()
            .map(|position| SamplePosition {
                x: position.x,
                y: position.y,
            })
            .collect())
    }

    /// Sets programmable sample positions from an owned Rust slice.
    pub fn set_sample_positions(&self, positions: &[SamplePosition]) -> Result<(), Error> {
        if positions.len() > 32
            || positions
                .iter()
                .any(|position| !position.x.is_finite() || !position.y.is_finite())
        {
            return Err(Error::invalid_argument(
                "sample positions must be finite and contain at most 32 entries",
            ));
        }
        if !self
            .inner
            .respondsToSelector(sel!(setSamplePositions:count:))
        {
            return Err(Error::unsupported(
                "programmable sample positions are unavailable",
            ));
        }
        let positions: Vec<_> = positions
            .iter()
            .map(|position| MTLSamplePosition {
                x: position.x,
                y: position.y,
            })
            .collect();
        // SAFETY: the pointer references `positions.len()` initialized values
        // and Metal copies them synchronously according to the API contract.
        unsafe {
            self.inner
                .setSamplePositions_count(positions.as_ptr(), positions.len())
        };
        Ok(())
    }

    /// Returns the optional visibility-result buffer.
    pub fn visibility_result_buffer(&self) -> Result<Option<Buffer>, Error> {
        self.inner
            .visibilityResultBuffer()
            .map(|inner| {
                let storage = crate::metal::StorageMode::try_from_system_raw(inner.storageMode().0)
                    .ok_or_else(|| Error::unsupported("Metal returned an unknown storage mode"))?;
                Ok(Buffer::new(inner, storage))
            })
            .transpose()
    }

    /// Sets or clears the visibility-result buffer.
    pub fn set_visibility_result_buffer(&self, value: Option<&Buffer>) {
        self.inner
            .setVisibilityResultBuffer(value.map(|buffer| &*buffer.inner));
    }
}

impl Default for RenderPassDescriptor {
    fn default() -> Self {
        Self::new()
    }
}

/// A compute-pass descriptor.
pub struct ComputePassDescriptor {
    pub(super) inner: Retained<objc2_metal::MTLComputePassDescriptor>,
    _thread_bound: ThreadBound,
}

impl ComputePassDescriptor {
    /// Creates a default compute-pass descriptor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: objc2_metal::MTLComputePassDescriptor::computePassDescriptor(),
            _thread_bound: ThreadBound::new(),
        }
    }

    /// Returns the dispatch scheduling type after availability checks.
    pub fn dispatch_type(&self) -> Result<DispatchType, Error> {
        require_selector(
            &self.inner,
            sel!(dispatchType),
            "compute-pass dispatch type",
        )?;
        let value = DispatchType::from_system_raw(self.inner.dispatchType().0);
        if !value.is_valid() {
            return Err(Error::unsupported(
                "Metal returned an unknown compute-pass dispatch type",
            ));
        }
        Ok(value)
    }

    /// Sets the dispatch scheduling type after value and availability checks.
    pub fn set_dispatch_type(&self, value: DispatchType) -> Result<(), Error> {
        if !value.is_valid() {
            return Err(Error::invalid_argument(
                "compute-pass dispatch type is not a declared Metal value",
            ));
        }
        require_selector(
            &self.inner,
            sel!(setDispatchType:),
            "compute-pass dispatch type setter",
        )?;
        self.inner.setDispatchType(MTLDispatchType(value.as_raw()));
        Ok(())
    }

    /// Returns a counter sample-buffer attachment by checked index.
    pub fn sample_buffer_attachment(
        &self,
        index: usize,
    ) -> Result<Option<ComputePassSampleBufferAttachmentDescriptor>, Error> {
        validate_attachment_index(
            index,
            MAX_COUNTER_SAMPLE_ATTACHMENTS,
            "counter sample-buffer",
        )?;
        require_selector(
            &self.inner,
            sel!(sampleBufferAttachments),
            "compute-pass sample-buffer attachments",
        )?;
        // SAFETY: The property selector is present, the indexed subscript is
        // Metal's documented attachment-array API, and the index is checked.
        let value = unsafe {
            let attachments: Retained<AnyObject> = msg_send![&self.inner, sampleBufferAttachments];
            let value: Option<Retained<AnyObject>> =
                msg_send![&*attachments, objectAtIndexedSubscript: index];
            value
        };
        Ok(value.map(ComputePassSampleBufferAttachmentDescriptor::from_inner))
    }

    /// Sets or clears a counter sample-buffer attachment at a checked index.
    pub fn set_sample_buffer_attachment(
        &self,
        index: usize,
        value: Option<&ComputePassSampleBufferAttachmentDescriptor>,
    ) -> Result<(), Error> {
        validate_attachment_index(
            index,
            MAX_COUNTER_SAMPLE_ATTACHMENTS,
            "counter sample-buffer",
        )?;
        require_selector(
            &self.inner,
            sel!(sampleBufferAttachments),
            "compute-pass sample-buffer attachments",
        )?;
        // SAFETY: The array is descriptor-owned, the indexed subscript is the
        // documented mutation API, the index is checked, and the optional
        // wrapper preserves the expected Objective-C class identity.
        unsafe {
            let attachments: Retained<AnyObject> = msg_send![&self.inner, sampleBufferAttachments];
            let _: () = msg_send![
                &*attachments,
                setObject: value.map(ComputePassSampleBufferAttachmentDescriptor::as_inner),
                atIndexedSubscript: index
            ];
        }
        Ok(())
    }
}

impl Default for ComputePassDescriptor {
    fn default() -> Self {
        Self::new()
    }
}
