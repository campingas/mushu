fn main() {
    println!("cargo:rerun-if-env-changed=MUSHU_BUILD_TAG");
    println!("cargo:rerun-if-env-changed=MUSHU_BUILD_SHA");
    println!("cargo:rerun-if-env-changed=MUSHU_BUILD_KIND");

    let version = std::env::var("CARGO_PKG_VERSION").expect("Cargo package version");
    let tag = std::env::var("MUSHU_BUILD_TAG").unwrap_or_else(|_| format!("v{version}"));
    let sha = std::env::var("MUSHU_BUILD_SHA").unwrap_or_else(|_| "unknown".into());
    let kind = std::env::var("MUSHU_BUILD_KIND").unwrap_or_else(|_| "dev".into());

    println!("cargo:rustc-env=MUSHU_BUILD_TAG={tag}");
    println!("cargo:rustc-env=MUSHU_BUILD_SHA={sha}");
    println!("cargo:rustc-env=MUSHU_BUILD_KIND={kind}");
}
