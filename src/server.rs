use std::collections::HashMap;

use anyhow::anyhow;
use lsp_server::{Connection, Message, Response};
use lsp_types::{
    notification::{DidChangeTextDocument, DidOpenTextDocument, Notification, PublishDiagnostics},
    request::{Formatting, Request},
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams, OneOf,
    Position, PublishDiagnosticsParams, Range, ServerCapabilities, TextDocumentSyncKind, TextEdit,
    Url,
};
use tan::api::parse_string_all;
use tan_formatting::pretty::Formatter;
use tan_lints::compute_diagnostics;
use tracing::{info, trace};

use crate::util::{dialect_from_document_uri, send_server_status_notification, VERSION};

pub struct Server {
    documents: HashMap<String, String>,
}

// #todo split further into methods.

impl Server {
    pub fn new() -> Self {
        Self {
            documents: HashMap::default(),
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        info!("Starting LSP server, v{}...", VERSION);

        let (connection, io_threads) = Connection::stdio();

        let server_capabilities = serde_json::to_value(ServerCapabilities {
            // definition_provider: Some(OneOf::Left(true)),
            // references_provider: Some(OneOf::Left(true)),
            // #Insight Enables didOpen/didChange notifications.
            text_document_sync: Some(lsp_types::TextDocumentSyncCapability::Kind(
                TextDocumentSyncKind::FULL,
            )),
            rename_provider: Some(OneOf::Left(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            ..Default::default()
        })
        .unwrap();

        let initialization_params = connection.initialize(server_capabilities)?;

        info!("Started.");
        send_server_status_notification(&connection, "started")?;

        // Run the server.
        self.run_loop(connection, initialization_params)?;

        // Wait for the two threads to end (typically by trigger LSP Exit event).
        io_threads.join()?;

        info!("Shutting down server...");

        Ok(())
    }

    // #todo return a more precise result.
    pub fn send_diagnostics(&self, connection: &Connection, uri: Url) -> anyhow::Result<()> {
        let Some(input) = self.documents.get(uri.as_str()) else {
            return Err(anyhow!("Unknown document").context("in send_diagnostics"));
        };

        let diagnostics = compute_diagnostics(input);

        let pdm = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics,
            version: None,
        };

        let notification = lsp_server::Notification {
            method: PublishDiagnostics::METHOD.to_owned(),
            params: serde_json::to_value(pdm).unwrap(),
        };

        connection
            .sender
            .send(Message::Notification(notification))?;

        Ok(())
    }

    pub fn run_loop(
        &mut self,
        connection: Connection,
        _params: serde_json::Value,
    ) -> anyhow::Result<()> {
        // #todo use params to get root_uri and perform initial diagnostics for all files.
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
                            send_server_status_notification(&connection, "formatting")?;

                            let (id, params) =
                                req.extract::<DocumentFormattingParams>(Formatting::METHOD)?;

                            let document = params.text_document;

                            let Some(input) = self.documents.get(document.uri.as_str()) else {
                                return Err(
                                    anyhow!("Unknown document").context("in Formatting::METHOD")
                                );
                            };

                            // #todo don't parse all the time? is this even possible, probably not the input changed here.

                            let Ok(exprs) = parse_string_all(input) else {
                                return Err(anyhow::anyhow!("Error"));
                            };

                            let dialect = dialect_from_document_uri(document.uri.as_str());

                            let formatter = Formatter::for_dialect(&exprs, dialect);
                            let formatted = formatter.format();

                            // #todo does it make sense to compute diffs?

                            // Select the whole document for replacement
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

                            send_server_status_notification(&connection, "formatted")?;

                            connection.sender.send(Message::Response(resp))?;

                            continue;
                        }
                        _ => continue,
                    }
                }
                Message::Response(resp) => {
                    trace!("Got response: {:?}.", resp);
                }
                Message::Notification(notification) => {
                    info!("got notification: {:?}.", notification);

                    match notification.method.as_ref() {
                        "textDocument/didOpen" => {
                            if let Ok(params) = notification
                                .extract::<DidOpenTextDocumentParams>(DidOpenTextDocument::METHOD)
                            {
                                let document = params.text_document;
                                self.documents
                                    .insert(document.uri.to_string(), document.text);
                                self.send_diagnostics(&connection, document.uri)?;
                            }
                        }
                        "textDocument/didChange" => {
                            if let Ok(params) = notification.extract::<DidChangeTextDocumentParams>(
                                DidChangeTextDocument::METHOD,
                            ) {
                                let document = params.text_document;
                                let changes = params.content_changes;
                                if let Some(change) = changes.first() {
                                    self.documents
                                        .insert(document.uri.to_string(), change.text.clone());
                                    self.send_diagnostics(&connection, document.uri)?;
                                }
                            }
                        }
                        _ => {
                            eprintln!("Unhandled: {}", notification.method);
                        }
                    }

                    // if let Ok(event) =
                    //     &event.extract::<DidChangeTextDocumentParams>(DidChangeTextDocument::METHOD)
                    // {
                    //     for change in event.content_changes.into_iter() {
                    //         dbg!(change.text);
                    //     }
                    // }

                    // #todo try to switch to incremental sync.
                }
            }
        }
        Ok(())
    }
}
