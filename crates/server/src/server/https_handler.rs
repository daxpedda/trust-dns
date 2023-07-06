use std::{io, net::SocketAddr, sync::Arc};

use bytes::{Bytes, BytesMut};
use futures_util::lock::Mutex;
use h3::server;
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, warn};
use trust_dns_proto::rr::Record;

use crate::{
    authority::MessageResponse,
    proto::http3::http3_server,
    server::{
        request_handler::RequestHandler, response_handler::ResponseHandler, server_future,
        Protocol, ResponseInfo,
    },
};

pub(crate) async fn http3_handler<T, I>(
    handler: Arc<T>,
    io: I,
    src_addr: SocketAddr,
    dns_hostname: Option<Arc<str>>,
) where
    T: RequestHandler,
    I: AsyncRead + AsyncWrite + Unpin,
{
    let dns_hostname = dns_hostname.clone();

    // Start the HTTP3 connection handshake
    let mut h3 = match server::handshake(io).await {
        Ok(h3) => h3,
        Err(err) => {
            warn!("handshake error from {}: {}", src_addr, err);
            return;
        }
    };

    // Accept all inbound HTTP3 streams sent over the connection.
    while let Some(next_request) = h3.accept().await {
        let (request, respond) = match next_request {
            Ok(next_request) => next_request,
            Err(err) => {
                warn!("error accepting request {}: {}", src_addr, err);
                return;
            }
        };

        debug!("Received request: {:#?}", request);
        let dns_hostname = dns_hostname.clone();
        let handler = handler.clone();
        let responder = Http3ResponseHandle(Arc::new(Mutex::new(respond)));

        match http3_server::message_from(dns_hostname, request).await {
            Ok(bytes) => handle_request(bytes, src_addr, handler, responder).await,
            Err(err) => warn!("error while handling request from {}: {}", src_addr, err),
        };

        // we'll continue handling requests from here.
    }
}

async fn handle_request<T>(
    bytes: BytesMut,
    src_addr: SocketAddr,
    handler: Arc<T>,
    responder: Http3ResponseHandle,
) where
    T: RequestHandler,
{
    server_future::handle_request(&bytes, src_addr, Protocol::Http3, handler, responder).await
}

// Rest of the code...

#[derive(Clone)]
struct HttpsResponseHandle(Arc<Mutex<::h2::server::SendResponse<Bytes>>>);

#[async_trait::async_trait]
impl ResponseHandler for HttpsResponseHandle {
    async fn send_response<'a>(
        &mut self,
        response: MessageResponse<
            '_,
            'a,
            impl Iterator<Item = &'a Record> + Send + 'a,
            impl Iterator<Item = &'a Record> + Send + 'a,
            impl Iterator<Item = &'a Record> + Send + 'a,
            impl Iterator<Item = &'a Record> + Send + 'a,
        >,
    ) -> io::Result<ResponseInfo> {
        use crate::proto::https::response;
        use crate::proto::https::HttpsError;
        use crate::proto::serialize::binary::BinEncoder;

        let mut bytes = Vec::with_capacity(512);
        // mut block
        let info = {
            let mut encoder = BinEncoder::new(&mut bytes);
            response.destructive_emit(&mut encoder)?
        };
        let bytes = Bytes::from(bytes);
        let response = response::new(bytes.len())?;

        debug!("sending response: {:#?}", response);
        let mut stream = self
            .0
            .lock()
            .await
            .send_response(response, false)
            .map_err(HttpsError::from)?;
        stream.send_data(bytes, true).map_err(HttpsError::from)?;

        Ok(info)
    }
}

