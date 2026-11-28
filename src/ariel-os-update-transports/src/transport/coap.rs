use core::net::SocketAddr;

use ariel_os_log::error;
use coap_message_utils::OptionsExt as _;
use coap_message_utils::option_value::{Block2RequestData, Block2ResponseData};
use coap_request::Stack;
use coap_request_implementations::AsUriPath as _;
use embedded_nal_async::Dns;
use embedded_nal_coap::RequestingCoAPClient;

use nourl::UrlScheme;

use crate::Error;

const CONCURRENT_REQUESTS: usize = 3;

pub struct NetworkTransportClient<'uri, 'buf, const N: usize> {
    pub(crate) client: RequestingCoAPClient<'static, CONCURRENT_REQUESTS>,
    buf: &'buf mut [u8; N],
    state: SuitState<'uri>,
}

/// Create a new CoAP client to send requests to a given uri.
///
/// # Errors
///
/// * If the URI parsing fails.
/// * If the DNS resolution fails.
pub(crate) async fn new_client<'uri, 'buf, DNS: Dns, const N: usize>(
    dns_client: &DNS,
    uri: &'uri str,
    buf: &'buf mut [u8; N],
) -> Result<NetworkTransportClient<'uri, 'buf, N>, Error> {
    let Ok(uri) = nourl::Url::parse(uri) else {
        return Err(Error::InvalidUri);
    };

    match uri.scheme() {
        UrlScheme::CoAP => {}
        _ => return Err(Error::InvalidUri),
    }

    let peer_socket_addr = if let Some(socket_addr) = uri.host_socket_address() {
        socket_addr
    } else {
        // TODO: might want to restrict to one type of IP addresses.
        if let Ok(ip_addr) = dns_client
            .get_host_by_name(uri.host(), embedded_nal_async::AddrType::Either)
            .await
        {
            SocketAddr::new(ip_addr, uri.port_or_default())
        } else {
            // TODO: could introduce an error for DNS.
            return Err(Error::Transport);
        }
    };

    let client = ariel_os_coap::coap_client().await;
    let client = client.to(peer_socket_addr);

    ariel_os_log::trace!("Addr: {}, path: {}", &peer_socket_addr, uri.path());

    let buffer_size = buf.len();

    Ok(NetworkTransportClient {
        client,
        buf,
        state: SuitState::new(uri.path(), buffer_size),
    })
}

impl<const N: usize> NetworkTransportClient<'_, '_, N> {
    /// Fetch and return a single chunk of data from the URI.
    ///
    /// # Errors
    ///
    /// * If the request fails or returns garbage data.
    pub async fn get(&mut self) -> Result<(&[u8], bool), Error> {
        let Some((has_more, coap_chunk)) = self
            .client
            .request(SuitPayloadRequest {
                buf: Some(self.buf),
                state: &mut self.state,
            })
            .await
            .map_err(|_| Error::Transport)?
        else {
            return Err(Error::Transport);
        };

        Ok((coap_chunk, has_more))
    }
}

// Inspired by
// <https://github.com/bergzand/Ariel-os/blob/0a44aaa8178083588f82b3f4187051ab4ec75365/examples/rustweek-demo/src/suit.rs#L41-L102>.

#[derive(Debug)]
struct SuitState<'uri> {
    path: &'uri str,
    // Next block2 request option to send
    request_block: Block2RequestData,
}

#[derive(Debug)]
struct SuitPayloadRequest<'buf, 'state, 'uri, const N: usize> {
    // This is an Option because processing the response takes a &mut Self, but the result should
    // be a 'buf-lived buffer.
    buf: Option<&'buf mut [u8; N]>,
    state: &'state mut SuitState<'uri>,
}

impl<'uri> SuitState<'uri> {
    fn new(path: &'uri str, buffer_size: usize) -> Self {
        Self {
            request_block: Block2RequestData::new_initial(buffer_size),
            path,
        }
    }
}

impl<'buf, const N: usize> coap_request::Request<RequestingCoAPClient<'static, CONCURRENT_REQUESTS>>
    for SuitPayloadRequest<'buf, '_, '_, N>
{
    type Output = Option<(bool, &'buf [u8])>;

    type Carry = ();

    fn build_request(
        &mut self,
        request: &mut <RequestingCoAPClient<'static, CONCURRENT_REQUESTS> as Stack>::RequestMessage<
            '_,
        >,
    ) -> impl Future<
        Output = Result<
            Self::Carry,
            <RequestingCoAPClient<'static, CONCURRENT_REQUESTS> as Stack>::RequestUnionError,
        >,
    > {
        use coap_message::MinimalWritableMessage;
        use coap_message::OptionNumber as _;

        type Request<'a> =
            <RequestingCoAPClient<'static, CONCURRENT_REQUESTS> as Stack>::RequestMessage<'a>;
        type OptionNum<'a> = <Request<'a> as MinimalWritableMessage>::OptionNumber;

        request.set_code(coap_numbers::code::GET);
        for part in self.state.path.as_uri_path() {
            if let Err(e) = request.add_option(
                OptionNum::new(coap_numbers::option::URI_PATH).unwrap(),
                part.as_bytes(),
            ) {
                return core::future::ready(Err(e));
            }
        }
        if let Err(e) = request.add_option_uint(
            OptionNum::new(coap_numbers::option::BLOCK2).unwrap(),
            self.state.request_block.to_option_value(false),
        ) {
            return core::future::ready(Err(e));
        }
        core::future::ready(Ok(()))
    }

    fn process_response(
        &mut self,
        response: &<RequestingCoAPClient<'static, CONCURRENT_REQUESTS> as Stack>::ResponseMessage<
            '_,
        >,
        _carry: Self::Carry,
    ) -> impl Future<Output = Self::Output> {
        use coap_message::ReadableMessage as _;

        let code: u8 = response.code().into();
        if code != coap_numbers::code::CONTENT {
            error!("Unexpected CoAP response code: {}", code);
            return core::future::ready(None);
        }

        let mut block2: Option<Block2ResponseData> = None;
        if response
            .options()
            .take_into(&mut block2)
            .ignore_elective_others()
            .is_err()
        {
            error!("Unprocessed critical options in response");
            return core::future::ready(None);
        }

        let Some(block2) = block2 else {
            error!("No block number in response");
            return core::future::ready(None);
        };

        if !block2.matches_request(&self.state.request_block, response.payload()) {
            error!(
                "Unexpected block {:?} (payload length {}) after requesting {:?}",
                block2,
                response.payload().len(),
                self.state.request_block
            );
            return core::future::ready(None);
        }

        let payload = response.payload();

        // Violation of block-wise transfer rules by the server
        if payload.len() > N {
            error!("Unexpected payload of size {}", payload.len());
            return core::future::ready(None);
        }
        let buffer = self.buf.take().expect("response gets processed only once");
        buffer[..payload.len()].copy_from_slice(payload);
        let out = &buffer[..payload.len()];
        let next_block = block2.next_request();
        let Ok(next_block) = next_block else {
            error!("Firmware size exceeds what can be expressed at this block size");
            return core::future::ready(None);
        };
        if let Some(next_block) = next_block {
            self.state.request_block = next_block;
            core::future::ready(Some((true, out)))
        } else {
            // Not updating request_block, but then again, we won't be called any more anyway.
            core::future::ready(Some((false, out)))
        }
    }
}
