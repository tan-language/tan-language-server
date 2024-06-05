use std::sync::Arc;

use serde::{Deserialize, Serialize};

use lsp_server::{Connection, Message};
use lsp_types::notification::Notification;

use tan::api::eval_string;
use tan::context::Context;
use tan::error::Error;
use tan::expr::Expr;
use tan::scope::Scope;
use tan::util::standard_names::CURRENT_MODULE_PATH;
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

// #todo move this helper to tan-analysis
pub fn lsp_range_from_tan_range(tan_range: tan::range::Range) -> lsp_types::Range {
    let start = lsp_types::Position {
        line: tan_range.start.line as u32,
        character: tan_range.start.col as u32,
    };
    let end = lsp_types::Position {
        line: tan_range.end.line as u32,
        character: tan_range.end.col as u32,
    };
    lsp_types::Range { start, end }
}

// #todo probably not required.
// #todo find a better name.
// pub fn make_context_for_parsing() -> Result<Context, std::io::Error> {
//     let context = Context::without_prelude();

//     // #todo prepare context out of this!

//     let current_dir = std::env::current_dir()?.display().to_string();

//     context
//         .top_scope
//         .insert(CURRENT_MODULE_PATH, Expr::string(current_dir));

//     Ok(context)
// }

// #todo #temp move elsewhere!
// #todo find a better name.
// #todo return the binding in more useful/processed format
// #todo use a fully initialized context.
pub fn parse_module_file(input: &str, context: &mut Context) -> Result<Arc<Scope>, Vec<Error>> {
    // #todo implement some context nesting helpers.
    context.scope = Arc::new(Scope::new(context.scope.clone()));
    let _ = eval_string(input, context);
    Ok(context.scope.clone())
}

#[cfg(test)]
mod tests {
    use crate::util::{make_context_for_parsing, parse_module_file};

    #[test]
    fn parse_module_file_usage() {
        let mut context = make_context_for_parsing().unwrap();

        // #todo #fix (`use` fucks-up the scope!!!)
        // #todo add unit-test for `use`
        // #todo also function invocation seems to fuck-up the scope.

        let input = r#"
        (let a 1)
        (let b 2)
        (let zonk (Func [a b] (+ a b)))
        "#;

        let scope = parse_module_file(input, &mut context).unwrap();
        let bindings = scope.bindings.read().expect("not poisoned");
        let symbols: Vec<String> = bindings.keys().cloned().collect();
        assert!(symbols.contains(&String::from("a")));
        assert!(symbols.contains(&String::from("b")));
        assert!(symbols.contains(&String::from("zonk")));
    }
}
