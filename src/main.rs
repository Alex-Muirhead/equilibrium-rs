use std::fs::File;
use std::io::{self, BufRead};
use std::path::Path;

fn read_lines<P>(filename: P) -> io::Result<io::Lines<io::BufReader<File>>>
where
    P: AsRef<Path>,
{
    let file = File::open(filename)?;
    Ok(io::BufReader::new(file).lines())
}

fn main() {
    let Ok(lines) = read_lines("./thermo.inp") else {
        eprintln!("Could not open file");
        return;
    };

    for line in lines.map_while(Result::ok) {
        // Skip comment lines
        if line.starts_with("!") {
            continue;
        }
        println!("{line}");
    }
}
