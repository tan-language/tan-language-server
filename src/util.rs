use tan_formatting::types::Dialect;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn dialect_from_document_uri(uri: &str) -> Dialect {
    if uri.ends_with(".data.tan") {
        Dialect::Data
    } else {
        Dialect::Code
    }
}
