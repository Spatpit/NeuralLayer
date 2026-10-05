//! Visual looks: ReShade presets the user imports. One look runs at a time.
//!
//! Importing copies a preset's referenced effects (renamed with a
//! `SpatpitLook_<id>_` prefix so looks never collide), their include files and
//! textures into the runtime folder, and stores a rewritten preset under
//! `looks/<id>.ini`. Nothing outside the app's own folder is modified.
use crate::graphics::Graphics;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const PREFIX: &str = "SpatpitLook_";

pub fn owned(effect: &str) -> bool {
    effect.starts_with(PREFIX)
}

fn looks_dir(runtime: &Path) -> PathBuf {
    runtime.join("looks")
}
fn shader_dir(runtime: &Path, id: &str) -> PathBuf {
    runtime.join("shaders").join("looks").join(id)
}
fn texture_dir(runtime: &Path, id: &str) -> PathBuf {
    runtime.join("textures").join("looks").join(id)
}

pub struct Look {
    pub id: String,
    pub name: String,
    /// Every referenced effect file is present.
    pub available: bool,
    techniques: Vec<(String, String)>, // effect file, technique name, in preset order
    values: HashMap<String, HashMap<String, [f32; 4]>>,
}
impl Look {
    pub fn effect_count(&self) -> usize {
        self.techniques.len()
    }

    fn load(runtime: &Path, id: &str) -> Option<Self> {
        let text = std::fs::read_to_string(looks_dir(runtime).join(format!("{id}.ini"))).ok()?;
        let mut name = id.to_string();
        let prefix = format!("{PREFIX}{id}_");
        let mut techniques = Vec::new();
        let mut values: HashMap<String, HashMap<String, [f32; 4]>> = HashMap::new();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.trim().trim_start_matches('\u{feff}');
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_string();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let (key, value) = (key.trim(), value.trim());
            if section.is_empty() {
                match key {
                    "Name" if !value.is_empty() => name = value.to_string(),
                    "Techniques" => {
                        techniques = value
                            .split(',')
                            .filter_map(|t| t.trim().split_once('@'))
                            .filter(|(n, e)| {
                                !n.is_empty()
                                    && e.starts_with(&prefix)
                                    && e.ends_with(".fx")
                                    && !e.contains(['/', '\\'])
                            })
                            .map(|(n, e)| (e.to_string(), n.to_string()))
                            .collect()
                    }
                    _ => {}
                }
                continue;
            }
            let parts: Result<Vec<f32>, _> = value.split(',').map(|v| v.trim().parse()).collect();
            if let Ok(parts) = parts {
                if !parts.is_empty() && parts.len() <= 4 && parts.iter().all(|v| v.is_finite()) {
                    let mut v = [0.; 4];
                    v[..parts.len()].copy_from_slice(&parts);
                    values
                        .entry(section.clone())
                        .or_default()
                        .insert(key.into(), v);
                }
            }
        }
        let present = files_below(&shader_dir(runtime, id));
        let available = !techniques.is_empty()
            && techniques.iter().all(|(effect, _)| {
                present
                    .iter()
                    .any(|p| file_name(p).eq_ignore_ascii_case(effect))
            });
        Some(Self {
            id: id.to_string(),
            name,
            available,
            techniques,
            values,
        })
    }
}

/// The imported looks and the active one.
pub struct Looks {
    pub list: Vec<Look>,
    /// Id of the active look; `None` is Off.
    pub selected: Option<String>,
    pub ready: bool,
    pub error: Option<String>,
    generation: u64,
    reload_at: Option<Instant>,
    started: Instant,
}
impl Looks {
    pub fn load(folder: &Path) -> Self {
        let runtime = folder.join(crate::optiscaler::runtime());
        let mut list: Vec<Look> = std::fs::read_dir(looks_dir(&runtime))
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                (path.extension()? == "ini")
                    .then(|| path.file_stem()?.to_str().map(String::from))?
            })
            .filter_map(|id| Look::load(&runtime, &id))
            .collect();
        list.sort_by_key(|l| l.name.to_lowercase());
        let prefs = std::fs::read_to_string(folder.join("SpatpitEffects.ini")).unwrap_or_default();
        let selected = prefs
            .lines()
            .find_map(|l| l.trim().strip_prefix("preset="))
            .map(str::trim)
            .filter(|id| list.iter().any(|l| l.id == *id && l.available))
            .map(String::from);
        Self {
            list,
            selected,
            ready: false,
            error: None,
            generation: u64::MAX,
            reload_at: None,
            started: Instant::now(),
        }
    }

    pub fn active(&self) -> Option<&Look> {
        let id = self.selected.as_deref()?;
        self.list.iter().find(|l| l.id == id)
    }

    /// Choose a look (`None` for Off) and remember it.
    pub fn set(&mut self, id: Option<&str>, folder: &Path) -> Result<(), String> {
        if let Some(id) = id {
            match self.list.iter().find(|l| l.id == id) {
                Some(look) if look.available => {}
                Some(look) => {
                    return Err(format!(
                        "{} is missing some of its shader files.",
                        look.name
                    ))
                }
                None => return Err("That look is no longer installed.".into()),
            }
        }
        std::fs::write(
            folder.join("SpatpitEffects.ini"),
            format!("preset={}\n", id.unwrap_or("off")),
        )
        .map_err(|e| format!("Could not save the look preference: {e}"))?;
        self.selected = id.map(String::from);
        self.ready = false;
        self.error = None;
        self.started = Instant::now();
        self.reload_at = None;
        Ok(())
    }

    /// Delete an imported look and its copied files.
    pub fn remove(&mut self, id: &str, folder: &Path) -> Result<(), String> {
        if self.selected.as_deref() == Some(id) {
            self.set(None, folder)?;
        }
        let runtime = folder.join(crate::optiscaler::runtime());
        for dir in [shader_dir(&runtime, id), texture_dir(&runtime, id)] {
            if dir.exists() {
                std::fs::remove_dir_all(&dir)
                    .map_err(|e| format!("Could not remove {}: {e}", dir.display()))?;
            }
        }
        let ini = looks_dir(&runtime).join(format!("{id}.ini"));
        if ini.exists() {
            std::fs::remove_file(&ini).map_err(|e| format!("Could not remove the look: {e}"))?;
        }
        self.list.retain(|l| l.id != id);
        Ok(())
    }

    pub fn tick(&mut self, graphics: &mut Graphics) -> Result<(), String> {
        match self.tick_inner(graphics) {
            Err(_)
                if graphics.effect_reloading()
                    && self.started.elapsed() < Duration::from_secs(30) =>
            {
                self.ready = false;
                graphics.clear_effect_loading_error();
                Ok(())
            }
            result => result,
        }
    }

    fn tick_inner(&mut self, graphics: &mut Graphics) -> Result<(), String> {
        if graphics.status().reshade_loaded == 0 {
            return Ok(());
        }
        let generation = graphics.effect_generation();
        if generation != self.generation {
            if self.ready {
                self.started = Instant::now();
            }
            self.generation = generation;
            self.ready = false;
        }
        if self.ready {
            return Ok(());
        }
        // Close the native allow-list before touching any shaders. Also prevents
        // saved ReShade states or the advanced panel from enabling another look.
        graphics.color_preset(&[])?;
        let techniques = graphics.techniques();
        let mut changed = false;
        for t in techniques.iter().filter(|t| owned(&t.effect) && t.enabled) {
            graphics.set_technique(&t.effect, &t.name, false)?;
            changed = true;
        }
        if changed {
            graphics.effects_changed();
        }
        let Some(look) = self.active() else {
            self.ready = true;
            return Ok(());
        };
        let loaded = look.techniques.iter().all(|(effect, name)| {
            techniques
                .iter()
                .any(|t| &t.effect == effect && &t.name == name)
        });
        let label = look.name.clone();
        if !loaded && self.error.is_none() {
            if self
                .reload_at
                .is_none_or(|t| t.elapsed() > Duration::from_secs(3))
            {
                // Newly imported files are unknown to ReShade until it rescans.
                if look
                    .techniques
                    .iter()
                    .all(|(effect, _)| !techniques.iter().any(|t| &t.effect == effect))
                {
                    graphics.reload_all_effects();
                } else {
                    for (effect, _) in &look.techniques {
                        if !techniques.iter().any(|t| &t.effect == effect) {
                            graphics.reload_effect(effect)?;
                        }
                    }
                }
                self.reload_at = Some(Instant::now());
            }
            if self.started.elapsed() > Duration::from_secs(30) {
                self.error = Some(format!(
                    "{label} did not finish loading. Its shaders may not compile here; select Off, then retry."
                ));
            }
        }
        let look = self.active().expect("checked above");
        if loaded && self.error.is_none() {
            for (effect, _) in &look.techniques {
                if let Some(values) = look.values.get(effect) {
                    for uniform in graphics.uniforms(effect) {
                        if let Some(value) = values.get(&uniform.name) {
                            graphics.set_uniform(effect, &uniform.name, value, false)?;
                        }
                    }
                }
            }
            graphics.color_preset(&look.techniques)?;
            for (effect, name) in &look.techniques {
                graphics.set_technique(effect, name, true)?;
            }
            graphics.effects_changed();
            // Compiled techniques can exist before ReShade has created their
            // GPU resources. Do not report ready until enable actually sticks.
            let applied = graphics.techniques();
            self.ready = look.techniques.iter().all(|(effect, name)| {
                applied
                    .iter()
                    .any(|t| &t.effect == effect && &t.name == name && t.enabled)
            });
            if !self.ready && self.started.elapsed() > Duration::from_secs(30) {
                self.error = Some(format!(
                    "{label} could not be activated. Select Off, then retry."
                ));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- import --

pub struct Imported {
    pub ids: Vec<String>,
    pub names: Vec<String>,
    /// Effects a preset referenced that were not found in the download.
    pub missing: Vec<String>,
}

fn file_name(path: &Path) -> &str {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
}

/// Every file below `dir`, bounded so a mistaken pick (e.g. a whole drive)
/// cannot stall the app.
fn files_below(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if depth < 8 {
                    stack.push((path, depth + 1));
                }
            } else {
                out.push(path);
                if out.len() > 20_000 {
                    return out;
                }
            }
        }
    }
    out
}

fn is_named(path: &Path, name: &str) -> bool {
    file_name(path).eq_ignore_ascii_case(name)
}

/// The preset's `Techniques=` entries as (technique, effect file).
fn techniques_of(text: &str) -> Vec<(String, String)> {
    text.lines()
        .find_map(|l| {
            l.trim()
                .trim_start_matches('\u{feff}')
                .strip_prefix("Techniques=")
        })
        .unwrap_or_default()
        .split(',')
        .filter_map(|t| t.trim().split_once('@'))
        .filter(|(n, e)| !n.is_empty() && e.to_ascii_lowercase().ends_with(".fx"))
        .map(|(n, e)| (n.trim().to_string(), e.trim().to_string()))
        .collect()
}

fn slug(name: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut base: String = name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .take(24)
        .collect();
    if base.is_empty() {
        base = "Look".into();
    }
    let mut id = base.clone();
    let mut n = 2;
    while taken(&id) {
        id = format!("{base}{n}");
        n += 1;
    }
    id
}

/// Directories holding shaders: every `Shaders` folder, else the root itself.
fn shader_roots(root: &Path, files: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots: Vec<PathBuf> = files
        .iter()
        .filter_map(|f| {
            f.ancestors()
                .skip(1)
                .take_while(|a| a.starts_with(root))
                .find(|a| is_named(a, "Shaders"))
                .map(Path::to_path_buf)
        })
        .collect();
    roots.sort();
    roots.dedup();
    if roots.is_empty() {
        roots.push(root.to_path_buf());
    }
    roots
}

fn copy_file(from: &Path, to: &Path) -> Result<(), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("Could not copy {}: {e}", from.display()))
}

/// Import every preset found in `source`: a `.zip`, or a preset `.ini` whose
/// `reshade-shaders` folder sits beside it (or a few levels up).
pub fn import(source: &Path, folder: &Path) -> Result<Imported, String> {
    let runtime = folder.join(crate::optiscaler::runtime());
    let is_zip = source
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"));
    let staging = runtime
        .join("looks")
        .join(format!(".import-{}", std::process::id()));
    let result = (|| {
        let (root, presets) = if is_zip {
            let _ = std::fs::remove_dir_all(&staging);
            std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
            // Windows 10+ ships bsdtar, which reads ZIP archives.
            let status = std::process::Command::new("tar")
                .arg("-xf")
                .arg(source)
                .arg("-C")
                .arg(&staging)
                .status()
                .map_err(|e| format!("Could not open the ZIP: {e}"))?;
            if !status.success() {
                return Err("Could not extract the ZIP file.".to_string());
            }
            let files = files_below(&staging);
            let presets: Vec<PathBuf> = files
                .iter()
                .filter(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")))
                .filter(|f| !is_named(f, "ReShade.ini"))
                .filter(|f| {
                    std::fs::read_to_string(f)
                        .map(|t| !techniques_of(&t).is_empty())
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            (staging.clone(), presets)
        } else {
            let parent = source.parent().ok_or("Invalid preset location")?;
            // The shaders usually sit beside the preset, or in the folder above.
            let root = parent
                .ancestors()
                .take(3)
                .find(|dir| dir.join("reshade-shaders").is_dir() || dir.join("Shaders").is_dir())
                .unwrap_or(parent)
                .to_path_buf();
            (root, vec![source.to_path_buf()])
        };
        if presets.is_empty() {
            return Err("No ReShade preset (.ini with a Techniques= line) was found.".into());
        }
        let files = files_below(&root);
        let fx: Vec<&PathBuf> = files
            .iter()
            .filter(|f| f.extension().is_some_and(|e| e.eq_ignore_ascii_case("fx")))
            .collect();
        let roots = shader_roots(&root, &fx.iter().map(|p| (*p).clone()).collect::<Vec<_>>());
        let texture_roots: Vec<PathBuf> = files
            .iter()
            .filter_map(|f| {
                f.ancestors()
                    .skip(1)
                    .take_while(|a| a.starts_with(&root))
                    .find(|a| is_named(a, "Textures"))
                    .map(Path::to_path_buf)
            })
            .fold(Vec::new(), |mut v, d| {
                if !v.contains(&d) {
                    v.push(d);
                }
                v
            });
        let mut imported = Imported {
            ids: Vec::new(),
            names: Vec::new(),
            missing: Vec::new(),
        };
        std::fs::create_dir_all(looks_dir(&runtime)).map_err(|e| e.to_string())?;
        for preset in presets {
            let text = std::fs::read_to_string(&preset)
                .map_err(|e| format!("Could not read {}: {e}", preset.display()))?;
            let name = preset
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("Look")
                .to_string();
            let id = slug(&name, |id| {
                looks_dir(&runtime).join(format!("{id}.ini")).exists()
                    || imported.ids.iter().any(|i| i == id)
            });
            let prefix = format!("{PREFIX}{id}_");
            let mut kept = Vec::new();
            let mut renamed = HashMap::new();
            let target = shader_dir(&runtime, &id);
            for (technique, effect) in techniques_of(&text) {
                let found = fx.iter().find(|f| is_named(f, &effect));
                let Some(found) = found else {
                    if !imported.missing.contains(&effect) {
                        imported.missing.push(effect);
                    }
                    continue;
                };
                let new_name = format!("{prefix}{}", file_name(found));
                if let std::collections::hash_map::Entry::Vacant(slot) =
                    renamed.entry(effect.to_ascii_lowercase())
                {
                    let base = roots
                        .iter()
                        .find(|r| found.starts_with(r))
                        .cloned()
                        .unwrap_or_else(|| root.clone());
                    let relative = found
                        .parent()
                        .and_then(|p| p.strip_prefix(&base).ok())
                        .unwrap_or(Path::new(""));
                    copy_file(found, &target.join(relative).join(&new_name))?;
                    slot.insert(new_name.clone());
                }
                kept.push((technique, new_name));
            }
            if kept.is_empty() {
                continue;
            }
            // Include files and other helpers keep their layout so relative
            // #include lines still resolve. Unused effects are not copied.
            for base in &roots {
                for file in files.iter().filter(|f| f.starts_with(base)) {
                    if file
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("fx"))
                    {
                        continue;
                    }
                    let relative = file.strip_prefix(base).unwrap_or(file);
                    copy_file(file, &target.join(relative))?;
                }
            }
            for base in &texture_roots {
                for file in files.iter().filter(|f| f.starts_with(base)) {
                    let relative = file.strip_prefix(base).unwrap_or(file);
                    copy_file(file, &texture_dir(&runtime, &id).join(relative))?;
                }
            }
            // The stored preset: our names, plus each kept effect's settings.
            let mut out = format!(
                "Name={name}\nTechniques={}\n",
                kept.iter()
                    .map(|(t, e)| format!("{t}@{e}"))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            let mut section: Option<String> = None;
            for line in text.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('[') && trimmed.ends_with(']') {
                    let effect = &trimmed[1..trimmed.len() - 1];
                    section = renamed.get(&effect.to_ascii_lowercase()).cloned();
                    if let Some(new) = &section {
                        out += &format!("\n[{new}]\n");
                    }
                } else if section.is_some() && trimmed.contains('=') {
                    out += trimmed;
                    out.push('\n');
                }
            }
            std::fs::write(looks_dir(&runtime).join(format!("{id}.ini")), out)
                .map_err(|e| format!("Could not save the look: {e}"))?;
            imported.ids.push(id);
            imported.names.push(name);
        }
        if imported.ids.is_empty() {
            return Err(if imported.missing.is_empty() {
                "The preset does not reference any effects.".into()
            } else {
                format!(
                    "None of the preset's shaders were found ({}). Include its reshade-shaders folder.",
                    imported.missing.join(", ")
                )
            });
        }
        ensure_search_paths(&runtime)?;
        Ok(imported)
    })();
    if is_zip {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

/// Let ReShade find looks in their own subfolders.
pub fn ensure_search_paths(runtime: &Path) -> Result<(), String> {
    let path = runtime.join("ReShade.ini");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Ok(());
    };
    let mut changed = false;
    let mut lines: Vec<String> = text.lines().map(String::from).collect();
    for (key, value) in [
        ("EffectSearchPaths", ".\\shaders\\**"),
        ("TextureSearchPaths", ".\\textures\\**"),
    ] {
        match lines.iter_mut().find(|l| l.starts_with(&format!("{key}="))) {
            Some(line) if line.split(['=', ',']).any(|p| p.trim() == value) => {}
            Some(line) => {
                *line = format!("{key}={value}");
                changed = true;
            }
            None => {
                let at = lines
                    .iter()
                    .position(|l| l.trim() == "[GENERAL]")
                    .map_or(0, |i| i + 1);
                lines.insert(at, format!("{key}={value}"));
                changed = true;
            }
        }
    }
    if changed {
        std::fs::write(&path, lines.join("\r\n") + "\r\n")
            .map_err(|e| format!("Could not update ReShade.ini: {e}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(root: &Path) -> PathBuf {
        let shaders = root.join("reshade-shaders/Shaders");
        std::fs::create_dir_all(shaders.join("Include")).unwrap();
        std::fs::create_dir_all(root.join("reshade-shaders/Textures")).unwrap();
        std::fs::write(shaders.join("Tint.fx"), "#include \"Include/Common.fxh\"\n").unwrap();
        std::fs::write(shaders.join("Unused.fx"), "// not referenced\n").unwrap();
        std::fs::write(shaders.join("Include/Common.fxh"), "// helper\n").unwrap();
        std::fs::write(root.join("reshade-shaders/Textures/lut.png"), b"png").unwrap();
        let preset = root.join("My Look.ini");
        std::fs::write(
            &preset,
            "Techniques=Tint@Tint.fx,Gone@Missing.fx\nTechniqueSorting=Tint@Tint.fx\n\n[Tint.fx]\nStrength=0.25\nColor=1,0.5,0\n",
        )
        .unwrap();
        preset
    }

    #[test]
    fn imports_preset_with_shaders_and_values() {
        let root = std::env::temp_dir().join(format!("neurallayer-looks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let app = root.join("app");
        let runtime = app.join(crate::optiscaler::runtime());
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::write(
            runtime.join("ReShade.ini"),
            "[GENERAL]\nEffectSearchPaths=.\\shaders\n",
        )
        .unwrap();
        let preset = sample(&root.join("download"));

        let result = import(&preset, &app).unwrap();
        assert_eq!(result.ids, ["MyLook"]);
        assert_eq!(result.missing, ["Missing.fx"]);
        let shaders = shader_dir(&runtime, "MyLook");
        assert!(shaders.join("SpatpitLook_MyLook_Tint.fx").is_file());
        assert!(shaders.join("Include/Common.fxh").is_file());
        assert!(!shaders.join("SpatpitLook_MyLook_Unused.fx").exists());
        assert!(texture_dir(&runtime, "MyLook").join("lut.png").is_file());
        let ini = std::fs::read_to_string(runtime.join("ReShade.ini")).unwrap();
        assert!(ini.contains("EffectSearchPaths=.\\shaders\\**"));
        assert!(ini.contains("TextureSearchPaths=.\\textures\\**"));

        let mut looks = Looks::load(&app);
        assert_eq!(looks.list.len(), 1);
        let look = &looks.list[0];
        assert_eq!(look.name, "My Look");
        assert!(look.available);
        assert_eq!(
            look.techniques,
            [("SpatpitLook_MyLook_Tint.fx".into(), "Tint".into())]
        );
        assert_eq!(
            look.values["SpatpitLook_MyLook_Tint.fx"]["Color"],
            [1., 0.5, 0., 0.]
        );
        looks.set(Some("MyLook"), &app).unwrap();
        assert_eq!(Looks::load(&app).selected.as_deref(), Some("MyLook"));

        // A second import of the same preset gets its own id.
        assert_eq!(import(&preset, &app).unwrap().ids, ["MyLook2"]);
        let mut looks = Looks::load(&app);
        looks.set(Some("MyLook"), &app).unwrap();
        looks.remove("MyLook", &app).unwrap();
        assert!(!shaders.exists());
        assert_eq!(looks.selected, None);
        assert_eq!(Looks::load(&app).list.len(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn imports_zip_downloads() {
        let root = std::env::temp_dir().join(format!("neurallayer-zip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let download = root.join("download");
        sample(&download);
        let zip = root.join("look.zip");
        let status = std::process::Command::new("tar")
            .args(["-a", "-cf"])
            .arg(&zip)
            .arg("-C")
            .arg(&download)
            .args(["My Look.ini", "reshade-shaders"])
            .status()
            .unwrap();
        assert!(status.success());
        let app = root.join("app");
        std::fs::create_dir_all(app.join(crate::optiscaler::runtime())).unwrap();
        let result = import(&zip, &app).unwrap();
        assert_eq!(result.names, ["My Look"]);
        let runtime = app.join(crate::optiscaler::runtime());
        assert!(shader_dir(&runtime, "MyLook")
            .join("SpatpitLook_MyLook_Tint.fx")
            .is_file());
        assert!(
            !runtime
                .join("looks")
                .read_dir()
                .unwrap()
                .flatten()
                .any(|e| e.file_name().to_string_lossy().starts_with(".import")),
            "Staging must be removed"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rejects_presets_without_shaders() {
        let root = std::env::temp_dir().join(format!("neurallayer-none-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let preset = root.join("Lonely.ini");
        std::fs::write(&preset, "Techniques=Tint@Tint.fx\n").unwrap();
        let app = root.join("app");
        let error = match import(&preset, &app) {
            Err(e) => e,
            Ok(_) => panic!("must fail"),
        };
        assert!(error.contains("Tint.fx"));
        let _ = std::fs::remove_dir_all(&root);
    }
}
