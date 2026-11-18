use axum::{extract::{Multipart, State}, http::HeaderMap, Json};
use serde::Serialize;
use uuid::Uuid;

use crate::db;
use crate::error::AppError;
use crate::AppState;

use super::extract_user_id;

// ---------------------------------------------------------------------------
// POST /api/media/upload
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub(crate) struct MediaUploadResponse {
    url: String,
    mime_type: String,
    width: Option<i32>,
    height: Option<i32>,
}

pub(super) async fn media_upload_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<MediaUploadResponse>, AppError> {
    let user_id = extract_user_id(&headers, &state.config.jwt_secret)?;

    let mut file_bytes: Vec<u8> = Vec::new();
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            filename = field.file_name().map(|s| s.to_string());
            content_type = field.content_type().map(|s| s.to_string());
            file_bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Read error: {}", e)))?
                .to_vec();
            break;
        }
    }

    if file_bytes.is_empty() {
        return Err(AppError::BadRequest("No file provided".into()));
    }

    let detected_mime = content_type
        .as_deref()
        .or_else(|| {
            filename
                .as_deref()
                .and_then(|f| mime_guess::from_path(f).first_raw())
        })
        .unwrap_or("application/octet-stream")
        .to_string();

    // Determine if image or video + size limits
    let is_video = detected_mime.starts_with("video/");
    let max_bytes = if is_video {
        100 * 1024 * 1024 // 100MB for video
    } else {
        10 * 1024 * 1024 // 10MB for images
    };

    if file_bytes.len() > max_bytes {
        return Err(AppError::BadRequest(format!(
            "File too large: {} bytes (max {} for {})",
            file_bytes.len(),
            max_bytes,
            if is_video { "video" } else { "images" },
        )));
    }

    // Validate content type — accept any image/* or video/*
    if !detected_mime.starts_with("image/") && !detected_mime.starts_with("video/") {
        return Err(AppError::BadRequest(format!(
            "Invalid content type: {}. Only images and videos are allowed",
            detected_mime
        )));
    }

    let ext = if is_video {
        detected_mime
            .strip_prefix("video/")
            .map(|s| match s {
                "mp4" => "mp4",
                "webm" => "webm",
                "quicktime" => "mov",
                "x-matroska" => "mkv",
                "ogg" => "ogg",
                _ => "mp4",
            })
            .unwrap_or("mp4")
            .to_string()
    } else {
        detected_mime
            .strip_prefix("image/")
            .unwrap_or("jpg")
            .replace("jpeg", "jpg")
    };

    let object_key = format!(
        "media/{}/{}.{}",
        state.config.domain,
        Uuid::new_v4(),
        ext
    );

    let url = if !state.config.s3_enabled {
        let file_path = format!("./uploads/{}", object_key);
        if let Some(parent) = std::path::Path::new(&file_path).parent() {
            std::fs::create_dir_all(parent).ok();
        }
        std::fs::write(&file_path, &file_bytes)
            .map_err(|e| AppError::Internal(format!("Failed to save file: {}", e)))?;
        if state.config.s3_public_base_url.is_empty() {
            format!("/uploads/{}", object_key)
        } else {
            format!("{}/{}", state.config.s3_public_base_url, object_key)
        }
    } else {
        let credentials = aws_sdk_s3::config::Credentials::new(
            &state.config.s3_access_key,
            &state.config.s3_secret_key,
            None,
            None,
            "static",
        );
        let s3_config = aws_sdk_s3::Config::builder()
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .endpoint_url(&state.config.s3_endpoint)
            .region(aws_sdk_s3::config::Region::new("us-east-1"))
            .credentials_provider(credentials)
            .force_path_style(true)
            .build();
        let client = aws_sdk_s3::Client::from_conf(s3_config);

        let body = aws_sdk_s3::primitives::ByteStream::from(file_bytes.clone());
        client
            .put_object()
            .bucket(&state.config.s3_bucket)
            .key(&object_key)
            .body(body)
            .content_type(&detected_mime)
            .send()
            .await
            .map_err(|e| AppError::Internal(format!("S3 upload failed: {}", e)))?;

        let base = if state.config.s3_public_base_url.is_empty() {
            state.config.s3_endpoint.trim_end_matches('/').to_string()
        } else {
            state.config.s3_public_base_url.trim_end_matches('/').to_string()
        };
        format!("{}/{}/{}", base, state.config.s3_bucket, object_key)
    };

    // Detect dimensions for images
    let (width, height) = if !is_video {
        detect_image_dimensions(&file_bytes, &detected_mime)
    } else {
        (None, None)
    };

    // Store media metadata in DB linked to user
    let _ = db::media::MediaRow::create_for_user(
        &state.db,
        user_id,
        &url,
        None,
        &detected_mime,
        width,
        height,
        None,
        Some(file_bytes.len() as i64),
    )
    .await;

    Ok(Json(MediaUploadResponse {
        url,
        mime_type: detected_mime,
        width,
        height,
    }))
}

/// Rudimentary image dimension detection from file bytes.
fn detect_image_dimensions(data: &[u8], mime: &str) -> (Option<i32>, Option<i32>) {
    match mime {
        "image/png" => {
            if data.len() >= 24 {
                let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
                let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
                (Some(w as i32), Some(h as i32))
            } else {
                (None, None)
            }
        }
        "image/jpeg" => {
            // Skip JPEG markers to find SOF segment
            let mut i = 2;
            while i + 9 < data.len() {
                if data[i] == 0xFF {
                    let marker = data[i + 1];
                    if marker == 0xC0 || marker == 0xC1 || marker == 0xC2 {
                        let h = u16::from_be_bytes([data[i + 5], data[i + 6]]);
                        let w = u16::from_be_bytes([data[i + 7], data[i + 8]]);
                        return (Some(w as i32), Some(h as i32));
                    }
                    let seg_len = u16::from_be_bytes([data[i + 2], data[i + 3]]);
                    i += seg_len as usize + 2;
                } else {
                    i += 1;
                }
            }
            (None, None)
        }
        "image/gif" => {
            if data.len() >= 10 {
                let w = u16::from_le_bytes([data[6], data[7]]);
                let h = u16::from_le_bytes([data[8], data[9]]);
                (Some(w as i32), Some(h as i32))
            } else {
                (None, None)
            }
        }
        "image/webp" => {
            if data.len() >= 32 {
                let w = u32::from_le_bytes([data[24], data[25], data[26], data[27]]) & 0x3FFF;
                let h = u32::from_le_bytes([data[28], data[29], data[30], data[31]]) & 0x3FFF;
                (Some(w as i32), Some(h as i32))
            } else {
                (None, None)
            }
        }
        _ => (None, None),
    }
}
