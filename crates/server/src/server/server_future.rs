use std::net::SocketAddr;
use std::sync::Arc;
use crate::server::request_handler::RequestHandler;
use crate::server::response_handler::ResponseHandler;
use crate::server::Protocol;

pub struct Http3Handler<T: RequestHandler> {
    handler: Arc<T>,
}

impl<T: RequestHandler> Http3Handler<T> {
    pub fn new(handler: Arc<T>) -> Self {
        Self { handler }
    }

    pub async fn handle(&self, src_addr: SocketAddr) {
        // TODO: Implement the logic for handling HTTP3 connections.
        // This will involve reading from the connection, processing the request, and sending the response.
    }
}

