use super::*;

/// A small preset as users download it: a preset INI beside reshade-shaders,
/// with an include file and a saved value the look depends on.
fn write_fixture(root: &std::path::Path) -> std::path::PathBuf {
    let shaders = root.join("reshade-shaders/Shaders");
    std::fs::create_dir_all(shaders.join("Include")).unwrap();
    std::fs::write(
        shaders.join("Include/TintCommon.fxh"),
        "#define TINT_ONE 1.0\n",
    )
    .unwrap();
    std::fs::write(
        shaders.join("Tint.fx"),
        r#"#include "Include/TintCommon.fxh"
texture2D TintSource : COLOR;
sampler2D TintSampler { Texture = TintSource; };
void TintVS(uint id : SV_VertexID, out float4 position : SV_Position, out float2 uv : TEXCOORD) {
    uv = float2((id << 1) & 2, id & 2);
    position = float4(uv * float2(2, -2) + float2(-1, 1), 0, 1);
}
uniform float Strength < ui_type = "slider"; ui_min = 0.0; ui_max = 1.0; > = 0.0;
float4 TintPS(float4 position : SV_Position, float2 uv : TEXCOORD) : SV_Target {
    float3 c = tex2D(TintSampler, uv).rgb;
    return float4(lerp(c, TINT_ONE - c, Strength), 1);
}
technique Tint { pass { VertexShader = TintVS; PixelShader = TintPS; } }
"#,
    )
    .unwrap();
    // Default Strength is 0: only the preset's saved value changes the image.
    let preset = root.join("Fixture Look.ini");
    std::fs::write(
        &preset,
        "Techniques=Tint@Tint.fx\n\n[Tint.fx]\nStrength=1.0\n",
    )
    .unwrap();
    preset
}

impl Scenario {
    /// Switch off every effect except the active look. Loading and importing
    /// reload effects from the saved preset, which can re-enable them. The
    /// preset file itself is never changed. True when nothing needed changing.
    fn only_looks(app: &mut App) -> bool {
        let mut clean = true;
        for t in app.graphics.techniques() {
            if t.enabled && !crate::looks::owned(&t.effect) {
                let _ = app.graphics.set_technique(&t.effect, &t.name, false);
                clean = false;
            }
        }
        clean
    }

    /// True once the active look (or Off) has been ready, with no other effect
    /// running, for a few rendered frames.
    fn look_settled(&mut self, app: &mut App) -> bool {
        if !Self::only_looks(app) || app.graphics.status().reshade_loaded == 0 {
            self.look_ready_at = None;
            return false;
        }
        if !app.looks.ready {
            self.look_ready_at = None;
            return false;
        }
        let since = *self.look_ready_at.get_or_insert_with(Instant::now);
        if since.elapsed() < Duration::from_millis(500) {
            return false;
        }
        self.look_ready_at = None;
        true
    }

    pub(super) fn tick_looks(&mut self, app: &mut App, ctx: &egui::Context) {
        assert!(app.error.is_none(), "{:?}", app.error);
        assert!(app.looks.error.is_none(), "{:?}", app.looks.error);
        let screenshot = |app: &mut App| {
            let b = native::surface_bounds(app.hwnd).unwrap();
            app.graphics
                .screenshot([b.width as u32, b.height as u32])
                .unwrap()
        };
        let sample = |image: &egui::ColorImage| {
            image.pixels[image.width() * (image.height() / 2) + image.width() / 6].to_array()
        };
        let changed = |a: [u8; 4], b: [u8; 4]| {
            a[..3]
                .iter()
                .zip(b[..3].iter())
                .map(|(x, y)| x.abs_diff(*y) as u32)
                .sum::<u32>()
                > 60
        };
        match self.step {
            230 => {
                app.menu_visible = false;
                if app.graphics.status().reshade_loaded == 0 {
                    app.load_effects(false);
                }
                // Pixel checks compare against the plain capture; see only_looks.
                app.options.effect = 0;
                app.options.reshade = 1;
                app.looks.set(None, &app.runtime_folder).unwrap();
                // Start from no imported looks (an interrupted run may leave some).
                for id in app
                    .looks
                    .list
                    .iter()
                    .map(|l| l.id.clone())
                    .collect::<Vec<_>>()
                {
                    app.looks.remove(&id, &app.runtime_folder).unwrap();
                }
                self.advance();
            }
            231 => {
                if !self.look_settled(app) {
                    return;
                }
                self.motion_pixel = sample(&screenshot(app));
                let fixture = std::path::Path::new("artifacts/look-fixture");
                let _ = std::fs::remove_dir_all(fixture);
                let preset = write_fixture(fixture);
                let result = crate::looks::import(&preset, &app.runtime_folder).unwrap();
                assert_eq!(result.names, ["Fixture Look"]);
                app.reload_looks(Some(&result.ids[0]));
                assert!(app.looks.selected.is_some());
                self.advance();
            }
            232 => {
                if !self.look_settled(app) {
                    return;
                }
                let image = screenshot(app);
                let pixel = sample(&image);
                crate::save_image("artifacts/look-on.png", &image);
                assert_eq!(pixel[3], 255, "A look must leave the output opaque");
                assert!(
                    changed(pixel, self.motion_pixel),
                    "The imported look and its saved value must change the image: {pixel:?} vs {:?}",
                    self.motion_pixel
                );
                app.looks.set(None, &app.runtime_folder).unwrap();
                self.advance();
            }
            233 => {
                if !self.look_settled(app) {
                    return;
                }
                assert_eq!(
                    sample(&screenshot(app)),
                    self.motion_pixel,
                    "Off must restore the original pixels"
                );
                // The same preset as a ZIP download.
                let zip = std::path::Path::new("artifacts/look-fixture.zip");
                let _ = std::fs::remove_file(zip);
                let status = std::process::Command::new("tar")
                    .args(["-a", "-cf"])
                    .arg(zip)
                    .args([
                        "-C",
                        "artifacts/look-fixture",
                        "Fixture Look.ini",
                        "reshade-shaders",
                    ])
                    .status()
                    .unwrap();
                assert!(status.success());
                let result = crate::looks::import(zip, &app.runtime_folder).unwrap();
                assert_eq!(app.looks.list.len(), 1);
                app.reload_looks(Some(&result.ids[0]));
                assert_eq!(app.looks.list.len(), 2, "Each import is its own look");
                self.advance();
            }
            234 => {
                if !self.look_settled(app) {
                    return;
                }
                assert!(changed(sample(&screenshot(app)), self.motion_pixel));
                // Removing the active look must also turn it off.
                let id = app.looks.selected.clone().unwrap();
                app.looks.remove(&id, &app.runtime_folder).unwrap();
                assert_eq!(app.looks.selected, None);
                self.advance();
            }
            235 => {
                if !self.look_settled(app) {
                    return;
                }
                assert_eq!(sample(&screenshot(app)), self.motion_pixel);
                for look in app
                    .looks
                    .list
                    .iter()
                    .map(|l| l.id.clone())
                    .collect::<Vec<_>>()
                {
                    app.looks.remove(&look, &app.runtime_folder).unwrap();
                }
                std::fs::write(
                    "artifacts/looks-check.txt",
                    "PASS: imported a preset from INI + reshade-shaders and from a ZIP; include file resolved; saved preset value applied; image changed and stayed opaque; Off and removing the active look restore exact original pixels.\n",
                )
                .unwrap();
                let args: Vec<String> = std::env::args().collect();
                match args.iter().position(|a| a == "--look-source") {
                    // Optionally also import a real download given on the command line.
                    Some(i) => {
                        let source = std::path::PathBuf::from(&args[i + 1]);
                        let result = crate::looks::import(&source, &app.runtime_folder).unwrap();
                        std::fs::write(
                            "artifacts/looks-real-import.txt",
                            format!(
                                "Imported {:?}; skipped missing: {:?}
",
                                result.names, result.missing
                            ),
                        )
                        .unwrap();
                        app.reload_looks(Some(&result.ids[0]));
                        self.advance();
                    }
                    None => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        self.step = 99;
                    }
                }
            }
            236 => {
                if !self.look_settled(app) {
                    return;
                }
                let image = screenshot(app);
                crate::save_image("artifacts/look-real.png", &image);
                let pixel = sample(&image);
                assert_eq!(pixel[3], 255, "A real look must leave the output opaque");
                assert!(
                    pixel[..3] != self.motion_pixel[..3],
                    "The real look must change the image"
                );
                let id = app.looks.selected.clone().unwrap();
                app.looks.remove(&id, &app.runtime_folder).unwrap();
                std::fs::write(
                    "artifacts/looks-real-check.txt",
                    format!("PASS: real preset compiled and applied; sample {pixel:?} vs original {:?}; removed.
", self.motion_pixel),
                )
                .unwrap();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                self.step = 99;
            }
            _ => unreachable!(),
        }
    }
}
