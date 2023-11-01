use std::io::Result;

fn main() -> Result<()> {
    prost_build::compile_protos(
        &[
            "../../src/util/sanity/shapes.proto",
            "../../src/util/sanity/heapdump.proto",
        ],
        &["../../src"],
    )?;
    Ok(())
}
