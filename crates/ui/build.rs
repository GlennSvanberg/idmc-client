fn main() {
    println!("cargo:rerun-if-changed=../../assets/logo/idmc-client.rc");
    println!("cargo:rerun-if-changed=../../assets/logo/idmc-client.ico");
    // GPUI loads the window icon with LoadImageW resource id 1. The .rc names that id.
    // On any target other than Windows this returns without linking.
    embed_resource::compile("../../assets/logo/idmc-client.rc", embed_resource::NONE)
        .manifest_optional()
        .expect("embed the Windows icon");
}
