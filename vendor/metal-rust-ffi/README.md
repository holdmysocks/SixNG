# metal-rust-ffi

Audited Objective-C interoperability boundary used by
[`metal-rust`](https://crates.io/crates/metal-rust).

This package exists separately so unsafe Objective-C message sends and
ownership assumptions have a crate-level boundary. It does not re-export raw
Objective-C objects or pointers, and its public Rust API is safe. Applications
should depend on `metal-rust` rather than using this implementation crate
directly.

The source layout follows Apple's metal-cpp framework and header organization.
