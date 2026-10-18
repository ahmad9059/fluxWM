# Source attribution

The initial compositor integration under `crates/fluxwm/src` is adapted from Smithay Smallvil at commit `023b78b1c5a05ca512767cacc07efca55fae8be4`. Its MIT copyright and license notice are preserved in [Smithay-MIT](LICENSES/Smithay-MIT.txt). FluxWM policy/config/layout/IPC code is developed separately. No niri/Hyprland source is copied. Dependency licenses require continued review as the workspace grows.

The optional native platform layer under `crates/flux-native` also uses the pinned Smithay session/DRM/libinput APIs and example ownership patterns; the same retained MIT notice applies. Its optional native dependency graph adds libseat, libudev, libinput, GBM/EGL/GLES and DRM bindings. The locked dependency graph is build evidence, not a completed distribution-license inventory; transitive Rust and linked-system-library licensing remains a release gate.
