//! Safe Foundation value types used by the Metal facade.

#[path = "NSBundle.rs"]
mod ns_bundle;
#[path = "NSCollections.rs"]
mod ns_collections;
#[path = "NSConstants.rs"]
mod ns_constants;
#[path = "NSError.rs"]
mod ns_error;
#[path = "NSNotification.rs"]
mod ns_notification;
#[path = "NSProcessInfo.rs"]
mod ns_process_info;
#[path = "NSURL.rs"]
mod ns_url;
#[path = "NSValueTypes.rs"]
mod ns_value_types;

pub use crate::generated_facade::foundation::*;
pub use ns_bundle::Bundle;
pub use ns_collections::{
    Array, Condition, Copying, Data, Date, Dictionary, Enumerator, Locking, Object, SecureCoding,
    Set, String, StringExt, Value, append_string, date_with_time_interval_since_now, enumerate,
    hash_value, is_equal,
};
pub use ns_constants::*;
pub use ns_error::{Error, ErrorKind};
pub use ns_notification::{Notification, NotificationCenter, Registration};
pub use ns_process_info::ProcessInfo;
pub use ns_url::URL;
pub use ns_value_types::{
    DeviceCertification, ErrorDomain, ErrorUserInfoKey, NotificationName, Number,
    ProcessPerformanceProfile,
};
