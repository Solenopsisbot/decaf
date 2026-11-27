//! Bounded Java 25 models for `java.net.http` builders and immutable values.
//!
//! Builder state lives in the runtime's persistent static-state map, keyed by the
//! typed handle. Fluent setters return the same handle, while `build()` snapshots
//! the state into a new client/request handle. Network operations intentionally
//! return `None` so the normal host boundary counts them.

use crate::host_ref::{HostKind, HostRef, Methods, Nested, instance_of_class};
use crate::{MethodRef, NativeError, StdHost, Value};

const HTTP_CLIENT: &str = "java/net/http/HttpClient";
const HTTP_CLIENT_BUILDER: &str = "java/net/http/HttpClient$Builder";
const HTTP_REQUEST: &str = "java/net/http/HttpRequest";
const HTTP_REQUEST_BUILDER: &str = "java/net/http/HttpRequest$Builder";
const STATE_PREFIX: &str = "\u{0}horizon-java/http-client/";

fn identity_equals(
    handle: &HostRef,
    other: &Value,
    _: &mut dyn Nested,
) -> Result<Option<bool>, NativeError> {
    Ok(match other {
        Value::Host(other) if other.kind() == handle.kind() => handle.same_object(other),
        _ => Some(false),
    })
}

const OBJECT_METHODS: Methods = Methods {
    to_string: |_, _| Ok(None),
    hash_code: |_, _| Ok(None),
    equals: identity_equals,
    instance_of: instance_of_class,
    crosses_jars: |_, _| false,
    compare_to: None,
};

pub(crate) fn client_types(_: &str) -> &'static [&'static str] {
    &[HTTP_CLIENT]
}
pub(crate) fn client_builder_types(_: &str) -> &'static [&'static str] {
    &[HTTP_CLIENT_BUILDER]
}
pub(crate) fn request_types(_: &str) -> &'static [&'static str] {
    &[HTTP_REQUEST]
}
pub(crate) fn request_builder_types(_: &str) -> &'static [&'static str] {
    &[HTTP_REQUEST_BUILDER]
}

pub(crate) const CLIENT_METHODS: Methods = OBJECT_METHODS;
pub(crate) const CLIENT_BUILDER_METHODS: Methods = OBJECT_METHODS;
pub(crate) const REQUEST_METHODS: Methods = OBJECT_METHODS;
pub(crate) const REQUEST_BUILDER_METHODS: Methods = OBJECT_METHODS;
pub(crate) const ENUM_METHODS: Methods = Methods {
    to_string: |handle, _| Ok(Some(handle.key().to_owned())),
    hash_code: |_, _| Ok(None),
    equals: identity_equals,
    instance_of: instance_of_class,
    crosses_jars: |_, _| false,
    compare_to: None,
};

fn enum_kind(owner: &str) -> Option<HostKind> {
    match owner {
        "java/net/http/HttpClient$Version" => Some(HostKind::HttpClientVersion),
        "java/net/http/HttpClient$Redirect" => Some(HostKind::HttpClientRedirect),
        _ => None,
    }
}

fn enum_value(owner: &str, name: &str) -> Value {
    Value::Host(HostRef::new(
        enum_kind(owner).expect("known HTTP enum"),
        name,
    ))
}

pub(crate) fn static_field(owner: &str, name: &str, descriptor: &str) -> Option<Value> {
    match (owner, name, descriptor) {
        ("java/net/http/HttpClient$Version", "HTTP_1_1", "Ljava/net/http/HttpClient$Version;")
        | ("java/net/http/HttpClient$Version", "HTTP_2", "Ljava/net/http/HttpClient$Version;")
        | ("java/net/http/HttpClient$Redirect", "NEVER", "Ljava/net/http/HttpClient$Redirect;")
        | ("java/net/http/HttpClient$Redirect", "NORMAL", "Ljava/net/http/HttpClient$Redirect;")
        | ("java/net/http/HttpClient$Redirect", "ALWAYS", "Ljava/net/http/HttpClient$Redirect;") => {
            Some(enum_value(owner, name))
        }
        _ => None,
    }
}

fn state_key(kind: &str, key: &str) -> String {
    format!("{STATE_PREFIX}{kind}/{key}")
}

fn state(runtime: &StdHost, kind: &str, key: &str) -> Option<serde_json::Value> {
    let key = state_key(kind, key);
    let Value::Ref(Some(text)) = runtime.persistent_static_fields.get(&key)? else {
        return None;
    };
    serde_json::from_str(text).ok()
}

fn save_state(runtime: &mut StdHost, kind: &str, key: &str, value: serde_json::Value) {
    std::sync::Arc::make_mut(&mut runtime.persistent_static_fields)
        .insert(state_key(kind, key), Value::Ref(Some(value.to_string())));
}

fn field_key(kind: &str, key: &str, field: &str) -> String {
    format!("{}/{}", state_key(kind, key), field)
}

fn save_field(runtime: &mut StdHost, kind: &str, key: &str, field: &str, value: Value) {
    std::sync::Arc::make_mut(&mut runtime.persistent_static_fields)
        .insert(field_key(kind, key, field), value);
}

fn field(runtime: &StdHost, kind: &str, key: &str, field_name: &str) -> Option<Value> {
    runtime
        .persistent_static_fields
        .get(&field_key(kind, key, field_name))
        .cloned()
}

fn copy_field(runtime: &mut StdHost, from: (&str, &str), to: (&str, &str), field_name: &str) {
    if let Some(value) = field(runtime, from.0, from.1, field_name) {
        save_field(runtime, to.0, to.1, field_name, value);
    }
}

fn allocation(kind: HostKind, class: &str) -> Value {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    Value::Host(HostRef::new(kind, format!("{class}#http-{serial}")))
}

fn null_pointer() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/NullPointerException".into(),
        message: None,
        frame: None,
    }
}

fn exception(class: &str, message: String) -> NativeError {
    NativeError::UncaughtException {
        class: class.into(),
        message: Some(message),
        frame: None,
    }
}

/// `HttpRequestBuilderImpl.uri(URI)` (also reached from `newBuilder(URI)`): JDK 25's
/// `requireNonNull(uri, "uri must be non-null")`, then `checkURI`: a scheme, `http` or
/// `https` in any case (the message names it lower-cased), and a host. `Ok(None)` for a
/// value that is no URI handle or a URI text this model doesn't parse exactly: the call
/// is then counted, never answered with a made-up exception.
fn checked_request_uri(value: &Value) -> Result<Option<String>, NativeError> {
    if matches!(value, Value::Ref(None)) {
        return Err(exception(
            "java/lang/NullPointerException",
            "uri must be non-null".into(),
        ));
    }
    let Some(text) = crate::host_jdk_value::uri_text(value) else {
        return Ok(None);
    };
    let Some(parts) = crate::host_jdk_value::uri_parts(text) else {
        return Ok(None);
    };
    let Some(scheme) = parts.scheme else {
        return Err(exception(
            "java/lang/IllegalArgumentException",
            "URI with undefined scheme".into(),
        ));
    };
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(exception(
            "java/lang/IllegalArgumentException",
            format!("invalid URI scheme {scheme}"),
        ));
    }
    if parts.host.is_none() {
        return Err(exception(
            "java/lang/IllegalArgumentException",
            format!("unsupported URI {text}"),
        ));
    }
    Ok(Some(text.to_owned()))
}

fn illegal_argument(message: &str) -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/IllegalArgumentException".into(),
        message: Some(message.into()),
        frame: None,
    }
}

fn class_cast() -> NativeError {
    NativeError::UncaughtException {
        class: "java/lang/ClassCastException".into(),
        message: None,
        frame: None,
    }
}

fn invalid_duration(value: &Value) -> Result<i128, NativeError> {
    let Some(nanos) = crate::host_jdk_value::duration_nanos(value) else {
        return Err(null_pointer());
    };
    if nanos <= 0 {
        return Err(illegal_argument(&format!(
            "Invalid duration: {}",
            crate::duration_text(nanos)
        )));
    }
    Ok(nanos)
}

fn string_arg(value: Option<&Value>) -> Option<&str> {
    match value {
        Some(Value::Ref(Some(value))) => Some(value),
        _ => None,
    }
}

fn valid_header_name(name: &str) -> bool {
    !name.is_empty() && name.bytes().all(is_token_byte)
}

fn is_token_byte(byte: u8) -> bool {
    byte.is_ascii() && byte > 0x20 && byte < 0x7f && !b"()<>@,;:\"/[]?={}\t".contains(&byte)
}

fn valid_header_value(value: &str) -> bool {
    !value.bytes().any(|byte| byte == b'\r' || byte == b'\n')
}

fn restricted_header(name: &str) -> bool {
    ["connection", "content-length", "expect", "host", "upgrade"]
        .iter()
        .any(|forbidden| name.eq_ignore_ascii_case(forbidden))
}

fn validate_header(name: &str, value: &str) -> Result<(), NativeError> {
    if !valid_header_name(name) {
        return Err(illegal_argument(&format!(
            "invalid header name: \"{name}\""
        )));
    }
    if restricted_header(name) {
        return Err(illegal_argument(&format!(
            "restricted header name: \"{name}\""
        )));
    }
    if !valid_header_value(value) {
        return Err(illegal_argument(&format!(
            "invalid header value: \"{value}\""
        )));
    }
    Ok(())
}

fn method_name(name: &str) -> Result<(), NativeError> {
    if name.is_empty() {
        return Err(illegal_argument("illegal method <empty string>"));
    }
    if !name.bytes().all(is_token_byte) {
        return Err(illegal_argument(&format!("illegal method \"{name}\"")));
    }
    Ok(())
}

fn value_enum(value: &Value, owner: &str, allowed: &[&str]) -> Result<String, NativeError> {
    if matches!(value, Value::Ref(None)) {
        return Err(null_pointer());
    }
    let kind = enum_kind(owner).ok_or_else(class_cast)?;
    let Value::Host(handle) = value else {
        return Err(class_cast());
    };
    if handle.kind() != kind {
        return Err(class_cast());
    }
    let name = handle.key();
    if allowed.contains(&name) {
        Ok(name.to_owned())
    } else {
        Err(class_cast())
    }
}

pub(crate) fn static_call(
    runtime: &mut StdHost,
    r: &MethodRef,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    match (r.owner.as_str(), r.name.as_str(), r.descriptor.as_str()) {
        ("java/net/http/HttpClient", "newBuilder", "()Ljava/net/http/HttpClient$Builder;") => {
            let value = allocation(HostKind::HttpClientBuilder, HTTP_CLIENT_BUILDER);
            if let Value::Host(handle) = &value {
                save_state(
                    runtime,
                    "client-builder",
                    handle.key(),
                    serde_json::json!({"timeout": null, "version": "HTTP_2", "redirects": "NEVER"}),
                );
            }
            Some(Ok(value))
        }
        ("java/net/http/HttpRequest", "newBuilder", "()Ljava/net/http/HttpRequest$Builder;") => {
            Some(Ok(new_request_builder(runtime, None)))
        }
        (
            "java/net/http/HttpRequest",
            "newBuilder",
            "(Ljava/net/URI;)Ljava/net/http/HttpRequest$Builder;",
        ) => match checked_request_uri(args.first()?) {
            Ok(Some(uri)) => Some(Ok(new_request_builder(runtime, Some(uri)))),
            Ok(None) => None,
            Err(error) => Some(Err(error)),
        },
        _ => None,
    }
}

fn new_request_builder(runtime: &mut StdHost, uri: Option<String>) -> Value {
    let value = allocation(HostKind::HttpRequestBuilder, HTTP_REQUEST_BUILDER);
    if let Value::Host(handle) = &value {
        save_state(
            runtime,
            "request-builder",
            handle.key(),
            serde_json::json!({"uri": uri, "timeout": null, "method": "GET", "headers": [], "expect": false}),
        );
    }
    value
}

fn receiver_key(receiver: &Value, kind: HostKind) -> Option<&str> {
    match receiver {
        Value::Host(handle) if handle.kind() == kind => Some(handle.key()),
        _ => None,
    }
}

pub(crate) fn instance_call(
    runtime: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let kind = match receiver {
        Value::Host(handle) => handle.kind(),
        _ => return None,
    };
    match kind {
        HostKind::HttpClientBuilder => builder_call(runtime, r, receiver, args),
        HostKind::HttpClient => client_call(runtime, r, receiver),
        HostKind::HttpRequestBuilder => request_builder_call(runtime, r, receiver, args),
        HostKind::HttpRequest => request_call(runtime, r, receiver),
        _ => None,
    }
}

fn builder_call(
    runtime: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let key = receiver_key(receiver, HostKind::HttpClientBuilder)?;
    let mut current = state(runtime, "client-builder", key).unwrap_or_else(
        || serde_json::json!({"timeout": null, "version": "HTTP_2", "redirects": "NEVER"}),
    );
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("connectTimeout", "(Ljava/time/Duration;)Ljava/net/http/HttpClient$Builder;") => {
            let nanos = match args.first() {
                Some(Value::Ref(None)) => return Some(Err(null_pointer())),
                Some(value) => match invalid_duration(value) {
                    Ok(nanos) => nanos,
                    Err(error) => return Some(Err(error)),
                },
                None => return None,
            };
            current["timeout"] = serde_json::Value::String(nanos.to_string());
            save_state(runtime, "client-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        ("version", "(Ljava/net/http/HttpClient$Version;)Ljava/net/http/HttpClient$Builder;") => {
            let value = args.first()?;
            let version = match value_enum(
                value,
                "java/net/http/HttpClient$Version",
                &["HTTP_1_1", "HTTP_2"],
            ) {
                Ok(version) => version,
                Err(error) => return Some(Err(error)),
            };
            current["version"] = serde_json::Value::String(version);
            save_state(runtime, "client-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        (
            "followRedirects",
            "(Ljava/net/http/HttpClient$Redirect;)Ljava/net/http/HttpClient$Builder;",
        ) => {
            let value = args.first()?;
            let redirects = match value_enum(
                value,
                "java/net/http/HttpClient$Redirect",
                &["NEVER", "NORMAL", "ALWAYS"],
            ) {
                Ok(redirects) => redirects,
                Err(error) => return Some(Err(error)),
            };
            current["redirects"] = serde_json::Value::String(redirects);
            save_state(runtime, "client-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        ("executor", "(Ljava/util/concurrent/Executor;)Ljava/net/http/HttpClient$Builder;")
        | ("proxy", "(Ljava/net/ProxySelector;)Ljava/net/http/HttpClient$Builder;")
        | ("authenticator", "(Ljava/net/Authenticator;)Ljava/net/http/HttpClient$Builder;")
        | ("sslContext", "(Ljavax/net/ssl/SSLContext;)Ljava/net/http/HttpClient$Builder;")
        | ("sslParameters", "(Ljavax/net/ssl/SSLParameters;)Ljava/net/http/HttpClient$Builder;") => {
            let value = args.first()?;
            if matches!(value, Value::Ref(None)) {
                return Some(Err(null_pointer()));
            }
            save_field(
                runtime,
                "client-builder",
                key,
                r.name.as_str(),
                value.clone(),
            );
            Some(Ok(receiver.clone()))
        }
        ("build", "()Ljava/net/http/HttpClient;") => {
            let client = allocation(HostKind::HttpClient, HTTP_CLIENT);
            if let Value::Host(handle) = &client {
                save_state(runtime, "client", handle.key(), current);
                for field_name in [
                    "executor",
                    "proxy",
                    "authenticator",
                    "sslContext",
                    "sslParameters",
                ] {
                    copy_field(
                        runtime,
                        ("client-builder", key),
                        ("client", handle.key()),
                        field_name,
                    );
                }
            }
            Some(Ok(client))
        }
        _ => None,
    }
}

fn client_call(
    runtime: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
) -> Option<Result<Value, NativeError>> {
    let key = receiver_key(receiver, HostKind::HttpClient)?;
    let current = state(runtime, "client", key).unwrap_or_else(
        || serde_json::json!({"timeout": null, "version": "HTTP_2", "redirects": "NEVER"}),
    );
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("connectTimeout", "()Ljava/util/Optional;") => Some(Ok(current["timeout"]
            .as_str()
            .map_or_else(crate::host_optional::empty_optional, |text| {
                crate::host_optional::optional_wrap(crate::host_jdk_value::duration(
                    text.parse().unwrap_or(0),
                ))
            }))),
        ("version", "()Ljava/net/http/HttpClient$Version;") => Some(Ok(enum_value(
            "java/net/http/HttpClient$Version",
            current["version"].as_str().unwrap_or("HTTP_2"),
        ))),
        ("followRedirects", "()Ljava/net/http/HttpClient$Redirect;") => Some(Ok(enum_value(
            "java/net/http/HttpClient$Redirect",
            current["redirects"].as_str().unwrap_or("NEVER"),
        ))),
        ("proxy", "()Ljava/util/Optional;")
        | ("authenticator", "()Ljava/util/Optional;")
        | ("executor", "()Ljava/util/Optional;") => {
            Some(Ok(field(runtime, "client", key, r.name.as_str())
                .map_or_else(
                    crate::host_optional::empty_optional,
                    crate::host_optional::optional_wrap,
                )))
        }
        ("sslContext", "()Ljavax/net/ssl/SSLContext;")
        | ("sslParameters", "()Ljavax/net/ssl/SSLParameters;") => {
            field(runtime, "client", key, r.name.as_str()).map(Ok)
        }
        // `send` and `sendAsync` deliberately fall through to the counted host boundary.
        _ => None,
    }
}

fn request_builder_call(
    runtime: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
    args: &[Value],
) -> Option<Result<Value, NativeError>> {
    let key = receiver_key(receiver, HostKind::HttpRequestBuilder)?;
    let mut current = state(runtime, "request-builder", key).unwrap_or_else(|| {
        serde_json::json!({"uri": null, "timeout": null, "method": "GET", "headers": [], "expect": false})
    });
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("uri", "(Ljava/net/URI;)Ljava/net/http/HttpRequest$Builder;") => {
            let uri = match checked_request_uri(args.first()?) {
                Ok(Some(uri)) => uri,
                Ok(None) => return None,
                Err(error) => return Some(Err(error)),
            };
            current["uri"] = serde_json::Value::String(uri);
            save_state(runtime, "request-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        ("timeout", "(Ljava/time/Duration;)Ljava/net/http/HttpRequest$Builder;") => {
            let value = args.first()?;
            let nanos = match invalid_duration(value) {
                Ok(nanos) => nanos,
                Err(error) => return Some(Err(error)),
            };
            current["timeout"] = serde_json::Value::String(nanos.to_string());
            save_state(runtime, "request-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        ("expectContinue", "(Z)Ljava/net/http/HttpRequest$Builder;") => {
            current["expect"] = serde_json::Value::Bool(
                args.first()
                    .and_then(|v| match v {
                        Value::Int(i) => Some(*i != 0),
                        _ => None,
                    })
                    .unwrap_or(false),
            );
            save_state(runtime, "request-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        ("GET", "()Ljava/net/http/HttpRequest$Builder;") => {
            current["method"] = serde_json::Value::String("GET".into());
            save_state(runtime, "request-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        ("header", "(Ljava/lang/String;Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;") => {
            let Some(name) = string_arg(args.first()) else {
                return Some(Err(null_pointer()));
            };
            let Some(value) = string_arg(args.get(1)) else {
                return Some(Err(null_pointer()));
            };
            if let Err(error) = validate_header(name, value) {
                return Some(Err(error));
            }
            current["headers"]
                .as_array_mut()?
                .push(serde_json::json!([name, value]));
            save_state(runtime, "request-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        (
            "setHeader",
            "(Ljava/lang/String;Ljava/lang/String;)Ljava/net/http/HttpRequest$Builder;",
        ) => {
            let Some(name) = string_arg(args.first()) else {
                return Some(Err(null_pointer()));
            };
            let Some(value) = string_arg(args.get(1)) else {
                return Some(Err(null_pointer()));
            };
            if let Err(error) = validate_header(name, value) {
                return Some(Err(error));
            }
            let headers = current["headers"].as_array_mut()?;
            headers.retain(|entry| {
                entry
                    .get(0)
                    .and_then(serde_json::Value::as_str)
                    .is_none_or(|old| !old.eq_ignore_ascii_case(name))
            });
            headers.push(serde_json::json!([name, value]));
            save_state(runtime, "request-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        ("build", "()Ljava/net/http/HttpRequest;") => {
            let Some(uri) = current["uri"].as_str() else {
                return Some(Err(NativeError::UncaughtException {
                    class: "java/lang/IllegalStateException".into(),
                    message: Some("uri is null".into()),
                    frame: None,
                }));
            };
            let request = allocation(HostKind::HttpRequest, HTTP_REQUEST);
            if let Value::Host(handle) = &request {
                current["uri"] = serde_json::Value::String(uri.into());
                save_state(runtime, "request", handle.key(), current);
                copy_field(
                    runtime,
                    ("request-builder", key),
                    ("request", handle.key()),
                    "version",
                );
            }
            Some(Ok(request))
        }
        ("version", "(Ljava/net/http/HttpClient$Version;)Ljava/net/http/HttpRequest$Builder;") => {
            let value = args.first()?;
            match value_enum(
                value,
                "java/net/http/HttpClient$Version",
                &["HTTP_1_1", "HTTP_2"],
            ) {
                Ok(_) => save_field(runtime, "request-builder", key, "version", value.clone()),
                Err(error) => return Some(Err(error)),
            }
            Some(Ok(receiver.clone()))
        }
        (
            "method",
            "(Ljava/lang/String;Ljava/net/http/HttpRequest$BodyPublisher;)Ljava/net/http/HttpRequest$Builder;",
        ) => {
            let Some(method) = string_arg(args.first()) else {
                return Some(Err(null_pointer()));
            };
            if let Err(error) = method_name(method) {
                return Some(Err(error));
            }
            if matches!(args.get(1), Some(Value::Ref(None)) | None) {
                return Some(Err(null_pointer()));
            }
            current["method"] = serde_json::Value::String(method.into());
            save_state(runtime, "request-builder", key, current);
            Some(Ok(receiver.clone()))
        }
        _ => None,
    }
}

fn request_call(
    runtime: &mut StdHost,
    r: &MethodRef,
    receiver: &Value,
) -> Option<Result<Value, NativeError>> {
    let key = receiver_key(receiver, HostKind::HttpRequest)?;
    let current = state(runtime, "request", key)?;
    match (r.name.as_str(), r.descriptor.as_str()) {
        ("uri", "()Ljava/net/URI;") => current["uri"]
            .as_str()
            .map(|uri| Ok(crate::host_jdk_value::uri(uri))),
        ("method", "()Ljava/lang/String;") => current["method"]
            .as_str()
            .map(|method| Ok(Value::Ref(Some(method.into())))),
        ("timeout", "()Ljava/util/Optional;") => Some(Ok(current["timeout"].as_str().map_or_else(
            crate::host_optional::empty_optional,
            |text| {
                crate::host_optional::optional_wrap(crate::host_jdk_value::duration(
                    text.parse().unwrap_or(0),
                ))
            },
        ))),
        ("expectContinue", "()Z") => Some(Ok(Value::Int(i32::from(
            current["expect"].as_bool().unwrap_or(false),
        )))),
        ("version", "()Ljava/util/Optional;") => {
            Some(Ok(field(runtime, "request", key, "version").map_or_else(
                crate::host_optional::empty_optional,
                crate::host_optional::optional_wrap,
            )))
        }
        _ => None,
    }
}
