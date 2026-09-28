// No console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Debug builds run audio callbacks under `assert_no_alloc`: any allocation on
// an audio thread aborts (hard rule).
#[cfg(debug_assertions)]
#[global_allocator]
static ALLOC: assert_no_alloc::AllocDisabler = assert_no_alloc::AllocDisabler;

fn main() {
    std::process::exit(voice_tuner_lib::run());
}
