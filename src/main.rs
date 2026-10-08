use pdffiller_core::{fill_pdf, report_json};
use std::{env, fs};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 || args.len() > 5 {
        eprintln!("Usage: pdffiller <template.pdf> <data.json> <output.pdf> [piece_info.json]");
        std::process::exit(2);
    }
    let template = fs::read(&args[1])?;
    let json = fs::read_to_string(&args[2])?;
    let piece_info = if args.len() == 5 {
        Some(fs::read_to_string(&args[4])?)
    } else {
        None
    };
    let (output, report) =
        fill_pdf(&template, &json, piece_info.as_deref()).map_err(std::io::Error::other)?;
    fs::write(&args[3], &output)?;
    println!("{}", report_json(&report));
    println!("Output: {}", args[3]);
    Ok(())
}
