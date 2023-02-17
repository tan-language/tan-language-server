use lsp_server::{Connection, Message, Response};
use lsp_types::{
    notification::{DidChangeWatchedFiles, Notification, PublishDiagnostics},
    request::{Formatting, Request},
    Diagnostic, DidChangeWatchedFilesParams, DocumentFormattingParams, OneOf, Position,
    PublishDiagnosticsParams, Range, ServerCapabilities, TextEdit,
};
use tan::error::Error;
use tan::{api::parse_string_all, range::Ranged};
use tan_fmt::pretty::Formatter;
use tan_lint::{lints::snake_case_names_lint::SnakeCaseNamesLint, Lint};
use tracing::{info, trace};
use tracing_subscriber::util::SubscriberInitExt;

// #TODO find a good name.
pub fn emit_parse_error_diagnostics(
    input: &str,
    errors: Vec<Ranged<Error>>,
) -> anyhow::Result<Vec<Diagnostic>> {
    let mut diagnostics: Vec<Diagnostic> = Vec::new();

    for error in errors {
        let start = tan::range::Position::from(error.1.start, input);
        let start = lsp_types::Position {
            line: start.line as u32,
            character: start.col as u32,
        };
        let end = tan::range::Position::from(error.1.end, input);
        let end = lsp_types::Position {
            line: end.line as u32,
            character: end.col as u32,
        };

        diagnostics.push(Diagnostic {
            range: Range { start, end },
            severity: None,
            code: None,
            code_description: None,
            source: None,
            message: error.0.to_string(),
            related_information: None,
            tags: None,
            data: None,
        });
    }

    Ok(diagnostics)
}

fn run(connection: Connection, _params: serde_json::Value) -> anyhow::Result<()> {
    // #TODO use params to get root_uri and perform initial diagnostics for all files.
    // let params: InitializeParams = serde_json::from_value(params).unwrap();

    for msg in &connection.receiver {
        trace!("got msg: {:?}", msg);
        match msg {
            Message::Request(req) => {
                if connection.handle_shutdown(&req)? {
                    return Ok(());
                }
                trace!("got request: {:?}", req);
                // match cast::<GotoDefinition>(req.clone()) {
                //     Ok((id, params)) => {
                //         eprintln!("got gotoDefinition request #{id}: {params:?}");
                //         let result = Some(GotoDefinitionResponse::Array(Vec::new()));
                //         let result = serde_json::to_value(&result).unwrap();
                //         let resp = Response {
                //             id,
                //             result: Some(result),
                //             error: None,
                //         };
                //         connection.sender.send(Message::Response(resp))?;
                //         continue;
                //     }
                //     Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                //     Err(ExtractError::MethodMismatch(req)) => req,
                // };
                // match cast::<References>(req.clone()) {
                //     Ok((id, params)) => {
                //         eprintln!("got references request #{id}: {params:?}");
                //         let result = Some(Vec::<String>::new());
                //         let result = serde_json::to_value(&result).unwrap();
                //         let resp = Response {
                //             id,
                //             result: Some(result),
                //             error: None,
                //         };
                //         connection.sender.send(Message::Response(resp))?;
                //         continue;
                //     }
                //     Err(err @ ExtractError::JsonError { .. }) => panic!("{err:?}"),
                //     Err(ExtractError::MethodMismatch(req)) => req,
                // };

                match req.method.as_ref() {
                    Formatting::METHOD => {
                        let (id, params) =
                            req.extract::<DocumentFormattingParams>(Formatting::METHOD)?;

                        let path = params.text_document.uri.path();
                        let input = std::fs::read_to_string(path)?;

                        let Ok(exprs) = parse_string_all(&input) else {
                            return Err(anyhow::anyhow!("Error"));
                        };

                        let mut formatter = Formatter::new(&exprs);
                        let formatted = formatter.format();

                        // Select the whole document dore replacement
                        let start = Position::new(0, 0);
                        let end = Position::new(u32::MAX, u32::MAX);
                        let document_range = Range::new(start, end);

                        let result = Some(vec![TextEdit::new(document_range, formatted)]);
                        let result = serde_json::to_value(&result).unwrap();
                        let resp = Response {
                            id,
                            result: Some(result),
                            error: None,
                        };

                        connection.sender.send(Message::Response(resp))?;

                        continue;
                    }
                    _ => continue,
                }
            }
            Message::Response(resp) => {
                trace!("got response: {:?}", resp);
            }
            Message::Notification(event) => {
                trace!("got notification: {:?}", event);
                if let Ok(event) =
                    event.extract::<DidChangeWatchedFilesParams>(DidChangeWatchedFiles::METHOD)
                {
                    for change in event.changes {
                        let path = change.uri.path();
                        let input = std::fs::read_to_string(path)?;
                        let result = parse_string_all(&input);

                        let diagnostics = match result {
                            Ok(exprs) => {
                                let mut diagnostics = Vec::new();

                                let mut lint = SnakeCaseNamesLint::new(&input);
                                lint.run(&exprs);
                                diagnostics.append(&mut lint.diagnostics);

                                diagnostics
                            }
                            Err(errors) => emit_parse_error_diagnostics(&input, errors)?,
                        };

                        let pdm = PublishDiagnosticsParams {
                            uri: change.uri.clone(),
                            diagnostics,
                            version: None,
                        };

                        let notification = lsp_server::Notification {
                            method: PublishDiagnostics::METHOD.to_owned(),
                            params: serde_json::to_value(&pdm).unwrap(),
                        };

                        connection
                            .sender
                            .send(Message::Notification(notification))?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .finish()
        .init();

    info!("starting LSP server");

    // Create the connection using stdio as the transport kind.
    let (connection, io_threads) = Connection::stdio();

    let server_capabilities = serde_json::to_value(&ServerCapabilities {
        // definition_provider: Some(OneOf::Left(true)),
        // references_provider: Some(OneOf::Left(true)),
        document_formatting_provider: Some(OneOf::Left(true)),
        ..Default::default()
    })
    .unwrap();

    let initialization_params = connection.initialize(server_capabilities)?;

    // Run the server.
    run(connection, initialization_params)?;

    // Wait for the two threads to end (typically by trigger LSP Exit event).
    io_threads.join()?;

    info!("shutting down server");

    Ok(())
}
