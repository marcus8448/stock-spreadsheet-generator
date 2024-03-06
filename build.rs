#[cfg(windows)]
extern crate winres;

#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set("OriginalFilename", "stock_spreadsheet_generator.exe");
    res.set("LegalCopyright", "Copyright © 2021-2022, 2024 marcus8448");
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn main() {}
