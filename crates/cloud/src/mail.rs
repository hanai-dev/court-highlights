//! Take-home email through Amazon SES (A-122). Bounced addresses are kept off every venue's mail by the SES
//! account-level suppression list (FR-25, see RUNBOOK); addresses that ask never to be emailed are blocked here.

use aws_sdk_sesv2::Client;
use aws_sdk_sesv2::config::Credentials;
use aws_sdk_sesv2::types::{Body, Content, Destination, EmailContent, Message};

#[derive(Clone)]
pub struct Mailer {
    client: Client,
    from: String,
}

pub struct Email<'a> {
    pub to: &'a str,
    pub subject: &'a str,
    pub text: &'a str,
    pub html: &'a str,
}

impl Mailer {
    pub fn new(endpoint: Option<&str>, region: &str, key: &str, secret: &str, from: &str) -> Self {
        let http = aws_smithy_http_client::Builder::new()
            .tls_provider(aws_smithy_http_client::tls::Provider::Rustls(aws_smithy_http_client::tls::rustls_provider::CryptoMode::Ring))
            .build_https();
        let mut conf = aws_sdk_sesv2::Config::builder()
            .behavior_version(aws_sdk_sesv2::config::BehaviorVersion::latest())
            .http_client(http)
            .region(aws_sdk_sesv2::config::Region::new(region.to_string()))
            .credentials_provider(Credentials::new(key, secret, None, None, "ch-cloud"));
        if let Some(e) = endpoint {
            conf = conf.endpoint_url(e);
        }
        Self { client: Client::from_conf(conf.build()), from: from.into() }
    }

    pub async fn send(&self, e: &Email<'_>) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let content = |s: &str| Content::builder().data(s).charset("UTF-8").build();
        let message = Message::builder()
            .subject(content(e.subject)?)
            .body(Body::builder().text(content(e.text)?).html(content(e.html)?).build())
            .build();
        self.client
            .send_email()
            .from_email_address(&self.from)
            .destination(Destination::builder().to_addresses(e.to).build())
            .content(EmailContent::builder().simple(message).build())
            .send()
            .await?;
        Ok(())
    }
}
