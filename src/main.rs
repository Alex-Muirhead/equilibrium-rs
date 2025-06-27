use std::fs::File;
use std::io::Read;

mod database;

fn main() {
    let mut file = match File::open("./thermo-snippet.inp") {
        Ok(file) => file,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    let mut raw_text = String::new();
    file.read_to_string(&mut raw_text)
        .expect("Could not read file.");

    let thermo_db = match database::parse_thermo_file(&raw_text) {
        Ok(db) => db,
        Err(e) => {
            eprintln!("{}", e);
            return;
        }
    };

    println!("Success!\n{:?}", thermo_db);
}
