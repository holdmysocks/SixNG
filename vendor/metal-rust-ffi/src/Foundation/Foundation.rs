#[path = "NSBundle.rs"]
mod ns_bundle;
#[path = "NSError.rs"]
mod ns_error;
#[path = "NSNotification.rs"]
mod ns_notification;
#[path = "NSProcessInfo.rs"]
mod ns_process_info;
#[path = "NSProcessInfoConstants.rs"]
mod ns_process_info_constants;
#[path = "NSURL.rs"]
mod ns_url;

pub use ns_bundle::Bundle;
pub use ns_error::Error;
pub(crate) use ns_error::metal_error;
pub use ns_notification::{Notification, NotificationCenter, Registration};
pub use ns_process_info::ProcessInfo;
pub use ns_process_info_constants::{
    device_certification_iphone_performance_gaming, process_performance_profile_default,
    process_performance_profile_sustained,
};
pub use ns_url::URL;
