fn main() {
    println!("cargo:rerun-if-changed=build-channel.txt");
    let channel = std::fs::read_to_string("build-channel.txt").unwrap_or_default();
    let channel = channel.trim();
    assert!(channel
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-'));
    let label = if channel.is_empty() {
        format!("v{}", std::env::var("CARGO_PKG_VERSION").unwrap())
    } else {
        channel.to_string()
    };
    println!("cargo:rustc-env=OVERLAY_BUILD_LABEL={label}");
    println!("cargo:rerun-if-changed=native");
    println!("cargo:rerun-if-changed=vendor/reshade");
    println!("cargo:rerun-if-changed=vendor/optiscaler");
    let output = std::path::PathBuf::from(std::env::var_os("OUT_DIR").unwrap());
    let shader = std::fs::read_to_string("native/shaders.hlsl").unwrap();
    std::fs::write(
        output.join("shader_source.h"),
        format!("static const char* spatpit_shaders = R\"HLSL({shader})HLSL\";"),
    )
    .unwrap();
    // Build upstream's caller module separately: its filename is part of the
    // neural runtime's calling convention. Ship it beside our executable.
    let forwarder = output.join("nvngx.dll_dlssnr.dll");
    let compiler = cc::Build::new().cpp(true).get_compiler();
    let motion_compiler = output.join("compile_motion.exe");
    let status = compiler
        .to_command()
        .args(["/nologo", "/MT", "/EHsc", "/std:c++20"])
        .arg("native/compile_motion.cpp")
        .arg(format!("/Fe{}", motion_compiler.display()))
        .arg(format!(
            "/Fo{}",
            output.join("compile_motion.obj").display()
        ))
        .args(["/link", "d3dcompiler.lib"])
        .status()
        .expect("Build motion shader compiler");
    assert!(status.success(), "Build motion shader compiler");
    let bytecode = output.join("neural_motion.cso");
    let status = std::process::Command::new(motion_compiler)
        .arg("native/neural_motion.hlsl")
        .arg(&bytecode)
        .status()
        .expect("Compile custom motion shader");
    assert!(status.success(), "Compile custom motion shader");
    let bytes = std::fs::read(bytecode)
        .unwrap()
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(",");
    std::fs::write(
        output.join("neural_motion_shader.h"),
        format!("static const unsigned char neural_motion_shader[] = {{{bytes}}};"),
    )
    .unwrap();
    let status = compiler
        .to_command()
        .args(["/nologo", "/LD", "/MT", "/O2", "/EHsc", "/std:c++20"])
        .arg("vendor/optiscaler/dlssnr_forwarder.cpp")
        .arg(format!(
            "/Fo{}",
            output.join("dlssnr_forwarder.obj").display()
        ))
        .arg("/link")
        .arg(format!("/OUT:{}", forwarder.display()))
        .arg(format!(
            "/IMPLIB:{}",
            output.join("dlssnr_forwarder.lib").display()
        ))
        .arg("d3d12.lib")
        .status()
        .expect("Build neural forwarder");
    assert!(status.success(), "Neural forwarder compilation failed");
    let profile_dir = output.ancestors().nth(3).unwrap();
    std::fs::copy(&forwarder, profile_dir.join("nvngx.dll_dlssnr.dll")).unwrap();
    cc::Build::new()
        .cpp(true)
        .file("native/graphics.cpp")
        .file("native/stream_probe.cpp")
        .file("native/taskbar.cpp")
        .file("native/runtime_import.cpp")
        .file("native/input_router.cpp")
        .file("native/neural_motion.cpp")
        .file("native/neural_motion_test.cpp")
        .include("native")
        .include("vendor/reshade")
        .include("vendor/optiscaler")
        .include(&output)
        .flag("/std:c++20")
        .flag("/EHsc")
        .flag("/utf-8")
        .flag("/permissive-")
        .warnings(false)
        .compile("spatpit_graphics");
    for library in [
        "d3d12",
        "dxgi",
        "d3d11",
        "d3dcompiler",
        "dcomp",
        "comctl32",
        "dwmapi",
        "windowsapp",
        "ole32",
        "advapi32",
        "shell32",
        "propsys",
    ] {
        println!("cargo:rustc-link-lib={library}");
    }
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/app.ico");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let manifest = std::fs::read_to_string("assets/app.manifest")
            .unwrap()
            .replace(
                "version=\"0.1.0.0\"",
                &format!(
                    "version=\"{}.{}.{}.0\"",
                    std::env::var("CARGO_PKG_VERSION_MAJOR").unwrap(),
                    std::env::var("CARGO_PKG_VERSION_MINOR").unwrap(),
                    std::env::var("CARGO_PKG_VERSION_PATCH").unwrap()
                ),
            );
        winres::WindowsResource::new()
            .set_icon("assets/app.ico")
            .append_rc_content("STRINGTABLE\nBEGIN\n101 \"NeuralLayer\"\nEND\n")
            .set_manifest(&manifest)
            .set("ProductName", "NeuralLayer")
            .set("ProductVersion", &label)
            .set("FileDescription", "NeuralLayer — portable window overlay")
            .set(
                "LegalCopyright",
                "GPL-3.0; original Spatpit code by Spatpit",
            )
            .compile()
            .expect("Windows resource compilation failed");
    }
}
