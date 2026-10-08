use pdffiller_core::{fill_pdf, report_json};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        eprintln!("Usage: pdffiller <template.pdf> <data.json> <output.pdf>");
        std::process::exit(2);
    }
    let template = fs::read(&args[1])?;
    let json = fs::read_to_string(&args[2])?;
    let (output, report) = fill_pdf(&template, &json).map_err(std::io::Error::other)?;
    fs::write(&args[3], &output)?;
    println!("{}", report_json(&report));
    println!("Output: {}", args[3]);
    Ok(())
}

[executed on device: QuyenLe (dc1d89ef-2452-4cf0-af98-88586f0bd77d)]