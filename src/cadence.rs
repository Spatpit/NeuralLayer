use std::time::{Duration, Instant};

pub struct Meter {
    pub draws: u64,
    since: Instant,
    previous: Option<[u64; 3]>,
    rates: Option<[f64; 3]>,
}
impl Default for Meter {
    fn default() -> Self {
        Self {
            draws: 0,
            since: Instant::now(),
            previous: None,
            rates: None,
        }
    }
}
impl Meter {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn due(&self) -> bool {
        self.previous.is_none() || self.since.elapsed() >= Duration::from_secs(1)
    }
    pub fn sample(&mut self, captures: u64, evaluations: u64) {
        let now = Instant::now();
        let counters = [captures, evaluations, self.draws];
        if let Some(previous) = self.previous {
            let seconds = now.duration_since(self.since).as_secs_f64();
            self.rates = Some(std::array::from_fn(|i| {
                counters[i].saturating_sub(previous[i]) as f64 / seconds
            }));
        }
        self.previous = Some(counters);
        self.since = now;
    }
    /// Capture FPS, neural evaluations per second and redraw FPS.
    pub fn rates(&self) -> Option<[f64; 3]> {
        self.rates
    }
    pub fn label(&self) -> String {
        match self.rates {
            Some([capture, neural, redraw]) => format!(
                "Capture {capture:.0} fps · redraw {redraw:.0} fps\nNeural evaluations {neural:.0}/s"
            ),
            None => "Measuring capture and processing rates...".into(),
        }
    }
}

// Opt-in diagnostic: exercises the real winit/egui loop, with its usual pacing.
// Use an isolated test installation: loading a preset can update its settings.
pub struct Probe {
    child: std::process::Child,
    output: std::path::PathBuf,
    neural: bool,
    menu: bool,
    source_hwnd: isize,
    configured: Option<Instant>,
    started: Instant,
    rows: Vec<String>,
}
impl Probe {
    pub fn from_args() -> Option<Self> {
        use std::os::windows::process::CommandExt;
        let args: Vec<_> = std::env::args().collect();
        let i = args.iter().position(|a| a == "--cadence-app")?;
        let output = std::path::PathBuf::from(args.get(i + 1).expect("--cadence-app OUTPUT.csv"));
        assert!(!output.exists(), "Preserve existing cadence traces");
        let child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--latency-source",
                "120",
                &format!("{}.source.csv", output.display()),
            ])
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        Some(Self {
            child,
            output,
            neural: !args.iter().any(|a| a == "--neural-off"),
            menu: args.iter().any(|a| a == "--cadence-menu"),
            source_hwnd: 0,
            configured: None,
            started: Instant::now(),
            rows: Vec::new(),
        })
    }
    pub fn tick(&mut self, app: &mut crate::App) -> bool {
        if self.configured.is_none() {
            assert!(
                self.started.elapsed() < Duration::from_secs(15),
                "Cadence source not ready"
            );
            let target = unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    FindWindowW, GetWindowThreadProcessId,
                };
                let hwnd = FindWindowW(
                    crate::native::wide("SpatpitLatencySource").as_ptr(),
                    std::ptr::null(),
                );
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, &mut pid);
                if hwnd.is_null() || pid != self.child.id() {
                    return false;
                }
                crate::native::Target {
                    hwnd: hwnd as isize,
                    pid,
                    title: "Spatpit latency source".into(),
                }
            };
            if app.graphics.status().reshade_loaded == 0 {
                return false;
            }
            self.source_hwnd = target.hwnd;
            app.fit(target);
            assert!(
                app.target.is_some(),
                "Cadence capture failed: {:?}",
                app.error
            );
            app.nr_options = crate::optiscaler::Options {
                motion_backend: 1,
                ..Default::default()
            };
            app.graphics.nr_configure(&app.nr_options);
            app.load_effects(true);
            assert!(
                app.error.is_none(),
                "Cadence effects failed: {:?}",
                app.error
            );
            app.options.neural = u32::from(self.neural);
            app.nr_options.enabled = u32::from(self.neural);
            app.graphics.nr_configure(&app.nr_options);
            app.frame_limit = 120;
            app.menu_visible = self.menu;
            app.compare_visible = false;
            self.configured = Some(Instant::now());
            return false;
        }
        let elapsed = self.configured.unwrap().elapsed().as_secs_f64();
        if elapsed < 10.0 {
            return false;
        }
        assert!(
            app.error.is_none(),
            "Cadence render failed: {:?}",
            app.error
        );
        let status = app.graphics.status();
        let nr = app.graphics.nr_status();
        assert!(status.capture_active != 0 && status.has_frame != 0);
        if self.neural {
            assert!(nr.active != 0 && nr.width == 1280 && nr.height == 720);
        } else {
            assert_eq!(nr.evaluations, 0);
        }
        self.rows.push(format!(
            "{elapsed:.6},{},{},{}",
            status.frames, nr.evaluations, app.frame_rates.draws
        ));
        if elapsed < 30.0 {
            return false;
        }
        if self.menu {
            let image = app.graphics.screenshot([2560, 1440]).unwrap();
            crate::save_image(&self.output.with_extension("png").to_string_lossy(), &image);
        }
        std::fs::write(
            &self.output,
            format!(
                "seconds,captures,evaluations,draws\n{}\n",
                self.rows.join("\n")
            ),
        )
        .unwrap();
        std::fs::write(
            self.output.with_extension("txt"),
            format!(
                "PASS: full app loop; 120 FPS source and limit; neural={} menu={}; {}\n",
                self.neural,
                self.menu,
                app.frame_rates.label()
            ),
        )
        .unwrap();
        true
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        if self.source_hwnd != 0 {
            unsafe {
                windows_sys::Win32::UI::WindowsAndMessaging::PostMessageW(
                    self.source_hwnd as _,
                    windows_sys::Win32::UI::WindowsAndMessaging::WM_CLOSE,
                    0,
                    0,
                );
            }
        }
        let until = Instant::now() + Duration::from_secs(3);
        while Instant::now() < until {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
