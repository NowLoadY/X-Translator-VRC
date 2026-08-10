fn main() {
    println!("cargo:rerun-if-changed=resources/branding/xrtranslate-logo.ico");

    #[cfg(windows)]
    {
        let mut resource = winres::WindowsResource::new();
        resource.set_icon("resources/branding/xrtranslate-logo.ico");
        resource
            .compile()
            .expect("cannot embed XRTranslate application icon");
    }
}
