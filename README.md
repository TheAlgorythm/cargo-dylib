# cargo-dylib

A cargo wrapper proof of concept for faster compilation of dependencies as dynamic libraries.
For incremental builds the linking of dynamic libraries is much faster then building a static binary as described in [this](https://robert.kra.hn/posts/2022-09-09-speeding-up-incremental-rust-compilation-with-dylibs/) blog post.
It works a bit like [`cargo-add-dynamic`](https://lib.rs/crates/cargo-add-dynamic), but doesn't alter the `Cargo.toml` in order to have a single source of truth for both fast incremental debug builds and release builds.
It does so by reading the `Cargo.toml`, laying out a shadow project structure and executes the command with a pointer to the shadow metadata.

As this is just a proof of concept it won't generate dylibs recursively yet.
