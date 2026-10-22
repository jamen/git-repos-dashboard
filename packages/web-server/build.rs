use std::{
    collections::hash_map::DefaultHasher,
    env, fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let packages_dir = manifest_dir.parent().expect("packages directory");
    let client_dir = packages_dir.join("web-client");
    let templates_dir = packages_dir.join("templates/templates");

    println!(
        "cargo::rerun-if-changed={}",
        client_dir.join("src").display()
    );
    println!(
        "cargo::rerun-if-changed={}",
        client_dir.join("package.json").display()
    );
    println!(
        "cargo::rerun-if-changed={}",
        client_dir.join("pnpm-lock.yaml").display()
    );
    println!(
        "cargo::rerun-if-changed={}",
        client_dir.join("tsconfig.json").display()
    );
    println!(
        "cargo::rerun-if-changed={}",
        client_dir.join("vite.config.js").display()
    );

    println!("cargo::rerun-if-changed={}", templates_dir.display());
    println!(
        "cargo::rerun-if-changed={}",
        manifest_dir.join("assets").display()
    );

    let dist_dir = client_dir.join("dist");

    if !client_dir.join("node_modules").exists() {
        run(&client_dir, &["install"]);
    }

    run(&client_dir, &["run", "build"]);

    emit_shell_id(&dist_dir);
}

fn emit_shell_id(dist_dir: &Path) {
    let mut hasher = DefaultHasher::new();

    for asset in ["main.js", "main.css"] {
        let bytes = fs::read(dist_dir.join(asset))
            .unwrap_or_else(|error| panic!("failed to read {asset}: {error}"));
        bytes.hash(&mut hasher);
    }

    println!("cargo::rustc-env=SHELL_ID=app.{:08x}", hasher.finish());
}

fn run(dir: &Path, args: &[&str]) {
    let status = Command::new("pnpm")
        .args(args)
        .env("CI", "true")
        .current_dir(dir)
        .status()
        .unwrap_or_else(|error| {
            panic!(
                "failed to run `pnpm {}` in {}: {error}\n\
                 pnpm is provided by the nix dev shell",
                args.join(" "),
                dir.display(),
            )
        });

    if !status.success() {
        panic!("`pnpm {}` failed with {status}", args.join(" "));
    }
}
