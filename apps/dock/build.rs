fn main() {
    println!("cargo:rerun-if-changed=../../assets/brand/muelle.ico");
    let mut resources = winresource::WindowsResource::new();
    resources.set_icon("../../assets/brand/muelle.ico");
    // Code signing checks that every signed file names the same product.
    resources.set("ProductName", "Muelle");
    resources.set("FileDescription", "Muelle dock");
    if let Err(error) = resources.compile() {
        // The icon is cosmetic: a missing resource compiler must not block the build.
        println!("cargo:warning=no se pudo incrustar el icono: {error}");
    }
}
