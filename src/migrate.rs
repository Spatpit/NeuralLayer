//! One-time rename of files written before the NeuralLayer rename, when
//! settings, presets and managed shaders used the "Silver" prefix.
use std::path::Path;

/// Files the app owns, beside the executable and in the runtime folder.
const APP_FILES: [&str; 3] = [
    "SilverOptiScaler.ini",
    "SilverEffects.ini",
    "SilverOverlay.ini",
];
const RUNTIME_FILES: [&str; 3] = [
    "SilverPreset.ini",
    "SilverNeural.ini",
    "SilverOptiScaler.log",
];
/// Names referenced inside ReShade and preset INI files. Only these exact
/// tokens are rewritten, so unrelated text (e.g. user paths) is untouched.
const TOKENS: [(&str, &str); 5] = [
    ("SilverPreset.ini", "SpatpitPreset.ini"),
    ("SilverNeural.ini", "SpatpitNeural.ini"),
    ("SilverClarity", "SpatpitClarity"),
    ("SilverKOMPLEX_", "SpatpitKOMPLEX_"),
    ("SilverREALXIV_", "SpatpitREALXIV_"),
];

fn new_name(old: &str) -> String {
    format!("Spatpit{}", &old["Silver".len()..])
}

/// Rename `dir/old` to its Spatpit name unless the new file already exists.
fn rename(dir: &Path, old: &str, moved: &mut usize) -> Result<(), String> {
    let from = dir.join(old);
    let to = dir.join(new_name(old));
    if from.is_file() && !to.exists() {
        std::fs::rename(&from, &to)
            .map_err(|e| format!("Could not rename {}: {e}", from.display()))?;
        *moved += 1;
    }
    Ok(())
}

/// Rename every `Silver*` file below `dir` (managed shader copies).
fn rename_tree(dir: &Path, moved: &mut usize) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            rename_tree(&path, moved)?;
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("Silver") {
                rename(dir, name, moved)?;
            }
        }
    }
    Ok(())
}

/// Rewrite references inside one text file.
fn rewrite(path: &Path, moved: &mut usize) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Ok(());
    };
    let mut updated = text.clone();
    for (old, new) in TOKENS {
        updated = updated.replace(old, new);
    }
    if updated != text {
        std::fs::write(path, updated)
            .map_err(|e| format!("Could not update {}: {e}", path.display()))?;
        *moved += 1;
    }
    Ok(())
}

/// Migrate an installation in `folder` (the executable's folder). Safe to
/// run on every start: it does nothing once the names are current.
/// Returns how many files were renamed or updated.
pub fn legacy_names(folder: &Path) -> Result<usize, String> {
    let runtime = folder.join(crate::optiscaler::runtime());
    let mut moved = 0;
    for name in APP_FILES {
        rename(folder, name, &mut moved)?;
    }
    if !runtime.is_dir() {
        return Ok(moved);
    }
    for name in RUNTIME_FILES {
        rename(&runtime, name, &mut moved)?;
    }
    rename_tree(&runtime.join("shaders"), &mut moved)?;
    rename_tree(&runtime.join("reshade-shaders").join("Shaders"), &mut moved)?;
    if let Ok(entries) = std::fs::read_dir(&runtime) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("ini"))
            {
                rewrite(&path, &mut moved)?;
            }
        }
    }
    // Our own color shader declares its technique by name.
    rewrite(
        &runtime.join("shaders").join("SpatpitClarity.fx"),
        &mut moved,
    )?;
    Ok(moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_and_rewrites_once() {
        let root = std::env::temp_dir().join(format!("neurallayer-migrate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let runtime = root.join(crate::optiscaler::runtime());
        let komplex = runtime.join("reshade-shaders/Shaders/KOMPLEX");
        std::fs::create_dir_all(&komplex).unwrap();
        std::fs::create_dir_all(runtime.join("shaders")).unwrap();
        std::fs::write(root.join("SilverOptiScaler.ini"), "passes=2\n").unwrap();
        std::fs::write(root.join("SilverEffects.ini"), "preset=komplex\n").unwrap();
        std::fs::write(
            runtime.join("SilverPreset.ini"),
            "Techniques=SilverClarity@SilverClarity.fx\n",
        )
        .unwrap();
        std::fs::write(
            runtime.join("ReShade.ini"),
            "PresetPath=.\\SilverNeural.ini\nEffectSearchPaths=C:\\Silverware\\\n",
        )
        .unwrap();
        std::fs::write(
            runtime.join("KOMPLEX-clean.ini"),
            "Techniques=Clarity@SilverKOMPLEX_Clarity.fx\n[SilverKOMPLEX_Clarity.fx]\n",
        )
        .unwrap();
        std::fs::write(
            runtime.join("shaders/SilverClarity.fx"),
            "technique SilverClarity {}\n",
        )
        .unwrap();
        std::fs::write(komplex.join("SilverKOMPLEX_Clarity.fx"), "// third-party\n").unwrap();
        // A newer file must win over a leftover old one.
        std::fs::write(root.join("SilverOverlay.ini"), "theme=light\n").unwrap();
        std::fs::write(root.join("SpatpitOverlay.ini"), "theme=dark\n").unwrap();

        assert!(legacy_names(&root).unwrap() > 0);
        let read = |p: &Path| std::fs::read_to_string(p).unwrap();
        assert_eq!(read(&root.join("SpatpitOptiScaler.ini")), "passes=2\n");
        assert!(root.join("SpatpitEffects.ini").is_file());
        assert_eq!(read(&root.join("SpatpitOverlay.ini")), "theme=dark\n");
        assert_eq!(
            read(&runtime.join("SpatpitPreset.ini")),
            "Techniques=SpatpitClarity@SpatpitClarity.fx\n"
        );
        assert_eq!(
            read(&runtime.join("ReShade.ini")),
            "PresetPath=.\\SpatpitNeural.ini\nEffectSearchPaths=C:\\Silverware\\\n"
        );
        assert!(read(&runtime.join("KOMPLEX-clean.ini")).contains("[SpatpitKOMPLEX_Clarity.fx]"));
        assert_eq!(
            read(&runtime.join("shaders/SpatpitClarity.fx")),
            "technique SpatpitClarity {}\n"
        );
        assert!(komplex.join("SpatpitKOMPLEX_Clarity.fx").is_file());
        assert!(!komplex.join("SilverKOMPLEX_Clarity.fx").exists());

        assert_eq!(
            legacy_names(&root).unwrap(),
            0,
            "Second run must change nothing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
