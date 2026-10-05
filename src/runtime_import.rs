//! Import a user-selected runtime without loading it or copying shaders/presets.
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom},
    path::Path,
};
extern "C" {
    fn spatpit_pick_neural_runtime(
        owner: *mut std::ffi::c_void,
        out: *mut u16,
        capacity: u32,
    ) -> i32;
    fn spatpit_pick_file(
        owner: *mut std::ffi::c_void,
        title: *const u16,
        filter_name: *const u16,
        filter_spec: *const u16,
        out: *mut u16,
        capacity: u32,
    ) -> i32;
}

/// Let the user choose a ReShade preset `.ini` or a downloaded `.zip`.
pub fn pick_preset(owner: isize) -> Result<Option<std::path::PathBuf>, String> {
    let mut path = vec![0u16; 32768];
    let title = crate::native::wide("Import a ReShade preset");
    let name = crate::native::wide("ReShade preset or ZIP download (*.ini; *.zip)");
    let spec = crate::native::wide("*.ini;*.zip");
    match unsafe {
        spatpit_pick_file(
            owner as _,
            title.as_ptr(),
            name.as_ptr(),
            spec.as_ptr(),
            path.as_mut_ptr(),
            path.len() as u32,
        )
    } {
        0 => Ok(None),
        1 => {
            let end = path
                .iter()
                .position(|&c| c == 0)
                .ok_or("Invalid selected path")?;
            use std::os::windows::ffi::OsStringExt;
            Ok(Some(std::ffi::OsString::from_wide(&path[..end]).into()))
        }
        _ => Err("Could not open the file picker".into()),
    }
}
fn validate(file: &mut File) -> Result<(), String> {
    let mut dos = [0u8; 64];
    file.read_exact(&mut dos).map_err(|_| "Not a Windows DLL")?;
    if &dos[..2] != b"MZ" {
        return Err("Not a Windows DLL".into());
    }
    let offset = u32::from_le_bytes(dos[60..64].try_into().unwrap()) as u64;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if offset < 64 || offset + 26 > size {
        return Err("Invalid DLL header".into());
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| e.to_string())?;
    let mut pe = [0u8; 26];
    file.read_exact(&mut pe).map_err(|_| "Invalid DLL header")?;
    if &pe[..4] != b"PE\0\0"
        || u16::from_le_bytes([pe[4], pe[5]]) != 0x8664
        || u16::from_le_bytes([pe[22], pe[23]]) & 0x2000 == 0
        || u16::from_le_bytes([pe[24], pe[25]]) != 0x20b
    {
        return Err("Select a 64-bit Windows DLL".into());
    }
    file.rewind().map_err(|e| e.to_string())
}
fn import(source: &Path, folder: &Path) -> Result<(), String> {
    if !source
        .file_name()
        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("nvngx_dlssnr.dll"))
    {
        return Err("Select nvngx_dlssnr.dll".into());
    }
    let mut input = File::open(source).map_err(|e| e.to_string())?;
    validate(&mut input)?;
    let runtime = folder.join(crate::optiscaler::runtime());
    fs::create_dir_all(&runtime).map_err(|e| e.to_string())?;
    let destination = runtime.join("nvngx_dlssnr.dll");
    if destination.exists() {
        return Err(
            "A neural runtime is already installed. Close the app before replacing it.".into(),
        );
    }
    let temporary = runtime.join("nvngx_dlssnr.importing");
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    let result = std::io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
    drop(output);
    if let Err(e) = result {
        let _ = fs::remove_file(&temporary);
        return Err(e.to_string());
    }
    if let Err(e) = fs::rename(&temporary, &destination) {
        let _ = fs::remove_file(&temporary);
        return Err(e.to_string());
    }
    Ok(())
}
pub fn pick_and_import(owner: isize, folder: &Path) -> Result<bool, String> {
    let mut path = vec![0u16; 32768];
    match unsafe { spatpit_pick_neural_runtime(owner as _, path.as_mut_ptr(), path.len() as u32) } {
        0 => Ok(false),
        1 => {
            let end = path
                .iter()
                .position(|&c| c == 0)
                .ok_or("Invalid selected path")?;
            use std::os::windows::ffi::OsStringExt;
            import(
                Path::new(&std::ffi::OsString::from_wide(&path[..end])),
                folder,
            )?;
            Ok(true)
        }
        _ => Err("Could not open the file picker".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn import_rejects_invalid_dll_and_does_not_copy_other_files() {
        let root = std::env::temp_dir().join(format!("neurallayer-import-{}", std::process::id()));
        fs::create_dir_all(root.join("input")).unwrap();
        let source = root.join("input/nvngx_dlssnr.dll");
        fs::write(&source, b"not a dll").unwrap();
        assert!(import(&source, &root.join("app")).is_err());
        assert!(!root.join("app/runtime-optiscaler").exists());
        let mut bytes = vec![0u8; 256];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[60..64].copy_from_slice(&64u32.to_le_bytes());
        bytes[64..68].copy_from_slice(b"PE\0\0");
        bytes[68..70].copy_from_slice(&0x8664u16.to_le_bytes());
        bytes[86..88].copy_from_slice(&0x2000u16.to_le_bytes());
        bytes[88..90].copy_from_slice(&0x20bu16.to_le_bytes());
        fs::write(&source, &bytes).unwrap();
        fs::write(root.join("input/unwanted.fx"), b"not imported").unwrap();
        import(&source, &root.join("app")).unwrap();
        assert_eq!(
            fs::read(root.join("app/runtime-optiscaler/nvngx_dlssnr.dll")).unwrap(),
            bytes
        );
        assert_eq!(
            fs::read_dir(root.join("app/runtime-optiscaler"))
                .unwrap()
                .count(),
            1
        );
        assert!(import(&source, &root.join("app")).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
