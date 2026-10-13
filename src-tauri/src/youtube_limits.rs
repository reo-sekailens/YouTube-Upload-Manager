#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum UploadLimitKind {
    ApiQuota,
    ChannelUpload,
}

/// Classifies documented provider reasons without trusting message substrings.
pub(crate) fn classify(status: u16, body: &str) -> Option<UploadLimitKind> {
    let (api_quota, channel_upload_limit) = limit_facts(status, body);
    if api_quota {
        Some(UploadLimitKind::ApiQuota)
    } else {
        channel_upload_limit.then_some(UploadLimitKind::ChannelUpload)
    }
}

/// Retains a channel limit even when a response also reports API quota exhaustion.
pub(crate) fn includes_channel_limit(status: u16, body: &str) -> bool {
    limit_facts(status, body).1
}

fn limit_facts(status: u16, body: &str) -> (bool, bool) {
    if !matches!(status, 400 | 403 | 429) {
        return (false, false);
    }

    let Ok(payload) = serde_json::from_str::<serde_json::Value>(body) else {
        return (false, false);
    };
    let Some(errors) = payload
        .get("error")
        .and_then(|error| error.get("errors"))
        .and_then(serde_json::Value::as_array)
    else {
        return (false, false);
    };
    let mut api_quota = false;
    let mut channel_upload_limit = false;
    for reason in errors
        .iter()
        .filter_map(|error| error.get("reason").and_then(serde_json::Value::as_str))
    {
        match reason {
            "quotaExceeded" | "dailyLimitExceeded" => api_quota = true,
            "uploadLimitExceeded" => channel_upload_limit = true,
            _ => {}
        }
    }
    (api_quota, channel_upload_limit)
}

#[cfg(test)]
mod tests {
    use super::{classify, includes_channel_limit, UploadLimitKind};

    fn response(reasons: &[&str]) -> String {
        serde_json::json!({
            "error": {
                "errors": reasons.iter().map(|reason| serde_json::json!({
                    "domain": "youtube",
                    "reason": reason,
                })).collect::<Vec<_>>()
            }
        })
        .to_string()
    }

    #[test]
    fn distinguishes_cloud_quota_from_channel_upload_limit() {
        for status in [400, 403, 429] {
            assert_eq!(
                classify(status, &response(&["quotaExceeded"])),
                Some(UploadLimitKind::ApiQuota)
            );
            assert_eq!(
                classify(status, &response(&["dailyLimitExceeded"])),
                Some(UploadLimitKind::ApiQuota)
            );
            assert_eq!(
                classify(status, &response(&["uploadLimitExceeded"])),
                Some(UploadLimitKind::ChannelUpload)
            );
            assert!(includes_channel_limit(
                status,
                &response(&["uploadLimitExceeded"])
            ));
            assert!(!includes_channel_limit(
                status,
                &response(&["quotaExceeded"])
            ));
            assert!(!includes_channel_limit(
                status,
                &response(&["dailyLimitExceeded"])
            ));
        }
    }

    #[test]
    fn cloud_quota_takes_precedence_in_either_reason_order() {
        for reasons in [
            ["uploadLimitExceeded", "quotaExceeded"],
            ["quotaExceeded", "uploadLimitExceeded"],
            ["uploadLimitExceeded", "dailyLimitExceeded"],
        ] {
            assert_eq!(
                classify(403, &response(&reasons)),
                Some(UploadLimitKind::ApiQuota)
            );
            for status in [400, 403, 429] {
                assert!(includes_channel_limit(status, &response(&reasons)));
            }
        }
    }

    #[test]
    fn documented_bad_request_channel_limit_is_recognized() {
        assert_eq!(
            classify(400, &response(&["uploadLimitExceeded"])),
            Some(UploadLimitKind::ChannelUpload)
        );
    }

    #[test]
    fn non_limit_http_statuses_do_not_trigger_a_pause() {
        for status in [200, 201, 204, 301, 401, 404, 500, 503] {
            assert_eq!(classify(status, &response(&["quotaExceeded"])), None);
            assert!(!includes_channel_limit(
                status,
                &response(&["uploadLimitExceeded"])
            ));
        }
    }

    #[test]
    fn reason_matching_is_exact_and_ignores_messages() {
        for reason in [
            "quotaExceededElsewhere",
            "pre_uploadLimitExceeded",
            "QuotaExceeded",
            "rateLimitExceeded",
            "userRateLimitExceeded",
            "forbidden",
        ] {
            assert_eq!(classify(403, &response(&[reason])), None);
            assert!(!includes_channel_limit(403, &response(&[reason])));
        }
        assert_eq!(
            classify(
                403,
                r#"{"error":{"message":"quotaExceeded uploadLimitExceeded","errors":[{"reason":"forbidden"}]}}"#
            ),
            None
        );
        assert!(!includes_channel_limit(
            403,
            r#"{"error":{"message":"uploadLimitExceeded","errors":[{"reason":"forbidden"}]}}"#
        ));
    }

    #[test]
    fn malformed_or_unstructured_payloads_are_not_classified() {
        for body in [
            "",
            "quotaExceeded",
            "{",
            "null",
            r#"{"reason":"quotaExceeded"}"#,
            r#"{"error":{"reason":"quotaExceeded"}}"#,
            r#"{"error":{"errors":{"reason":"quotaExceeded"}}}"#,
            r#"{"error":{"errors":[]}}"#,
            r#"{"error":{"errors":[{"reason":123},null,"quotaExceeded"]}}"#,
        ] {
            assert_eq!(classify(403, body), None, "body: {body}");
            assert!(!includes_channel_limit(403, body), "body: {body}");
        }
    }

    #[test]
    fn unrelated_or_malformed_entries_do_not_hide_a_valid_reason() {
        assert_eq!(
            classify(
                429,
                r#"{"error":{"errors":[null,{"reason":42},{"reason":"backendError"},{"reason":"quotaExceeded"}]}}"#
            ),
            Some(UploadLimitKind::ApiQuota)
        );
        assert!(includes_channel_limit(
            429,
            r#"{"error":{"errors":[null,{"reason":42},{"reason":"backendError"},{"reason":"uploadLimitExceeded"}]}}"#
        ));
    }

    #[test]
    fn channel_limit_requires_exact_structured_reason() {
        for body in [
            "uploadLimitExceeded",
            r#"{"reason":"uploadLimitExceeded"}"#,
            r#"{"error":{"reason":"uploadLimitExceeded"}}"#,
            r#"{"error":{"errors":{"reason":"uploadLimitExceeded"}}}"#,
            r#"{"error":{"errors":["uploadLimitExceeded"]}}"#,
            r#"{"error":{"errors":[{"reason":"uploadLimitExceededAfterRetry"}]}}"#,
        ] {
            assert!(!includes_channel_limit(400, body), "body: {body}");
        }
    }
}
