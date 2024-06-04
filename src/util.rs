use serde::{Deserialize, Serialize};

use lsp_server::{Connection, Message};
use lsp_types::notification::Notification;

use tan::error::Error;
use tan::expr::Expr;
use tan_formatting::types::Dialect;

use crossbeam::channel::SendError;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn dialect_from_document_uri(uri: &str) -> Dialect {
    // #todo I don't think dialect is the correct word.
    // #todo introduce HTML and CSS dialects.
    if uri.ends_with(".data.tan") || uri.ends_with(".config.tan") {
        Dialect::Data
    } else {
        Dialect::Code
    }
}

#[derive(Debug)]
pub enum PublishServerStatus {}

impl Notification for PublishServerStatus {
    type Params = PublishServerStatusParams;
    const METHOD: &'static str = "tan/publishServerStatus";
}

#[derive(Serialize, Deserialize)]
pub struct PublishServerStatusParams {
    pub text: String,
}

pub fn send_server_status_notification(
    connection: &Connection,
    text: &str,
) -> Result<(), SendError<Message>> {
    let text = format!("👅 {text}");

    let pss = PublishServerStatusParams { text };

    let notification = lsp_server::Notification {
        method: PublishServerStatus::METHOD.to_owned(),
        params: serde_json::to_value(pss).unwrap(),
    };

    connection
        .sender
        .send(Message::Notification(notification))?;

    Ok(())
}

// // #todo #temp move elsewhere!
// pub fn eval_module_file() -> Result<Expr, Vec<Error>> {
//     todo!()
// }
