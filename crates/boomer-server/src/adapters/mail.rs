//! SMTP (`lettre`, port 587 STARTTLS) and local-dev logging adapters for [`MailSender`].

use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use tracing::info;

use crate::config::SmtpConfig;
use crate::ports::{BoxFuture, MailSender, PortError};

/// Production SMTP magic-link sender using `lettre` over STARTTLS (port 587).
pub struct LettreMailSender {
    host: String,
    from: Mailbox,
    transport: AsyncSmtpTransport<Tokio1Executor>,
}

impl LettreMailSender {
    /// Builds an SMTP transport from [`SmtpConfig`].
    ///
    /// # Errors
    /// Returns [`PortError::Invalid`] if the sender address or SMTP config is invalid.
    pub fn new(cfg: &SmtpConfig) -> Result<Self, PortError> {
        let from: Mailbox = cfg
            .from
            .parse()
            .map_err(|err| PortError::Invalid(format!("invalid SMTP_FROM: {err}")))?;

        let mut builder = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&cfg.host)
            .map_err(|err| PortError::Invalid(format!("invalid SMTP_HOST: {err}")))?
            .port(cfg.port);

        if !cfg.username.is_empty() {
            builder =
                builder.credentials(Credentials::new(cfg.username.clone(), cfg.password.clone()));
        }

        Ok(Self {
            host: cfg.host.clone(),
            from,
            transport: builder.build(),
        })
    }
}

impl MailSender for LettreMailSender {
    fn send_magic_link(
        &self,
        to_email: &str,
        login_url: &str,
    ) -> BoxFuture<'_, Result<(), PortError>> {
        let to_email = to_email.to_owned();
        let login_url = login_url.to_owned();
        let host = self.host.clone();
        Box::pin(async move {
            let to: Mailbox = to_email
                .parse()
                .map_err(|err| PortError::Invalid(format!("invalid recipient email: {err}")))?;

            let email = Message::builder()
                .from(self.from.clone())
                .to(to)
                .subject("Your Boomergames sign-in link")
                .body(format!(
                    "Welcome to Boomergames!\n\n\
                     Click this link within 15 minutes to sign in:\n\n\
                     {login_url}\n\n\
                     If you did not request this email, you can safely ignore it.\n"
                ))
                .map_err(|err| PortError::Unavailable(format!("failed to build email: {err}")))?;

            self.transport
                .send(email)
                .await
                .map_err(|err| PortError::Unavailable(format!("SMTP send failed: {err}")))?;

            let recipient_domain = to_email
                .rsplit_once('@')
                .map_or("unknown", |(_, domain)| domain);
            info!(
                smtp_host = %host,
                recipient_domain = %recipient_domain,
                "dispatched magic-link email"
            );
            Ok(())
        })
    }
}

/// Development fallback logging magic links via `tracing::info!` when SMTP is not configured.
#[derive(Debug, Default)]
pub struct LogMailSender;

impl MailSender for LogMailSender {
    fn send_magic_link(
        &self,
        to_email: &str,
        login_url: &str,
    ) -> BoxFuture<'_, Result<(), PortError>> {
        let to = to_email.to_owned();
        let url = login_url.to_owned();
        Box::pin(async move {
            info!(to = %to, login_url = %url, "dev magic-link email");
            Ok(())
        })
    }
}
