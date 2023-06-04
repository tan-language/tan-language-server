use lsp_server::{Connection, Message, Response};
use lsp_types::{
    notification::{DidChangeWatchedFiles, Notification, PublishDiagnostics},
    request::{Formatting, Request},
    DidChangeWatchedFilesParams, DocumentFormattingParams, OneOf, Position,
    PublishDiagnosticsParams, Range, ServerCapabilities, TextEdit, Url,
};
use tan::api::parse_string_all;
use tan_fmt::pretty::Formatter;
use tan_lint::compute_diagnostics;
use tracing::{info, trace};
use tracing_subscriber::util::SubscriberInitExt;

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn send_diagnostics(connection: &Connection, uri: Url) -> anyhow::Result<()> {
    let diagnostics = compute_diagnostics(uri.as_str());

    // #Insight
    // We send a notification even for empty diagnostics to clear previous
    // diagnostics.

    // if diagnostics.is_empty() {
    //     return Ok(());
    // }

    let pdm = PublishDiagnosticsParams {
        uri: uri.clone(),
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

    Ok(())
}

fn run(connection: Connection, _params: serde_json::Value) -> anyhow::Result<()> {
    // #TODO use params to get root_uri and perform initial diagnostics for all files.
    // let params: InitializeParams = serde_json::from_value(params).unwrap();
    // eprintln!("{params:#?}");

    for msg in &connection.receiver {
        trace!("Got msg: {:?}.", msg);
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
                trace!("Got response: {:?}.", resp);
            }
            Message::Notification(event) => {
                trace!("got notification: {:?}.", event);
                if let Ok(event) =
                    event.extract::<DidChangeWatchedFilesParams>(DidChangeWatchedFiles::METHOD)
                {
                    for change in event.changes {
                        // let path = change.uri.path();
                        // let input = std::fs::read_to_string(path)?;
                        // let result = parse_string_all(&input);

                        // let diagnostics = match result {
                        //     Ok(exprs) => {
                        //         let mut diagnostics = Vec::new();

                        //         let mut lint = SnakeCaseNamesLint::new(&input);
                        //         lint.run(&exprs);
                        //         diagnostics.append(&mut lint.diagnostics);

                        //         diagnostics
                        //     }
                        //     Err(errors) => gen_parse_error_diagnostics(&input, errors)?,
                        // };

                        // let pdm = PublishDiagnosticsParams {
                        //     uri: change.uri.clone(),
                        //     diagnostics,
                        //     version: None,
                        // };

                        // let notification = lsp_server::Notification {
                        //     method: PublishDiagnostics::METHOD.to_owned(),
                        //     params: serde_json::to_value(&pdm).unwrap(),
                        // };

                        // connection
                        //     .sender
                        //     .send(Message::Notification(notification))?;

                        send_diagnostics(&connection, change.uri)?;
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
        .with_ansi(false)
        .finish()
        .init();

    info!("Starting LSP server, v{VERSION}...");

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

    info!("Started.");

    // Run the server.
    run(connection, initialization_params)?;

    // Wait for the two threads to end (typically by trigger LSP Exit event).
    io_threads.join()?;

    info!("Shutting down server...");

    Ok(())
}
