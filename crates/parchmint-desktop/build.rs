use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=../../packaging/icons/parchmint.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo output directory"));
    let icon = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("crate directory"))
        .join("../../packaging/icons/parchmint.ico")
        .canonicalize()
        .expect("application icon");
    let source = output.join("parchmint.rc");
    let resource = output.join("parchmint.res");
    fs::write(
        &source,
        format!(
            "1 ICON \"{}\"\n",
            icon.display()
                .to_string()
                .trim_start_matches(r"\\?\")
                .replace('\\', "\\\\")
        ),
    )
    .expect("write icon resource");
    let compiler = if Command::new("rc.exe").arg("/?").output().is_ok() {
        PathBuf::from("rc.exe")
    } else {
        let kits = PathBuf::from(env::var_os("ProgramFiles(x86)").expect("Windows SDK location"))
            .join("Windows Kits/10/bin");
        let mut versions: Vec<_> = fs::read_dir(kits)
            .expect("installed Windows SDK")
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        versions.sort();
        versions
            .into_iter()
            .rev()
            .map(|path| path.join("x64/rc.exe"))
            .find(|path| path.is_file())
            .expect("Windows resource compiler")
    };
    assert!(
        Command::new(compiler)
            .arg("/nologo")
            .arg("/fo")
            .arg(&resource)
            .arg(source)
            .status()
            .expect("compile icon resource")
            .success(),
        "resource compilation failed"
    );
    println!("cargo:rustc-link-arg-bin=parchmint={}", resource.display());
}
