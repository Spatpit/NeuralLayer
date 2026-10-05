//! The FFI boundary contains only owned handles and plain numeric buffers.
use std::{
    collections::HashMap,
    ffi::{c_char, c_void, CStr, CString},
    path::Path,
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [u8; 4],
}
#[repr(C)]
struct Draw {
    first_index: u32,
    index_count: u32,
    base_vertex: u32,
    texture: u32,
    clip: [f32; 4],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Options {
    pub sharpness: f32,
    pub saturation: f32,
    pub contrast: f32,
    pub split: f32,
    pub effect: u32,
    pub comparison: u32,
    pub paused: u32,
    pub reshade: u32,
    pub neural: u32,
    pub idle_resize: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            sharpness: 0.3,
            saturation: 1.0,
            contrast: 1.0,
            split: 0.5,
            effect: 0,
            comparison: 0,
            paused: 0,
            reshade: 0,
            neural: 0,
            idle_resize: 0,
        }
    }
}
#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct Status {
    pub capture_active: u32,
    pub has_frame: u32,
    pub width: u32,
    pub height: u32,
    pub reshade_loaded: u32,
    pub techniques: u32,
    pub frames: u64,
    pub frame_age_ms: u64,
}
#[repr(C)]
struct RawTechnique {
    effect: [u8; 256],
    name: [u8; 128],
    enabled: u32,
}
#[repr(C)]
struct RawUniform {
    name: [u8; 128],
    label: [u8; 256],
    category: [u8; 128],
    tooltip: [u8; 512],
    items: [u8; 1024],
    kind: u32,
    components: u32,
    bounded: u32,
    values: [f32; 4],
    minimum: [f32; 4],
    maximum: [f32; 4],
}
pub struct Technique {
    pub effect: String,
    pub name: String,
    pub enabled: bool,
}
pub struct Uniform {
    pub name: String,
    pub label: String,
    pub category: String,
    pub tooltip: String,
    pub items: Vec<String>,
    pub kind: u32,
    pub components: usize,
    pub bounded: bool,
    pub values: [f32; 4],
    pub minimum: [f32; 4],
    pub maximum: [f32; 4],
}
fn field(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes.split(|c| *c == 0).next().unwrap_or_default()).into_owned()
}
extern "C" {
    fn spatpit_create(hwnd: *mut c_void, runtime_folder: *const u16) -> *mut c_void;
    fn spatpit_destroy(engine: *mut c_void);
    fn spatpit_error(engine: *mut c_void) -> *const c_char;
    fn spatpit_capture(engine: *mut c_void, target: *mut c_void, client_only: i32) -> i32;
    fn spatpit_stop(engine: *mut c_void);
    fn spatpit_texture(
        engine: *mut c_void,
        id: u32,
        width: u32,
        height: u32,
        data: *const u8,
    ) -> i32;
    fn spatpit_free_texture(engine: *mut c_void, id: u32);
    fn spatpit_render(
        engine: *mut c_void,
        width: u32,
        height: u32,
        scale: f32,
        vertices: *const Vertex,
        vertex_count: u32,
        indices: *const u32,
        index_count: u32,
        draws: *const Draw,
        draw_count: u32,
        options: Options,
    ) -> i32;
    fn spatpit_status(engine: *mut c_void, status: *mut Status);
    fn spatpit_screenshot(engine: *mut c_void, data: *mut u8, width: u32, height: u32) -> i32;
    fn spatpit_load_reshade(engine: *mut c_void, dll: *const u16, config: *const c_char) -> i32;
    fn spatpit_reshade_preset(engine: *mut c_void, path: *const c_char);
    fn spatpit_techniques(
        engine: *mut c_void,
        callback: unsafe extern "C" fn(*const RawTechnique, *mut c_void),
        data: *mut c_void,
    );
    fn spatpit_uniforms(
        engine: *mut c_void,
        effect: *const c_char,
        callback: unsafe extern "C" fn(*const RawUniform, *mut c_void),
        data: *mut c_void,
    );
    fn spatpit_set_technique(
        engine: *mut c_void,
        effect: *const c_char,
        name: *const c_char,
        enabled: i32,
    ) -> i32;
    fn spatpit_set_uniform(
        engine: *mut c_void,
        effect: *const c_char,
        name: *const c_char,
        values: *const f32,
        reset: i32,
    ) -> i32;
    fn spatpit_save_preset(engine: *mut c_void) -> i32;
    fn spatpit_reload_effect(engine: *mut c_void, effect: *const c_char) -> i32;
    fn spatpit_effect_generation(engine: *mut c_void) -> u64;
    fn spatpit_clear_effect_loading_error(engine: *mut c_void);
    fn spatpit_effect_reloading(engine: *mut c_void) -> i32;
    fn spatpit_effects_changed(engine: *mut c_void);
    fn spatpit_color_preset(engine: *mut c_void, list: *const c_char);
    fn spatpit_native_overlay(engine: *mut c_void, open: i32) -> i32;
    fn spatpit_nr_configure(engine: *mut c_void, options: *const crate::optiscaler::Options);
    fn spatpit_nr_status(engine: *mut c_void, status: *mut crate::optiscaler::Status);
    fn spatpit_nr_reset(engine: *mut c_void);
    fn spatpit_native_overlay_active(engine: *mut c_void) -> i32;
}
/// CPU copy of an egui texture, kept so partial updates can be applied.
struct Texture {
    width: usize,
    height: usize,
    rgba: Vec<u8>,
}
pub struct Graphics {
    engine: *mut c_void,
    textures: HashMap<u32, Texture>,
}
impl Graphics {
    /// True when the last failed effect call hit a transient ReShade reload.
    pub fn effect_reloading(&self) -> bool {
        unsafe { spatpit_effect_reloading(self.engine) != 0 }
    }
    pub fn effect_generation(&self) -> u64 {
        unsafe { spatpit_effect_generation(self.engine) }
    }
    pub fn clear_effect_loading_error(&mut self) {
        unsafe { spatpit_clear_effect_loading_error(self.engine) };
    }
    pub fn reload_effect(&mut self, name: &str) -> Result<(), String> {
        let name = CString::new(name).map_err(|e| e.to_string())?;
        self.check(unsafe { spatpit_reload_effect(self.engine, name.as_ptr()) })
    }
    /// Rescan the shader folders and recompile every effect (e.g. after an import).
    pub fn reload_all_effects(&mut self) {
        unsafe { spatpit_reload_effect(self.engine, std::ptr::null()) };
    }
    pub fn effects_changed(&mut self) {
        unsafe { spatpit_effects_changed(self.engine) };
    }
    pub fn color_preset(&mut self, techniques: &[(String, String)]) -> Result<(), String> {
        let list = techniques
            .iter()
            .map(|(effect, name)| format!("{name}@{effect}"))
            .collect::<Vec<_>>()
            .join(",");
        let list = CString::new(list).map_err(|e| e.to_string())?;
        unsafe { spatpit_color_preset(self.engine, list.as_ptr()) };
        Ok(())
    }
    pub fn nr_configure(&mut self, options: &crate::optiscaler::Options) {
        unsafe {
            spatpit_nr_configure(self.engine, options);
        }
    }
    pub fn nr_status(&self) -> crate::optiscaler::Status {
        let mut s = crate::optiscaler::Status::default();
        unsafe {
            spatpit_nr_status(self.engine, &mut s);
        }
        s
    }
    pub fn nr_reset(&mut self) {
        unsafe {
            spatpit_nr_reset(self.engine);
        }
    }

    pub fn native_overlay(&mut self, open: bool) -> Result<(), String> {
        self.check(unsafe { spatpit_native_overlay(self.engine, open as i32) })
    }
    pub fn native_overlay_active(&self) -> bool {
        unsafe { spatpit_native_overlay_active(self.engine) != 0 }
    }
    pub fn techniques(&self) -> Vec<Technique> {
        unsafe extern "C" fn collect(raw: *const RawTechnique, data: *mut c_void) {
            let raw = &*raw;
            let list = &mut *(data as *mut Vec<Technique>);
            list.push(Technique {
                effect: field(&raw.effect),
                name: field(&raw.name),
                enabled: raw.enabled != 0,
            });
        }
        let mut result: Vec<Technique> = Vec::new();
        unsafe { spatpit_techniques(self.engine, collect, &mut result as *mut _ as _) };
        result
    }
    pub fn uniforms(&self, effect: &str) -> Vec<Uniform> {
        unsafe extern "C" fn collect(raw: *const RawUniform, data: *mut c_void) {
            let raw = &*raw;
            let list = &mut *(data as *mut Vec<Uniform>);
            list.push(Uniform {
                name: field(&raw.name),
                label: field(&raw.label),
                category: field(&raw.category),
                tooltip: field(&raw.tooltip),
                items: raw
                    .items
                    .split(|c| *c == 0)
                    .filter(|part| !part.is_empty())
                    .map(|part| String::from_utf8_lossy(part).into_owned())
                    .collect(),
                kind: raw.kind,
                components: raw.components.min(4) as usize,
                bounded: raw.bounded != 0,
                values: raw.values,
                minimum: raw.minimum,
                maximum: raw.maximum,
            });
        }
        let Ok(effect) = CString::new(effect) else {
            return Vec::new();
        };
        let mut result: Vec<Uniform> = Vec::new();
        unsafe {
            spatpit_uniforms(
                self.engine,
                effect.as_ptr(),
                collect,
                &mut result as *mut _ as _,
            )
        };
        result
    }
    pub fn set_technique(&mut self, effect: &str, name: &str, enabled: bool) -> Result<(), String> {
        let effect = CString::new(effect).map_err(|e| e.to_string())?;
        let name = CString::new(name).map_err(|e| e.to_string())?;
        self.check(unsafe {
            spatpit_set_technique(self.engine, effect.as_ptr(), name.as_ptr(), enabled as i32)
        })
    }
    pub fn set_uniform(
        &mut self,
        effect: &str,
        name: &str,
        values: &[f32; 4],
        reset: bool,
    ) -> Result<(), String> {
        let effect = CString::new(effect).map_err(|e| e.to_string())?;
        let name = CString::new(name).map_err(|e| e.to_string())?;
        self.check(unsafe {
            spatpit_set_uniform(
                self.engine,
                effect.as_ptr(),
                name.as_ptr(),
                values.as_ptr(),
                reset as i32,
            )
        })
    }
    pub fn save_preset(&mut self) -> Result<(), String> {
        self.check(unsafe { spatpit_save_preset(self.engine) })
    }
    pub fn new(hwnd: isize) -> Result<Self, String> {
        let runtime = if std::env::args().any(|a| a == "--safe-graphics") {
            "runtime-disabled"
        } else {
            crate::optiscaler::runtime()
        };
        let folder: Vec<u16> = crate::runtime_folder()
            .join(runtime)
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let engine = unsafe { spatpit_create(hwnd as _, folder.as_ptr()) };
        if engine.is_null() {
            return Err(unsafe { CStr::from_ptr(spatpit_error(engine)) }
                .to_string_lossy()
                .into());
        }
        Ok(Self {
            engine,
            textures: HashMap::new(),
        })
    }
    pub fn error(&self) -> String {
        unsafe { CStr::from_ptr(spatpit_error(self.engine)) }
            .to_string_lossy()
            .into()
    }
    fn check(&self, result: i32) -> Result<(), String> {
        if result == 0 {
            Err(self.error())
        } else {
            Ok(())
        }
    }
    pub fn capture(&mut self, hwnd: isize, client: bool) -> Result<(), String> {
        self.check(unsafe { spatpit_capture(self.engine, hwnd as _, client as _) })
    }
    pub fn stop(&mut self) {
        unsafe { spatpit_stop(self.engine) };
    }
    pub fn status(&self) -> Status {
        let mut status = Status::default();
        unsafe { spatpit_status(self.engine, &mut status) };
        status
    }
    pub fn load_reshade(&mut self, folder: &Path) -> Result<(), String> {
        if self.status().reshade_loaded != 0 {
            return Ok(());
        }
        let dll = folder
            .join(crate::optiscaler::runtime())
            .join("ReShade64.dll");
        if !dll.is_file() {
            return Err("ReShade is missing from the runtime-optiscaler folder. Extract the complete NeuralLayer ZIP again, then restart.".into());
        }
        let wide: Vec<_> = dll
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let config = CString::new(
            folder
                .join(crate::optiscaler::runtime())
                .join("ReShade.ini")
                .to_string_lossy()
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        self.check(unsafe { spatpit_load_reshade(self.engine, wide.as_ptr(), config.as_ptr()) })?;
        let preset = CString::new(
            folder
                .join(crate::optiscaler::runtime())
                .join("SpatpitPreset.ini")
                .to_string_lossy()
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        unsafe { spatpit_reshade_preset(self.engine, preset.as_ptr()) };
        Ok(())
    }
    pub fn preset(&mut self, folder: &Path, neural: bool) -> Result<(), String> {
        if neural
            && !folder
                .join(crate::optiscaler::runtime())
                .join("nvngx_dlssnr.dll")
                .exists()
        {
            return Err(
                "Neural runtime missing. Use Import neural runtime on the Neural page to add nvngx_dlssnr.dll."
                    .into(),
            );
        }
        let path = CString::new(
            folder
                .join(crate::optiscaler::runtime())
                .join(if neural {
                    "SpatpitNeural.ini"
                } else {
                    "SpatpitPreset.ini"
                })
                .to_string_lossy()
                .as_bytes(),
        )
        .map_err(|e| e.to_string())?;
        unsafe {
            spatpit_reshade_preset(self.engine, path.as_ptr());
        }
        Ok(())
    }
    pub fn render(
        &mut self,
        size: [u32; 2],
        scale: f32,
        primitives: &[egui::ClippedPrimitive],
        delta: &egui::TexturesDelta,
        options: Options,
    ) -> Result<(), String> {
        // Apply every change first, then upload each touched texture once.
        let mut touched = Vec::new();
        for (id, change) in &delta.set {
            let egui::TextureId::Managed(id) = *id else {
                continue;
            };
            let id = id as u32;
            let (size, pixels): ([usize; 2], Vec<u8>) = match &change.image {
                egui::ImageData::Color(image) => (
                    image.size,
                    image.pixels.iter().flat_map(|p| p.to_array()).collect(),
                ),
                egui::ImageData::Font(image) => (
                    image.size,
                    image
                        .srgba_pixels(None)
                        .flat_map(|p| p.to_array())
                        .collect(),
                ),
            };
            match change.pos {
                Some([x, y]) => {
                    let Some(full) = self.textures.get_mut(&id) else {
                        continue;
                    };
                    let row_bytes = size[0] * 4;
                    for row in 0..size[1] {
                        let offset = ((row + y) * full.width + x) * 4;
                        full.rgba[offset..offset + row_bytes]
                            .copy_from_slice(&pixels[row * row_bytes..(row + 1) * row_bytes]);
                    }
                }
                None => {
                    self.textures.insert(
                        id,
                        Texture {
                            width: size[0],
                            height: size[1],
                            rgba: pixels,
                        },
                    );
                }
            }
            if !touched.contains(&id) {
                touched.push(id);
            }
        }
        for id in touched {
            let full = &self.textures[&id];
            self.check(unsafe {
                spatpit_texture(
                    self.engine,
                    id,
                    full.width as _,
                    full.height as _,
                    full.rgba.as_ptr(),
                )
            })?;
        }
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        let mut draws = Vec::new();
        for primitive in primitives {
            if let egui::epaint::Primitive::Mesh(mesh) = &primitive.primitive {
                let egui::TextureId::Managed(id) = mesh.texture_id else {
                    continue;
                };
                draws.push(Draw {
                    first_index: indices.len() as _,
                    index_count: mesh.indices.len() as _,
                    base_vertex: vertices.len() as _,
                    texture: id as _,
                    clip: [
                        primitive.clip_rect.min.x,
                        primitive.clip_rect.min.y,
                        primitive.clip_rect.max.x,
                        primitive.clip_rect.max.y,
                    ],
                });
                vertices.extend(mesh.vertices.iter().map(|v| Vertex {
                    pos: [v.pos.x, v.pos.y],
                    uv: [v.uv.x, v.uv.y],
                    color: v.color.to_array(),
                }));
                indices.extend_from_slice(&mesh.indices);
            }
        }
        self.check(unsafe {
            spatpit_render(
                self.engine,
                size[0],
                size[1],
                scale,
                vertices.as_ptr(),
                vertices.len() as _,
                indices.as_ptr(),
                indices.len() as _,
                draws.as_ptr(),
                draws.len() as _,
                options,
            )
        })?;
        for id in &delta.free {
            if let egui::TextureId::Managed(id) = *id {
                unsafe { spatpit_free_texture(self.engine, id as _) };
                self.textures.remove(&(id as u32));
            }
        }
        Ok(())
    }
    pub fn screenshot(&self, size: [u32; 2]) -> Result<egui::ColorImage, String> {
        let mut bytes = vec![0; size[0] as usize * size[1] as usize * 4];
        self.check(unsafe {
            spatpit_screenshot(self.engine, bytes.as_mut_ptr(), size[0], size[1])
        })?;
        Ok(egui::ColorImage {
            size: [size[0] as _, size[1] as _],
            pixels: bytes
                .chunks_exact(4)
                .map(|p| egui::Color32::from_rgba_premultiplied(p[0], p[1], p[2], p[3]))
                .collect(),
        })
    }
}
impl Drop for Graphics {
    fn drop(&mut self) {
        unsafe { spatpit_destroy(self.engine) };
    }
}
