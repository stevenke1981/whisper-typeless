fn main() {
    slint_build::compile("ui/appwindow.slint").expect("Slint build failed");

    // Vulkan 版：延遲載入 vulkan-1.dll，讓沒有 Vulkan 執行環境的機器仍能啟動
    // （--self-check、CPU 推論路徑）；實際使用 GPU 前由程式以 LoadLibrary 探測。
    println!("cargo:rerun-if-env-changed=CARGO_FEATURE_VULKAN");
    let vulkan = std::env::var_os("CARGO_FEATURE_VULKAN").is_some();
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").is_ok_and(|e| e == "msvc");
    if vulkan && msvc {
        println!("cargo:rustc-link-arg=/DELAYLOAD:vulkan-1.dll");
        println!("cargo:rustc-link-arg=delayimp.lib");
    }
}
