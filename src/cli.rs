//! Diagnostic command-line modes (replays, benches and fixtures). Each mode
//! runs instead of the app and exits with 0 on pass, 1 on failure. The live
//! `--self-test` scenarios run inside the normal app; see `smoke`.
use crate::native::wide;
use std::ffi::{c_char, CString};

struct Args(Vec<String>);
impl Args {
    fn has(&self, flag: &str) -> bool {
        self.0.iter().any(|a| a == flag)
    }
    fn index(&self, flag: &str) -> Option<usize> {
        self.0.iter().position(|a| a == flag)
    }
    /// The value after `flag`, parsed, or `default` when the flag is absent.
    fn value<T: std::str::FromStr>(&self, flag: &str, default: T) -> T {
        match self.index(flag) {
            None => default,
            Some(i) => self
                .0
                .get(i + 1)
                .unwrap_or_else(|| fail(&format!("{flag} needs a value")))
                .parse()
                .unwrap_or_else(|_| fail(&format!("Invalid value for {flag}"))),
        }
    }
    /// Positional parameter `n` after the mode flag at `i`.
    fn at(&self, i: usize, n: usize, usage: &str) -> &str {
        self.0
            .get(i + n)
            .map(String::as_str)
            .unwrap_or_else(|| fail(usage))
    }
    fn number<T: std::str::FromStr>(&self, i: usize, n: usize, usage: &str) -> T {
        self.at(i, n, usage).parse().unwrap_or_else(|_| fail(usage))
    }
}

fn fail(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(2)
}

fn exit(pass: i32) -> ! {
    std::process::exit(if pass != 0 { 0 } else { 1 })
}

fn c_string(value: &str) -> CString {
    CString::new(value).unwrap_or_else(|_| fail("Paths may not contain NUL"))
}

unsafe extern "C" {
    fn spatpit_replay_reshade(
        input: *const u16,
        output: *const u16,
        w: u32,
        h: u32,
        runtime: *const u16,
        preset: *const u16,
    ) -> i32;
    fn spatpit_latency_source(rate: u32, output: *const u16) -> i32;
    fn spatpit_latency_bench(
        output: *const u16,
        runtime: *const u16,
        rate: u32,
        seconds: u32,
        neural_enabled: u32,
        source_rate: u32,
    ) -> i32;
    fn spatpit_test_idle_resize(runtime: *const u16, report: *const u16) -> i32;
    fn spatpit_replay_app(
        input: *const u16,
        output: *const u16,
        w: u32,
        h: u32,
        runtime: *const u16,
        backend: u32,
        style: u32,
        protection: i32,
        dump_motion: i32,
        render_rate: u32,
        model_scale: f32,
        zero_motion: i32,
        motion_input: *const u16,
        passes: u32,
        resize_cycles: u32,
    ) -> i32;
    fn spatpit_replay_motion(
        log: *const c_char,
        input: *const c_char,
        output: *const c_char,
        w: u32,
        h: u32,
        runtime: *const c_char,
        protection: i32,
        zero_motion: i32,
        style: u32,
    ) -> i32;
    fn spatpit_test_motion(output: *const c_char) -> i32;
}

/// Runs a diagnostic mode when one is requested. Returns only when the
/// normal app should start.
pub fn run() {
    let args = Args(std::env::args().collect());

    if let Some(i) = args.index("--replay-reshade") {
        const USAGE: &str =
            "--replay-reshade INPUT.rgba OUTPUT.rgba WIDTH HEIGHT RUNTIME PRESET.ini";
        let input = wide(args.at(i, 1, USAGE));
        let output = wide(args.at(i, 2, USAGE));
        let runtime = wide(args.at(i, 5, USAGE));
        let preset = wide(args.at(i, 6, USAGE));
        exit(unsafe {
            spatpit_replay_reshade(
                input.as_ptr(),
                output.as_ptr(),
                args.number(i, 3, USAGE),
                args.number(i, 4, USAGE),
                runtime.as_ptr(),
                preset.as_ptr(),
            )
        });
    }

    if let Some(i) = args.index("--latency-source") {
        let rate = args.0.get(i + 1).and_then(|v| v.parse().ok()).unwrap_or(60);
        let output = wide(args.0.get(i + 2).map(String::as_str).unwrap_or(""));
        exit(unsafe { spatpit_latency_source(rate, output.as_ptr()) });
    }

    if let Some(i) = args.index("--latency-bench") {
        const USAGE: &str =
            "--latency-bench OUTPUT.csv RUNTIME RATE SECONDS [--neural-off] [--source-rate 60|120]";
        let output = wide(args.at(i, 1, USAGE));
        let runtime = wide(args.at(i, 2, USAGE));
        exit(unsafe {
            spatpit_latency_bench(
                output.as_ptr(),
                runtime.as_ptr(),
                args.number(i, 3, USAGE),
                args.number(i, 4, USAGE),
                u32::from(!args.has("--neural-off")),
                args.value("--source-rate", 60),
            )
        });
    }

    if let Some(i) = args.index("--test-idle-resize") {
        const USAGE: &str = "--test-idle-resize RUNTIME REPORT";
        let runtime = wide(args.at(i, 1, USAGE));
        let report = wide(args.at(i, 2, USAGE));
        exit(unsafe { spatpit_test_idle_resize(runtime.as_ptr(), report.as_ptr()) });
    }

    if let Some(i) = args.index("--replay-app") {
        const USAGE: &str =
            "--replay-app INPUT.rgba OUTPUT.rgba WIDTH HEIGHT RUNTIME BACKEND(0/1) STYLE(0/1/2/3=Balanced)";
        let input = wide(args.at(i, 1, USAGE));
        let output = wide(args.at(i, 2, USAGE));
        let runtime = wide(args.at(i, 5, USAGE));
        let motion_input = wide(&args.value("--motion-input", String::new()));
        exit(unsafe {
            spatpit_replay_app(
                input.as_ptr(),
                output.as_ptr(),
                args.number(i, 3, USAGE),
                args.number(i, 4, USAGE),
                runtime.as_ptr(),
                args.number(i, 6, USAGE),
                args.number(i, 7, USAGE),
                i32::from(!args.has("--unprotected")),
                i32::from(args.has("--dump-motion")),
                args.value("--render-rate", 60),
                args.value("--model-scale", 0.5),
                i32::from(args.has("--zero-motion")),
                motion_input.as_ptr(),
                args.value("--passes", 1),
                args.value("--resize-cycles", 0),
            )
        });
    }

    if let Some(i) = args.index("--replay-motion") {
        const USAGE: &str = "--replay-motion INPUT.rgba OUTPUT.rgba WIDTH HEIGHT RUNTIME_FOLDER";
        let input = c_string(args.at(i, 1, USAGE));
        let output = c_string(args.at(i, 2, USAGE));
        let w = args.number(i, 3, USAGE);
        let h = args.number(i, 4, USAGE);
        let runtime = c_string(args.at(i, 5, USAGE));
        std::fs::create_dir_all("artifacts").unwrap_or_else(|e| fail(&e.to_string()));
        let log = c_string("artifacts/custom-motion-replay.txt");
        let protection = i32::from(!args.has("--unprotected"));
        let zero_motion = i32::from(args.has("--zero-motion"));
        let style: u32 = args.value("--replay-style", 0);
        if style > 2 {
            fail("--replay-style needs 0, 1 or 2");
        }
        if zero_motion != 0 && protection != 0 {
            fail(
                "--zero-motion requires --unprotected so confidence cannot hide the neural output",
            );
        }
        exit(unsafe {
            spatpit_replay_motion(
                log.as_ptr(),
                input.as_ptr(),
                output.as_ptr(),
                w,
                h,
                runtime.as_ptr(),
                protection,
                zero_motion,
                style,
            )
        });
    }

    if args.has("--test-motion") {
        std::fs::create_dir_all("artifacts").unwrap_or_else(|e| fail(&e.to_string()));
        let path = c_string("artifacts/custom-motion-check.txt");
        exit(unsafe { spatpit_test_motion(path.as_ptr()) });
    }

    if args.has("--capture-probe") {
        crate::smoke::capture_probe();
        std::process::exit(0);
    }
    if args.has("--test-window") {
        crate::smoke::fixture();
        std::process::exit(0);
    }
}
