use std::collections::HashMap;

use anyhow::anyhow;
use lsp_server::{Connection, Message, Response};
use lsp_types::{
    notification::{DidChangeTextDocument, DidOpenTextDocument, Notification, PublishDiagnostics},
    request::{DocumentSymbolRequest, Formatting, Request},
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, DocumentFormattingParams,
    DocumentSymbolParams, DocumentSymbolResponse, Location, OneOf, Position,
    PublishDiagnosticsParams, Range, ServerCapabilities, SymbolInformation, SymbolKind,
    TextDocumentSyncKind, TextEdit, Uri,
};
use tan::{error::Error, expr::Expr};
use tan_formatting::pretty::Formatter;
use tan_lints::compute_diagnostics;
use tracing::{info, trace};

use crate::util::{
    dialect_from_document_uri, lsp_range_from_tan_range, lsp_range_top, make_analysis_context,
    parse_module_file, parse_string_all, send_server_status_notification, VERSION,
};

// #insight
// For debugging use trace! and similar functions, the traces are logged in the
// `Tan Language` tab of the Output panel, in VS Code.

pub struct Server {
    documents: HashMap<String, String>,
    parsed_documents: HashMap<String, Result<Vec<Expr>, Vec<Error>>>,
    // #todo also cache 'parsed/compiled' documents -> partial modules.
}

// #todo split further into methods.

impl Server {
    pub fn new() -> Self {
        Self {
            documents: HashMap::default(),
            parsed_documents: HashMap::default(),
        }
    }

    pub fn run(&mut self) -> anyhow::Result<()> {
        info!("Starting LSP server, v{}...", VERSION);

        let (connection, io_threads) = Connection::stdio();

        let server_capabilities = serde_json::to_value(ServerCapabilities {
            // definition_provider: Some(OneOf::Left(true)),
            // references_provider: Some(OneOf::Left(true)),
            // #insight Enables didOpen/didChange notifications.
            document_symbol_provider: Some(OneOf::Left(true)),
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

    // #todo find a good name.
    pub fn process_document(&mut self, uri: &Uri, text: &str) {
        let input = text.to_string();
        let uri = uri.to_string();
        let exprs = parse_string_all(&input);
        self.parsed_documents.insert(uri.clone(), exprs);
        self.documents.insert(uri, input);
    }

    // #todo return a more precise result.
    pub fn send_diagnostics(&self, connection: &Connection, uri: Uri) -> anyhow::Result<()> {
        let Some(parse_result) = self.parsed_documents.get(uri.as_str()) else {
            return Err(anyhow!("invalid document").context("in send_diagnostics"));
        };

        let diagnostics = compute_diagnostics(parse_result);

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

        // #insight we cache the analysis context out of the loop.
        let mut analysis_context = make_analysis_context().unwrap();

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

                    // #todo also handle "textDocument/hover".

                    match req.method.as_ref() {
                        // "textDocument/documentSymbol"
                        DocumentSymbolRequest::METHOD => {
                            // #todo imports are problematic, tan function have wrong ranges, foreign functions have no ranges
                            // #todo override range with the module-uri range
                            // #todo what about the signatures? leave as is and fianlize the real signature, or even nest them.

                            let (id, params) =
                                req.extract::<DocumentSymbolParams>(DocumentSymbolRequest::METHOD)?;
                            // #todo Flat (SymbolInformation) vs Nested (DocumentSymbol)

                            // #insight, actually Flat works just fine, Nested is too noisy.

                            // #todo for some reason, the Nested form was not working! investigate.
                            // #todo maybe we need to populate `children`?
                            // #[allow(deprecated)]
                            // let _ds = DocumentSymbol {
                            //     name: String::from("dummy"),
                            //     detail: None,
                            //     kind: SymbolKind::FUNCTION,
                            //     tags: None,
                            //     deprecated: None,
                            //     range,
                            //     selection_range: range,
                            //     children: None,
                            // };

                            // #todo cache the parsing between documentSymbol, formatting, linting etc!

                            let Some(Ok(exprs)) =
                                self.parsed_documents.get(params.text_document.uri.as_str())
                            else {
                                // #todo what should be done here?
                                trace!("!!!!! should NOT happen?");
                                continue;
                            };

                            let Ok(scope) = parse_module_file(exprs, &mut analysis_context) else {
                                // #todo what to do here?
                                continue;
                            };
                            let bindings = scope.bindings.read().expect("not poisoned");

                            let mut infos: Vec<SymbolInformation> = Vec::new();

                            // #insight VS Code automatically sorts the documentSymbols in source/range order.

                            for (name, expr) in bindings.iter() {
                                let range = if let Some(tan_range) = expr.range() {
                                    lsp_range_from_tan_range(tan_range)
                                } else {
                                    // #todo is this clause really needed?
                                    lsp_range_top()
                                };

                                let location = Location {
                                    uri: params.text_document.uri.clone(),
                                    range,
                                };

                                let Expr::Type(typ) = expr.dyn_type(&analysis_context) else {
                                    // #todo should never happen.
                                    // #todo dyn_type has not a convenient interface here.
                                    panic!("cannot infere dynamic type");
                                };

                                let kind = match typ.as_str() {
                                    // #todo add more variants here!
                                    "Func" => SymbolKind::FUNCTION,
                                    _ => SymbolKind::VARIABLE,
                                };

                                // #insight VS Code Outline automatically sorts by range.
                                // #insight if a symbol range includes other symbol ranges, VS Code automatically nests.

                                #[allow(deprecated)]
                                infos.push(SymbolInformation {
                                    name: name.clone(),
                                    kind,
                                    tags: None,
                                    deprecated: None,
                                    location: location.clone(),
                                    container_name: None,
                                });
                            }

                            // #todo maybe it needs children array populated?
                            // let result = DocumentSymbolResponse::Nested(vec![ds]);

                            let result = DocumentSymbolResponse::Flat(infos);
                            let result =
                                serde_json::to_value::<DocumentSymbolResponse>(result).unwrap();
                            let resp = Response {
                                id,
                                result: Some(result),
                                error: None,
                            };
                            connection.sender.send(Message::Response(resp))?;
                        }
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

                            // #todo extract as helper!
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
                                self.process_document(&document.uri, &document.text);
                                self.send_diagnostics(&connection, document.uri)?;
                            }
                        }
                        "textDocument/didChange" => {
                            // #todo #perf support incremental updates for formatting, documentSymbols, etc...
                            if let Ok(params) = notification.extract::<DidChangeTextDocumentParams>(
                                DidChangeTextDocument::METHOD,
                            ) {
                                let document = params.text_document;
                                let changes = params.content_changes;
                                if let Some(change) = changes.first() {
                                    self.process_document(&document.uri, &change.text);
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
