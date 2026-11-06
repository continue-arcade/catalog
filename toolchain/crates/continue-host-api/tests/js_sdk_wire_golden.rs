use continue_host_api::{
    core, crypto, fs, kv, log, net, secret, settings, system, FileRef, CONTINUE_HOST_API_VERSION,
};
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Golden {
    schema_version: &'static str,
    host_api_version: &'static str,
    calls: Vec<Call>,
    responses: Vec<Response>,
    errors: Vec<Value>,
}

#[derive(Serialize)]
struct Call {
    method: &'static str,
    params: Value,
}

#[derive(Serialize)]
struct Response {
    method: &'static str,
    response: Value,
}

fn response(method: &'static str, response: impl Serialize) -> Response {
    Response {
        method,
        response: serde_json::to_value(response).expect("Host API response必须可序列化"),
    }
}

fn call(method: &'static str, params: impl Serialize) -> Call {
    Call {
        method,
        params: serde_json::to_value(params).expect("Host API request必须可序列化"),
    }
}

fn golden() -> Golden {
    let file = FileRef::new("extension-cache", "cover.jpg").expect("fixture FileRef必须有效");
    let response_file = file.clone();
    Golden {
        schema_version: "continue.host-api-js-golden.v1",
        host_api_version: CONTINUE_HOST_API_VERSION,
        calls: vec![
            call(settings::GET_METHOD, settings::GetRequest {}),
            call(
                core::HELLO_METHOD,
                core::HelloRequest {
                    api_version: CONTINUE_HOST_API_VERSION.to_owned(),
                },
            ),
            call(core::CAPABILITIES_METHOD, core::CapabilitiesRequest {}),
            call(system::INFO_METHOD, system::SystemInfoRequest {}),
            call(
                net::REQUEST_METHOD,
                net::NetRequest {
                    method: net::HttpMethod::Post,
                    url: "https://api.example.test/v4/games".to_owned(),
                    headers: vec![net::Header {
                        name: "Client-ID".to_owned(),
                        value: "fixture".to_owned(),
                    }],
                    query: vec![net::QueryParam {
                        name: "page".to_owned(),
                        value: "1".to_owned(),
                    }],
                    body: Some(net::RequestBody::Text {
                        value: "fields name;".to_owned(),
                        content_type: Some("text/plain".to_owned()),
                    }),
                    response_mode: net::ResponseMode::Json,
                    timeout_ms: Some(2_000),
                },
            ),
            call(
                net::DOWNLOAD_METHOD,
                net::DownloadRequest {
                    method: net::HttpMethod::Get,
                    url: "https://images.example.test/cover.jpg".to_owned(),
                    headers: Vec::new(),
                    query: Vec::new(),
                    destination: file.clone(),
                    timeout_ms: None,
                    overwrite: false,
                },
            ),
            call(
                kv::GET_METHOD,
                kv::GetRequest {
                    key: "token".to_owned(),
                },
            ),
            call(
                kv::SET_METHOD,
                kv::SetRequest {
                    key: "token".to_owned(),
                    value: json!({ "value": "opaque" }),
                },
            ),
            call(
                kv::DELETE_METHOD,
                kv::DeleteRequest {
                    key: "token".to_owned(),
                },
            ),
            call(
                secret::GET_METHOD,
                secret::SecretGetRequest {
                    name: "client-secret".to_owned(),
                },
            ),
            call(
                log::WRITE_METHOD,
                log::LogWriteRequest {
                    level: log::LogLevel::Info,
                    message: "fixture".to_owned(),
                    fields: Some(json!({ "count": 1 })),
                },
            ),
            call(
                fs::READ_TEXT_METHOD,
                fs::ReadTextRequest { file: file.clone() },
            ),
            call(
                fs::WRITE_TEXT_METHOD,
                fs::WriteTextRequest {
                    file: file.clone(),
                    content: "hello".to_owned(),
                    create: true,
                },
            ),
            call(fs::STAT_METHOD, fs::StatRequest { file: file.clone() }),
            call(
                fs::LIST_METHOD,
                fs::ListRequest {
                    directory: FileRef::new("extension-cache", "")
                        .expect("fixture directory FileRef必须有效"),
                },
            ),
            call(fs::MOUNTS_METHOD, fs::MountsRequest {}),
            call(
                crypto::HASH_FILE_METHOD,
                crypto::HashFileRequest {
                    file,
                    algorithm: crypto::HashAlgorithm::Sha256,
                },
            ),
        ],
        responses: vec![
            response(
                settings::GET_METHOD,
                settings::GetResponse {
                    settings: std::collections::BTreeMap::from([("limit".into(), json!(8))]),
                },
            ),
            response(
                core::HELLO_METHOD,
                core::HelloResponse {
                    api_version: CONTINUE_HOST_API_VERSION.to_owned(),
                    session_id: continue_host_api::SessionId("session-fixture".to_owned()),
                },
            ),
            response(
                core::CAPABILITIES_METHOD,
                core::CapabilitiesResponse {
                    methods: vec![system::INFO_METHOD.to_owned()],
                    capabilities: vec![continue_host_api::Capability::SystemInfo],
                },
            ),
            response(
                system::INFO_METHOD,
                system::SystemInfoResponse {
                    platform: system::HostPlatform::Macos,
                    api_version: CONTINUE_HOST_API_VERSION.to_owned(),
                    app: system::ApplicationInfo {
                        name: "Continue Arcade".to_owned(),
                        version: "0.1.0".to_owned(),
                    },
                },
            ),
            response(
                net::REQUEST_METHOD,
                net::NetResponse {
                    status: 200,
                    headers: vec![net::Header {
                        name: "content-type".to_owned(),
                        value: "application/json".to_owned(),
                    }],
                    final_url: "https://api.example.test/v4/games".to_owned(),
                    body: net::NetResponseBody::Json(json!({ "id": 42 })),
                },
            ),
            response(
                net::DOWNLOAD_METHOD,
                net::DownloadResponse {
                    status: 200,
                    headers: Vec::new(),
                    final_url: "https://images.example.test/cover.jpg".to_owned(),
                    file: response_file.clone(),
                    bytes_written: 12,
                    content_type: Some("image/jpeg".to_owned()),
                    content_length: Some(12),
                    etag: None,
                    last_modified: None,
                },
            ),
            response(
                kv::GET_METHOD,
                kv::GetResponse {
                    value: Some(json!({ "value": "opaque" })),
                },
            ),
            response(kv::SET_METHOD, kv::SetResponse {}),
            response(kv::DELETE_METHOD, kv::DeleteResponse {}),
            response(
                secret::GET_METHOD,
                secret::SecretGetResponse {
                    value: "fixture-secret".to_owned(),
                },
            ),
            response(log::WRITE_METHOD, log::LogWriteResponse {}),
            response(
                fs::READ_TEXT_METHOD,
                fs::ReadTextResponse {
                    content: "hello".to_owned(),
                },
            ),
            response(fs::WRITE_TEXT_METHOD, fs::WriteTextResponse {}),
            response(
                fs::STAT_METHOD,
                fs::StatResponse {
                    kind: fs::FileType::File,
                    size: 5,
                    modified: None,
                },
            ),
            response(
                fs::LIST_METHOD,
                fs::ListResponse {
                    entries: vec![fs::FileEntry {
                        name: "cover.jpg".to_owned(),
                        kind: fs::FileType::File,
                        size: Some(12),
                        modified: None,
                    }],
                },
            ),
            response(
                fs::MOUNTS_METHOD,
                fs::MountsResponse {
                    mounts: vec![fs::MountInfo {
                        name: "extension-cache".to_owned(),
                        writable: true,
                    }],
                },
            ),
            response(
                crypto::HASH_FILE_METHOD,
                crypto::HashFileResponse {
                    algorithm: crypto::HashAlgorithm::Sha256,
                    value: "00ff".to_owned(),
                },
            ),
        ],
        errors: [
            continue_host_api::ApiErrorKind::InvalidArgument,
            continue_host_api::ApiErrorKind::Unsupported,
            continue_host_api::ApiErrorKind::PermissionDenied,
            continue_host_api::ApiErrorKind::NotFound,
            continue_host_api::ApiErrorKind::AlreadyExists,
            continue_host_api::ApiErrorKind::Conflict,
            continue_host_api::ApiErrorKind::Timeout,
            continue_host_api::ApiErrorKind::Cancelled,
            continue_host_api::ApiErrorKind::RateLimited,
            continue_host_api::ApiErrorKind::QuotaExceeded,
            continue_host_api::ApiErrorKind::NetworkError,
            continue_host_api::ApiErrorKind::RemoteError,
            continue_host_api::ApiErrorKind::IoError,
            continue_host_api::ApiErrorKind::InternalError,
        ]
        .into_iter()
        .map(|kind| {
            serde_json::to_value(continue_host_api::ApiError::new(kind, "fixture error"))
                .expect("ApiError必须可序列化")
        })
        .collect(),
    }
}

#[test]
fn checked_in_javascript_sdk_wire_golden_matches_rust_authority() {
    let generated = serde_json::to_value(golden()).expect("golden必须可编码");
    let checked_in: Value = serde_json::from_str(include_str!(
        "../../../packages/extension-sdk-js/src/host-api-wire.golden.json"
    ))
    .expect("checked-in JS SDK golden必须是JSON");
    assert_eq!(
        generated, checked_in,
        "Host API Rust contract已变化；请从本测试的generated值更新JS SDK golden"
    );
}
