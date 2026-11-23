//! Audited capture-manager, descriptor, and scope boundary.

use crate::ThreadBound;
use crate::foundation::{Error, metal_error};
use crate::metal::generated_object_types::metal::{CaptureDescriptor, CaptureScope};
use crate::metal::generated_value_types::CaptureDestination;
use crate::metal::{CommandQueue, Device, Metal4CommandQueue};
use objc2::msg_send;
use objc2::rc::Retained;
use objc2::runtime::{AnyObject, AnyProtocol, NSObjectProtocol, ProtocolObject, Sel};
use objc2::sel;
use objc2_foundation::{NSString, NSURL};
use objc2_metal::{
    MTLCaptureDescriptor, MTLCaptureDestination, MTLCaptureManager, MTLCaptureScope,
};
use std::path::{Path, PathBuf};

fn shared_manager() -> Result<Retained<MTLCaptureManager>, Error> {
    let class = objc2::runtime::AnyClass::get(c"MTLCaptureManager")
        .ok_or_else(|| Error::unsupported("MTLCaptureManager is unavailable"))?;
    // SAFETY: Objective-C classes implement respondsToSelector:, and this
    // selector has the standard BOOL return ABI.
    let available: bool =
        unsafe { msg_send![class, respondsToSelector: sel!(sharedCaptureManager)] };
    if !available {
        return Err(Error::unsupported(
            "MTLCaptureManager::sharedCaptureManager is unavailable",
        ));
    }
    // SAFETY: The runtime class and exact class selector were checked above.
    // Apple defines the result as its retained process singleton.
    Ok(unsafe { MTLCaptureManager::sharedCaptureManager() })
}

fn require_selector(manager: &MTLCaptureManager, selector: Sel, name: &str) -> Result<(), Error> {
    if manager.respondsToSelector(selector) {
        Ok(())
    } else {
        Err(Error::unsupported(format!(
            "MTLCaptureManager::{name} is unavailable"
        )))
    }
}

fn conforms_to(object: &AnyObject, protocol: &AnyProtocol) -> bool {
    // SAFETY: NSObject-compatible Metal objects implement conformsToProtocol:,
    // and both arguments and BOOL return use Objective-C runtime ABI types.
    unsafe { msg_send![object, conformsToProtocol: protocol] }
}

fn native_destination(destination: CaptureDestination) -> Result<MTLCaptureDestination, Error> {
    match destination.as_raw() {
        1 => Ok(MTLCaptureDestination::DeveloperTools),
        2 => Ok(MTLCaptureDestination::GPUTraceDocument),
        _ => Err(Error::invalid_argument(
            "capture destination is not a declared Metal value",
        )),
    }
}

fn scope_protocol(scope: &CaptureScope) -> &ProtocolObject<dyn MTLCaptureScope> {
    // SAFETY: CaptureScope has no public raw constructor. Every instance is
    // produced by MTLCaptureManager and therefore conforms to MTLCaptureScope.
    unsafe { &*(std::ptr::from_ref(scope.as_inner()).cast()) }
}

fn mtl4_queue_protocol(
    queue: &Metal4CommandQueue,
) -> &ProtocolObject<dyn objc2_metal::MTL4CommandQueue> {
    // SAFETY: Metal4CommandQueue is only constructed from newMTL4CommandQueue,
    // and its generated shell retains that exact MTL4CommandQueue object.
    unsafe { &*(std::ptr::from_ref(queue.as_generated().as_inner()).cast()) }
}

fn owned_scope(inner: Retained<ProtocolObject<dyn MTLCaptureScope>>) -> CaptureScope {
    // SAFETY: Erasing a retained protocol-qualified object to AnyObject keeps
    // the identical object pointer and ownership count.
    CaptureScope::from_inner(unsafe { Retained::cast_unchecked(inner) })
}

fn descriptor_ref(descriptor: &CaptureDescriptor) -> &MTLCaptureDescriptor {
    // SAFETY: CaptureDescriptor is only constructed by the generated checked
    // MTLCaptureDescriptor constructor; the object pointer has this class.
    unsafe { &*(std::ptr::from_ref(descriptor.as_inner()).cast()) }
}

fn new_descriptor() -> Result<Retained<MTLCaptureDescriptor>, Error> {
    let class = objc2::runtime::AnyClass::get(c"MTLCaptureDescriptor")
        .ok_or_else(|| Error::unsupported("MTLCaptureDescriptor is unavailable"))?;
    // SAFETY: Objective-C classes implement respondsToSelector:, and this
    // selector has the standard BOOL return ABI.
    let available: bool = unsafe { msg_send![class, respondsToSelector: sel!(new)] };
    if !available {
        return Err(Error::unsupported(
            "MTLCaptureDescriptor::new is unavailable",
        ));
    }
    Ok(MTLCaptureDescriptor::new())
}

fn configure_output(
    descriptor: &MTLCaptureDescriptor,
    destination: CaptureDestination,
    output_path: Option<&str>,
) -> Result<(), Error> {
    let destination = native_destination(destination)?;
    if !descriptor.respondsToSelector(sel!(setDestination:)) {
        return Err(Error::unsupported(
            "MTL::CaptureDescriptor::setDestination is unavailable",
        ));
    }
    match (destination, output_path) {
        (MTLCaptureDestination::DeveloperTools, None) => {}
        (MTLCaptureDestination::DeveloperTools, Some(_)) => {
            return Err(Error::invalid_argument(
                "Developer Tools capture does not accept an output path",
            ));
        }
        (MTLCaptureDestination::GPUTraceDocument, Some(path)) => {
            if !descriptor.respondsToSelector(sel!(setOutputURL:)) {
                return Err(Error::unsupported(
                    "MTL::CaptureDescriptor::setOutputURL is unavailable",
                ));
            }
            if path.as_bytes().contains(&0) {
                return Err(Error::invalid_argument("capture output path contains NUL"));
            }
            if !Path::new(path).is_absolute() {
                return Err(Error::invalid_argument(
                    "capture output path must be absolute",
                ));
            }
            let path = NSString::from_str(path);
            descriptor.setOutputURL(Some(&NSURL::fileURLWithPath(&path)));
        }
        (MTLCaptureDestination::GPUTraceDocument, None) => {
            return Err(Error::invalid_argument(
                "GPU trace document capture requires an output path",
            ));
        }
        _ => unreachable!("capture destination was validated above"),
    }
    descriptor.setDestination(destination);
    Ok(())
}

impl CaptureDescriptor {
    fn native_capture_object(&self) -> Result<Option<Retained<AnyObject>>, Error> {
        let descriptor = descriptor_ref(self);
        if !descriptor.respondsToSelector(sel!(captureObject)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::captureObject is unavailable",
            ));
        }
        Ok(descriptor.captureObject())
    }

    /// Returns the typed device target, when the capture object is a device.
    pub fn capture_device(&self) -> Result<Option<Device>, Error> {
        let Some(object) = self.native_capture_object()? else {
            return Ok(None);
        };
        let protocol = AnyProtocol::get(c"MTLDevice")
            .ok_or_else(|| Error::unsupported("MTLDevice protocol is unavailable"))?;
        if !conforms_to(&object, protocol) {
            return Ok(None);
        }
        // SAFETY: Runtime protocol conformance was checked above; the retained
        // object and ownership are unchanged by the protocol-qualified cast.
        let device = unsafe { Retained::cast_unchecked(object) };
        Ok(Some(Device::from_inner(device)))
    }

    /// Returns the typed queue target, when the capture object is a queue.
    pub fn capture_command_queue(&self) -> Result<Option<CommandQueue>, Error> {
        let Some(object) = self.native_capture_object()? else {
            return Ok(None);
        };
        let protocol = AnyProtocol::get(c"MTLCommandQueue")
            .ok_or_else(|| Error::unsupported("MTLCommandQueue protocol is unavailable"))?;
        if !conforms_to(&object, protocol) {
            return Ok(None);
        }
        // SAFETY: Runtime protocol conformance was checked above; the retained
        // object and ownership are unchanged by the protocol-qualified cast.
        let queue = unsafe { Retained::cast_unchecked(object) };
        Ok(Some(CommandQueue::new(queue)))
    }

    /// Returns the typed scope target, when the capture object is a scope.
    pub fn capture_scope(&self) -> Result<Option<CaptureScope>, Error> {
        let Some(object) = self.native_capture_object()? else {
            return Ok(None);
        };
        let protocol = AnyProtocol::get(c"MTLCaptureScope")
            .ok_or_else(|| Error::unsupported("MTLCaptureScope protocol is unavailable"))?;
        if !conforms_to(&object, protocol) {
            return Ok(None);
        }
        // SAFETY: Runtime protocol conformance was checked above; the retained
        // object and ownership are unchanged by the protocol-qualified cast.
        let scope = unsafe { Retained::cast_unchecked(object) };
        Ok(Some(owned_scope(scope)))
    }

    /// Returns whether the descriptor contains any capture object.
    pub fn has_capture_object(&self) -> Result<bool, Error> {
        Ok(self.native_capture_object()?.is_some())
    }

    /// Rejects capture object classes outside Metal's documented target set.
    pub fn validate_capture_object_type(&self) -> Result<(), Error> {
        if !self.has_capture_object()?
            || self.capture_device()?.is_some()
            || self.capture_command_queue()?.is_some()
            || self.capture_scope()?.is_some()
        {
            Ok(())
        } else {
            Err(Error::unsupported(
                "capture descriptor contains an unsupported target type",
            ))
        }
    }

    /// Selects a device as the descriptor's capture target.
    pub fn set_capture_device(&self, device: &Device) -> Result<(), Error> {
        let descriptor = descriptor_ref(self);
        if !descriptor.respondsToSelector(sel!(setCaptureObject:)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::setCaptureObject is unavailable",
            ));
        }
        descriptor.set_capture_device(&device.inner);
        Ok(())
    }

    /// Selects a command queue as the descriptor's capture target.
    pub fn set_capture_command_queue(&self, queue: &CommandQueue) -> Result<(), Error> {
        let descriptor = descriptor_ref(self);
        if !descriptor.respondsToSelector(sel!(setCaptureObject:)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::setCaptureObject is unavailable",
            ));
        }
        descriptor.set_capture_command_queue(&queue.inner);
        Ok(())
    }

    /// Selects a capture scope as the descriptor's capture target.
    pub fn set_capture_scope(&self, scope: &CaptureScope) -> Result<(), Error> {
        let descriptor = descriptor_ref(self);
        if !descriptor.respondsToSelector(sel!(setCaptureObject:)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::setCaptureObject is unavailable",
            ));
        }
        descriptor.set_capture_scope(scope_protocol(scope));
        Ok(())
    }

    /// Sets a validated destination and optional absolute output path.
    pub fn set_capture_output(
        &self,
        destination: CaptureDestination,
        output_path: Option<&str>,
    ) -> Result<(), Error> {
        let descriptor = descriptor_ref(self);
        configure_output(descriptor, destination, output_path)
    }

    /// Returns the output file path, when the descriptor contains a file URL.
    pub fn output_path(&self) -> Result<Option<PathBuf>, Error> {
        let descriptor = descriptor_ref(self);
        if !descriptor.respondsToSelector(sel!(outputURL)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::outputURL is unavailable",
            ));
        }
        Ok(descriptor
            .outputURL()
            .and_then(|url| url.path())
            .map(|path| PathBuf::from(path.to_string())))
    }
}

impl CaptureScope {
    fn begin_scope(&self) -> Result<(), Error> {
        let object = self.as_inner();
        // SAFETY: Every Objective-C object implements respondsToSelector:.
        let has_begin: bool = unsafe { msg_send![object, respondsToSelector: sel!(beginScope)] };
        // SAFETY: Every Objective-C object implements respondsToSelector:.
        let has_end: bool = unsafe { msg_send![object, respondsToSelector: sel!(endScope)] };
        if !has_begin || !has_end {
            return Err(Error::unsupported(
                "MTL::CaptureScope begin/end selectors are unavailable",
            ));
        }
        // SAFETY: The selector was checked and CaptureScope objects conform to
        // MTLCaptureScope, whose beginScope method has a void return ABI.
        unsafe {
            let _: () = msg_send![object, beginScope];
        }
        Ok(())
    }

    fn end_scope(&self) {
        // SAFETY: begin_scope verifies endScope before returning success, and
        // the RAII facade only calls this for such a successful begin.
        unsafe { msg_send![self.as_inner(), endScope] }
    }

    /// Runs an operation inside a balanced capture scope.
    ///
    /// The scope is ended during normal return and panic unwinding.
    pub fn with_scope<T>(&self, operation: impl FnOnce() -> T) -> Result<T, Error> {
        self.begin_scope()?;
        let _guard = ScopeEndGuard { scope: self };
        Ok(operation())
    }

    /// Returns the device associated with this scope.
    pub fn device(&self) -> Result<Device, Error> {
        let object = self.as_inner();
        // SAFETY: Every Objective-C object implements respondsToSelector:.
        let available: bool = unsafe { msg_send![object, respondsToSelector: sel!(device)] };
        if !available {
            return Err(Error::unsupported(
                "MTL::CaptureScope::device is unavailable",
            ));
        }
        // SAFETY: MTLCaptureScope declares device with this retained protocol
        // object return type, and the selector was checked above.
        let device = unsafe { msg_send![object, device] };
        Ok(Device::from_inner(device))
    }

    /// Returns the queue associated with this scope, if it is queue-specific.
    pub fn command_queue(&self) -> Result<Option<CommandQueue>, Error> {
        let object = self.as_inner();
        // SAFETY: Every Objective-C object implements respondsToSelector:.
        let available: bool = unsafe { msg_send![object, respondsToSelector: sel!(commandQueue)] };
        if !available {
            return Err(Error::unsupported(
                "MTL::CaptureScope::commandQueue is unavailable",
            ));
        }
        // SAFETY: MTLCaptureScope declares commandQueue with this nullable,
        // retained protocol-object return type, and the selector was checked.
        let queue: Option<Retained<ProtocolObject<dyn objc2_metal::MTLCommandQueue>>> =
            unsafe { msg_send![object, commandQueue] };
        Ok(queue.map(CommandQueue::new))
    }
}

struct ScopeEndGuard<'scope> {
    scope: &'scope CaptureScope,
}

impl Drop for ScopeEndGuard<'_> {
    fn drop(&mut self) {
        self.scope.end_scope();
    }
}

/// An active Metal capture session.
pub struct CaptureSession {
    manager: Retained<MTLCaptureManager>,
    active: bool,
    _thread_bound: ThreadBound,
}

impl CaptureSession {
    /// Returns whether the runtime supports a capture destination.
    pub fn supports_destination(destination: CaptureDestination) -> Result<bool, Error> {
        let manager = shared_manager()?;
        require_selector(&manager, sel!(supportsDestination:), "supportsDestination")?;
        Ok(manager.supportsDestination(native_destination(destination)?))
    }

    /// Returns whether a process-wide capture is active.
    pub fn is_capturing() -> Result<bool, Error> {
        let manager = shared_manager()?;
        require_selector(&manager, sel!(isCapturing), "isCapturing")?;
        Ok(manager.isCapturing())
    }

    /// Creates a capture scope for every queue belonging to a device.
    pub fn new_scope_for_device(device: &Device) -> Result<CaptureScope, Error> {
        let manager = shared_manager()?;
        require_selector(
            &manager,
            sel!(newCaptureScopeWithDevice:),
            "newCaptureScopeWithDevice",
        )?;
        Ok(owned_scope(
            manager.newCaptureScopeWithDevice(&device.inner),
        ))
    }

    /// Creates a capture scope for one command queue.
    pub fn new_scope_for_command_queue(queue: &CommandQueue) -> Result<CaptureScope, Error> {
        let manager = shared_manager()?;
        require_selector(
            &manager,
            sel!(newCaptureScopeWithCommandQueue:),
            "newCaptureScopeWithCommandQueue",
        )?;
        Ok(owned_scope(
            manager.newCaptureScopeWithCommandQueue(&queue.inner),
        ))
    }

    /// Creates a capture scope for one Metal 4 command queue.
    pub fn new_scope_for_mtl4_command_queue(
        queue: &Metal4CommandQueue,
    ) -> Result<CaptureScope, Error> {
        let manager = shared_manager()?;
        require_selector(
            &manager,
            sel!(newCaptureScopeWithMTL4CommandQueue:),
            "newCaptureScopeWithMTL4CommandQueue",
        )?;
        Ok(owned_scope(manager.newCaptureScopeWithMTL4CommandQueue(
            mtl4_queue_protocol(queue),
        )))
    }

    /// Returns the process default capture scope.
    pub fn default_scope() -> Result<Option<CaptureScope>, Error> {
        let manager = shared_manager()?;
        require_selector(&manager, sel!(defaultCaptureScope), "defaultCaptureScope")?;
        Ok(manager.defaultCaptureScope().map(owned_scope))
    }

    /// Sets or clears the process default capture scope.
    pub fn set_default_scope(scope: Option<&CaptureScope>) -> Result<(), Error> {
        let manager = shared_manager()?;
        require_selector(
            &manager,
            sel!(setDefaultCaptureScope:),
            "setDefaultCaptureScope",
        )?;
        manager.setDefaultCaptureScope(scope.map(scope_protocol));
        Ok(())
    }

    /// Starts capture using a fully configured safe descriptor.
    pub fn start_descriptor(descriptor: &CaptureDescriptor) -> Result<Self, Error> {
        let manager = shared_manager()?;
        require_selector(
            &manager,
            sel!(startCaptureWithDescriptor:error:),
            "startCaptureWithDescriptor:error:",
        )?;
        require_selector(&manager, sel!(stopCapture), "stopCapture")?;
        manager
            .startCaptureWithDescriptor_error(descriptor_ref(descriptor))
            .map_err(|error| metal_error(&error))?;
        Ok(Self {
            manager,
            active: true,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Starts device-wide GPU trace document capture at an absolute path.
    pub fn start(device: &Device, output_path: &str) -> Result<Self, Error> {
        Self::start_for_device(
            device,
            CaptureDestination::CaptureDestinationGPUTraceDocument,
            Some(output_path),
        )
    }

    /// Starts a device capture with a validated destination configuration.
    pub fn start_for_device(
        device: &Device,
        destination: CaptureDestination,
        output_path: Option<&str>,
    ) -> Result<Self, Error> {
        let descriptor = new_descriptor()?;
        if !descriptor.respondsToSelector(sel!(setCaptureObject:)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::setCaptureObject is unavailable",
            ));
        }
        descriptor.set_capture_device(&device.inner);
        configure_output(&descriptor, destination, output_path)?;
        Self::start_native_descriptor(&descriptor)
    }

    /// Starts a command-queue capture with a validated destination.
    pub fn start_for_command_queue(
        queue: &CommandQueue,
        destination: CaptureDestination,
        output_path: Option<&str>,
    ) -> Result<Self, Error> {
        let descriptor = new_descriptor()?;
        if !descriptor.respondsToSelector(sel!(setCaptureObject:)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::setCaptureObject is unavailable",
            ));
        }
        descriptor.set_capture_command_queue(&queue.inner);
        configure_output(&descriptor, destination, output_path)?;
        Self::start_native_descriptor(&descriptor)
    }

    /// Starts a capture-scope capture with a validated destination.
    pub fn start_for_scope(
        scope: &CaptureScope,
        destination: CaptureDestination,
        output_path: Option<&str>,
    ) -> Result<Self, Error> {
        let descriptor = new_descriptor()?;
        if !descriptor.respondsToSelector(sel!(setCaptureObject:)) {
            return Err(Error::unsupported(
                "MTL::CaptureDescriptor::setCaptureObject is unavailable",
            ));
        }
        descriptor.set_capture_scope(scope_protocol(scope));
        configure_output(&descriptor, destination, output_path)?;
        Self::start_native_descriptor(&descriptor)
    }

    fn start_native_descriptor(descriptor: &MTLCaptureDescriptor) -> Result<Self, Error> {
        let manager = shared_manager()?;
        require_selector(
            &manager,
            sel!(startCaptureWithDescriptor:error:),
            "startCaptureWithDescriptor:error:",
        )?;
        require_selector(&manager, sel!(stopCapture), "stopCapture")?;
        manager
            .startCaptureWithDescriptor_error(descriptor)
            .map_err(|error| metal_error(&error))?;
        Ok(Self {
            manager,
            active: true,
            _thread_bound: ThreadBound::new(),
        })
    }

    /// Stops capture and consumes the session.
    pub fn finish(mut self) {
        self.stop();
    }

    fn stop(&mut self) {
        if self.active {
            self.manager.stopCapture();
            self.active = false;
        }
    }
}

impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.stop();
    }
}
