use anyhow::Result;
use std::fs::File;

fn main() -> Result<()> {
    let mut f = File::open("preconv.laz")?;
    let header = las::raw::Header::read_from(&mut f)?;
    println!("{:#?}", &header);

    Ok(())
}
