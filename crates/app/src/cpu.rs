//! CPU feature dispatch for the release image (x86-64 only).
//!
//! whisper.cpp (ggml) is compiled ahead of time for one instruction set, and an AVX2
//! build crashes with an illegal instruction on older or low-power CPUs (many NAS
//! Celerons and Atoms have no AVX at all). The Docker image therefore ships two builds of
//! this binary: `akasha` (portable: ggml limited to SSE4.2) and `akasha-avx2` next to it
//! (AVX/AVX2/FMA/F16C/BMI2). [`dispatch`] runs first in `main` of the portable build and,
//! when the CPU has every feature the fast build needs, replaces the process with it
//! (same arguments and environment). Rust code itself always targets baseline x86-64.
//!
//! `AKASHA_CPU_VARIANT=baseline` forces the portable build. Local builds (no sibling
//! `akasha-avx2`) are untouched.

/// Which ggml build this binary is (set by the Docker build; `native` otherwise).
pub fn build_variant() -> &'static str {
    option_env!("AKASHA_BUILD_CPU_VARIANT").unwrap_or("native")
}

/// Environment variable that forces a variant (`auto`, default, or `baseline`).
pub const VARIANT_ENV: &str = "AKASHA_CPU_VARIANT";
#[cfg_attr(not(all(target_arch = "x86_64", unix)), allow(dead_code))]
/// Set on re-exec so the fast build never dispatches again.
const GUARD_ENV: &str = "AKASHA_CPU_DISPATCHED";
#[cfg_attr(not(all(target_arch = "x86_64", unix)), allow(dead_code))]
/// File name suffix of the fast build.
const FAST_SUFFIX: &str = "-avx2";

/// Re-exec the AVX2 build when it exists and this CPU supports it. Returns only when this
/// binary should keep running. Call before starting threads or logging.
pub fn dispatch() {
    #[cfg(all(target_arch = "x86_64", unix))]
    imp::dispatch();
}

/// The features `akasha-avx2` was compiled for, present on this CPU.
#[cfg(target_arch = "x86_64")]
pub fn has_fast_features() -> bool {
    std::arch::is_x86_feature_detected!("avx")
        && std::arch::is_x86_feature_detected!("avx2")
        && std::arch::is_x86_feature_detected!("fma")
        && std::arch::is_x86_feature_detected!("f16c")
        && std::arch::is_x86_feature_detected!("bmi2")
}

#[cfg(all(target_arch = "x86_64", unix))]
mod imp {
    use std::os::unix::process::CommandExt;
    use std::path::PathBuf;

    use super::{FAST_SUFFIX, GUARD_ENV, VARIANT_ENV, build_variant, has_fast_features};

    pub(super) fn dispatch() {
        if build_variant() == "baseline" && !std::arch::is_x86_feature_detected!("sse4.2") {
            eprintln!(
                "warning: this CPU has no SSE4.2; speech-to-text (whisper.cpp) needs it and \
                 transcription will crash. Set AKASHA_WHISPER_MODEL=none."
            );
        }
        if build_variant() != "baseline" || std::env::var_os(GUARD_ENV).is_some() {
            return;
        }
        let forced = std::env::var(VARIANT_ENV).unwrap_or_default();
        if forced.eq_ignore_ascii_case("baseline") || !has_fast_features() {
            return;
        }
        let Some(fast) = fast_path() else { return };
        let mut args = std::env::args_os();
        let argv0 = args.next().unwrap_or_else(|| fast.clone().into_os_string());
        // `exec` only returns on failure; then keep running the portable build.
        let err = std::process::Command::new(&fast)
            .arg0(argv0)
            .args(args)
            .env(GUARD_ENV, "1")
            .exec();
        eprintln!(
            "warning: could not start {} ({err}); using the portable build",
            fast.display()
        );
    }

    /// `<this executable>-avx2`, if it exists.
    fn fast_path() -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let mut name = exe.file_name()?.to_os_string();
        name.push(FAST_SUFFIX);
        let fast = exe.with_file_name(name);
        fast.is_file().then_some(fast)
    }
}
