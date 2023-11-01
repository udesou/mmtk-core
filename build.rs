use std::io::Result;

fn main() -> Result<()> {
    built::write_built_file().expect("Failed to acquire build-time information");
    prost_build::compile_protos(
        &[
            "src/util/sanity/shapes.proto",
            "src/util/sanity/heapdump.proto",
        ],
        &["src/"],
    )?;
    Ok(())
}
