//! Built-in intent resolver strategies.
//!
//! Each strategy handles one or more intent types and generates concrete
//! AST nodes (functions, types) from intent declarations.

pub(crate) mod cache;
pub(crate) mod cli;
pub(crate) mod console;
pub(crate) mod database;
pub(crate) mod file_processor;
pub(crate) mod http;
pub(crate) mod http_server;
pub(crate) mod json_api;
pub(crate) mod math;
pub(crate) mod queue;
pub(crate) mod worker;

pub(crate) use cache::CacheStrategy;
pub(crate) use cli::CliStrategy;
pub(crate) use console::ConsoleAppStrategy;
pub(crate) use database::DatabaseStrategy;
pub(crate) use file_processor::FileProcessorStrategy;
pub(crate) use http::ServeHttpStrategy;
pub(crate) use http_server::HttpServerStrategy;
pub(crate) use json_api::JsonApiStrategy;
pub(crate) use math::MathModuleStrategy;
pub(crate) use queue::QueueStrategy;
pub(crate) use worker::WorkerStrategy;
