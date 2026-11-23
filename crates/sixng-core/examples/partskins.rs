//! Research: build (or load) the part-skin table and print it as JSON.
//! partskins <game root> <work dir>
fn main() {
    let a: Vec<String> = std::env::args().collect();
    sixng_core::names::set_work_dir(std::path::Path::new(&a[2]));
    match sixng_core::partskin::get(std::path::Path::new(&a[1])) {
        Ok(t) => println!("{}", serde_json::to_string(t).unwrap()),
        Err(e) => eprintln!("{e}"),
    }
}
