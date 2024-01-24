pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn dialect_from_document_uri(uri: &str) -> &'static str {
    if uri.ends_with(".data.tan") {
        "data"
    } else {
        "code"
    }
}
