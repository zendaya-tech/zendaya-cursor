fn main() {
    // Keep local Windows builds small: wgpu dependency symbols produce large PDBs.
    // Retain cached dependency objects without emitting the final debug PDB.
    if std::env::var("TARGET").is_ok_and(|v| v.ends_with("windows-msvc"))
        && std::env::var("PROFILE").is_ok_and(|v| v == "debug")
    {
        println!("cargo:rustc-link-arg=/DEBUG:NONE");
    }
}
