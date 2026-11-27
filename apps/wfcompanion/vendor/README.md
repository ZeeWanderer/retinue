# Vendored Native Sources

The shared renderer owns the [Blend2D and AsmJit submodules](../../../crates/retinue-render/vendor).
The gitlinks pin the versions. Its [native build](../../../crates/retinue-render/native/CMakeLists.txt)
uses the AsmJit checkout and links the C bridge against static Blend2D.
Each submodule includes its own license.
