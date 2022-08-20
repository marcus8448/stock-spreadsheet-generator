#[cfg(windows)]
extern crate winres;

#[cfg(windows)]
fn main() {
    let mut res = winres::WindowsResource::new();
    res.set("OriginalFilename", "stock_spreadsheet_generator.exe");
    res.set("ProductName", "Stock Spreadsheet Generator");
    res.set("FileDescription", "Stock Spreadsheet Generator");
    res.set("LegalCopyright", "Copyright © 2022, marcus8448");
    res.set_version_info(winres::VersionInfo::FILEVERSION, 0 << 48 | 5 << 32 | 1 << 16);
    res.set_version_info(winres::VersionInfo::PRODUCTVERSION, 0 << 48 | 5 << 32 | 1 << 16);
    res.compile().unwrap();
}

#[cfg(not(windows))]
fn main() {
}