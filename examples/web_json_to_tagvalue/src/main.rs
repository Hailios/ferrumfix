//! Starts an HTTP server on any open port and listens for JSON FIX messages.

use fefix::backend::Backend;
use fefix::{
    json, tagvalue, tagvalue::slr, tagvalue::PushyMessage, AppVersion, Dictionary, Encoding,
};

#[tokio::main]
async fn main() -> tide::Result<()> {
    server().listen("127.0.0.1:8080").await?;
    Ok(())
}

fn server() -> tide::Server<State> {
    let state = State::new();
    let mut app = tide::with_state(state);
    app.at("/").get(serve_hello_world);
    app.at("/fix-json").post(serve_json_relay);
    app
}

/// [`State`] contains any global state necessary to serve web requests. In this
/// case, JSON (en/de)coding devices.
#[derive(Clone)]
struct State {
    codec: json::Codec<tagvalue::slr::Message, json::ConfigPrettyPrint>,
}

impl State {
    fn new() -> Self {
        Self::default()
    }
}

impl Default for State {
    fn default() -> Self {
        let dictionary = Dictionary::from_version(AppVersion::Fix42);
        Self {
            codec: json::Codec::new(dictionary, json::ConfigPrettyPrint),
        }
    }
}

async fn serve_hello_world(_req: tide::Request<State>) -> tide::Result {
    Ok("Hello, world!".to_string().into())
}

async fn serve_json_relay(mut req: tide::Request<State>) -> tide::Result {
    let mut decoder = req.state().codec.clone();
    let message = {
        let body: Vec<u8> = req.body_bytes().await?;
        decoder.decode(&body[..]).unwrap()
    };
    let mut buffer = Vec::new();
    let body_response = {
        let mut config = tagvalue::Configurable::default();
        config.set_separator(b'|');
        let mut encoder = tagvalue::Codec::<slr::Message>::with_dict(
            Dictionary::from_version(AppVersion::Fix42),
            config,
        );
        let msg = &mut PushyMessage::default();
        Backend::for_each::<(), _>(message, |tag, value| {
            msg.add_field(tag, value.clone());
            Ok(())
        })
        .unwrap();
        encoder.encode(&mut buffer, &msg).unwrap();
        let buffer_string = std::str::from_utf8(&buffer[..]).unwrap();
        buffer_string
    };
    Ok(body_response.into())
}

#[cfg(test)]
mod test {
    use super::*;
    use fefix::tagvalue::slr;
    use tide::http::{Method, Request, Response};

    /// A simple `Heartbeat` message generated by
    /// <http://www.validfix.com/fix-analyzer.html>.
    const EXAMPLE_JSON_MESSAGE: &str = r#"
{
    "Header": {
        "BeginString": "FIX.4.2",
        "MsgType": "0",
        "MsgSeqNum": "12",
        "SenderCompID": "A",
        "TargetCompID": "B",
        "SendingTime": "20160802-21:14:38.717"
    },
    "Body": {},
    "Trailer": {}
}
"#;

    #[tokio::test]
    async fn hello_world() {
        let server = server();
        let req = Request::new(Method::Get, "http://localhost:8080/");
        let mut response: Response = server.respond(req).await.unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(response.body_string().await.unwrap(), "Hello, world!");
    }

    #[tokio::test]
    async fn example_heartbeat() {
        let server = server();
        let body_json = EXAMPLE_JSON_MESSAGE;
        let mut req = Request::new(Method::Post, "http://localhost:8080/fix-json");
        req.set_body(body_json);
        let mut response: Response = server.respond(req).await.unwrap();
        let body_tagvalue = response.take_body().into_string().await.unwrap();
        let mut decoder_json = json::Codec::<slr::Message, json::ConfigPrettyPrint>::new(
            Dictionary::from_version(AppVersion::Fix42),
            json::ConfigPrettyPrint,
        );
        let mut config = tagvalue::Configurable::default();
        config.set_separator(b'|');
        let mut decoder_tagvalue = tagvalue::Codec::<slr::Message>::with_dict(
            Dictionary::from_version(AppVersion::Fix42),
            config,
        );
        let msg_json = decoder_json.decode(body_json.as_bytes()).unwrap();
        println!("{}", body_tagvalue);
        let msg_tagvalue = decoder_tagvalue.decode(body_tagvalue.as_bytes()).unwrap();
        assert_eq!(msg_json.get_field(8), msg_tagvalue.get_field(8));
        assert_eq!(msg_json.get_field(35), msg_tagvalue.get_field(35));
        assert_eq!(msg_json.get_field(49), msg_tagvalue.get_field(49));
        assert_eq!(msg_json.get_field(56), msg_tagvalue.get_field(56));
    }
}
