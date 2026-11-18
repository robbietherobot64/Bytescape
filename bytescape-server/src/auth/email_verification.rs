use chrono::{Duration, Utc};
use lettre::{
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    message::header::ContentType,
};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::AppError;

/// Generate a 6-digit verification code and store it in the database.
pub async fn generate_code(pool: &PgPool, email: &str) -> Result<String, AppError> {
    // Invalidate any previous codes for this email
    sqlx::query("DELETE FROM email_verifications WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await?;

    let code: String = (0..6)
        .map(|_| {
            let digit: u8 = rand::random::<u8>() % 10;
            (digit + b'0') as char
        })
        .collect();

    let expires_at = Utc::now() + Duration::minutes(10);

    sqlx::query(
        r#"
        INSERT INTO email_verifications (id, email, code, expires_at)
        VALUES ($1, $2, $3, $4)
        "#,
    )
    .bind(Uuid::new_v4())
    .bind(email)
    .bind(&code)
    .bind(expires_at)
    .execute(pool)
    .await?;

    Ok(code)
}

/// Verify a 6-digit code for the given email.
/// On success the verification record is deleted. On 5th failed attempt the record is invalidated.
pub async fn verify_code(pool: &PgPool, email: &str, code: &str) -> Result<(), AppError> {
    let row = sqlx::query_as::<_, (String, i32, chrono::DateTime<Utc>)>(
        "SELECT code, attempts, expires_at FROM email_verifications WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("No verification code found for this email".into()))?;

    let (stored_code, attempts, expires_at) = row;

    if Utc::now() > expires_at {
        sqlx::query("DELETE FROM email_verifications WHERE email = $1")
            .bind(email)
            .execute(pool)
            .await?;
        return Err(AppError::BadRequest("Verification code has expired. Please request a new one.".into()));
    }

    if code != stored_code {
        let new_attempts = attempts + 1;
        if new_attempts >= 5 {
            sqlx::query("DELETE FROM email_verifications WHERE email = $1")
                .bind(email)
                .execute(pool)
                .await?;
            return Err(AppError::BadRequest("Too many incorrect attempts. Please request a new verification code.".into()));
        }
        sqlx::query("UPDATE email_verifications SET attempts = $1 WHERE email = $2")
            .bind(new_attempts)
            .bind(email)
            .execute(pool)
            .await?;
        return Err(AppError::BadRequest("Invalid verification code".into()));
    }

    // Success — clean up
    sqlx::query("DELETE FROM email_verifications WHERE email = $1")
        .bind(email)
        .execute(pool)
        .await?;

    Ok(())
}

/// Send a verification code via SMTP. Returns whether the email was actually sent (false = SMTP not configured, code was logged instead).
pub async fn send_verification_email(
    smtp_host: &str,
    smtp_port: u16,
    smtp_username: &str,
    smtp_password: &str,
    from_address: &str,
    to_email: &str,
    code: &str,
) -> Result<bool, AppError> {
    let body = format!(
        "Your Bytescape verification code is: {}\n\nThis code expires in 10 minutes.\n\nIf you did not request this code, please ignore this email.",
        code
    );

    let email = Message::builder()
        .from(from_address.parse().map_err(|e| AppError::Internal(format!("Invalid from address: {}", e)))?)
        .to(to_email.parse().map_err(|e| AppError::Internal(format!("Invalid to address: {}", e)))?)
        .subject("Your Bytescape Verification Code")
        .header(ContentType::TEXT_PLAIN)
        .body(body)
        .map_err(|e| AppError::Internal(format!("Failed to build email: {}", e)))?;

    if smtp_username.is_empty() || smtp_password.is_empty() {
        tracing::warn!(
            "SMTP credentials not configured — verification code for {} is: {}",
            to_email, code
        );
        return Ok(false);
    }

    let creds = Credentials::new(smtp_username.to_string(), smtp_password.to_string());

    let mailer = AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(smtp_host)
        .map_err(|e| AppError::Internal(format!("SMTP relay error: {}", e)))?
        .port(smtp_port)
        .credentials(creds)
        .build();

    mailer
        .send(email)
        .await
        .map_err(|e| AppError::Internal(format!("Failed to send email: {}", e)))?;

    Ok(true)
}
