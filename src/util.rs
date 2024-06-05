use std::sync::Arc;

use serde::{Deserialize, Serialize};

use lsp_server::{Connection, Message};
use lsp_types::notification::Notification;

use tan::api::compile;
use tan::context::Context;
use tan::error::Error;
use tan::eval::eval;
use tan::expr::{expr_clone, Expr};
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

pub fn lsp_range_top() -> lsp_types::Range {
    let start = lsp_types::Position::new(0, 0);
    // let end = lsp_types::Position::new(u32::MAX, u32::MAX);
    lsp_types::Range::new(start, start)
}

#[allow(dead_code)]
pub fn lsp_range_whole_document() -> lsp_types::Range {
    let start = lsp_types::Position::new(0, 0);
    let end = lsp_types::Position::new(u32::MAX, u32::MAX);
    lsp_types::Range::new(start, end)
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

// #insight used to initialize current_module_path.
// #todo find a better name.
// #todo extract this helper function, it's useful in multiple places.
pub fn make_analysis_context() -> Result<Context, std::io::Error> {
    let context = Context::new();

    let current_dir = std::env::current_dir()?.display().to_string();

    context
        .top_scope
        .insert(CURRENT_MODULE_PATH, Expr::string(current_dir));

    Ok(context)
}

// #todo #temp move elsewhere!
// #todo find a better name.
// #todo return the binding in more useful/processed format
// #todo use a fully initialized context.
pub fn parse_module_file(exprs: &[Expr], context: &mut Context) -> Result<Arc<Scope>, Vec<Error>> {
    // #todo implement some context nesting helpers.
    context.scope = Arc::new(Scope::new(context.scope.clone()));

    // #todo #IMPORTANT I think eval is _not_ really needed! maybe just compile!
    // let _ = eval_string(input, context);

    // let exprs = compile_exprs(exprs, context)?;

    // let mut compiled_exprs = Vec::new();

    // for expr in exprs {
    //     // #todo only compile the let expressions?
    //     // #todo why do we need this compile? seems that the function def is not handled.
    //     // #todo #IMPORTANT remove this fucking clone!
    //     let expr = compile(expr_clone(expr), context)?;
    //     if !expr.is_none() {
    //         compiled_exprs.push(expr);
    //     }
    // }

    // let compiled_exprs = exprs;

    // #insight only process top-level `let` definitions.
    // #insight ignore problematic `use` imports.

    // #todo implement a formalized method for custom evaluators like the following.

    for expr in exprs {
        if let Some(terms) = expr.as_list() {
            if let Some(op) = terms.first() {
                if let Some(sym) = op.as_symbol() {
                    if sym == "let" {
                        // #todo why do we need this compile? seems that the function def is not handled.
                        // #todo #IMPORTANT remove this fucking clone!
                        let expr = compile(expr_clone(expr), context)?;
                        // #todo what to do about the error case here?
                        // let _ = tan::eval::eval_let::eval_let(op, &terms[1..], context);
                        let _ = eval(&expr, context);
                    }
                }
            }
        }
    }

    Ok(context.scope.clone())
}

#[cfg(test)]
mod tests {
    use tan::{api::parse_string_all, context::Context};

    use crate::util::parse_module_file;

    #[test]
    fn parse_module_file_usage() {
        let mut context = Context::new();

        let input = r#"
        (let a 1)
        (let b 2)
        (let zonk (Func [a b] (+ a b)))
        "#;

        let exprs = parse_string_all(input).unwrap();
        let scope = parse_module_file(&exprs, &mut context).unwrap();
        let bindings = scope.bindings.read().expect("not poisoned");
        let symbols: Vec<String> = bindings.keys().cloned().collect();
        assert!(symbols.contains(&String::from("a")));
        assert!(symbols.contains(&String::from("b")));
        assert!(symbols.contains(&String::from("zonk")));

        // #todo check case where use URI is invalid!

        let input = r#"
        (use /rng)
        (let b 2)
        (let zonk (Func [x y] (+ x y)))
        "#;

        let exprs = parse_string_all(input).unwrap();
        let scope = parse_module_file(&exprs, &mut context).unwrap();
        let bindings = scope.bindings.read().expect("not poisoned");
        let symbols: Vec<String> = bindings.keys().cloned().collect();
        // #insight we ignore `use` imports
        // assert!(symbols.contains(&String::from("rng/random")));
        assert!(symbols.contains(&String::from("b")));
        assert!(symbols.contains(&String::from("zonk")));

        let input = r#"
        (let b 2)
        (let zonk (Func [x y] (+ x y)))
        (let z (zonk b 4))
        (let a 1)
        "#;

        let exprs = parse_string_all(input).unwrap();
        let scope = parse_module_file(&exprs, &mut context).unwrap();
        let bindings = scope.bindings.read().expect("not poisoned");
        let symbols: Vec<String> = bindings.keys().cloned().collect();
        assert!(symbols.contains(&String::from("z")));
    }
}
