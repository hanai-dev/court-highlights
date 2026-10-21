//! Finding cameras on the venue network (FR-30, FR-40): a WS-Discovery probe lists ONVIF cameras; with the
//! camera's username and password the box asks the camera for its stream address (ONVIF Media GetStreamUri), so
//! the Owner never has to type an RTSP path.

use base64::Engine;
use sha1::{Digest, Sha1};
use std::time::Duration;

const MULTICAST: &str = "239.255.255.250:3702";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Found {
    /// The camera's ONVIF device service, e.g. http://192.168.1.21/onvif/device_service
    pub xaddr: String,
    pub host: String,
    pub name: String,
}

/// Text between `<…:tag>` and the next `</` (ONVIF answers are small; no XML library needed).
fn tag<'a>(xml: &'a str, tag: &str) -> Option<&'a str> {
    let start = xml.find(&format!(":{tag}>")).or_else(|| xml.find(&format!("<{tag}>")))?;
    let rest = &xml[start..];
    let open_end = rest.find('>')? + 1;
    let body = &rest[open_end..];
    Some(body[..body.find("</")?].trim())
}

fn uuid() -> String {
    let mut b = [0u8; 16];
    getrandom::fill(&mut b).expect("OS random source");
    let h = crate::auth::hex(&b);
    format!("{}-{}-{}-{}-{}", &h[0..8], &h[8..12], &h[12..16], &h[16..20], &h[20..32])
}

fn parse_match(xml: &str) -> Option<Found> {
    let xaddr = tag(xml, "XAddrs")?.split_whitespace().find(|a| a.starts_with("http://") || a.starts_with("https://"))?.to_string();
    let host = url::Url::parse(&xaddr).ok()?.host_str()?.to_string();
    let scopes = tag(xml, "Scopes").unwrap_or_default();
    let scope = |key: &str| scopes.split_whitespace().find_map(|s| s.split_once(&format!("onvif.org/{key}/")).map(|(_, v)| v.replace("%20", " ")));
    let name = [scope("name"), scope("hardware")].into_iter().flatten().collect::<Vec<_>>().join(" · ");
    Some(Found { xaddr, host, name: if name.is_empty() { "Camera".into() } else { name } })
}

/// Probes the local network for up to `wait` and returns each camera once.
pub async fn probe(wait: Duration) -> Vec<Found> {
    let probe = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><e:Envelope xmlns:e="http://www.w3.org/2003/05/soap-envelope" xmlns:w="http://schemas.xmlsoap.org/ws/2004/08/addressing" xmlns:d="http://schemas.xmlsoap.org/ws/2005/04/discovery" xmlns:dn="http://www.onvif.org/ver10/network/wsdl"><e:Header><w:MessageID>uuid:{}</w:MessageID><w:To e:mustUnderstand="true">urn:schemas-xmlsoap-org:ws:2005:04:discovery</w:To><w:Action e:mustUnderstand="true">http://schemas.xmlsoap.org/ws/2005/04/discovery/Probe</w:Action></e:Header><e:Body><d:Probe><d:Types>dn:NetworkVideoTransmitter</d:Types></d:Probe></e:Body></e:Envelope>"#,
        uuid()
    );
    let Ok(socket) = tokio::net::UdpSocket::bind("0.0.0.0:0").await else { return Vec::new() };
    if let Err(e) = socket.send_to(probe.as_bytes(), MULTICAST).await {
        tracing::warn!(error = %e, "camera discovery probe not sent");
        return Vec::new();
    }
    let mut found: Vec<Found> = Vec::new();
    let deadline = tokio::time::Instant::now() + wait;
    let mut buf = vec![0u8; 65_536];
    while let Ok(Ok((n, _))) = tokio::time::timeout_at(deadline, socket.recv_from(&mut buf)).await {
        if let Some(f) = std::str::from_utf8(&buf[..n]).ok().and_then(parse_match)
            && !found.iter().any(|x| x.xaddr == f.xaddr)
        {
            found.push(f);
        }
    }
    found
}

/// WS-Security UsernameToken with a password digest (ONVIF Core §5.12).
fn security(username: &str, password: &str) -> String {
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).expect("OS random source");
    let created = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let digest = Sha1::new().chain_update(nonce).chain_update(created.as_bytes()).chain_update(password.as_bytes()).finalize();
    let b64 = base64::engine::general_purpose::STANDARD;
    format!(
        r#"<s:Header><Security s:mustUnderstand="1" xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-secext-1.0.xsd"><UsernameToken><Username>{}</Username><Password Type="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-username-token-profile-1.0#PasswordDigest">{}</Password><Nonce EncodingType="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-soap-message-security-1.0#Base64Binary">{}</Nonce><Created xmlns="http://docs.oasis-open.org/wss/2004/01/oasis-200401-wss-wssecurity-utility-1.0.xsd">{}</Created></UsernameToken></Security></s:Header>"#,
        xml_escape(username),
        b64.encode(digest),
        b64.encode(nonce),
        created
    )
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

async fn soap(http: &reqwest::Client, url: &str, username: &str, password: &str, body: &str) -> Result<String, String> {
    let envelope = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?><s:Envelope xmlns:s="http://www.w3.org/2003/05/soap-envelope" xmlns:tds="http://www.onvif.org/ver10/device/wsdl" xmlns:trt="http://www.onvif.org/ver10/media/wsdl" xmlns:tt="http://www.onvif.org/ver10/schema">{}<s:Body>{body}</s:Body></s:Envelope>"#,
        security(username, password)
    );
    let res = http
        .post(url)
        .header("content-type", "application/soap+xml; charset=utf-8")
        .body(envelope)
        .send()
        .await
        .map_err(|_| "The camera didn't answer. Check it is on the same network as the box.".to_string())?;
    let status = res.status();
    let text = res.text().await.map_err(|e| e.to_string())?;
    if status == reqwest::StatusCode::UNAUTHORIZED || text.contains("NotAuthorized") {
        return Err("The camera refused that username or password.".into());
    }
    if !status.is_success() {
        return Err(format!("The camera answered with an error ({status})."));
    }
    Ok(text)
}

/// The camera's main stream address: Media service from GetCapabilities, first profile, its RTSP stream.
pub async fn stream_uri(xaddr: &str, username: &str, password: &str) -> Result<String, String> {
    let http = reqwest::Client::builder().timeout(Duration::from_secs(8)).build().map_err(|e| e.to_string())?;
    let caps = soap(&http, xaddr, username, password, "<tds:GetCapabilities><tds:Category>Media</tds:Category></tds:GetCapabilities>").await?;
    let media = caps.find(":Media>").and_then(|i| tag(&caps[i..], "XAddr")).unwrap_or(xaddr).to_string();
    let profiles = soap(&http, &media, username, password, "<trt:GetProfiles/>").await?;
    let token = profiles.split("token=\"").nth(1).and_then(|t| t.split('"').next()).ok_or("The camera has no video profile.")?.to_string();
    let uri = soap(
        &http,
        &media,
        username,
        password,
        &format!(
            "<trt:GetStreamUri><trt:StreamSetup><tt:Stream>RTP-Unicast</tt:Stream><tt:Transport><tt:Protocol>RTSP</tt:Protocol></tt:Transport></trt:StreamSetup><trt:ProfileToken>{}</trt:ProfileToken></trt:GetStreamUri>",
            xml_escape(&token)
        ),
    )
    .await?;
    let found = tag(&uri, "Uri").ok_or("The camera didn't give a stream address.")?.replace("&amp;", "&");
    if !found.starts_with("rtsp://") {
        return Err("The camera's stream address isn't RTSP.".into());
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_matches_give_address_and_name() {
        let xml = r#"<SOAP-ENV:Envelope><SOAP-ENV:Body><d:ProbeMatches><d:ProbeMatch><d:Types>dn:NetworkVideoTransmitter</d:Types>
            <d:Scopes>onvif://www.onvif.org/type/video_encoder onvif://www.onvif.org/name/Court%20Cam onvif://www.onvif.org/hardware/IPC-B140</d:Scopes>
            <d:XAddrs>http://192.168.1.21/onvif/device_service http://[fe80::1]/onvif/device_service</d:XAddrs></d:ProbeMatch></d:ProbeMatches></SOAP-ENV:Body></SOAP-ENV:Envelope>"#;
        let f = parse_match(xml).unwrap();
        assert_eq!(f.xaddr, "http://192.168.1.21/onvif/device_service");
        assert_eq!(f.host, "192.168.1.21");
        assert_eq!(f.name, "Court Cam · IPC-B140");
        assert_eq!(parse_match("<x/>"), None);
    }

    #[test]
    fn stream_uri_answers_are_read_and_unescaped() {
        let xml = "<trt:GetStreamUriResponse><trt:MediaUri><tt:Uri>rtsp://10.0.0.5:554/stream1?a=1&amp;b=2</tt:Uri></trt:MediaUri></trt:GetStreamUriResponse>";
        assert_eq!(tag(xml, "Uri").unwrap().replace("&amp;", "&"), "rtsp://10.0.0.5:554/stream1?a=1&b=2");
    }
}
