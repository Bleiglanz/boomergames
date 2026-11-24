//! External I/O adapters implementing server ports (HTTP, WebSocket, PostgreSQL, SMTP, Google, Memory).

pub mod db;
pub mod google;
pub mod http;
pub mod mail;
pub mod memory;
pub mod ws;
