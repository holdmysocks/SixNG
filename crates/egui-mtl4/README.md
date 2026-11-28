# egui-mtl4

Experimental `egui` painter using Apple's Metal 4 command API through
`metal-rust`.

This crate is GPL-3.0-only, matching the SixNG source from which it was
extracted. It is a workspace crate today; consumers can use it by path or
copy the package into a compatible workspace. The Metal-Rust dependency is
pinned to the `maple-quartz18/Metal-Rust` fork revision used by SixNG.

The caller owns the window, `CAMetalLayer`, drawable presentation, command
queue, and the residency set containing any external textures. The painter
owns its egui textures, argument table, pipeline, and three in-flight upload
slots. Do not drop or evict external textures before submitted work completes.

The current implementation supports textured egui meshes but not custom paint
callbacks. Each frame slot has a 4 MiB vertex buffer; larger frames return an
error. The API and rendering path are experimental and require macOS 26 and a
Metal 4-capable GPU.
